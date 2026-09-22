#!/usr/bin/env bash
# Orchestrates #144's allocator measurement end to end: builds the glibc and
# musl images, starts the plain-HTTP fixture server, then runs each variant
# ONE AT A TIME (never concurrently - concurrent runs corrupted benchmark
# numbers by 1.6-1.9x earlier in this project), sampling RSS throughout and
# driving load with a cache-miss-guaranteed request mix. Results land under
# results/<timestamp>/.
#
# Usage: ./scripts/run-bench.sh [--duration SECONDS] [--concurrency N] [--runs N]
#
# Requires: docker, on a host with a real Linux daemon (not run through
# QEMU emulation - see the README for why that matters for this specific
# measurement).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
NET="allocator-bench-net"
SOURCE_NAME="allocator-bench-source"
DRIVER_IMAGE="allocator-bench-driver"

DURATION=300
CONCURRENCY=32
RUNS=2
CPUS="2"
MEMORY="768m"

while [ $# -gt 0 ]; do
  case "$1" in
    --duration) DURATION="$2"; shift 2 ;;
    --concurrency) CONCURRENCY="$2"; shift 2 ;;
    --runs) RUNS="$2"; shift 2 ;;
    --cpus) CPUS="$2"; shift 2 ;;
    --memory) MEMORY="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
done

TS="$(date +%Y%m%d-%H%M%S)"
RESULTS_DIR="$HERE/results/$TS"
mkdir -p "$RESULTS_DIR"

log() { echo "[run-bench] $*" >&2; }

cleanup() {
  log "cleanup: stopping any leftover containers"
  for c in allocator-bench-glibc allocator-bench-musl allocator-bench-glibc-arena2 "$SOURCE_NAME"; do
    docker rm -f "$c" >/dev/null 2>&1 || true
  done
}
trap cleanup EXIT

log "building images (context: $REPO_ROOT)"
if ! docker build -f "$HERE/docker/Dockerfile.glibc" -t allocator-bench-glibc "$REPO_ROOT" \
    > "$RESULTS_DIR/build-glibc.log" 2>&1; then
  log "glibc build FAILED - see $RESULTS_DIR/build-glibc.log"
  tail -n 60 "$RESULTS_DIR/build-glibc.log" >&2
  exit 1
fi
log "glibc image built"

if ! docker build -f "$HERE/docker/Dockerfile.musl" -t allocator-bench-musl "$REPO_ROOT" \
    > "$RESULTS_DIR/build-musl.log" 2>&1; then
  log "musl build FAILED - this is itself a #144 finding. See $RESULTS_DIR/build-musl.log"
  tail -n 60 "$RESULTS_DIR/build-musl.log" >&2
  echo "MUSL_BUILD_FAILED" > "$RESULTS_DIR/MUSL_BUILD_FAILED"
  exit 1
fi
log "musl image built"

docker build -f "$HERE/docker/Dockerfile.source-server" -t allocator-bench-source "$REPO_ROOT" \
  > "$RESULTS_DIR/build-source.log" 2>&1
docker build -t "$DRIVER_IMAGE" "$HERE/driver" > "$RESULTS_DIR/build-driver.log" 2>&1
log "source-server and driver images built"

log "extracting binaries for size/linkage inspection"
# distroless/cc-debian12 and scratch both lack a shell, so extract the
# binaries the same way for both: create (never start) a container from the
# image and `docker cp` out of it.
docker create --name extract-glibc allocator-bench-glibc >/dev/null
docker cp extract-glibc:/app/emgr "$RESULTS_DIR/emgr-glibc" >/dev/null
docker rm -f extract-glibc >/dev/null

docker create --name extract-musl allocator-bench-musl >/dev/null
docker cp extract-musl:/app/emgr "$RESULTS_DIR/emgr-musl" >/dev/null
docker rm -f extract-musl >/dev/null

{
  echo "=== glibc emgr size ==="
  ls -la "$RESULTS_DIR/emgr-glibc"
  echo
  echo "=== musl emgr size ==="
  ls -la "$RESULTS_DIR/emgr-musl"
  echo
  echo "(host is macOS - no ldd here; see static-linkage-check.txt for the"
  echo " authoritative in-container ldd/file check that verify-static.sh runs next)"
} > "$RESULTS_DIR/binary-info.txt"
cat "$RESULTS_DIR/binary-info.txt"

log "static-linkage confirmation (in-container, authoritative - macOS has no ldd)"
bash "$HERE/scripts/verify-static.sh" "$RESULTS_DIR"

log "network + source server"
docker network create "$NET" >/dev/null 2>&1 || true
docker rm -f "$SOURCE_NAME" >/dev/null 2>&1 || true
docker run -d --name "$SOURCE_NAME" --network "$NET" allocator-bench-source >/dev/null
sleep 1

SOURCE_BASE="http://${SOURCE_NAME}:8080"

