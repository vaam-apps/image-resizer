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
| `encode/jpeg_baseline/photo` | 0.92 ms | 2.83 ms | **3.09x slower** |
| `encode/jpeg_444/photo` | 1.28 ms | 2.69 ms | **2.09x slower** |
| `encode/jpeg_progressive/photo` | 24.76 ms | 2.84 ms | **0.11x** |
| `encode/jpeg_444_progressive/photo` | 33.57 ms | 2.77 ms | **0.08x** |
| `decode/jpeg/photo_1920x1080` | 4.71 ms | 10.90 ms | **2.32x slower** |
| `decode/jpeg/photo_640x360` | 0.60 ms | 1.48 ms | **2.45x slower** |
| `encode/avif/photo` | 62.69 ms | 93.61 ms | **1.49x slower** |
| `encode/webp/photo` | 20.30 ms | 107.40 ms | **5.29x slower** |
| `encode/png_best/photo` (control) | 64.53 ms | 65.67 ms | 1.02x |
| `encode/png_default/photo` (control) | 1.36 ms | 1.26 ms | 0.93x |

### End-to-end pipeline (the number that actually matters)

Micro-benchmarks isolate one codec call; this is the whole request path — decode, resize,
encode.

| Pipeline | C build | Pure Rust | Ratio |
|---|---|---|---|
| `photo_like_thumbnail_jpg` | 6.06 ms | 8.32 ms | **1.37x slower** |
| `photo_4k_large_downscale_thumbnail_jpg` | 19.14 ms | 28.93 ms | **1.51x slower** |
| `photo_real_thumbnail_jpg` | 3.64 ms | 5.55 ms | **1.53x slower** |
| `photo_real_large_large_downscale_thumbnail_jpg` | 3.39 ms | 5.71 ms | **1.69x slower** |
| `photo_with_exif_strip_metadata_default` | 6.07 ms | 8.40 ms | **1.38x slower** |
| `alpha_resize_webp` | 5.30 ms | 19.80 ms | **3.73x slower** |
| `flat_resize_png` (control) | 8.59 ms | 8.19 ms | 0.95x |

**A JPEG request costs roughly 1.4–1.7x what it did; a WebP one costs 3.7x.** The PNG
control at 0.95x confirms these are real codec effects rather than environment drift.
Given that rivalling imgproxy on cold-cache latency is this project's stated goal, this is
the regression to weigh against the dependency win — it is not a rounding error.

An earlier revision of this table reported 1.6–1.9x for the JPEG pipelines. That run had
the baseline and the after measurement executing **concurrently**, so both contended for
CPU; re-running each alone gives the figures above. The tell was that the JPEG ratios moved
while nothing in the JPEG path had changed. Benchmark runs here must not overlap.

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
| **WebP** | 1.208x | 1.174x | 1.200x |
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
| **Pure Rust** - WebP | **0.798x** | **0.951x** | 1.072x |
| **Pure Rust** - JPEG progressive | 1.382x | 1.278x | 1.177x |
| C stack - AVIF | 0.496x | 0.656x | 0.781x |
| C stack - WebP | 0.656x | 0.805x | 0.884x |
| C stack - JPEG progressive | 0.663x | 0.759x | 0.874x |

Two conclusions, both actionable:

1. **AVIF still wins decisively** - 21-43% smaller than JPEG, barely changed from
   the C stack.
2. **WebP is worth serving again, but read the number with the caveat below.**
   After the encoder work it is 20% smaller than JPEG at low quality, 5% smaller
   in the middle and 7% larger at high quality. **However**, DSSIM flatters it:
   see "DSSIM is not a neutral referee" below. Adjusting for that, the low-quality
   advantage is nearer ~15% than 20%. Still worth serving to a client that takes
   WebP but not AVIF, below the top quality band.
