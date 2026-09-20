# ADR 0006: Removing every C/C++ dependency, and what it costs

- Status: **Accepted**, with measured regressions recorded below and open work listed.
- Date: 2026-09-20

## Context

`emgr` linked six C/C++ libraries: mozjpeg/libjpeg-turbo (JPEG both directions), libwebp
(WebP both directions), libavif with AOM and dav1d (AVIF both directions), mimalloc (the
global allocator), aws-lc (rustls' crypto provider) and zstd (HTTP compression). Building
the image needed `nasm`, `cmake`, `meson` and `ninja-build`, declared in two separate
places — the Dockerfile and four jobs in `ci.yml`.

The decision to remove all of them is a project-level one and is not re-litigated here.
This ADR records **what replaced each, and what it measurably cost**, because most of the
replacements are worse on at least one axis and that should be written down rather than
discovered later.

## Decision

| Was | Now | Pure Rust? |
|---|---|---|
| mozjpeg (encode) | `jpeg-encoder` | yes |
| mozjpeg `Decompress::scale` | `jpeg-decoder` `Decoder::scale()` | yes |
| libwebp | `vaam-image-webp` (fork of `image-webp`) | yes |
| libavif + AOM (encode) | `ravif` → `rav1e` | yes |
| libavif + dav1d (decode) | `vaam-avif-decode` → `rav1d` | yes |
| mimalloc | platform allocator | n/a |
| aws-lc-rs | `rustls-graviola` | yes |
| zstd-sys | dropped; gzip/brotli/deflate kept | yes |

Two forks were necessary and both are meant to be temporary:

- **`vaam-image-webp`** — no *published* crate encodes lossy WebP in pure Rust.
  `image-webp` 0.2.4 is lossless-only, and lossless WebP runs 5–12x larger than lossy on
  photographs. Upstream `main` has a working lossy VP8 encoder that has not been released.
- **`vaam-avif-decode`** — `avif-decode` declares `rav1d` twice, once per target, and only
  the non-x86 table disables its `asm` feature. rav1d's asm assembles `.S` with a C
  compiler and `.asm` with NASM, so **x86_64 builds required a toolchain the images no
  longer ship** while aarch64 did not. Cargo feature unification is additive, so this
  could not be fixed from `Cargo.toml`. Upstream's non-x86 exception is itself only a
  workaround for a rav1d packaging bug (memorysafety/rav1d#1449), so even the aarch64
  behaviour was incidental.

## Method

Two axes, measured separately, because they move in opposite directions.

**Speed** — the repo's own criterion benches (`benches/encode.rs`, `benches/decode.rs`),
same fixtures and same machine, run against a pristine checkout of the base commit and
against this branch. **PNG is the control**: nothing in this change touches it, so its
ratio bounds environment drift. It lands at 0.92–1.01x, so differences beyond a few
percent elsewhere are real.

**Size** — nominal quality numbers are not comparable across encoders, so every size
figure below is **DSSIM-matched**: each encoder's quality scale is bisected to hit the
same perceptual target and the resulting byte sizes are compared. Corpus is the Kodak
True Color suite (real photographs; the `bench-imgproxy` fixtures are synthetic
gradient+noise and compress unrealistically — ADRs 0001, 0003, 0004 and 0005 all record
this trap, and it is avoided again here).

## Results

### Speed (median, ratio vs the C build; >1.00 is slower)

| Benchmark | C build | Pure Rust | Ratio |
|---|---|---|---|
| `encode/jpeg_baseline/photo` | 0.92 ms | 2.76 ms | **3.00x slower** |
| `encode/jpeg_444/photo` | 1.28 ms | 2.65 ms | **2.07x slower** |
| `encode/jpeg_progressive/photo` | 24.76 ms | 2.80 ms | **0.11x — 9x faster** |
| `encode/jpeg_444_progressive/photo` | 33.57 ms | 2.74 ms | **0.08x — 12x faster** |
| `decode/jpeg/photo_1920x1080` | 4.71 ms | 10.90 ms | **2.32x slower** |
| `decode/jpeg/photo_640x360` | 0.60 ms | 1.48 ms | **2.45x slower** |
| `encode/avif/photo` | 62.69 ms | 89.22 ms | **1.42x slower** |
| `encode/webp/photo` | 20.30 ms | 29.06 ms | **1.43x slower** |
| `encode/png_best/photo` (control) | 64.53 ms | 64.96 ms | 1.01x |
| `encode/png_default/photo` (control) | 1.36 ms | 1.25 ms | 0.92x |

### End-to-end pipeline (the number that actually matters)

Micro-benchmarks isolate one codec call; this is the whole request path — decode, resize,
encode.

| Pipeline | C build | Pure Rust | Ratio |
|---|---|---|---|
| `photo_like_thumbnail_jpg` | 6.04 ms | 9.65 ms | **1.60x slower** |
| `photo_4k_large_downscale_thumbnail_jpg` | 19.19 ms | 33.87 ms | **1.76x slower** |
| `photo_real_thumbnail_jpg` | 3.59 ms | 6.10 ms | **1.70x slower** |
| `photo_real_large_large_downscale_thumbnail_jpg` | 3.35 ms | 6.39 ms | **1.91x slower** |
| `photo_with_exif_strip_metadata_default` | 6.05 ms | 9.67 ms | **1.60x slower** |
| `alpha_resize_webp` | 5.20 ms | 6.28 ms | 1.21x slower |
| `flat_resize_png` (control) | 8.45 ms | 8.23 ms | 0.97x |

**A JPEG request now costs roughly 1.6–1.9x what it did.** The PNG control at 0.97x on the
same run confirms that is a real codec effect and not environment drift. Given that
rivalling imgproxy on cold-cache latency is this project's stated goal, this is the
regression to weigh against the dependency win — it is not a rounding error.

The progressive-JPEG result is not a pure win. mozjpeg's progressive path ran
`JCP_MAX_COMPRESSION`, which is why it cost ~19x its own baseline mode;
`jpeg-encoder`'s progressive costs about the same as its baseline. It is doing
substantially less work, and the size table below is where that shows up.

### Size, DSSIM-matched (Kodak, n=6, median; >1.00 is bigger)

| Target DSSIM | JPEG: `jpeg-encoder` / mozjpeg | WebP: fork / libwebp |
|---|---|---|
| ≤ 0.0150 (low quality) | **1.407x** | **1.893x** |
| ≤ 0.0080 (mid) | **1.257x** | **1.924x** |
| ≤ 0.0035 (high quality) | **1.156x** | **2.191x** |

JPEG costs 16–41% more bytes, worst at low quality — which is where most CDN traffic
sits. The cause is that `jpeg-encoder` implements no trellis quantisation; nothing in
pure Rust currently does at production maturity.

WebP costs 89–119% more bytes. The WebP figure is **after** implementing
rate-distortion intra mode decision in the fork, which improved it from 2.148x / 2.185x /
2.287x. Before that work, `choose_macroblock_info` ignored both its arguments and returned
flat DC prediction for every macroblock of every image, with skip never signalled.

### Benchmarks that are NOT comparable, and must not be read as results

`decode/avif/*` and `decode/webp/*` appear to show large speedups (AVIF as much as 0.20x).
**Ignore them.** Those benches decode fixtures produced by the encoder under test, and
both encoders changed: AVIF fixtures moved from AOM 4:2:0 to ravif 4:4:4, and WebP
fixtures from libwebp to the fork. The two runs decode different bitstreams, so the
comparison measures content, not decoder speed. Making these comparable needs a fixed
corpus of pre-encoded files checked in, which does not exist yet.

## A correctness bug found on the way

Implementing mode decision in the WebP fork surfaced a latent bug in its boolean encoder.
`add_one_to_output` popped already-written `0xFF` bytes during carry propagation and,
failing a `value < 255` check, discarded them instead of writing back `0x00` and
continuing the carry — physically shortening the stream and desynchronising every
subsequent decoder read.

It was invisible before because the old encoder was too primitive to trigger it: a single
fixed DC mode with skip never signalled almost never produces a carry across a saturated
byte. It manifested as silent corruption at exactly one (image, quality) pair — kodim02 at
q85, DSSIM 0.0636 against ~0.008 either side — with **no error signal**, and both this
crate's decoder and libwebp agreeing byte-for-byte on the wrong pixels, which is what
proved the fault was in the bitstream rather than a decoder. Reported upstream as
image-rs/image-webp#191 with the fix.

Worth generalising: preset-based quality sweeps (q60/q75/q82/q90) cannot catch a defect
that occupies a single quality step. This was found by a DSSIM bisection happening to land
on q85.

## Consequences

Accepted regressions: JPEG is slower to encode at baseline quality, slower to decode, and
16–41% larger; WebP is ~2x larger; AVIF encode is 1.42x slower.

Behaviour changes, each pinned by a test rather than left to be discovered:

- **AVIF decode no longer recovers EXIF, ICC or orientation.** `avif-parse` does not parse
  those properties, so `decode` returns `Orientation::NoTransforms` and `None`/`None`.
  Encode still writes EXIF.
- **AVIF encodes 4:4:4**; the AOM path used 4:2:0. Larger output independent of quality.
- **Animated WebP is lossless-only.** The fork cannot encode animation at all, so the
  `VP8X`/`ANIM`/`ANMF` container is assembled directly around per-frame bitstreams;
  `quality` no longer applies to animated output.
- **AVIF dimensions come from the AV1 sequence header**, not the container's `ispe` box.
  This is the stronger guarantee — `ispe` is metadata that can disagree with the stream it
  describes — and it means the old `ispe`-patching bomb fixture no longer constructs a
  bomb.

Not measured, and honestly unknown:

- **The x86_64 cost of building rav1d without assembly.** dav1d's asm is a large part of
  why it is fast. This could not be measured here: the only available machine is aarch64,
  where upstream already builds asm-free and where Apple's assembler rejects rav1d's own
  `-march` flags. **Someone should measure this on x86_64 before trusting AVIF decode
  latency in production.**
- The platform allocator versus mimalloc under real concurrent load.
- AVIF output size, which moved on two axes at once (encoder and chroma subsampling).

Open work, in rough order of value:

1. Measure rav1d-without-asm on x86_64.
2. Close the WebP gap: B_PRED 4x4 mode search, then trellis quantisation.
3. JPEG decode is 2.3x slower and `jpeg-decoder` is in maintenance mode upstream. The
   faster `zune-jpeg` has no scaled-decode API at all, which is why it was not chosen — but
   a hybrid (zune for full-size decodes, `jpeg-decoder` only when DCT scaling applies) is
   worth measuring.
4. Check in a fixed pre-encoded corpus so `decode/avif` and `decode/webp` become
   comparable across encoder changes.
5. Retire both forks when upstream releases lossy WebP encoding and a rav1d-asm feature
   flag.
