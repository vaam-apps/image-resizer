//! Issue #138: measure what building rav1d WITH assembly costs on x86_64,
//! versus this service's actual asm-free fork - see `Cargo.toml`'s own
//! doc comment for why this is an orchestrator plus two isolated
//! subprocess crates (`decoder-asm/`, `decoder-noasm/`) rather than one
//! binary linking both decoders directly.
//!
//! # What this does
//!
//! 1. Reads `benches/fixtures/real/{blue-marble,earthrise}.jpg` under the
//!    given repo root, decodes them with `image`, and encodes each to AVIF
//!    at a couple of qualities with `ravif` - the same crate/call pattern
//!    `src/services/image/avif_codec.rs::encode_inner` uses in production.
//! 2. Builds `decoder-asm` and `decoder-noasm` in release mode.
//! 3. For each (fixture, quality), alternates calling the two decoder
//!    binaries in short rounds (never running all of one before any of the
//!    other) so thermal drift cannot favour whichever goes first, and
//!    asserts both report identical pixel hashes for the same AVIF bytes
//!    before trusting any timing from that pair.
//! 4. Prints a per-fixture/quality table (asm-on median ms, asm-off median
//!    ms, ratio off/on) plus an overall summary, to stdout and, if
//!    `--markdown-out <path>` is given, as a markdown table to that file
//!    too (the workflow feeds that into `$GITHUB_STEP_SUMMARY`).
//!
//! Run locally (from the repo root):
//! ```text
//! cargo run --release --manifest-path tools/avif-asm-bench/Cargo.toml -- .
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Encode qualities to compare at - `80` matches `handler.rs::
/// DEFAULT_AVIF_QUALITY`; `50` is a second, lower point so the comparison
/// isn't resting on a single nominal quality (higher quality generally
/// means more AV1 blocks to decode, so asm's advantage isn't assumed to be
/// quality-invariant).
const QUALITIES: [u8; 2] = [50, 80];
/// Matches `handler.rs::DEFAULT_AVIF_SPEED` - the actual production speed
/// setting, so the corpus's AV1 bitstream complexity (block sizes, tools
/// used) matches what this service really emits.
const SPEED: u8 = 6;

/// Total timed decodes per side per (fixture, quality) = ROUNDS *
/// ITERS_PER_ROUND. Split into rounds (rather than one block of ITERS each
/// side) specifically so the two sides interleave - see the module doc
/// comment.
const ROUNDS: usize = 6;
const ITERS_PER_ROUND: usize = 5;

struct Fixture {
    name: &'static str,
    relative_path: &'static str,
}

const FIXTURES: [Fixture; 2] = [
    Fixture {
        name: "blue-marble",
        relative_path: "benches/fixtures/real/blue-marble.jpg",
    },
    Fixture {
        name: "earthrise",
        relative_path: "benches/fixtures/real/earthrise.jpg",
    },
];

struct DecoderSide {
    label: &'static str,
    crate_dir: &'static str,
    bin_name: &'static str,
}

const ASM: DecoderSide = DecoderSide {
    label: "asm-on (upstream avif-decode)",
    crate_dir: "decoder-asm",
    bin_name: "decoder-asm",
};
const NOASM: DecoderSide = DecoderSide {
    label: "asm-off (vaam-avif-decode fork)",
    crate_dir: "decoder-noasm",
    bin_name: "decoder-noasm",
};