3. **Progressive JPEG is no longer the smaller option.** It used to be 13-34%
   smaller than baseline JPEG; it is now 18-38% larger, because mozjpeg's
   trellis was doing that work. Its remaining argument is progressive *rendering*,
   not size.

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
| JPEG | 0.95-1.08 ms | 2.79-3.04 ms | **~2.9x slower** |
| JPEG progressive | 16.05-26.72 ms | 2.82-3.05 ms | **0.11-0.18x** |
| WebP | 20.47-25.84 ms | 112.52-128.25 ms | **5.0x-5.5x slower** |
| AVIF | 66.53-100.11 ms | 59.76-124.61 ms | **0.81x-1.23x** |
| PNG (control) | 78.33 ms | 76.06 ms | 1.00x |

Two of these must be read with the size table, not quoted alone:

- **Progressive JPEG is not 6x faster in any useful sense.** mozjpeg ran
  `JCP_MAX_COMPRESSION` with trellis; `jpeg-encoder` does far less work and ships
  a file up to 2x larger for it.
- **AVIF encode is *faster* at low and mid quality** (0.81-0.83x), slower only at
  the strictest target (1.23x). This contradicts the fixed-quality bench above
  (1.49x slower) and both are correct: at a fixed nominal q80 ravif does more work
  than AOM at q80, while at matched perceptual quality it needs a lower setting
  and comes out ahead. The matched-quality figure describes production.
- **WebP encode is now 5x**, up from ~1.5x before the encoder work. That is the
  price of the B_PRED search plus two extra whole-frame passes (skip-probability
  and token statistics), bought with ~46% fewer bytes. For a service that caches
  results it is a good trade; for a cache-miss-heavy deployment it is not.

### Benchmarks that are NOT comparable, and must not be read as results

`decode/avif/*` and `decode/webp/*` in `benches/decode.rs` appear to show large
speedups (AVIF as much as 0.20x). **Ignore them.** They decode fixtures produced
by the encoder under test, and both encoders changed, so the two runs decode
different bitstreams - that measures content, not decoder speed. The like-for-like
table above is the real answer. Checking a fixed pre-encoded corpus into the repo
would fix the benches themselves.

## DSSIM is not a neutral referee

Every size figure here comes from bisecting each encoder's quality scale to a
**DSSIM** target. That is only sound if DSSIM ranks the formats the way another
perceptual metric would. It does not.

Re-scoring the DSSIM-matched files with **SSIMULACRA2** (higher is better;
90 = visually lossless, 70 = good, 50 = fair), median over 24 images:

| DSSIM target | AVIF | JPEG | WebP |
|---|---|---|---|
| 0.0150 | **26.83** | 26.43 | 24.63 (**-2.20**) |
| 0.0080 | **48.43** | 47.09 | 46.25 (**-2.18**) |
| 0.0035 | 66.30 | 66.33 | **66.49** |

Files DSSIM calls equal quality are not equal: WebP lands ~2.2 points below AVIF
at the two looser targets. So **WebP's size numbers are optimistic** - at genuinely
matched perceptual quality it would need more bytes than the table above shows.
Calibrating against this corpus's own rate/quality curve (roughly 1 SSIMULACRA2
point per 3% of bytes around this range), the 0.798x figure is nearer 0.85x. That
is an estimate from a local slope, not a measurement.

Two things this does *not* undermine:

- **AVIF's lead**, which holds on both metrics at every level and is the
  load-bearing conclusion for negotiation.
- **The WebP encoder improvements**, which compare the same encoder against
  itself and are therefore unaffected by cross-format metric bias. SSIMULACRA2
  independently confirms them: adaptive quantisation improved it by +1.1 to +1.8
  points at fixed quality *while also shrinking files*.

The gap was worse before that work (-4.82 at the loosest target) and has closed
as the encoder improved, which is consistent with it being an artefact of a weak
encoder rather than of the format.

`examples/codec_report.rs` therefore carries a `Metric` switch rather than
hardcoding DSSIM. Anyone re-running these numbers should report both.

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

## Rebuilding the WebP encoder

The encoder was, as found, missing four of the five things libwebp does. Each gap
was visible in the source rather than inferred, and each was fixed and measured
separately:

