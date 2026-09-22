#!/usr/bin/env python3
"""Load driver for #144's allocator measurement (glibc vs static musl).

Hits a running `emgr` instance's imgproxy-compatible signed-path endpoint
(`/unsigned/{processing_options}/plain/{source_url}.{ext}` -
`ALLOW_UNSIGNED_REQUESTS=true` on the target lets the literal `unsigned`
signature segment through, so no HMAC signing is needed here) with a fixed
number of concurrent workers for a fixed wall-clock duration.

Every request is engineered to be a cache miss (`services/cache/handler.rs`'s
`generate_key` hashes the source `url` string itself along with
width/height/quality/format/etc - see `CacheService::generate_key`): each
request's source URL carries a unique `?v={nonce}` query parameter, which is
part of the hashed `url` field even though the fixture server ignores it when
resolving which file to serve. Width/height/quality/format are also
randomized per request, both to add a second, independent uniqueness source
and to produce a realistic mix of decode/resize/encode costs rather than
hammering one code path.

The embedded source URL's `?`/`=` are percent-encoded (`%3F`/`%3D`) because
this whole string is itself one path segment of the OUTER request to the
service under test - a literal `?` there would be parsed as the start of the
outer request's own query string by the HTTP client, truncating the path
before the service ever saw it. `services::url::source::parse_plain_source`
percent-decodes the segment after axum has already extracted it, so the
service sees the real `?v=...` on the URL it fetches.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import random
import statistics
import sys
import time
from dataclasses import dataclass, field

import aiohttp

FIXTURES = [
    "blue-marble_1280x720.jpg",
    "blue-marble_640x360.jpg",
    "earthrise_1280x720.jpg",
    "earthrise_640x360.jpg",
]

# Weighted so the run finishes in a bounded time - avif encode is
# meaningfully slower than jpg/webp/png in this codebase (see
# ~/dev/articles' "measure, don't assume, on codecs" lesson: don't assume the
# ratio, just don't let the slowest format dominate wall-clock by giving it
# equal share of an already-long run) while still exercising it.
FORMAT_WEIGHTS = [("jpg", 40), ("webp", 35), ("png", 15), ("avif", 10)]
FORMATS = [f for f, w in FORMAT_WEIGHTS for _ in range(w)]


@dataclass
class Outcome:
    latencies_ms: list[float] = field(default_factory=list)
    status_counts: dict[str, int] = field(default_factory=dict)
    error_counts: dict[str, int] = field(default_factory=dict)
    total: int = 0


def build_path(source_base: str) -> tuple[str, dict]:
    width = random.randint(64, 1600)
    height = random.randint(64, 1600)
    quality = random.randint(40, 95)
    fmt = random.choice(FORMATS)
    img = random.choice(FIXTURES)
    nonce = random.getrandbits(48)

    source = f"{source_base}/{img}%3Fv%3D{nonce}.{fmt}"
    path = f"/unsigned/rs:fill:{width}:{height}/q:{quality}/plain/{source}"
    meta = {"width": width, "height": height, "quality": quality, "format": fmt, "image": img}
    return path, meta


async def worker(
    name: int,
    session: aiohttp.ClientSession,
    base_url: str,
    source_base: str,
    end_time: float,
    outcome: Outcome,
    format_mix: dict,
    timeout_s: float,
) -> None:
    while time.monotonic() < end_time:
        path, meta = build_path(source_base)
        format_mix[meta["format"]] = format_mix.get(meta["format"], 0) + 1
        url = base_url + path
        start = time.monotonic()
        try:
            async with session.get(url, timeout=aiohttp.ClientTimeout(total=timeout_s)) as resp:
                await resp.read()
                elapsed_ms = (time.monotonic() - start) * 1000
                outcome.latencies_ms.append(elapsed_ms)
                key = str(resp.status)
                outcome.status_counts[key] = outcome.status_counts.get(key, 0) + 1
        except Exception as exc:  # noqa: BLE001 - report every failure kind, not just one
            elapsed_ms = (time.monotonic() - start) * 1000
            outcome.latencies_ms.append(elapsed_ms)
            key = type(exc).__name__
            outcome.error_counts[key] = outcome.error_counts.get(key, 0) + 1
        outcome.total += 1


def percentile(sorted_vals: list[float], pct: float) -> float:
    if not sorted_vals:
        return 0.0
    idx = min(len(sorted_vals) - 1, int(len(sorted_vals) * pct))
    return sorted_vals[idx]


async def main_async(args: argparse.Namespace) -> dict:
    connector = aiohttp.TCPConnector(limit=0)
    async with aiohttp.ClientSession(connector=connector) as session:
        outcome = Outcome()
        format_mix: dict = {}
        end_time = time.monotonic() + args.duration
        start_wall = time.time()
        tasks = [
            asyncio.create_task(
                worker(i, session, args.base_url, args.source_base, end_time, outcome, format_mix, args.timeout)
            )
            for i in range(args.concurrency)
        ]
        await asyncio.gather(*tasks)
        end_wall = time.time()

    lat_sorted = sorted(outcome.latencies_ms)
    errors_total = sum(outcome.error_counts.values())
    non_2xx = sum(v for k, v in outcome.status_counts.items() if not k.startswith("2"))
    wall_s = end_wall - start_wall

    summary = {
        "variant": args.variant,
        "base_url": args.base_url,
        "concurrency": args.concurrency,
        "duration_s_requested": args.duration,
        "duration_s_actual": round(wall_s, 3),
        "total_requests": outcome.total,
        "throughput_rps": round(outcome.total / wall_s, 2) if wall_s > 0 else 0,
        "status_counts": outcome.status_counts,
        "error_counts": outcome.error_counts,
        "errors_total": errors_total,
        "non_2xx_total": non_2xx,
        "latency_ms": {
            "mean": round(statistics.fmean(lat_sorted), 2) if lat_sorted else 0,
            "p50": round(percentile(lat_sorted, 0.50), 2),
            "p90": round(percentile(lat_sorted, 0.90), 2),
            "p99": round(percentile(lat_sorted, 0.99), 2),
            "max": round(lat_sorted[-1], 2) if lat_sorted else 0,
        },
        "request_mix": {
            "format_counts": format_mix,
            "width_range": [64, 1600],
            "height_range": [64, 1600],
            "quality_range": [40, 95],
            "source_images": FIXTURES,
            "cache_miss_guarantee": "unique ?v={nonce} query param per request, plus randomized w/h/q/format",
        },
    }
    return summary


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", required=True, help="e.g. http://allocator-bench-glibc:3000")
    parser.add_argument("--source-base", required=True, help="e.g. http://allocator-bench-source:8080")
    parser.add_argument("--concurrency", type=int, default=32)
    parser.add_argument("--duration", type=float, default=180.0, help="seconds")
    parser.add_argument("--timeout", type=float, default=30.0, help="per-request timeout, seconds")
    parser.add_argument("--variant", default="unknown")
    parser.add_argument("--seed", type=int, default=None)
    parser.add_argument("--out", default=None, help="path to also write the JSON summary")
    args = parser.parse_args()

    if args.seed is not None:
        random.seed(args.seed)

    summary = asyncio.run(main_async(args))
    text = json.dumps(summary, indent=2)
    print(text)
    if args.out:
        with open(args.out, "w") as fh:
            fh.write(text)


if __name__ == "__main__":
    main()
