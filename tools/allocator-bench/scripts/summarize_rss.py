#!/usr/bin/env python3
"""Summarize a sample-rss.sh CSV into peak and steady-state (late-run
median) RSS.

Steady-state is defined as the median of samples in the last third of the
run's wall-clock span - late enough that startup/warm-up RSS growth has
already happened, so a transient early spike doesn't get averaged into
"steady", and a late fragmentation-driven climb (the exact effect #144
predicts for glibc) still shows up because the window sits at the end of
the run, not a fixed sample count.
"""

from __future__ import annotations

import argparse
import csv


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("csv_path")
    args = parser.parse_args()

    rows = []
    with open(args.csv_path) as fh:
        reader = csv.DictReader(fh)
        for row in reader:
            rows.append((int(row["epoch_s"]), int(row["rss_kb"])))

    if not rows:
        print("no samples")
        return

    rows.sort()
    t0, t1 = rows[0][0], rows[-1][0]
    span = max(t1 - t0, 1)
    late_cutoff = t0 + span * 2 // 3

    all_rss = [r for _, r in rows]
    late_rss = sorted(r for t, r in rows if t >= late_cutoff)

    peak_kb = max(all_rss)
    steady_kb = late_rss[len(late_rss) // 2] if late_rss else all_rss[-1]

    intervals = [b[0] - a[0] for a, b in zip(rows, rows[1:])]
    mean_interval = sum(intervals) / len(intervals) if intervals else 0

    print(f"samples={len(rows)}")
    print(f"span_s={span}")
    print(f"mean_sample_interval_s={mean_interval:.2f}")
    print(f"peak_rss_kb={peak_kb} ({peak_kb / 1024:.1f} MiB)")
    print(f"steady_state_rss_kb={steady_kb} ({steady_kb / 1024:.1f} MiB)")
    print(f"first_rss_kb={all_rss[0]} ({all_rss[0] / 1024:.1f} MiB)")
    print(f"last_rss_kb={all_rss[-1]} ({all_rss[-1] / 1024:.1f} MiB)")


if __name__ == "__main__":
    main()
