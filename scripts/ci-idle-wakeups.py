#!/usr/bin/env python3
"""Idle wakeups of a running process on Linux, from /proc (no perf needed).

Samples every thread's context-switch counters and the process CPU time, waits,
samples again and prints the deltas. A thread that blocks (epoll, futex, sleep)
and is woken again counts one voluntary context switch, so the main thread's
voluntary switches over an idle window are its wakeups.

Usage: ci-idle-wakeups.py <pid> [--seconds N] [--max-main-per-second R]

With --max-main-per-second, exits 1 when the main thread woke more often than R
times a second (docs/decisions/sweep-idle-heartbeat.md). Std-only; run with -I.
"""

import argparse
import os
import sys
import time


def read_threads(pid):
    """{tid: (name, voluntary, nonvoluntary)} for every thread of pid."""
    threads = {}
    task_dir = f"/proc/{pid}/task"
    try:
        tids = os.listdir(task_dir)
    except OSError:
        return threads
    for tid in tids:
        name = "?"
        voluntary = nonvoluntary = None
        try:
            with open(f"{task_dir}/{tid}/status", encoding="utf-8", errors="replace") as f:
                for line in f:
                    key, _, value = line.partition(":")
                    value = value.strip()
                    if key == "Name":
                        name = value
                    elif key == "voluntary_ctxt_switches":
                        voluntary = int(value)
                    elif key == "nonvoluntary_ctxt_switches":
                        nonvoluntary = int(value)
        except (OSError, ValueError):
            continue
        if voluntary is not None and nonvoluntary is not None:
            threads[int(tid)] = (name, voluntary, nonvoluntary)
    return threads


def read_cpu_ticks(pid):
    """utime + stime of the whole process, in clock ticks, or None."""
    try:
        with open(f"/proc/{pid}/stat", encoding="utf-8", errors="replace") as f:
            stat = f.read()
    except OSError:
        return None
    # The command name may contain spaces and parentheses; fields follow the last ')'.
    fields = stat[stat.rfind(")") + 2 :].split()
    try:
        # Fields 14 and 15 of stat(5); the list starts at field 3 (state).
        return int(fields[11]) + int(fields[12])
    except (IndexError, ValueError):
        return None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("pid", type=int)
    parser.add_argument("--seconds", type=float, default=10.0)
    parser.add_argument("--max-main-per-second", type=float, default=None)
    args = parser.parse_args()

    before = read_threads(args.pid)
    cpu_before = read_cpu_ticks(args.pid)
    if args.pid not in before or cpu_before is None:
        print(f"idle wakeups: process {args.pid} not found", file=sys.stderr)
        return 1
    start = time.monotonic()
    time.sleep(args.seconds)
    elapsed = time.monotonic() - start
    after = read_threads(args.pid)
    cpu_after = read_cpu_ticks(args.pid)
    if args.pid not in after or cpu_after is None:
        print(f"idle wakeups: process {args.pid} exited during the window", file=sys.stderr)
        return 1

    rows = []
    total = 0
    for tid, (name, voluntary, nonvoluntary) in after.items():
        if tid not in before:
            continue
        _, voluntary0, nonvoluntary0 = before[tid]
        woke = voluntary - voluntary0
        preempted = nonvoluntary - nonvoluntary0
        total += woke
        rows.append((woke, preempted, tid, name))
    rows.sort(reverse=True)
    main_woke = next((woke for woke, _, tid, _ in rows if tid == args.pid), 0)

    ticks = os.sysconf("SC_CLK_TCK")
    cpu_percent = 100.0 * (cpu_after - cpu_before) / ticks / elapsed
    main_rate = main_woke / elapsed
    print(
        f"idle wakeups over {elapsed:.1f} s: main thread {main_woke} ({main_rate:.1f}/s), "
        f"all threads {total} ({total / elapsed:.1f}/s), cpu {cpu_percent:.2f}%"
    )
    for woke, preempted, tid, name in rows:
        if woke or preempted:
            main = " (main)" if tid == args.pid else ""
            print(f"  {name:<16} tid {tid:<8} woke {woke:<6} preempted {preempted}{main}")

    if args.max_main_per_second is not None and main_rate > args.max_main_per_second:
        print(
            f"FAIL idle wakeups: main thread woke {main_rate:.1f}/s, "
            f"limit {args.max_main_per_second:g}/s"
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
