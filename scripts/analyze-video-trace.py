#!/usr/bin/env python3
"""Summarize a QUILL_TRACE_VIDEO=1 stderr log (docs/build.md, "Video diagnostics").

Usage: python3 scripts/analyze-video-trace.py <trace.log> [display_hz]

Per player: pull count and spacing, new pictures shown and their spacing,
a histogram of the gaps between new pictures in display vsyncs (120 Hz by
default), and how many new pictures skipped a source frame ("double
steps": an item-time step over 1.5 times the typical one).
"""

import collections
import statistics
import sys


def quantile(values, fraction):
    values = sorted(values)
    if not values:
        return float("nan")
    return values[min(len(values) - 1, int(fraction * len(values)))]


def main():
    if len(sys.argv) < 2:
        print(__doc__.strip())
        return 2
    tick_ms = 1000.0 / (float(sys.argv[2]) if len(sys.argv) > 2 else 120.0)
    rows = []
    with open(sys.argv[1], errors="replace") as log:
        for line in log:
            if not line.startswith("VT,"):
                continue
            fields = line.strip().split(",")
            try:
                rows.append(
                    (
                        int(fields[1]),
                        int(fields[2]),
                        float(fields[3]),
                        int(fields[4]),
                        float(fields[5]),
                        float(fields[6]),
                    )
                )
            except (IndexError, ValueError):
                continue
    players = collections.defaultdict(list)
    for row in rows:
        players[row[0]].append(row)
    for player, pulls in sorted(players.items()):
        walls = [row[1] / 1000 for row in pulls]
        span = walls[-1] - walls[0]
        gaps = [b - a for a, b in zip(walls, walls[1:])]
        new = [row for row in pulls if row[3] == 1]
        new_gaps = [(b[1] - a[1]) / 1000 for a, b in zip(new, new[1:])]
        steps = [b[4] - a[4] for a, b in zip(new, new[1:])]
        playing = sum(1 for row in pulls if row[5] > 0)
        fps = len(new) / max(1e-9, span / 1000)
        print(
            f"player {player}: {len(pulls)} pulls over {span:.0f} ms, "
            f"{len(new)} new frames ({fps:.1f} fps), rate>0 in {playing} pulls"
        )
        if gaps:
            print(
                f"  pull gap ms: p50={quantile(gaps, .5):.1f} p90={quantile(gaps, .9):.1f} "
                f"p99={quantile(gaps, .99):.1f} max={max(gaps):.1f}"
            )
        if new_gaps:
            print(
                f"  new-frame wall gap ms: p50={quantile(new_gaps, .5):.1f} "
                f"p90={quantile(new_gaps, .9):.1f} p99={quantile(new_gaps, .99):.1f} "
                f"max={max(new_gaps):.1f} sd={statistics.pstdev(new_gaps):.1f}"
            )
            histogram = collections.Counter(round(gap / tick_ms) for gap in new_gaps)
            print("  gap histogram (in vsyncs):", dict(sorted(histogram.items())))
        if steps:
            typical = quantile(steps, 0.5)
            doubles = sum(1 for step in steps if step > 1.5 * typical)
            print(
                f"  item-time step per new frame ms: p50={typical:.1f} "
                f"min={min(steps):.1f} max={max(steps):.1f}; steps>1 source frame: {doubles}"
            )
        if len(new) > 2:
            first, last = new[0], new[-1]
            print(
                f"  wall advance {(last[1] - first[1]) / 1000:.0f} ms vs item advance "
                f"{last[2] - first[2]:.0f} ms (display ts advance {last[4] - first[4]:.0f})"
            )
    return 0


if __name__ == "__main__":
    sys.exit(main())