struct DecodeResult {
    dims: (u32, u32),
    hash: String,
    nanos: Vec<u128>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("avif-asm-bench: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut positional = Vec::new();
    let mut markdown_out: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--markdown-out" {
            let path = args
                .next()
                .ok_or("--markdown-out requires a path argument")?;
            markdown_out = Some(PathBuf::from(path));
        } else {
            positional.push(arg);
        }
    }
    let repo_root = PathBuf::from(positional.first().cloned().unwrap_or_else(|| ".".into()));
    let harness_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let target_triple = host_target_triple()?;
    println!("target triple: {target_triple}");

    let asm_has_asm = asm_feature_enabled(&harness_dir.join(ASM.crate_dir), &target_triple)?;
    let noasm_has_asm = asm_feature_enabled(&harness_dir.join(NOASM.crate_dir), &target_triple)?;
    println!("{}: rav1d `asm` feature enabled = {asm_has_asm}", ASM.label);
    println!(
        "{}: rav1d `asm` feature enabled = {noasm_has_asm}",
        NOASM.label
    );
    if !asm_has_asm {
        println!(
            "NOTE: the asm-on side reports no asm for target {target_triple}. On a \
             non-x86 target that is expected - upstream only enables asm for x86/x86_64 - \
             and a ratio near 1.0 below is the correct result rather than a measurement of \
             #138; run the avif-asm-bench workflow on an x86_64 runner for that. On an \
             x86/x86_64 target this line is itself the bug: check it against the ratios \
             below before believing either. A large ratio with this note present means the \
             detection is wrong, not the timings - assembly is the only feature that \
             differs between the two sides."
        );
    }
    if noasm_has_asm {
        return Err(format!(
            "BUG: {target_triple}'s no-asm side ({}) resolved rav1d's `asm` feature as \
             enabled - the comparison would be meaningless. Refusing to run.",
            NOASM.crate_dir
        ));
    }

    println!("building {} (release)...", ASM.crate_dir);
    build_release(&harness_dir.join(ASM.crate_dir))?;
    println!("building {} (release)...", NOASM.crate_dir);
    build_release(&harness_dir.join(NOASM.crate_dir))?;

    let asm_bin = release_binary(&harness_dir.join(ASM.crate_dir), ASM.bin_name);
    let noasm_bin = release_binary(&harness_dir.join(NOASM.crate_dir), NOASM.bin_name);

    let tmp_dir = std::env::temp_dir().join(format!("avif-asm-bench-{}", std::process::id()));
    fs::create_dir_all(&tmp_dir).map_err(|e| format!("creating {tmp_dir:?}: {e}"))?;

    println!(
        "\niterations per side per (fixture, quality): {} ({} rounds x {} iters, alternating)\n",
        ROUNDS * ITERS_PER_ROUND,
        ROUNDS,
        ITERS_PER_ROUND
    );

    let mut rows: Vec<(String, u8, f64, f64, f64)> = Vec::new();

    for fixture in &FIXTURES {
        let jpg_path = repo_root.join(fixture.relative_path);
        let jpg_bytes =
            fs::read(&jpg_path).map_err(|e| format!("reading fixture {jpg_path:?}: {e}"))?;
        let image = image::load_from_memory(&jpg_bytes)
            .map_err(|e| format!("decoding fixture {jpg_path:?} as JPEG: {e}"))?;
        let rgba = image.to_rgba8();
        let (width, height) = rgba.dimensions();

        for &quality in &QUALITIES {
            let avif_bytes = encode_avif(&rgba, width, height, quality)?;
            let avif_path = tmp_dir.join(format!("{}-q{quality}.avif", fixture.name));
            fs::write(&avif_path, &avif_bytes)
                .map_err(|e| format!("writing {avif_path:?}: {e}"))?;

            println!(
                "== {} @ quality {quality} ({} bytes AVIF, {width}x{height} source) ==",
                fixture.name,
                avif_bytes.len()
            );

            let asm_result = run_decoder_alternating(&asm_bin, &noasm_bin, &avif_path)?;

            if asm_result.0.dims != asm_result.1.dims || asm_result.0.hash != asm_result.1.hash {
                return Err(format!(
                    "{} @ quality {quality}: decoders disagree - asm-on dims={:?} hash={}, \
                     asm-off dims={:?} hash={} - a speed comparison between decoders that \
                     disagree is meaningless, refusing to report timings",
                    fixture.name,
                    asm_result.0.dims,
                    asm_result.0.hash,
                    asm_result.1.dims,
                    asm_result.1.hash
                ));
            }

            let asm_median_ms = median_ms(&asm_result.0.nanos);
            let noasm_median_ms = median_ms(&asm_result.1.nanos);
            let ratio = noasm_median_ms / asm_median_ms;

            println!(
                "  pixels match ({:?}, hash {})",
                asm_result.0.dims, asm_result.0.hash
            );
            println!(
                "  asm-on median:  {asm_median_ms:.3} ms  ({} samples)",
                asm_result.0.nanos.len()
            );
            println!(
                "  asm-off median: {noasm_median_ms:.3} ms  ({} samples)",
                asm_result.1.nanos.len()
            );
            println!("  ratio (off/on): {ratio:.3}x\n");

            rows.push((
                fixture.name.to_string(),
                quality,
                asm_median_ms,
                noasm_median_ms,
                ratio,
            ));
        }
    }

    let _ = fs::remove_dir_all(&tmp_dir);

    let mean_ratio = rows.iter().map(|r| r.4).sum::<f64>() / rows.len() as f64;
    println!("== summary ==");
    println!(
        "mean ratio (asm-off / asm-on) across {} fixture/quality combinations: {mean_ratio:.3}x",
        rows.len()
    );

    if let Some(path) = markdown_out {
        write_markdown(&path, &target_triple, &rows, mean_ratio)?;
    }

    Ok(())
}

