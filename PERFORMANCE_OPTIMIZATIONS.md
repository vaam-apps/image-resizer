# Performance notes for the image resize pipeline

## Overview

This document describes the actual performance-relevant mechanisms in the
download -> process -> store pipeline: what bounds concurrency, what copies
memory (and what doesn't), and what the build profiles actually do. It used
to also carry a set of throughput/memory/CPU multipliers ("3-5x throughput",
"50% memory reduction", ...) that didn't trace to any measurement in this
repo, plus a `[profile.release]` block that never existed in `Cargo.toml`.
Both were removed (#31): a number nobody can reproduce is worse than no
number at all in a project whose pitch is speed. What's below is limited to
mechanisms you can read in the source and numbers you can reproduce with the
commands given.

The same discipline applies to #134, which replaced every C/C++ image codec
(`mozjpeg`, the `webp` crate/libwebp, `libavif`/AOM/dav1d) and the global
allocator (`mimalloc`) with pure-Rust equivalents. Every number in this
document that was measured against one of the old C implementations is
marked inline as `**TODO(re-measure)**` rather than carried forward or
guessed at; re-measurement is tracked separately from this documentation
pass.

## HTTP client configuration

- **Connection pooling**: `reqwest::Client` reuses connections, with a
  configurable pool size per host (`connection_pool_size`, default 50).
- **HTTP/2**: configurable (`enable_http2`); reqwest negotiates it via ALPN
  when the origin supports it. Note: `PerformanceConfig::default()` and the
  `high_throughput`/`low_latency` presets all set this `true`, but the
  plain (no `PERFORMANCE_PROFILE`) construction path actually used at
  startup - `PerformanceConfig::from(&EnvConfig)`'s fallback branch,
  `src/config/performance.rs` - defaults an unset `ENABLE_HTTP2` to
  `false`, not `true`. A deployment that never sets `ENABLE_HTTP2` and
  never sets `PERFORMANCE_PROFILE` therefore runs with HTTP/2 off, despite
  every other default in this codebase agreeing on `true`. See
  `docs/configuration/performance.md` for the same discrepancy documented
  against the full settings table.
- **Keep-alive / timeouts**: both configurable (`keep_alive_timeout`,
  `http_timeout`), applied per request via a client pinned to the
  already-validated `(host, addr)` pair (`ImageService::build_pinned_client`,
  `src/services/image/handler.rs`) - see the SSRF guard notes there for why
  the client is rebuilt per validated address rather than reused across
  hosts.

## Concurrency bounds

Three independent bounds shed load with a distinguishable error rather than
queue unboundedly, at three different layers of the request:

- **Router-level total concurrency**: `saturation_and_timeout_middleware`
  (`src/modules/router/middlewares.rs`, #43) wraps every route in a
  fixed-size `tokio::sync::Semaphore` (`MAX_CONCURRENT_REQUESTS`, default
  512) plus a `tokio::time::timeout` (`REQUEST_TIMEOUT_SECS`, default 30s).
  A non-blocking `try_acquire_owned()` sheds with `503` the instant the
  service is at capacity, standing in for
  `tower::limit::ConcurrencyLimitLayer` + `tower_http::timeout::TimeoutLayer`
  (neither `tower`'s `limit`/`load-shed` nor tower-http's `timeout` feature
  is enabled in this workspace's `Cargo.toml`). A per-IP token-bucket rate
  limiter (`tower_governor`, `RATE_LIMIT_BURST`/`RATE_LIMIT_PERIOD_MS`,
  default 20 burst / 100ms refill) sits in front of it. These four
  variables are read directly from the process environment in
  `middlewares.rs`, not via `EnvConfig`/`envconfig` like the rest of this
  document's variables - see `docs/configuration/performance.md`.
- **Downloads**: `download_semaphore`, sized from `max_concurrent_downloads`
  (default 20), acquired with a blocking `.acquire()` in `download_image` -
  a download that can't get a permit right away waits for one, since a
  bounded queue of in-flight HTTP requests is cheap.
- **CPU processing**: `processing_semaphore`, sized from
  `max_concurrent_processing` (default: CPU count), acquired with a
  *non-blocking* `try_acquire_owned()` in `process_image` (#30). When all
  permits are taken, the call fails immediately with an error containing
  `"permit"` - `AppError::classify_resize_error`
  (`src/modules/utils/err.rs`) maps that to `503 Service Unavailable`. This
  used to be dead configuration: `max_concurrent_processing` was threaded
  through three presets, environment-overridden, and unit-tested, but its
  only use anywhere in the crate was a `debug!` log line. Nothing gated
  concurrent calls to `process_image`, so the real limiting factor was
  whatever ran the CPU stage.

Separately from all three, **single-flight request coalescing**
(`src/services/resize/handler.rs`, #37) means concurrent callers asking for
the *same cache key* never multiply load in the first place: once a cache
miss is confirmed, the first caller becomes the leader (registered in a
`dashmap`-backed in-flight map) and does the real download/process/upload
work; every other concurrent caller for that key subscribes to a
`tokio::sync::broadcast` channel and receives the leader's result instead of
running its own. An `InFlightGuard` with a `Drop` impl guarantees followers
are unblocked (with a synthetic failure) even if the leader panics or its
future is cancelled mid-flight, so a dead leader can never leave a follower
waiting forever.

### Why `tokio::task::spawn_blocking`, not a custom rayon pool

The CPU stage used to run on a hand-rolled `rayon::ThreadPool`
(`cpu_pool.spawn(closure)`). Rayon's entire value proposition is
work-stealing parallelism *within* a job - `par_iter`, `rayon::join`,
`rayon::scope`, `par_chunks` - and none of those are used anywhere in this
crate (verified by grep). `process_image_blocking_with_limits` is a strictly
sequential decode -> resize -> encode for one image; nothing here ever fans
a job out across a pool. The rayon pool was, in effect, a blocking-task pool
with two problems `spawn_blocking` doesn't have: an unbounded queue in front
of it, and a worker count fixed at process startup instead of scaling with
the runtime. `process_image` now moves the decode/resize/encode work onto
`tokio::task::spawn_blocking`, gated by the `processing_semaphore` above, and
still off the async runtime's own worker threads.

`rayon` is no longer a dependency at all - it has been removed from
`Cargo.toml` entirely (`grep -c '^rayon' Cargo.toml` returns 0). It still
appears, transitively, in `Cargo.lock` (pulled in by other dependencies
unrelated to this crate's own code), but nothing in this crate's
`Cargo.toml` or `src/` references it.

### Cancellation on caller disconnect

The blocking closure holds both the semaphore permit and a
`tokio::sync::oneshot::Sender` for its whole duration. Before doing any
decode/resize/encode work it checks whether the paired `Receiver` has
already been dropped (`tx.is_closed()`) - which happens when the caller's
own future was cancelled, e.g. the client disconnected and axum dropped the
in-flight response future. A task that was still queued on the blocking pool
when that happened skips the CPU work entirely instead of paying full
decode/resize/encode cost for a response nobody will read. A task already
mid-decode when the disconnect happens still runs to completion - there is
no preemption point inside a synchronous decode/resize/encode call - so this
bounds *queued*, not in-flight, waste.

## Memory handling

- **Downloads are read in chunks with the size cap enforced per chunk**, not
  after the whole body is buffered (#22): `download_image` streams the
  response body via `bytes_stream()` and aborts the moment the running total
  exceeds `max_image_size`, so a chunked-encoded (no `Content-Length`) or
  dishonest origin can't force an unbounded buffer. This is *not* end-to-end
  streaming decode, though - the validated chunks are still accumulated into
  one contiguous in-memory buffer before decoding starts, since the `image`
  crate decodes from a complete buffer, not incrementally from a byte
  stream.
- **No double copy between download and processing** (#31): that
  accumulation buffer is a `bytes::BytesMut`, and `download_image` returns
  it via `.freeze()` - a type conversion into `bytes::Bytes`, not a copy.
  `process_image` takes that `Bytes` and clones the handle (an atomic
  refcount bump) to move it into the blocking task, instead of the previous
  `Bytes::copy_from_slice(&vec)` that re-copied the entire image body just
  to change its container type.
- **Pre-allocated output buffers**: `estimate_output_size` sizes the output
  `Vec<u8>` up front from the format and pixel count, using `checked_mul` on
  `usize` rather than plain `u32` multiplication (#26/#36) - `u32`
  arithmetic wraps silently on overflow in release builds, `checked_mul`
  instead falls back to `usize::MAX` (an allocation that will fail loudly)
  for a request that would otherwise wrap into a tiny, wrong buffer size.
- **Explicit decode limits** (`image::Limits`, #26): width/height/alloc
  bounds derived from `max_src_resolution_mp`, set explicitly instead of
  inheriting the `image` crate's default 512MiB `max_alloc`, as defense in
  depth behind the header-only resolution check that runs before a full
  decode is attempted.

## Image processing

- **Format detection** from magic bytes, avoiding a second guess-the-format
  pass when the bytes are already in hand.
- **Resize** goes through [`fast_image_resize`](https://docs.rs/fast_image_resize)
  (SIMD), not the `image` crate's own resize kernel (#63 stage 1). Filter
  selection is unchanged in name - `Triangle` for small (<=300px per side)
  targets, `Lanczos3` otherwise, a real, measured lever, not yet
  configurable per request (#35) - but is now mapped onto
  `fast_image_resize`'s own filter set (`Triangle` -> `fir::FilterType::Bilinear`,
  `Lanczos3` -> `fir::FilterType::Lanczos3` by name) instead of the `image`
  crate's kernel. Measured ~5x faster on a downscale for the same filter
  (`resize_fir/downscale lanczos3`: 3.43ms vs. the old `image`-crate
  kernel's ~17ms in `.bench-baseline/BASELINE.md`), with imperceptible
  quality difference (DSSIM 0.0000047-0.0000093 against the old kernel's
  output, same source).
- **JPEG decode** goes through [`jpeg-decoder`](https://docs.rs/jpeg-decoder)
  instead of `mozjpeg`/libjpeg-turbo (#134; #63 stage 2/#67 had put mozjpeg
  there in the first place, replacing the `image` crate's `zune-jpeg`).
  `jpeg-decoder` was picked over `image`'s own `zune-jpeg` for one specific
  reason: `Decoder::scale()`. `zune-jpeg` is faster but exposes no
  scaled-decode API at all, so decoding through it here would silently drop
  the optimization below. Two paths:
  - **DCT-scaled decode**: when the requested output is a >=2x downscale,
    `select_jpeg_dct_scale` (`src/services/image/handler.rs`) picks the most
    aggressive DCT scale `jpeg-decoder`'s `Decoder::scale()` exposes (1/2,
    1/4, or 1/8) whose decoded output is still >= the resize target, and
    decodes directly at that resolution instead of decoding full-size and
    discarding most of the data during resize. This is the same mechanism
    mozjpeg provided; the win it measured (2.21x faster for a 4K source to a
    small thumbnail, 26.21ms vs 58.03ms for full decode + resize) was
    specific to mozjpeg's DCT-scaled decode. Re-measured against
    `jpeg-decoder`'s `scale()` on a 2200x1100 photo: full-size 8.99 ms,
    1/2 5.54 ms, 1/4 4.26 ms, 1/8 4.06 ms. So the win survives the swap but
    flattens out - most of it is realised by 1/4, and 1/8 buys almost
    nothing more, because the fixed per-image costs stop being the dominant
    term. `scale()` also rounds up the same way libjpeg does (1100 at 1/8
    reports 138, not 137), which is what the scaled-dimension test pins.
  - **Full-size decode**, otherwise. Under mozjpeg this path (extended by
    #67 to cover *every* JPEG, not just the DCT-scaled case, after `image`
    0.25.10's `zune-jpeg` 0.5.x made its Huffman bit-refill EOF check
    fallible where 0.4.x's was an infallible `bool` - a real per-byte cost)
    measured ~1.5x faster than `zune-jpeg` across a 36-photo real corpus
    regardless of downscale ratio. Whether `jpeg-decoder`'s full-size decode
    beats `zune-jpeg` by a similar margin is unmeasured:
    **TODO(re-measure)**.
  - Either path falls back to the `image`-crate (`zune-jpeg`) decoder on any
    `jpeg-decoder` failure (including a caught panic) rather than failing
    the request.
- **JPEG encode** goes through [`jpeg-encoder`](https://docs.rs/jpeg-encoder),
  a pure-Rust encoder with its own AVX2 path, instead of `mozjpeg::Compress`
  (#134; #76 had put mozjpeg there in the first place, replacing
  `image::codecs::jpeg::JpegEncoder`, whose encoder has no progressive-mode
  switch and hardcodes 4:2:2 chroma subsampling). `jpeg-encoder` carries the
  APIs the mozjpeg path relied on - `set_progressive`,
  `set_quantization_tables`, and `add_app_segment` for the raw APP1/APP2
  markers EXIF and ICC are written through - so the `jpgo:` option surface
  is unchanged. What it does *not* implement is trellis quantisation, so
  output was *expected* to be measurably larger than mozjpeg's at matched
  quality. Measured across all 24 Kodak photographs at matched DSSIM, it is
  **at parity: 0.995x / 1.000x / 1.000x**.

  The expectation was wrong for a specific and easily-repeated reason: it
  compared against `mozjpeg::Compress` at its own defaults, which enable
  trellis, while this service's non-progressive path set `JCP_FASTEST`,
  under which `trellis_quant = (compress_profile == JCP_MAX_COMPRESSION)` is
  false. There was no trellis on the default path to lose. The trellis cost
  is real but lands only on the opt-in progressive path, where output is
  1.32x-2.01x larger. See `adr/0006-pure-rust-codecs.md`.

  mozjpeg's two-profile split no longer applies - `jpeg-encoder` is one
  encoder, not a fastest/max-compression pair. Concretely, progressive and
  baseline now cost about the same (2.84 ms and 2.83 ms on the `photo`
  fixture) where mozjpeg's progressive profile cost 24.76 ms against its
  baseline 0.92 ms. That is not a free 9x: the progressive path is doing far
  less work and ships a 1.32x-2.01x larger file for it. The old numbers
  below are void rather than dated (was:
  mozjpeg's `JCP_FASTEST` default profile, used for non-progressive output,
  measured ~5% smaller and 3-4x faster than the pre-#76 `image`-crate
  encoder, with a better mean DSSIM than mozjpeg's own max-compression
  profile at the same nominal quality; `JCP_MAX_COMPRESSION`, mozjpeg's own
  default profile and used only for explicitly-requested progressive output
  `jpgo:1:...`, measured ~12% smaller than the pre-#76 encoder but 3-8x its
  encode time - a ~19x-baseline cost by the time this document's earlier
  revisions summarized it). One piece of that history is worth keeping for
  what it warns future encoder swaps about: constructing a
  `mozjpeg::Compress` without calling `set_fastest_defaults` selected the
  max-compression profile by default, which very nearly shipped as the
  default for *every* JPEG encode under mozjpeg (a +16% pipeline
  regression) - see `.bench-baseline/BASELINE.md`'s "JPEG encoder cutover"
  section for the full trap. `jpeg-encoder` has no such implicit-default
  footgun since there is only one profile, but the general lesson (check an
  encoder's *actual* default construction path, not just its name) still
  applies to any future encoder swap.
- **WebP** static-image encode and decode go through
  [`vaam-image-webp`](https://github.com/vaam-apps/vaam-image-webp) (#134)
  instead of the [`webp`](https://docs.rs/webp) crate (real libwebp via
  FFI), which itself had replaced the `image` crate's lossless-only WebP
  encoder (#32, #60) and the `image-webp` decoder (#66). `vaam-image-webp`
  is this org's fork of `image-rs/image-webp`, tracking its unreleased
  lossy VP8 encoder branch (upstream image-webp#161/#164/#172) rather than
  a released version, because no published crate encodes lossy WebP in pure
  Rust as of this writing - `image-webp`'s current release is lossless
  only, and lossless WebP runs roughly 5-12x larger than lossy on
  photographic content, which would make the format useless for this
  service's main job. The fork is pinned by `rev`, not `branch`, so an
  unreleased encoder moving under this project is exactly the failure the
  pin prevents; it is temporary by construction and should be dropped in
  favor of the crates.io release once upstream cuts one. Whether
  `vaam-image-webp` matches real libwebp's encode/decode speed and output
  size - the numbers this section used to carry (encode via the `webp`
  crate; decode measured 2.24x faster median on the Kodak corpus, 24 real
  photos, against `image-webp`, DSSIM delta 0.00000000 against the old
  decoder) - is unmeasured against this new implementation:
  **TODO(re-measure)**. See `adr/0001-image-engine.md` and
  `adr/0003-webp-measurement.md` for how the *previous* encode-side
  comparison was run; that methodology, not its numbers, is what a
  re-measurement should reuse.
- **AVIF** encode and decode (#67/#68 first added the pair) go through
  pure-Rust crates instead of `libavif` (AOM + dav1d, vendored C, built via
  `cmake`) (#134): [`ravif`](https://docs.rs/ravif) (wrapping `rav1e`) for
  encode, [`avif-decode`](https://docs.rs/avif-decode) (wrapping `rav1d`,
  the Rust port of `dav1d`) for decode. `avif-decode` was preferred over
  depending on `rav1d` directly because `rav1d`'s public surface is still
  dav1d's C-shaped API, where `avif-decode` offers a plain `from_avif` ->
  `to_image` Rust interface. This is, on the encode side, a return to a
  pure-Rust encoder after `adr/0004-avif-measurement.md` and
  `adr/0005-avif-measurement-libavif-mozjpeg.md` record the earlier
  evaluation that moved encode *from* `ravif`/`rav1e` *to* `libavif`/AOM -
  those ADRs are the historical record of that call and are not revised
  here. #134's motivation is dependency composition (removing every C/C++
  dependency from the build), not a re-run of the perceptual-quality
  comparison those ADRs made, so whether the AOM-vs-`rav1e`
  quality/speed tradeoff they measured still holds against the `rav1e`
  release in use today is unmeasured: **TODO(re-measure)**. See the
  `ravif`/`avif-decode` dependency comments in `Cargo.toml` for the current
  codec/dependency choice.
- **Upscale guard, off by default** (#36): a request naming output
  dimensions larger than the source image is capped to the source's
  dimensions per axis unless the request opts in via `enlarge: true`
  (`ResizeQuery::enlarge`, `src/models/params.rs`), mirroring imgproxy's
  `enlarge` option. Upscaling is measurably expensive - the committed
  benchmark baseline (`.bench-baseline/BASELINE.md`) puts
  `resize/upscale/lanczos3` at 143ms vs 17.4ms for the equivalent downscale
  on the old `image`-crate kernel (~8x; the `fast_image_resize` upscale path
  is faster in absolute terms but shows the same multiplier) - so leaving it
  unguarded let a single request against a tiny source name an arbitrarily
  expensive output size.
- **No redundant crop**: the fixed-size ("fill") resize path used to check
  whether `resize_to_fill`'s output already matched the requested dimensions
  before conditionally cropping. `resize_to_fill` always crops to exactly
  the requested size internally (verified against
  `image-0.25.10/src/images/dynimage.rs:943-962`), so that check was always
  true and the manual crop branch was dead code; it has been removed (#36).

## Build profiles

There is no `[profile.release]` override in `Cargo.toml` - `cargo build
--release` uses Cargo's own stock release profile. Two named, opt-in
profiles exist for cases that want more:

```toml
# Maximum-speed profile - what the Docker image actually builds with
# (see Dockerfile: every `cargo build` invocation there passes
# `--profile perf`).
[profile.perf]
inherits = "release"
lto = "fat"
opt-level = 3
codegen-units = 1
# Deliberately NOT panic = "abort": this service decodes attacker-supplied
# image bytes, and a panic inside a codec would take the whole process down
# with it - a trivial DoS. Unwinding keeps a future catch boundary around
# the decode stage possible. See #29.
panic = "unwind"
strip = true

# Minimum-size profile, used for the healthcheck binary.
[profile.prod]
inherits = "release"
lto = true
opt-level = "z"
codegen-units = 1
strip = true
```

Neither is the default for a plain `cargo build`/`cargo test`/`cargo bench`
- they only apply when `--profile perf`/`--profile prod` is passed
explicitly (as the Dockerfile does for the shipped binaries).

## Runtime configuration

- **Tokio runtime**: no longer built via the `#[tokio::main]` attribute
  macro (that only accepts compile-time literals, so a fixed worker count
  was baked in). `src/main.rs` now builds the runtime by hand with
  `tokio::runtime::Builder::new_multi_thread().worker_threads(n)`, where `n`
  is `TOKIO_WORKER_THREADS` if set, else `effective_cpu_count()` - the same
  cgroup-aware CPU count used to size `max_concurrent_processing`
  (`src/modules/utils/cgroup.rs`, #44), not `num_cpus::get()` directly, so a
  CPU-quota-limited container sizes the runtime for its actual quota rather
  than the host's full core count.
- **Allocator**: the platform `System` allocator (`src/main.rs`), replacing
  `mimalloc`/`libmimalloc-sys` (#134, dropped along with every other C/C++
  dependency). No throughput/latency comparison between the two has been
  run for this workload: **TODO(re-measure)**.

## Benchmarking

Reproduce the pipeline numbers (decode + resize + optional filters +
encode, via `ImageService::process_image_blocking`, the same code path
production traffic runs - see that function's doc comment):

```bash
cargo bench --features local_fs --bench pipeline -- --sample-size 20 --measurement-time 2 --warm-up-time 1
```

Other benches cover the pipeline stages and cache-key hashing in isolation:
`cargo bench --features local_fs --bench decode|resize|encode|cache_key`.

This document does not embed a snapshot of those numbers - they move too
often (six documented, code-verified changes since this document was first
written: the `fast_image_resize` kernel swap, DCT-scaled JPEG decode, wave-2
features, the JPEG encoder cutover, full-size mozjpeg decode, and #134's
swap of every codec - JPEG, WebP, AVIF - plus the allocator to pure-Rust
implementations) for a copy pasted here to stay honest for long.
**`.bench-baseline/BASELINE.md`
is the current, single source of truth for every criterion number**,
including the full per-filter/per-format table, the observation that
upscaling runs ~8x slower than the equivalent downscale (what motivates the
upscale guard above), and a running log of measurement traps this project
has already fallen into once (stale encoder defaults, redirect-blended
end-to-end metrics, DCT-scale threshold assumptions) so the next person
re-running these benches doesn't repeat them.

**End-to-end, against imgproxy**: the criterion numbers above are
single-operation micro-benchmarks inside the Rust binary; they do not cover
routing, source fetch, storage round trips, or the redirect hop. The full
three-way comparison against imgproxy (`bench-imgproxy/`, a k6 harness) is
also tracked in `.bench-baseline/BASELINE.md`, and is summarized honestly in
the [README's Performance section](README.md#performance). That cold-cache
comparison ran the full processing pipeline through the old C codecs, so the
ratio it reported is void after #134: **TODO(re-measure)** (was:
~3.48x slower on p50, ~2.86x less throughput than imgproxy on a cold cache).
emgr is still expected to be measurably faster than imgproxy on a warm
cache regardless of that re-measurement - imgproxy has no result cache of
its own, so that comparison is architectural (cache hit vs. full
reprocessing), not a processing-speed win, and doesn't run through any of
the codecs #134 touched. The cold and warm numbers are the same
architectural trade-off seen from two sides, not two independent facts.

No throughput/memory/CPU-utilization multiplier is claimed anywhere in this
document that doesn't trace to a command and a number in
`.bench-baseline/BASELINE.md` or `bench-imgproxy/README.md`.
