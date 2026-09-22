//! Minimal decode-timing worker for issue #138. Deliberately dumb: read one
//! already-encoded AVIF file from disk, decode it `iterations` times with
//! whichever `avif_decode` this crate was built against, and print raw
//! per-iteration timings plus a pixel hash. All comparison/median/ratio
//! logic lives in the orchestrator (`../src/main.rs`), which invokes this
//! binary (and `decoder-asm`, its twin) as a subprocess per round.
//!
//! This file is intentionally near-identical to `decoder-asm/src/main.rs`.
//! The only difference between the two crates is which package their
//! `avif-decode` dependency alias resolves to (see each crate's own
//! `Cargo.toml`). Keeping the code itself identical means any timing
//! difference the orchestrator observes traces to the dependency, not to
//! divergent decode logic between the two binaries.
//!
//! # Protocol (stdout)
//!
//! ```text
//! DIMS <width> <height>
//! HASH <16 lowercase hex chars, FNV-1a/64 over the flattened RGB(A)8 bytes>
//! ITER <elapsed nanoseconds>
//! ITER <elapsed nanoseconds>
//! ...                          (one per requested iteration)
//! ```
//!
//! Any failure (bad args, missing file, decode error) prints to stderr and
//! exits non-zero - the orchestrator treats that as fatal, not as a data
//! point, per the task's "fail loudly" requirement for a disagreeing
//! decode.

use std::env;
use std::fs;
use std::process::ExitCode;
use std::time::Instant;

/// FNV-1a/64 - not cryptographic, just cheap and dependency-free (this
/// crate's only Cargo dependency is `avif-decode` itself, kept minimal on
/// purpose so `cargo tree -e features -i rav1d` run inside this directory
/// reflects nothing but that one dependency's own feature resolution).
fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01B3;
    let mut hash = OFFSET;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Flattens any `avif_decode::Image` variant into raw interleaved
/// component bytes (8-bit variants as-is, 16-bit as little-endian pairs),
/// for hashing and for the orchestrator's identical-pixels assertion. Every
/// AVIF this harness decodes was itself encoded by `ravif::Encoder::
/// encode_rgba` from an 8-bit RGBA source (see the orchestrator), so in
/// practice only `Rgba8`/`Rgb8` are ever reachable here - the other arms
/// are filled in anyway so this stays correct if that ever changes, rather
/// than silently mishandling a variant the corpus doesn't currently
/// exercise.
fn flatten(image: avif_decode::Image) -> (Vec<u8>, usize, usize) {
    match image {
        avif_decode::Image::Rgba8(pixels) => {
            let (buf, w, h) = pixels.into_contiguous_buf();
            let mut bytes = Vec::with_capacity(buf.len() * 4);
            for px in buf {
                bytes.extend_from_slice(&[px.r, px.g, px.b, px.a]);
            }
            (bytes, w, h)
        }
        avif_decode::Image::Rgb8(pixels) => {
            let (buf, w, h) = pixels.into_contiguous_buf();
            let mut bytes = Vec::with_capacity(buf.len() * 3);
            for px in buf {
                bytes.extend_from_slice(&[px.r, px.g, px.b]);
            }
            (bytes, w, h)
        }
        avif_decode::Image::Gray8(pixels) => {
            let (buf, w, h) = pixels.into_contiguous_buf();
            // `.value()`: an inherent method `rgb::Gray` generates via its
            // `inherent_impls!` macro - no direct `rgb` dependency needed
            // to call it, unlike a fully-qualified `rgb::Gray::value` path.
            let bytes: Vec<u8> = buf.into_iter().map(|g| g.value()).collect();
            (bytes, w, h)
        }
        avif_decode::Image::Rgba16(pixels) => {
            let (buf, w, h) = pixels.into_contiguous_buf();
            let mut bytes = Vec::with_capacity(buf.len() * 8);
            for px in buf {
                for word in [px.r, px.g, px.b, px.a] {
                    bytes.extend_from_slice(&word.to_le_bytes());
                }
            }
            (bytes, w, h)
        }
        avif_decode::Image::Rgb16(pixels) => {
            let (buf, w, h) = pixels.into_contiguous_buf();
            let mut bytes = Vec::with_capacity(buf.len() * 6);
            for px in buf {
                for word in [px.r, px.g, px.b] {
                    bytes.extend_from_slice(&word.to_le_bytes());
                }
            }
            (bytes, w, h)
        }
        avif_decode::Image::Gray16(pixels) => {
            let (buf, w, h) = pixels.into_contiguous_buf();
            let mut bytes = Vec::with_capacity(buf.len() * 2);
            for g in buf {
                bytes.extend_from_slice(&g.value().to_le_bytes());
            }
            (bytes, w, h)
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: decoder-noasm <avif-path> <iterations>")?;
    let iterations: usize = args
        .next()
        .ok_or("usage: decoder-noasm <avif-path> <iterations>")?
        .parse()
        .map_err(|e| format!("invalid iteration count: {e}"))?;

    let bytes = fs::read(&path).map_err(|e| format!("reading {path}: {e}"))?;

    // One untimed decode first, purely to report dimensions/hash - kept
    // out of the timed loop below so print/IO setup never pollutes a
    // measured sample.
    let first = avif_decode::Decoder::from_avif(&bytes)
        .map_err(|e| format!("{path}: from_avif: {e}"))?
        .to_image()
        .map_err(|e| format!("{path}: to_image: {e}"))?;
    let (flat, w, h) = flatten(first);
    println!("DIMS {w} {h}");
    println!("HASH {:016x}", fnv1a_64(&flat));

    let mut nanos = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let image = avif_decode::Decoder::from_avif(&bytes)
            .map_err(|e| format!("{path}: from_avif: {e}"))?
            .to_image()
            .map_err(|e| format!("{path}: to_image: {e}"))?;
        let elapsed = start.elapsed();
        std::hint::black_box(&image);
        nanos.push(elapsed.as_nanos());
    }

    for n in nanos {
        println!("ITER {n}");
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("decoder-noasm: {e}");
            ExitCode::FAILURE
        }
    }
}
