#!/usr/bin/env bash
# Samples the real VmRSS of a running container's PID 1 at a fixed cadence,
# until killed (the caller backgrounds this and sends SIGTERM when the load
# phase ends).
#
# Neither the glibc (distroless/cc-debian12) nor the musl (scratch) image
# has a shell or `cat` inside it, so `docker exec` cannot read
# /proc/1/status from inside the target container itself. Instead, a
# throwaway busybox container is started sharing the target's PID
# namespace (`--pid=container:<name>`), which makes the target's PID 1
# visible as /proc/1 inside the sidecar too - giving real VmRSS without
# needing anything installed in the target image. `docker stats` was
# considered and rejected for this: on cgroup v2 its MemUsage is
# memory.current, which includes page cache (e.g. from the source-image
# HTTP fetch), not just the process's own resident set - a real risk of
# overstating one variant vs the other if their page-cache behavior
# differs for reasons unrelated to the allocator.
#
# Usage: sample-rss.sh <container_name> <output_csv> [interval_seconds]
set -euo pipefail

CONTAINER="$1"
OUT_CSV="$2"
INTERVAL="${3:-3}"

echo "epoch_s,rss_kb" > "$OUT_CSV"

while true; do
  NOW=$(date +%s)
  RSS=$(docker run --rm --pid="container:${CONTAINER}" busybox:1.36 \
    sh -c "cat /proc/1/status 2>/dev/null | grep VmRSS | awk '{print \$2}'" 2>/dev/null || true)

  if [ -z "${RSS}" ]; then
    # Container gone (stopped) or PID 1 already exited - stop sampling.
    break
  fi

  echo "${NOW},${RSS}" >> "$OUT_CSV"
  sleep "$INTERVAL"
done