| Gap | Evidence in the source | Outcome |
|---|---|---|
| Loop filter never applied | `loop_filter::` appears only in the decoder | **fixed** - level was 63, now 0: mean -4.0% |
| B_PRED (4x4 intra) | `LumaMode::B => unreachable!()` | **fixed** - the largest single win, ~-30% |
| Token probability adaptation | "currently just not updating these" | **fixed** - -2.9% / -5.5% / -4.7% |
| Adaptive quantisation | `segments_enabled: false`, `todo!()` if set | **fixed** - smaller *and* better quality |
| Trellis / token optimisation | absent | **implemented and reverted** - see below |

Measured identically at each step, median size against libwebp on Kodak:

| Step | <= 0.0150 | <= 0.0080 | <= 0.0035 |
|---|---|---|---|
| As found | 2.148x | 2.185x | 2.287x |
| + RD 16x16 mode decision | 1.893x | 1.924x | 2.191x |
| + loop-filter fix | 1.857x | 1.924x | 2.191x |
| + B_PRED | 1.306x | 1.285x | 1.363x |
| + token probability adaptation | 1.256x | 1.244x | 1.307x |
| **+ adaptive quantisation** | **1.208x** | **1.174x** | **1.200x** |

**Roughly 46% fewer bytes than the encoder started with**, and enough to move WebP
from larger-than-JPEG at every level back to 0.798x / 0.951x / 1.072x of it.

Three findings worth keeping:

- **The loop-filter bug generalises.** `filter_level` was signalled at the maximum
  while the encoder never ran the filter, so it predicted from unfiltered pixels
  while the decoder predicted from filtered ones. A sweep across 0/8/16/32/63
  degrades monotonically, which is the signature of encoder/decoder drift rather
  than a quality trade. Setting it to 0 is still a stopgap: filtering the
  reconstruction and deriving the level from the quantiser should beat 0, because
  the filter exists to help prediction.
- **Adaptive quantisation improved both axes at once** - smaller files *and*
  higher SSIMULACRA2, at no encode-time cost. That is unusual enough to note; the
  classifier is one variance pass plus a sort, negligible beside the RD search.
  It also surfaced a latent bug: `write_optional_signed_value` wrote its sign flag
  backwards relative to the spec and to this crate's own decoder. It was dead code
  because every caller passed `None`, and segmentation is the first that does not.
- **Trellis was implemented and reverted.** On kodim01 at q55 it saved 8 bytes
  (0.03%), scored slightly worse on both DSSIM and SSIMULACRA2, and cost 4.06x the
  encode time. The decisive measurement is that the cost persists with its lambda
  multiplier at zero, where the search is a no-op and reproduces the previous
  output byte for byte - so the 4x is the DP machinery itself and no tuning makes
  it cheaper. VP8's coefficient tokens are cheap and its blocks are 4x4, so there
  is little rate to recover per block; this encoder's remaining loss is in
  prediction and quantiser choice, not coefficient coding.

**The cost is encode time: WebP went from ~1.5x libwebp to 5.0x-5.5x** at matched
quality, from the B_PRED search plus two extra whole-frame passes (skip
probability and token statistics). For a service that caches results this is a
good trade - encode is paid once per image, size on every delivery - but it is a
real regression for a cache-miss-heavy deployment, and it is what moves the
`alpha_resize_webp` pipeline to 3.73x.

What remains: the proper loop-filter fix above, and reducing the three-pass
structure (skip-probability dry run, token-statistics dry run, real pass) which
recomputes mode decision and reconstruction each time with no caching between.
That is where the 5x lives, and it is an engineering problem rather than a codec
one.

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

- **A JPEG request costs ~1.4-1.7x end to end** - about 2.9x on encode and 2.4x on decode.
  This is the headline cost and the one to weigh against the dependency win.
- **A WebP request costs 3.7x end to end.** WebP encode is 5.0-5.5x at matched quality,
  bought deliberately: the same work cut output size ~46%. Encode is paid once per cached
  image, bytes on every delivery, so this is a good trade for a caching service and a poor
  one without a cache.
- **WebP output is 1.17-1.21x larger** than libwebp's, down from 1.88-2.25x before the
  encoder work.
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
