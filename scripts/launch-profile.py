#!/usr/bin/env python3
"""Where Quill's main thread spends a launch, between startup trace marks.

    scripts/launch-profile.py [--runs N] [--cold] [--depth D] [--from DIR] LABEL [WINDOW ...]

Launches the bundle that scripts/launch-bench.py made for LABEL
(~/Library/Caches/quill-launch-bench/LABEL/Quill.app) N times under a
system-wide Time Profiler recording (`xctrace record --all-processes`, so the
launch still goes through LaunchServices), then prints, for each gap between
two QUILL_STARTUP_TRACE marks, how many ms of main-thread samples fell in it
and which functions they were in, summed over the runs.

A sample is charged to its innermost frame in the quill executable (the code
Quill owns that led to it) and to the library its leaf frame is in. A gap with
few samples for its length was spent waiting: Time Profiler only samples
running threads. WINDOW limits the output to gaps that start at those marks.
`--depth` prints that many frames from the leaf instead, for the call path.

Traces stay in the session's temporary directory: xctrace writing to a
removable volume asks for consent. Quill is quit through NSRunningApplication
as launch-bench.py quits it. Credentials come from QUILL_ENV_FILE as there.
"""
import argparse
import collections
import datetime
import importlib.util
import json
import pathlib
import re
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ElementTree

sys.dont_write_bytecode = True
HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("launch_bench", HERE / "launch-bench.py")
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


