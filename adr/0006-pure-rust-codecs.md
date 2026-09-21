# ADR 0006: Removing every C/C++ dependency, and what it costs

- Status: **Accepted**, with measured regressions recorded below and open work listed.
- Date: 2026-09-20 (size, decode and cross-format figures corrected 2026-09-21)

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

These are the repo's criterion benches, which encode at a **fixed nominal
quality**. That is the right shape for tracking regressions but the wrong one for
comparing encoders, since equal quality numbers are not equal quality. See
"Encode time, at the matched-quality point" below, where AVIF in particular
reverses sign.

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

### Size, DSSIM-matched (Kodak, n=24, median; >1.00 is bigger)

**Corrected 2026-09-21.** An earlier revision of this ADR reported JPEG as
1.16x-1.41x larger. That was wrong, and wrong in an instructive way: it compared
`jpeg-encoder` against raw `mozjpeg::Compress` at its *default* settings, which
enable trellis quantisation. Production never used those settings. The
non-progressive path sets mozjpeg's `JCP_FASTEST` profile, under which trellis is
off - `trellis_quant = (compress_profile == JCP_MAX_COMPRESSION)`. Measuring the
library instead of the call site invented a regression that does not exist.

Re-measured through `ImageService::encode_jpeg`/`encode_webp`/`avif_codec::encode`
themselves (`examples/codec_report.rs`), all 24 Kodak images, zero failed
bisections:

| Format | <= 0.0150 | <= 0.0080 | <= 0.0035 |
|---|---|---|---|
| **JPEG** (default, non-progressive) | **0.995x** | **1.000x** | **1.000x** |
| **JPEG progressive** (`jpgo:1:`) | **2.014x** | **1.658x** | **1.317x** |
| **WebP** | 1.878x | 1.970x | 2.247x |
| **AVIF** | 1.158x | 1.101x | 1.040x |
| PNG (control, lossless) | 1.000x | - | - |

The trellis loss is real, but it lands **only on the opt-in progressive path**,
where it is worse than the old figure suggested - up to 2x at low quality. The
default path is at parity because it never had trellis to lose. PNG is
byte-identical, as it must be: same encoder on both sides.

AVIF costs 4-16%, not the larger penalty feared from ravif encoding 4:4:4 where
AOM used 4:2:0.

### Cross-format: what to actually serve

Same corpus and method, comparing formats *within* each build rather than across
builds. This is what should drive `.auto` negotiation.

| Relative to that build's JPEG | <= 0.0150 | <= 0.0080 | <= 0.0035 |
|---|---|---|---|
| **Pure Rust** - AVIF | **0.567x** | **0.700x** | **0.787x** |
| **Pure Rust** - WebP | 1.145x | 1.465x | 1.978x |
| **Pure Rust** - JPEG progressive | 1.382x | 1.278x | 1.177x |
| C stack - AVIF | 0.496x | 0.656x | 0.781x |
| C stack - WebP | 0.656x | 0.805x | 0.884x |
| C stack - JPEG progressive | 0.663x | 0.759x | 0.874x |

Two conclusions, both actionable:

1. **AVIF still wins decisively** - 21-43% smaller than JPEG, barely changed from
   the C stack.
2. **WebP has inverted.** Under libwebp it was 12-34% *smaller* than JPEG; it is
   now 15-98% *larger*. Preferring WebP over JPEG is now actively harmful, and
   negotiation should reflect that until the encoder improves. The same is true of
   progressive JPEG, which used to be the smaller option and no longer is.

### Decode, like for like (n=24 images x 3 targets, same bytes into both builds)

An earlier revision said decode could not be compared, because the `decode/*`
benches decode fixtures made by the encoder under test. That remains true of
those benches - but `examples/codec_report.rs` sidesteps it by decoding **the
same C-produced files** in both builds:

| Format | C | Pure Rust | Ratio |
|---|---|---|---|
| JPEG | 0.72 ms | 1.78 ms | **2.44x slower** |
| JPEG progressive | 0.79 ms | 1.83 ms | **2.27x slower** |
| WebP | 1.95 ms | 5.66 ms | **2.89x slower** |
| AVIF | 5.00 ms | 6.45 ms | **1.28x slower** |