fn encode_avif(
    rgba: &image::RgbaImage,
    width: u32,
    height: u32,
    quality: u8,
) -> Result<Vec<u8>, String> {
    use rgb::FromSlice;
    let pixels = rgba.as_raw().as_rgba();
    let buffer = imgref::Img::new(pixels, width as usize, height as usize);
    let encoded = ravif::Encoder::new()
        .with_quality(f32::from(quality))
        .with_alpha_quality(f32::from(quality))
        .with_speed(SPEED)
        .encode_rgba(buffer)
        .map_err(|e| format!("ravif encode failed: {e}"))?;
    Ok(encoded.avif_file)
}

/// Runs `ROUNDS` alternating rounds of `ITERS_PER_ROUND` decodes each,
/// asm-side first then no-asm-side each round (never all-of-one-then-all-
/// of-the-other), accumulating each side's per-iteration nanosecond samples
/// plus its (dims, hash) - which should be identical across every round
/// since the input bytes never change.
fn run_decoder_alternating(
    asm_bin: &Path,
    noasm_bin: &Path,
    avif_path: &Path,
) -> Result<(DecodeResult, DecodeResult), String> {
    let mut asm_nanos = Vec::with_capacity(ROUNDS * ITERS_PER_ROUND);
    let mut noasm_nanos = Vec::with_capacity(ROUNDS * ITERS_PER_ROUND);
    let mut asm_dims = None;
    let mut asm_hash = None;
    let mut noasm_dims = None;
    let mut noasm_hash = None;

    for round in 0..ROUNDS {
        let a = invoke_decoder(asm_bin, avif_path, ITERS_PER_ROUND)
            .map_err(|e| format!("round {round}, asm-on: {e}"))?;
        asm_dims.get_or_insert(a.0);
        asm_hash.get_or_insert(a.1.clone());
        asm_nanos.extend(a.2);

        let n = invoke_decoder(noasm_bin, avif_path, ITERS_PER_ROUND)
            .map_err(|e| format!("round {round}, asm-off: {e}"))?;
        noasm_dims.get_or_insert(n.0);
        noasm_hash.get_or_insert(n.1.clone());
        noasm_nanos.extend(n.2);
    }

    Ok((
        DecodeResult {
            dims: asm_dims.unwrap(),
            hash: asm_hash.unwrap(),
            nanos: asm_nanos,
        },
        DecodeResult {
            dims: noasm_dims.unwrap(),
            hash: noasm_hash.unwrap(),
            nanos: noasm_nanos,
        },
    ))
}

/// (dimensions, pixel hash, per-iteration nanosecond samples) - one
/// decoder subprocess invocation's parsed result.
type DecoderInvocation = ((u32, u32), String, Vec<u128>);

/// Runs `bin_path <avif_path> <iterations>` and parses its stdout protocol
/// (see `decoder-asm/src/main.rs`'s module doc comment): a `DIMS`/`HASH`
/// line pair followed by `iterations` `ITER <nanos>` lines.
fn invoke_decoder(
    bin_path: &Path,
    avif_path: &Path,
    iterations: usize,
) -> Result<DecoderInvocation, String> {
    let output = Command::new(bin_path)
        .arg(avif_path)
        .arg(iterations.to_string())
        .output()
        .map_err(|e| format!("spawning {bin_path:?}: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "{bin_path:?} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut dims = None;
    let mut hash = None;
    let mut nanos = Vec::with_capacity(iterations);

    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("DIMS ") {
            let mut parts = rest.split_whitespace();
            let w: u32 = parts
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| format!("bad DIMS line: {line}"))?;
            let h: u32 = parts
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| format!("bad DIMS line: {line}"))?;
            dims = Some((w, h));
        } else if let Some(rest) = line.strip_prefix("HASH ") {
            hash = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("ITER ") {
            let n: u128 = rest
                .trim()
                .parse()
                .map_err(|e| format!("bad ITER line {line:?}: {e}"))?;
            nanos.push(n);
        }
    }

    let dims = dims.ok_or_else(|| format!("{bin_path:?}: no DIMS line in output"))?;
    let hash = hash.ok_or_else(|| format!("{bin_path:?}: no HASH line in output"))?;
    if nanos.len() != iterations {
        return Err(format!(
            "{bin_path:?}: expected {iterations} ITER lines, got {}",
            nanos.len()
        ));
    }

    Ok((dims, hash, nanos))
}

fn median_ms(nanos: &[u128]) -> f64 {
    let mut sorted = nanos.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    let median_ns = if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) as f64 / 2.0
    } else {
        sorted[mid] as f64
    };
    median_ns / 1_000_000.0
}