wait_healthy() {
  local cname="$1"
  for _ in $(seq 1 30); do
    if docker run --rm --network "$NET" "$DRIVER_IMAGE" -c "
import urllib.request
urllib.request.urlopen('http://${cname}:3000/health', timeout=2).read()
" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

run_one_variant() {
  local variant="$1" image="$2" run_idx="$3" extra_env="$4"
  local cname="allocator-bench-${variant}"
  local tag="${variant}-run${run_idx}"

  log "=== starting $tag (cpus=$CPUS memory=$MEMORY extra_env=[$extra_env]) ==="
  docker rm -f "$cname" >/dev/null 2>&1 || true

  # CDN_BASE_URL points the 301 redirect the resize endpoint issues (#53)
  # back at this same container's own local_fs download route
  # (`GET /api/images/files/{key}`) rather than its default
  # `http://localhost:9000/image-cache` - unreachable from the driver
  # container, which has its own separate loopback. Confirmed by hand: with
  # the default, `fetch_one.py`'s follow-redirect fails with
  # ConnectionRefusedError instead of ever reading the resized bytes.
  #
  # RATE_LIMIT_BURST/PERIOD_MS: the load driver's own default burst=20/
  # period=100ms (`modules/router/middlewares.rs`) rate-limits per source
  # IP, and every driver request comes from the one driver container's one
  # IP - confirmed by hand this throttles the driver down to a couple
  # hundred req/s of mostly `429`s before ever reaching the decode path.
  # Raised far past anything this bench's concurrency can hit, so the
  # governor layer is a no-op here - this measurement is about the
  # allocator, not the rate limiter.
  #
  # MAX_CONCURRENT_PROCESSING/DOWNLOADS: default `max_concurrent_processing`
  # is `effective_cpu_count()` (`config/performance.rs`), i.e. 2 at this
  # bench's `--cpus=2` - confirmed by hand this sheds most requests as
  # `503` well before $CONCURRENCY is reached, which would measure the
  # semaphore's rejection path instead of sustained decode/resize/encode
  # load. Raised so $CONCURRENCY concurrent requests genuinely queue and
  # execute rather than being shed - matching the issue's own framing
  # ("many threads, allocation-heavy is exactly the case" for the
  # allocator question), and a real, if aggressive, oversubscription ratio
  # a production deployment might also choose for a bursty workload.
  # shellcheck disable=SC2086
  docker run -d --name "$cname" --network "$NET" \
    --cpus="$CPUS" --memory="$MEMORY" \
    -e "ALLOWED_SOURCES=${SOURCE_BASE}/" \
    -e "ALLOW_UNSIGNED_REQUESTS=true" \
    -e "CDN_BASE_URL=http://${cname}:3000/api/images/files" \
    -e "RATE_LIMIT_BURST=1000000" \
    -e "RATE_LIMIT_PERIOD_MS=1" \
    -e "MAX_CONCURRENT_PROCESSING=${CONCURRENCY}" \
    -e "MAX_CONCURRENT_DOWNLOADS=$((CONCURRENCY * 2))" \
    $extra_env \
    "$image" >/dev/null

  log "$tag: waiting for health"
  if ! wait_healthy "$cname"; then
    log "$tag: FAILED to become healthy"
    docker logs "$cname" > "$RESULTS_DIR/${tag}-startup-failure.log" 2>&1 || true
    docker rm -f "$cname" >/dev/null 2>&1 || true
    return 1
  fi

  # Output-identity check: one fixed, non-cache-busted request, sha256'd.
  docker run --rm --network "$NET" -v "$RESULTS_DIR:/out" "$DRIVER_IMAGE" fetch_one.py \
    --base-url "http://${cname}:3000" --source-base "$SOURCE_BASE" \
    --save "/out/${tag}-sample-output.bin" > "$RESULTS_DIR/${tag}-sample-output.txt" 2>&1 || true

  local rss_csv="$RESULTS_DIR/${tag}-rss.csv"
  bash "$HERE/scripts/sample-rss.sh" "$cname" "$rss_csv" 3 &
  local sampler_pid=$!

  log "$tag: load phase ($DURATION s, concurrency=$CONCURRENCY)"
  docker run --rm --network "$NET" -v "$RESULTS_DIR:/out" "$DRIVER_IMAGE" load_driver.py \
    --base-url "http://${cname}:3000" --source-base "$SOURCE_BASE" \
    --concurrency "$CONCURRENCY" --duration "$DURATION" --variant "$tag" \
    --out "/out/${tag}-summary.json" | tee "$RESULTS_DIR/${tag}-summary-stdout.json"

  kill "$sampler_pid" 2>/dev/null || true
  wait "$sampler_pid" 2>/dev/null || true

  python3 "$HERE/scripts/summarize_rss.py" "$rss_csv" > "$RESULTS_DIR/${tag}-rss-summary.txt" || true
  cat "$RESULTS_DIR/${tag}-rss-summary.txt"

  docker rm -f "$cname" >/dev/null 2>&1 || true
  log "=== finished $tag ==="
}

# Alternate glibc/musl across repeats so any host-state drift over the
# session isn't systematically attributed to one variant.
for i in $(seq 1 "$RUNS"); do
  run_one_variant glibc allocator-bench-glibc "$i" ""
  run_one_variant musl allocator-bench-musl "$i" ""
done

# Cheap zero-dependency lever named in #144: one extra glibc run with
# MALLOC_ARENA_MAX=2.
run_one_variant glibc-arena2 allocator-bench-glibc 1 "-e MALLOC_ARENA_MAX=2"

log "cross-variant output-identity check (glibc-run1 vs musl-run1)"
{
  echo "=== output identity: glibc-run1 vs musl-run1 ==="
  G_SHA=$(grep '^sha256=' "$RESULTS_DIR/glibc-run1-sample-output.txt" 2>/dev/null | cut -d= -f2)
  M_SHA=$(grep '^sha256=' "$RESULTS_DIR/musl-run1-sample-output.txt" 2>/dev/null | cut -d= -f2)
  echo "glibc-run1 sha256: ${G_SHA:-<missing>}"
  echo "musl-run1  sha256: ${M_SHA:-<missing>}"
  if [ -n "$G_SHA" ] && [ "$G_SHA" = "$M_SHA" ]; then
    echo "RESULT: IDENTICAL - same request produced byte-identical output on both variants."
  else
    echo "RESULT: MISMATCH or missing data - do not trust the RSS/throughput comparison until this is understood."
  fi
} | tee "$RESULTS_DIR/output-identity-check.txt"

log "all runs complete. Results in $RESULTS_DIR"