AVIF at 1.28x is the standout, and it is `rav1d` **with assembly disabled**
against `dav1d` **with NEON assembly enabled**. That is far better than the
"dav1d's asm is most of its speed" assumption recorded here as an open risk. It
is aarch64 evidence and does not settle x86_64, where SIMD gaps are generally
wider - but a catastrophic x86_64 regression now looks much less likely.

### Encode time, at the matched-quality point

| Format | C | Pure Rust | Ratio |
|---|---|---|---|
| JPEG | 0.95-1.08 ms | 2.88-3.07 ms | **~3.0x slower** |
| JPEG progressive | 16.05-26.72 ms | 2.93-3.15 ms | **0.12-0.18x** |
| WebP | 20.47-25.84 ms | 30.95-39.10 ms | **~1.5x slower** |
| AVIF | 66.53-100.11 ms | 60.03-126.20 ms | **0.82x-1.28x** |
| PNG (control) | 78.33 ms | 78.46 ms | 1.01x |

Two of these must be read with the size table, not quoted alone:

- **Progressive JPEG is not 6x faster in any useful sense.** mozjpeg ran
  `JCP_MAX_COMPRESSION` with trellis; `jpeg-encoder` does far less work and ships
  a file up to 2x larger for it.
- **AVIF encode is *faster* at low and mid quality** (0.82-0.85x), slower only at
  the strictest target (1.28x). This contradicts the fixed-quality bench above
  (1.42x slower) and both are correct: at a fixed nominal q80 ravif does more work
  than AOM at q80, while at matched perceptual quality it needs a lower setting
  and comes out ahead. The matched-quality figure describes production.

### Benchmarks that are NOT comparable, and must not be read as results

`decode/avif/*` and `decode/webp/*` in `benches/decode.rs` appear to show large
speedups (AVIF as much as 0.20x). **Ignore them.** They decode fixtures produced
by the encoder under test, and both encoders changed, so the two runs decode
different bitstreams - that measures content, not decoder speed. The like-for-like
table above is the real answer. Checking a fixed pre-encoded corpus into the repo
would fix the benches themselves.

## Are the replacements actually *correct*? (audited 2026-09-21)

Everything above measures size and speed. That is not the same question as
whether these implementations are right, so the decoders were checked directly:
the same C-encoded files decoded by both builds, compared pixel for pixel
(`examples/codec_report.rs conform`), 72 files per format.

| Format | bit-exact | max abs diff | mean abs diff | verdict |
|---|---|---|---|---|
| **WebP** | **72/72** | 0 | 0.00000 | conformant |
| JPEG | 0/72 | 4 | 0.047 | expected |
| JPEG progressive | 0/72 | 4 | 0.050 | expected |
| **AVIF** | 0/72 | **64** | **0.785** | **investigate** |

VP8 and AV1 decoding are exactly specified, so a conforming decoder must be
bit-identical. JPEG is not - the standard deliberately leaves IDCT precision
open - so a max difference of 4 is normal.

**The pure-Rust VP8 decoder is bit-exact against libwebp on every file.**

**AVIF is not, and the difference is systematic rather than random.** `rav1d`'s
output is brighter than libavif's on all three channels in all 72 files (mean
+0.527 R, +0.520 G, +0.555 B; every file positive, min +0.076, max +0.920). A
uniform ~+0.5 offset is the signature of a rounding or range-conversion
disagreement in **YUV -> RGB**, not of a fault in AV1 decoding itself - rav1d is
a line-by-line port of dav1d, and the residual would not be uniform if the
coefficient path differed. Against the original images libavif is closer in
**72 of 72** cases (median MSE 35.47 vs 37.22, ~5% worse).

**Investigated, and the obvious explanation is wrong.** The difference histogram
looked decisive: ~80% of bytes differ by 0 or +1 (the rounding bias), but 6.75%
differ by more than 2 and by as much as 64, concentrated on *alternating pixels*
with the sign flipping between rows - the classic signature of chroma upsampling,
since 4:2:0 chroma is half resolution. `avif-decode` calls `yuv`'s
`yuv420_to_rgb`, which replicates each chroma sample, while libavif interpolates
by default; and `yuv` ships `yuv420_to_rgb_bilinear`.

Switching to the bilinear variants made it **worse** on both metrics - median
MSE 37.22 -> 38.59, median DSSIM 0.007646 -> 0.007826 (+1.18%) - so whatever
libavif does, it is not plain bilinear, and the hypothesis is retired rather than
shipped. The change was reverted.