fn build_release(crate_dir: &Path) -> Result<(), String> {
    let status = Command::new("cargo")
        .args(["build", "--release", "--quiet"])
        .current_dir(crate_dir)
        .status()
        .map_err(|e| format!("spawning cargo build in {crate_dir:?}: {e}"))?;
    if !status.success() {
        return Err(format!("cargo build failed in {crate_dir:?}: {status}"));
    }
    Ok(())
}

fn release_binary(crate_dir: &Path, bin_name: &str) -> PathBuf {
    let exe = if cfg!(windows) {
        format!("{bin_name}.exe")
    } else {
        bin_name.to_string()
    };
    crate_dir.join("target").join("release").join(exe)
}

/// The triple this process is actually running as, parsed from `rustc
/// -vV`'s `host:` line - used both to label the report and to pick which
/// target `cargo tree` resolves cfg predicates against for the asm-feature
/// check below. On an x86_64 GitHub Actions runner this is
/// `x86_64-unknown-linux-gnu`; on this aarch64 laptop it's
/// `aarch64-apple-darwin`.
fn host_target_triple() -> Result<String, String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|e| format!("running rustc -vV: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_string)
        .ok_or_else(|| "rustc -vV output had no `host:` line".to_string())
}

/// Derives whether rav1d's `asm` feature is actually enabled in
/// `crate_dir`'s resolved dependency graph, from the graph itself rather
/// than assumed - same technique `ci.yml`'s `no-native-deps` job already
/// uses (`cargo tree -e features ... | grep -E 'feature "asm'`), scoped
/// to a single isolated crate here so it reports that crate's own truth
/// rather than the whole workspace's.
///
/// Matches the *exact* `rav1d feature "asm"` line, not a prefix match -
/// rav1d also has `asm_arm64_dotprod`/`asm_arm64_i8mm`/`asm_arm64_sve2`
/// features, which contain "asm" as a substring but are a different,
/// aarch64-specific concern from the x86/x86_64 assembly this issue is
/// about.
fn asm_feature_enabled(crate_dir: &Path, target_triple: &str) -> Result<bool, String> {
    let output = Command::new("cargo")
        .args([
            "tree",
            "--target",
            target_triple,
            "-e",
            "features",
            "-i",
            "rav1d",
        ])
        .current_dir(crate_dir)
        .output()
        .map_err(|e| format!("running cargo tree in {crate_dir:?}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo tree failed in {crate_dir:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);

    // `cargo tree -i` prints "nothing to print" and exits 0 when the package
    // is not in the graph at all, so an absent `rav1d` line cannot be read as
    // "asm is off" - it is indistinguishable from "this ran somewhere wrong".
    // That distinction is not academic: the #138 CI run reported `asm enabled
    // = false` for BOTH sides while the two binaries differed by 3.03x, and
    // printed a note telling the reader to dismiss the real numbers as a null
    // result. Refuse to answer rather than answer falsely - the same rule
    // `ci.yml`'s `no-native-deps` job had to learn.
    if !stdout.lines().any(|line| line.contains("rav1d")) {
        return Err(format!(
            "cargo tree in {crate_dir:?} produced no `rav1d` line for target \
             {target_triple}, so the asm feature cannot be determined. Output was:\n{stdout}"
        ));
    }

    Ok(stdout.lines().any(|line| {
        line.trim_start_matches(|c: char| !c.is_alphanumeric() && c != '"')
            == "rav1d feature \"asm\""
    }))
}

fn write_markdown(
    path: &Path,
    target_triple: &str,
    rows: &[(String, u8, f64, f64, f64)],
    mean_ratio: f64,
) -> Result<(), String> {
    let mut out = String::new();
    out.push_str("### AVIF decode: rav1d asm vs no-asm (issue #138)\n\n");
    out.push_str(&format!("Target: `{target_triple}`\n\n"));
    out.push_str(&format!(
        "Iterations per side per row: {} ({} rounds x {} iters, alternating)\n\n",
        ROUNDS * ITERS_PER_ROUND,
        ROUNDS,
        ITERS_PER_ROUND
    ));
    out.push_str(
        "| fixture | quality | asm-on median (ms) | asm-off median (ms) | ratio (off/on) |\n",
    );
    out.push_str("|---|---|---|---|---|\n");
    for (fixture, quality, asm_ms, noasm_ms, ratio) in rows {
        out.push_str(&format!(
            "| {fixture} | {quality} | {asm_ms:.3} | {noasm_ms:.3} | {ratio:.3}x |\n"
        ));
    }
    out.push_str(&format!(
        "\n**Mean ratio (asm-off / asm-on): {mean_ratio:.3}x**\n"
    ));
    fs::write(path, out).map_err(|e| format!("writing {path:?}: {e}"))
}
