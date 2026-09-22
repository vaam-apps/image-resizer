#!/usr/bin/env python3
"""Fetch one deterministic (non-cache-busted) request and print its sha256.

Used to confirm the glibc and musl variants produce byte-identical output
for the same request before trusting any RSS/throughput comparison between
them - different output would mean different work, not just a different
allocator. Run once against each variant with the same arguments and diff
the printed hashes (and, with --save, the raw bytes).
"""

from __future__ import annotations

import argparse
import hashlib
import sys
import urllib.request


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--source-base", required=True)
    parser.add_argument("--image", default="blue-marble_1280x720.jpg")
    parser.add_argument("--width", type=int, default=800)
    parser.add_argument("--height", type=int, default=600)
    parser.add_argument("--quality", type=int, default=80)
    parser.add_argument("--format", default="webp")
    parser.add_argument("--save", default=None, help="path to write the raw response bytes")
    args = parser.parse_args()

    source = f"{args.source_base}/{args.image}"
    path = (
        f"/unsigned/rs:fill:{args.width}:{args.height}/q:{args.quality}"
        f"/plain/{source}.{args.format}"
    )
    url = args.base_url + path

    with urllib.request.urlopen(url, timeout=30) as resp:
        body = resp.read()
        status = resp.status
        content_type = resp.headers.get("Content-Type")

    digest = hashlib.sha256(body).hexdigest()

    if args.save:
        with open(args.save, "wb") as fh:
            fh.write(body)

    print(f"url={url}")
    print(f"status={status}")
    print(f"content_type={content_type}")
    print(f"bytes={len(body)}")
    print(f"sha256={digest}")


if __name__ == "__main__":
    main()