The useful result is the scale. On MSE the gap looks like 4.4%; **perceptually it
is about 1%** (DSSIM 0.007567 for libavif against 0.007646 for rav1d), and the
brightness bias is ~0.2%. MSE substantially oversells this. It is recorded
because nothing in the test suite was checking decoder fidelity at all and
"faithful port" was being taken on trust - but it does not justify further work,
and certainly not a hand-written upsampler.

## What the WebP encoder is still missing

The remaining ~1.9-2.2x is structural, and the gaps are visible in the source
rather than inferred:

| Gap | Evidence | Status |
|---|---|---|
| Loop filter never applied | `loop_filter::` appears only in the decoder | **fixed**: level was 63, now 0, mean -4.0% |
| B_PRED (4x4 intra) | `LumaMode::B => unreachable!()` in the encoder | not implemented |
| Adaptive quantisation | `segments_enabled: false`, and `todo!()` if set | not implemented |
| Token probability adaptation | "currently just not updating these" | not implemented |
| Trellis / token optimisation | absent | not implemented |

The loop-filter finding is worth generalising: `filter_level` was signalled at
the maximum while the encoder never ran the filter, so it predicted from
unfiltered pixels while the decoder predicted from filtered ones. A sweep across
0/8/16/32/63 degrades monotonically - the signature of encoder/decoder drift
rather than a quality trade. Fixing it properly (filter the reconstruction, then
derive the level from the quantiser) should beat 0, since the filter exists to
help prediction.

B_PRED is likely the largest remaining item: libwebp leans on it heavily for
detailed blocks, and this encoder cannot emit it at all.

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

Accepted regressions, as finally measured:

- **A JPEG request costs ~1.6-1.9x end to end** - about 3x on encode and ~2.4x on decode.
  This is the headline cost and the one to weigh against the dependency win.
- **WebP output is ~1.9-2.2x larger**, which has inverted its usefulness: it is now bigger
  than JPEG rather than smaller.
- **Progressive JPEG output is 1.3-2.0x larger**, the one place mozjpeg's trellis actually
  applied.
- **WebP decode is 2.9x slower**, the worst decode regression.

Explicitly *not* regressions, contrary to an earlier revision of this ADR:

- **Default JPEG output size is at parity** (0.995x-1.000x). The earlier 1.16x-1.41x figure
  measured the wrong mozjpeg configuration - see the size section.
- **AVIF output is only 4-16% larger**, and remains 21-43% smaller than JPEG.
- **AVIF encode is faster at low and mid quality** at matched perceptual quality.
- **AVIF decode is only 1.28x slower** - with rav1d's assembly disabled against dav1d with
  NEON enabled.

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

- **The x86_64 cost of building rav1d without assembly.** Partially answered: on aarch64,
  asm-free rav1d decodes only 1.28x slower than dav1d with NEON, which is much better than
  assumed. It remains unmeasured on x86_64, where SIMD gaps are usually wider, and the only
  machine available is aarch64 (Apple's assembler also rejects rav1d's own `-march` flags,
  so the asm-on side cannot be built locally either). Still worth confirming on x86_64
  hardware, but no longer a likely catastrophe.
- The platform allocator versus mimalloc under real concurrent load.

Open work, in rough order of value:

1. **Re-tune `.auto` negotiation.** AVIF is 21-43% smaller than JPEG and WebP is now
   *larger* than JPEG, so the current preference order is actively costing bytes on every
   WebP delivery. This is the highest-value item and needs no codec work.
2. Close the WebP gap: B_PRED 4x4 mode search, then trellis quantisation. Until then WebP
   earns its place only for clients that accept it but not AVIF.
3. Confirm rav1d-without-asm on x86_64.
4. JPEG decode is 2.4x slower and `jpeg-decoder` is in maintenance mode upstream. The
   faster `zune-jpeg` has no scaled-decode API at all, which is why it was not chosen — but
   a hybrid (zune for full-size decodes, `jpeg-decoder` only when DCT scaling applies) is
   worth measuring.
5. Check in a fixed pre-encoded corpus so `decode/avif` and `decode/webp` become
   comparable across encoder changes.
6. Retire both forks when upstream releases lossy WebP encoding and a rav1d-asm feature
   flag.