def record(app, out, cold, creds):
    """One launch under xctrace; returns the startup marks (ms after `open`, and unix s)."""
    recorder = subprocess.Popen(
        ["xcrun", "xctrace", "record", "--template", "Time Profiler", "--all-processes",
         "--time-limit", "9s", "--output", str(out)],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    for line in recorder.stdout:
        if "Ctrl-C" in line:
            break
    time.sleep(1.0)
    trace = bench.WORK / "trace-profile.txt"
    marks = bench.launch(app, trace, creds, cold, onscreen=True)
    recorder.communicate()
    unix = {}
    for line in trace.read_text().splitlines():
        name, _, micros = line.partition(" ")
        if micros.strip().isdigit():
            unix.setdefault(name, int(micros) / 1e6)
    return marks, unix


def trace_start(path):
    toc = subprocess.run(["xcrun", "xctrace", "export", "--input", str(path), "--toc"],
                         check=True, capture_output=True, text=True).stdout
    start = ElementTree.fromstring(toc).find("run/info/summary/start-date").text
    return datetime.datetime.fromisoformat(start).timestamp()


def main_thread_samples(path):
    """(trace-relative s, [frames leaf first as (binary, name)]) for quill's main thread."""
    xml = pathlib.Path(tempfile.mkdtemp()) / "samples.xml"
    with open(xml, "w") as sink:
        subprocess.run(["xcrun", "xctrace", "export", "--input", str(path), "--xpath",
                        '/trace-toc/run[@number="1"]/data/table[@schema="time-profile"]'],
                       check=True, stdout=sink)
    seen = {}

    def resolve(element):
        if element is None:
            return None
        ref = element.get("ref")
        return seen.get(ref) if ref else element

    samples = []
    for _, row in ElementTree.iterparse(xml):
        if row.get("id"):
            seen[row.get("id")] = row
        if row.tag != "row":
            continue
        thread = resolve(row.find("thread"))
        process = resolve(row.find("process"))
        if thread is None or process is None or not (process.get("fmt") or "").startswith("quill "):
            continue
        if not (thread.get("fmt") or "").startswith("Main Thread "):
            continue
        stamp = int(resolve(row.find("sample-time")).text) / 1e9
        stack = resolve(row.find("tagged-backtrace"))
        if stack is None:
            stack = resolve(row.find("backtrace"))
        if stack is not None and stack.find("backtrace") is not None:
            stack = resolve(stack.find("backtrace"))
        frames = []
        for frame in (stack if stack is not None else []):
            frame = resolve(frame)
            if frame is None or frame.tag != "frame":
                continue
            binary = frame.find("binary")
            binary = resolve(binary) if binary is not None else None
            frames.append((binary.get("name") if binary is not None else "?", frame.get("name")))
        samples.append((stamp, frames))
    xml.unlink()
    return samples


def owner(frames):
    """The innermost quill frame and the two frames it called, outermost first."""
    for index, (binary, name) in enumerate(frames):
        if binary == "quill":
            callees = [callee for _, callee in reversed(frames[max(index - 2, 0):index])]
            return " ▸ ".join([name, *callees])
    return " ▸ ".join(name for _, name in reversed(frames[-3:])) or "(no frames)"


def demangle(names):
    """Rust and C++ names as rustfilt and c++filt print them, by input name."""
    names = sorted(set(names))
    text = "\n".join(names)
    for tool in (["rustfilt"], ["c++filt"]):
        try:
            text = subprocess.run(tool, input=text, capture_output=True, text=True, check=True).stdout
        except (OSError, subprocess.CalledProcessError):
            pass
    plain = text.splitlines()
    return dict(zip(names, plain)) if len(plain) == len(names) else {}


def short(name, width=150):
    return name if len(name) <= width else name[: width - 1] + "…"


def clean(name):
    """A demangled name without its crate hashes and generic noise."""
    name = re.sub(r"::h[0-9a-f]{16}$", "", name)
    name = re.sub(r"\[[0-9a-f]{16}\]", "", name)
    return name.replace("<impl ", "<")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--runs", type=int, default=4)
    parser.add_argument("--cold", action="store_true")
    parser.add_argument("--depth", type=int, default=0)
    parser.add_argument("--top", type=int, default=8)
    parser.add_argument("--from", dest="source", metavar="DIR",
                        help="report on the runs a previous call recorded in DIR")
    parser.add_argument("label")
    parser.add_argument("windows", nargs="*")
    opts = parser.parse_args()
    app = bench.WORK / opts.label / "Quill.app"
    creds = bench.credentials()
    work = pathlib.Path(opts.source) if opts.source else pathlib.Path(
        tempfile.mkdtemp(prefix="quill-launch-profile-"))
    print(f"traces in {work}", flush=True)
    gaps = collections.defaultdict(lambda: {"ms": [], "owners": collections.Counter(),
                                            "libs": collections.Counter(), "paths": collections.Counter()})
    order = {}
    for index in range(opts.runs):
        out = work / f"run{index}.trace"
        saved = work / f"run{index}.json"
        if opts.source:
            marks, unix = json.loads(saved.read_text())
        else:
            marks, unix = record(app, out, opts.cold, creds)
            saved.write_text(json.dumps([marks, unix]))
        start = trace_start(out)
        names = sorted(unix, key=unix.get)
        for position, name in enumerate(names):
            order.setdefault(name, position)
        counted = collections.Counter()
        for stamp, frames in main_thread_samples(out):
            at = start + stamp
            earlier = [name for name in names if unix[name] <= at]
            if not earlier:
                continue
            gap = earlier[-1]
            counted[gap] += 1
            entry = gaps[gap]
            entry["owners"][owner(frames)] += 1
            if frames:
                entry["libs"][frames[0][0]] += 1
            if opts.depth:
                entry["paths"][" < ".join(n for _, n in frames[: opts.depth])] += 1
        for position, name in enumerate(names):
            following = names[position + 1] if position + 1 < len(names) else None
            length = (unix[following] - unix[name]) * 1000 if following else 0.0
            gaps[name]["ms"].append((length, counted[name]))
        print(f"run {index}: chat_list {marks.get('chat_list', 0):.0f} ms, "
              f"onscreen {marks.get('onscreen', 0):.0f} ms", flush=True)
        if not opts.source:
            time.sleep(2)
    runs = opts.runs
    symbols = set()
    for entry in gaps.values():
        for key in (*entry["owners"], *entry["paths"]):
            symbols.update(part for part in key.replace(" < ", " ▸ ").split(" ▸ "))
    plain = demangle(symbols)

    def readable(key, separator):
        return separator.join(short(clean(plain.get(part, part)), 70)
                              for part in key.split(separator))

    for name in sorted(gaps, key=lambda n: order.get(n, 99)):
        if opts.windows and name not in opts.windows:
            continue
        entry = gaps[name]
        length = sum(ms for ms, _ in entry["ms"]) / max(len(entry["ms"]), 1)
        busy = sum(count for _, count in entry["ms"]) / runs
        print(f"\n{name}: {length:.0f} ms long, {busy:.0f} ms of main-thread samples (mean of {runs})")
        for function, count in entry["owners"].most_common(opts.top):
            print(f"  {count / runs:6.1f}  {readable(function, ' ▸ ')}")
        libs = ", ".join(f"{lib} {count / runs:.0f}" for lib, count in entry["libs"].most_common(5))
        print(f"    leaf libraries: {libs}")
        for path, count in entry["paths"].most_common(opts.top):
            print(f"  {count / runs:6.1f}  {readable(path, ' < ')}")


if __name__ == "__main__":
    main()
