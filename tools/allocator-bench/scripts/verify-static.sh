#!/usr/bin/env bash
# Authoritative static-linkage check for the musl `emgr` binary (and, as a
# contrast case, the glibc one), run INSIDE a Linux container - macOS has no
# `ldd` at all (it's a glibc/Linux tool), so this cannot run on the host
# directly even though the binaries were `docker cp`'d out to the host
# filesystem by run-bench.sh.
#
# Usage: verify-static.sh <results_dir>  (expects <results_dir>/emgr-glibc
# and <results_dir>/emgr-musl to already exist, as run-bench.sh produces).
set -euo pipefail

RESULTS_DIR="$1"

docker run --rm -v "$RESULTS_DIR:/bin-out:ro" debian:bookworm-slim bash -c '
set -e
echo "=== glibc emgr: ldd ==="
ldd /bin-out/emgr-glibc || true
echo
echo "=== glibc emgr: file ==="
apt-get -qq update >/dev/null 2>&1 && apt-get -qq install -y file >/dev/null 2>&1 || true
file /bin-out/emgr-glibc || true
echo
echo "=== musl emgr: ldd (expect \"not a dynamic executable\" for a genuinely static binary) ==="
ldd /bin-out/emgr-musl || true
echo
echo "=== musl emgr: file ==="
file /bin-out/emgr-musl || true
' | tee "$RESULTS_DIR/static-linkage-check.txt"

echo "[verify-static] wrote $RESULTS_DIR/static-linkage-check.txt"
