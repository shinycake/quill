#!/usr/bin/env python3
"""Summarize a QUILL_TRACE_VIDEO=1 stderr log (docs/build.md, "Video diagnostics").

Usage: python3 scripts/analyze-video-trace.py <trace.log> [display_hz]

Per player: pull count and spacing, new pictures shown and their spacing,
a histogram of the gaps between new pictures in display vsyncs (120 Hz by
default), and how many new pictures skipped a source frame ("double
steps": an item-time step over 1.5 times the typical one). Then, when the
log has them, the decoded pictures per player and layout (`VF` lines:
bytes the renderer uploads and the CPU conversion time) and the Windows
renderer's uploads per path (`VU` lines: NV12 surface textures or the
image atlas). docs/decisions/video-player-nv12.md has a before/after run.
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
    # (player, format) -> [(bytes, convert_us)], from `VF` lines.
    pictures = collections.defaultdict(list)
    # path -> [(bytes, upload_us)], from `VU` lines.
    uploads = collections.defaultdict(list)
    with open(sys.argv[1], errors="replace") as log:
        for line in log:
            fields = line.strip().split(",")
            if line.startswith("VF,"):
                try:
                    pictures[(int(fields[1]), fields[2])].append(
                        (int(fields[5]), int(fields[6]))
                    )
                except (IndexError, ValueError):
                    pass
                continue
            if line.startswith("VU,"):
                try:
                    uploads[fields[1]].append((int(fields[2]), int(fields[3])))
                except (IndexError, ValueError):
                    pass
                continue
            if not line.startswith("VT,"):
                continue
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
    for (player, layout), shown in sorted(pictures.items()):
        sizes = [size for size, _ in shown]
        convert = [us for _, us in shown]
        print(
            f"player {player} {layout}: {len(shown)} pictures, "
            f"{statistics.mean(sizes) / 1024:.0f} KiB each, convert us: "
            f"p50={quantile(convert, .5)} p90={quantile(convert, .9)} max={max(convert)}"
        )
    for path, done in sorted(uploads.items()):
        sizes = [size for size, _ in done]
        took = [us for _, us in done]
        print(
            f"upload {path}: {len(done)} uploads, {sum(sizes) / 1048576:.1f} MiB, "
            f"us: p50={quantile(took, .5)} p90={quantile(took, .9)} max={max(took)}"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
