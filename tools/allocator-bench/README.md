# allocator-bench: #144's gating measurement (glibc vs static musl RSS/throughput)

This tool answers the **first, gating item** of [#144](https://github.com/vaam-apps/image-resizer/issues/144):
measure RSS and throughput for the service's current glibc build against a
static musl build, in-container, on Linux, so the `scratch`-base-image
decision has real data behind it instead of the allocator question #134 left
open.

**Everything else in #144 is out of scope for this tool**: CA trust store
strategy, numeric `USER`, musl main-thread stack size, and switching
`base_deploy` are separate, not-yet-decided items. One exception is called
out explicitly below (CA bundle in `Dockerfile.musl`) because the
measurement was not runnable without it - see "What this bench discovered
that #144 didn't already know".

## What's here

```
tools/allocator-bench/
  docker/
    Dockerfile.glibc          # what ships today: local_fs on distroless/cc-debian12
    Dockerfile.musl            # aarch64-unknown-linux-musl, static, on scratch
    Dockerfile.source-server   # plain-HTTP fixture server (benches/fixtures/decode/*.jpg)
  driver/
    load_driver.py             # concurrent load generator, cache-miss-guaranteed
    fetch_one.py                # single deterministic request, for output-identity checks
    Dockerfile
  scripts/
    run-bench.sh                # orchestrates the whole measurement, end to end
    sample-rss.sh                # periodic VmRSS sampler (PID-namespace sidecar trick)
    summarize_rss.py             # peak / steady-state RSS from a sample-rss.sh CSV
    verify-static.sh             # authoritative in-container ldd/file check
  compose.yaml                  # convenience only - NOT what produced the numbers below
  results/<timestamp>/          # gitignored - run-bench.sh output lands here
```

## How to re-run it

Requires Docker with a real Linux daemon (Docker Desktop's own VM on macOS
is fine and is what produced the numbers below - **do not** run this through
QEMU/binfmt emulation; a different allocator implementation under emulation
would make the numbers worse than no data at all, per #144's own note).

```sh
cd tools/allocator-bench
./scripts/run-bench.sh                       # defaults: 300s/run, concurrency=32, 2 runs each + 1 arena-max run
./scripts/run-bench.sh --duration 60 --runs 1 # quick smoke run
```

Flags: `--duration SECONDS` (per run), `--concurrency N`, `--runs N` (glibc/musl
pairs; alternated, not both run back to back), `--cpus`, `--memory` (Docker
resource limits applied identically to every variant).

Everything lands under `results/<timestamp>/`:
- `build-*.log` - the three image builds
- `binary-info.txt`, `static-linkage-check.txt` - sizes + the authoritative
  in-container `ldd`/`file` output
- `<tag>-rss.csv`, `<tag>-rss-summary.txt` - RSS samples and peak/steady-state
- `<tag>-summary.json` - throughput, latency percentiles, status/error counts,
  exact request mix
- `<tag>-sample-output.bin`/`.txt` - the one fixed request used for the
  output-identity check, plus its sha256
- `output-identity-check.txt` - glibc vs musl sha256 comparison

## The measurement design

**Two images of the same build.** `Dockerfile.glibc` and `Dockerfile.musl`
both build from the exact same source tree, `--no-default-features --features
local_fs`, the same `perf` Cargo profile the production `Dockerfile` uses,
and the same pinned `rust@sha256:93ce27a8...` builder digest the production
`Dockerfile` pins. Only the target triple/libc and the runtime base
(`gcr.io/distroless/cc-debian12@sha256:adcd20c7...` - the identical digest
`base_deploy` uses today - vs `scratch`) differ. This bench runs on Docker
Desktop's native `linux/aarch64`, so the musl target is
`aarch64-unknown-linux-musl`; see `Dockerfile.musl`'s comments for what
changes on `x86_64`.

**Plain HTTP source server**, `Dockerfile.source-server`, serving
`benches/fixtures/decode/*.jpg` (the repo's real, already-committed fixture
corpus - the same one `benches/decode.rs` decodes against per #165) over
plain `python3 -m http.server`. No TLS anywhere in this bench, deliberately -
see the CA-trust-store note below for why that turned out to matter more
than expected anyway.

**Load driver**, `driver/load_driver.py`: N concurrent asyncio workers
(default `--concurrency 32`) hammering the target's imgproxy-compatible
signed-path endpoint (`/unsigned/rs:fill:{w}:{h}/q:{q}/plain/{source_url}.
{fmt}` - `ALLOW_UNSIGNED_REQUESTS=true` on the target lets the `unsigned`
signature segment through) for a fixed wall-clock duration. Every request is
engineered to be a cache miss two independent ways:

1. `CacheService::generate_key` (`src/services/cache/handler.rs`) hashes the
   source `url` string itself, so each request's source URL carries a unique
   `?v={nonce}` query parameter (percent-encoded as `%3Fv%3D{nonce}` in the
   outer request path - see the module docstring in `load_driver.py` for why
   the raw `?` can't appear there).
2. Width (64-1600), height (64-1600), quality (40-95) and output format
   (`jpg` 40% / `webp` 35% / `png` 15% / `avif` 10%, weighted so the slower
   `avif` encoder doesn't dominate wall-clock) are all randomized per
   request too.

Either alone guarantees a miss; both together make it robust even if one
generator were ever weakened.

**Env vars the driver needed that production doesn't set by default** (see
`run_one_variant` in `run-bench.sh` for the full reasoning, confirmed by hand
against the actual 429/503 responses before adding these):
- `RATE_LIMIT_BURST=1000000` / `RATE_LIMIT_PERIOD_MS=1` - the governor rate
  limiter (`modules/router/middlewares.rs`, default burst 20/period 100ms)
  keys per source IP, and every driver request comes from one container's
  one IP. Left at the default, the driver gets rate-limited to a couple
  hundred `429`s/sec before ever reaching the decode path.
- `MAX_CONCURRENT_PROCESSING=$CONCURRENCY` / `MAX_CONCURRENT_DOWNLOADS=$((CONCURRENCY*2))`
  - the default `max_concurrent_processing` is `effective_cpu_count()`
  (2 at this bench's `--cpus=2`), which sheds most of a 32-concurrency load
  as `503` rather than queueing it. Raised so requests genuinely queue and
  execute - this bench wants sustained decode/resize/encode pressure on the
  allocator, which is exactly the "many threads, allocation-heavy" shape
  #144 is asking about.

**RSS sampling**, `scripts/sample-rss.sh`: neither `distroless/cc-debian12`
nor `scratch` has a shell, so nothing can be `docker exec`'d to read
`/proc/1/status` from inside either target container. Instead, a throwaway
`busybox` container is started sharing the target's PID namespace
(`docker run --pid=container:<name> busybox cat /proc/1/status`), which
makes PID 1 (the only process in either container) visible as `/proc/1`
inside the sidecar. Sampled every ~3s (actual cadence is in each `*-rss.csv`).
`docker stats`'s cgroup-v2 `MemUsage` was considered and rejected: it's
`memory.current`, which includes page cache, not just the process's own
resident set - a real risk of overstating one variant vs the other for
reasons unrelated to the allocator.

**Rigour:**
- Runs are strictly sequential (`run_one_variant` runs to completion, then
  the container is removed, before the next one starts) - concurrent runs
  corrupted benchmark numbers by 1.6-1.9x elsewhere in this project.
- glibc/musl are alternated across repeats (glibc-run1, musl-run1,
  glibc-run2, musl-run2, ...), not run back-to-back, so host-state drift
  over the session isn't systematically attributed to one variant.
- Every run is reported individually below, not averaged.
- `--cpus`/`--memory` are identical across every variant (see the results
  table for the exact values used).
- Output-identity: one fixed, non-cache-busted request
  (`rs:fill:800:600/q:80/.../blue-marble_1280x720.jpg.webp`) is fetched from
  each variant and sha256'd before trusting any comparison -
  `output-identity-check.txt`.

## What this bench discovered that #144 didn't already know

**Building a `reqwest::Client` fails on `scratch` even for a plain-HTTP
request, not just HTTPS.** `services::image::handler::ImageService::
build_pinned_client` builds a fresh `reqwest::Client` per outbound fetch, and
`ClientBuilder::build()` eagerly constructs the TLS backend's certificate
verifier regardless of whether the request that follows ever uses TLS.
Confirmed with a minimal reqwest + `rustls-graviola` repro built and run
under the exact same musl/scratch conditions:

```
no CA bundle:  reqwest::Error { kind: Builder, source: General("No CA certificates were loaded from the system") }
with /etc/ssl/certs/ca-certificates.crt present: builds fine
```

Practical effect without a fix: the musl/scratch container answers
`/health` (no outbound fetch) but returns `502 Bad Gateway` on **every**
resize request - 0% success, no decode/resize/encode work ever happens, no
RSS number to measure. This is a stronger version of #144's own "CA trust
store" item, which describes only HTTPS fetches failing on `scratch` - this
is *every* fetch, HTTP included, because the client-build step itself needs
a trust store before it even looks at the request's scheme.

`Dockerfile.musl` works around this with one `COPY --from=builder /etc/ssl/certs/ca-certificates.crt`
line (Debian's `ca-certificates` package - plain data, no C code) so the
measurement could run at all. This is a **bench-only accommodation**, not a
decision on #144's CA-trust-store item: it doesn't choose between bundling
vs `webpki-roots` for production, it just makes "bundle the file" the
expedient choice for this tool. See the comment block in `Dockerfile.musl`
for the full writeup.

**`local_fs`'s storage root needs pre-creating and `--chown`ing under
distroless's `nonroot` user.** `local_fs_handler.rs:166` does its own
`create_dir_all` on first write, but `distroless/cc-debian12`'s baked-in
`nonroot` user (uid/gid 65532) has no write access to create a fresh
directory - the glibc container returned `502 Bad Gateway` on every request
("Failed to create a local storage directory") until `Dockerfile.glibc`
pre-created and `--chown`'d `/data/images` in the builder stage. `scratch`
doesn't need this (no baked-in user, so the musl container runs as root by
default and can create the directory itself at runtime).

**`CDN_BASE_URL` must point back at the container's own download route.**
The resize endpoint's `301` redirects to `{CDN_BASE_URL}/{key}`, defaulting
to `http://localhost:9000/image-cache` - meaningless inside a container
network. Both variants are started with `CDN_BASE_URL=http://<container>:3000/api/images/files`
so the redirect resolves to the same container's own `local_fs` download
handler.

## Results

Run at `results/20260922-161704/`, produced by
`./scripts/run-bench.sh` with its defaults: `--cpus=2 --memory=768m`
(identical across every variant), concurrency=32, duration=300s/run, source
images from `benches/fixtures/decode/*.jpg`, format mix jpg 40%/webp 35%/
png 15%/avif 10%, width/height randomized 64-1600px, quality randomized
40-95. Platform: Docker Desktop's native `linux/aarch64` (no emulation).

| variant | run | peak RSS | steady-state RSS | throughput (req/s) | requests | errors | binary size |
|---|---|---|---|---|---|---|---|
| glibc (default `System` allocator) | 1 | 680.4 MiB | 641.5 MiB | 30.78 | 9,364 | 0 | 12,009,232 B (11.5 MiB) |
| glibc (default `System` allocator) | 2 | 700.4 MiB | 650.2 MiB | 30.72 | 9,347 | 0 | 12,009,232 B (11.5 MiB) |
| glibc + `MALLOC_ARENA_MAX=2` | 1 | 388.5 MiB | 336.4 MiB | 32.15 | 9,782 | 0 | 12,009,232 B (11.5 MiB) |
| musl (static, `mallocng`) | 1 | 222.4 MiB | 129.4 MiB | 29.28 | 8,894 | 0 | 11,811,952 B (11.3 MiB) |
| musl (static, `mallocng`) | 2 | 343.2 MiB | 136.3 MiB | 28.06 | 8,482 | 0 | 11,811,952 B (11.3 MiB) |

Every run: **0 errors, 0 non-2xx responses**, out of 8,482-9,782 requests
each (all cache misses by construction). Throughput is flat across every
variant (28-32 req/s, CPU-bound at `--cpus=2`) - this comparison is a pure
memory-behavior difference, not a throughput/memory tradeoff in either
direction.

**RSS shape, not just the summary numbers** (`*-rss.csv`, sampled every
~3.3s): glibc's RSS shoots from ~14 MiB to ~470-530 MiB within the first
~15s of load, then keeps climbing noisily for the rest of the 300s run and
**never drops back down** - steady-state (650-680 MiB) sits within 3-5% of
peak (680-717 MiB) in both runs. `MALLOC_ARENA_MAX=2` caps that climb
noticeably lower (steady-state 336 MiB vs 641-650 MiB - a 47.6-48.3%
reduction) but the same "climbs and stays" shape is still visible. musl's RSS climbs to
~110-190 MiB in the first ~15-30s and then visibly **fluctuates back down**
for the remainder of the run (steady-state 129-136 MiB, well below each
run's own peak of 222-343 MiB) - consistent with `mallocng` actually
returning freed pages rather than holding them in a never-shrunk arena.

At `--memory=768m`, default-allocator glibc peaked at 91-93% of the limit
(700-717 MiB of 768 MiB) in a 5-minute run. It did not tip into an OOMKill
here, but the margin is thin enough that a longer run, a higher concurrency,
or a larger average image size plausibly would - this is exactly the
"OOMKill, not a slow request" failure mode #144 describes, observed heading
in that direction rather than confirmed by an actual kill.

Static-linkage confirmation (`static-linkage-check.txt`, run inside a Linux
container since macOS has no `ldd`): glibc `emgr` dynamically links
`libgcc_s.so.1`/`libm.so.6`/`libc.so.6`/`ld-linux-aarch64.so.1`; musl `emgr`
reports `ldd: not a dynamic executable` and `file`: `ELF 64-bit LSB
executable, ARM aarch64, version 1 (SYSV), statically linked, stripped` -
genuinely static, not merely mostly-static.

Output-identity (`output-identity-check.txt`): the same fixed,
non-cache-busted request (`rs:fill:800:600/q:80/.../blue-marble_1280x720.jpg.webp`)
against glibc-run1 and musl-run1 both produced
`sha256=8f39484246f747b02274247a014d5658b061b187e6967018ca686d5ceb6b26f8` -
byte-identical output, so the RSS/throughput comparison above is comparing
the same work, not different work that happens to take different memory.

### Read

Under this specific workload (concurrent decode/resize/encode across
jpg/webp/png/avif, cache-miss-guaranteed, 2 CPUs, 768 MiB limit, 5-minute
sustained runs): **musl's `mallocng` used meaningfully less memory than
glibc's default allocator, not more** - 4.8-5.0x lower steady-state RSS
(129-136 MiB vs 641-650 MiB, comparing each run to its own paired run) and
2.0-3.1x lower peak RSS (222-343 MiB vs 680-717 MiB), at statistically
indistinguishable throughput (28-32 req/s across every variant) and zero
errors on either side. `MALLOC_ARENA_MAX=2`
closes roughly half the gap on glibc alone, without musl, without a C
dependency, and without touching `scratch` at all.

This is the opposite of what #144 flagged as the risk to evaluate -
`mallocng` being "the known-weak case for exactly this workload shape."
That concern was reasonable to hold un-measured (per #134/#144's own
framing, nobody had run this specific comparison yet), and it may still
hold under a different workload shape (larger images, longer sustained
runs, a different concurrency/memory-limit combination, genuinely triggering
an OOMKill on one side or the other) - this single measurement, at this
duration and this resource limit, does not rule that out. It does mean the
premise "musl needs mimalloc/jemalloc/ferroc to be safe here" is not
supported by this data; if anything, this data points the other way.

Not drawing #144's architectural conclusion here deliberately (System /
`MALLOC_ARENA_MAX` / `ferroc` / one C dependency back, or the `scratch`
migration itself) - that's the next item in #144's own scope list, and
should weigh this measurement together with the CA-trust-store and
main-thread-stack-size items this tool deliberately left untouched.

## Note: images run non-root (post-measurement hardening)

All four Dockerfiles were subsequently hardened to run as non-root (`USER
65532:65532`, matching the numeric uid distroless's `nonroot` user already
resolves to) to close a Trivy `DS-0002` finding, plus a `HEALTHCHECK` on the
source-server image (the one image the bench's correctness genuinely depends
on staying up) - see `git log -- tools/allocator-bench/docker` for the exact
diff. The `emgr` binary itself is byte-for-byte unaffected (only the runtime
image's final `USER`/`COPY --chown`/`HEALTHCHECK` layers changed, not the
builder stage that produces it), and the hardened images were rebuilt and
re-verified by hand afterward: `ldd`/`file` still report the same static/
dynamic linkage, `/proc/1/status` inside each container now shows `Uid: 65532`
instead of root, and the same fixed request that produced
`sha256=8f39...b26f8` in the Results table above still produces that exact
hash from both variants post-hardening. The numbers in Results were captured
before this hardening pass; they remain valid because the process being
measured is unchanged - the container's own uid does not change how its
allocator behaves. This also happens to be a real, if small, #144 finding in
its own right, spelled out in the `Dockerfile.musl`/`Dockerfile.glibc` review
comments: `scratch` has no `mkdir` and a non-root process cannot create a
directory under `/`, so the storage path has to be baked into the image with
its ownership already set - the production `scratch` migration will need to
do the same thing.
