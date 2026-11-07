#!/usr/bin/env python3
"""The cold launch floor: how long macOS takes to put a plain AppKit window on screen.

    python3 -I scripts/launch-floor/floorbench.py [--runs N] [--cold]
        [--quill LABEL=BUNDLE ...] [titled] [borderless] [noflush] [linked] [binds] [opnew] [preload] [preload2]

Builds FloorApp.app from floor.m under ~/Library/Caches/quill-launch-bench
(signed with QUILL_CODESIGN_IDENTITY when set) and launches each variant in
turn: `titled` (a standard window), `borderless` (no title bar buttons),
`noflush` (a titled window without the Core Animation flush before
`[NSApp run]`, as a plain AppKit app), `linked` (the titled window in a
binary that also links every system framework the first `--quill` bundle's
executable links, to tell the frameworks' load cost from Quill's own),
`binds` (`linked`, plus a pointer to every symbol that executable binds, so
dyld resolves the same imports at launch), `opnew` (`titled`, plus replacements
of C++ operator new and delete, as ThorVG defines them), `preload` (`titled`,
loading WritingToolsUI on a background thread from `main`, which AppKit
otherwise loads on the main thread during `finishLaunching`), `preload2`
(`preload`, with SwiftUI, which the title bar buttons need, loaded first). `--quill` adds a Quill bundle that
launch-bench.py already made, launched the way launch-bench.py launches it,
so the floor and Quill alternate in one session. Every side gets one
unrecorded launch first. `--cold` runs `sudo -n /usr/sbin/purge` before every
launch. Prints each side's marks as medians of ms after `open`; `onscreen` is
when WindowServer first lists the window (scripts/winwatch.swift).
"""
import argparse
import importlib.util
import os
import pathlib
import statistics
import subprocess
import sys
import time

sys.dont_write_bytecode = True
HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("launch_bench", HERE.parent / "launch-bench.py")
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)
WRITING_TOOLS_UI = "/System/Library/PrivateFrameworks/WritingToolsUI.framework/WritingToolsUI"
SWIFT_UI = "/System/Library/Frameworks/SwiftUI.framework/SwiftUI"
# name: (app name, environment)
VARIANTS = {
    "titled": ("FloorApp", []),
    "borderless": ("FloorApp", ["FLOOR_BORDERLESS=1"]),
    "noflush": ("FloorApp", ["FLOOR_NO_FLUSH=1"]),
    "linked": ("FloorLinked", []),
    "binds": ("FloorBinds", []),
    "opnew": ("FloorOpNew", []),
    "preload": ("FloorApp", ["FLOOR_PRELOAD=" + WRITING_TOOLS_UI]),
    "preload2": ("FloorApp", [f"FLOOR_PRELOAD={SWIFT_UI}:{WRITING_TOOLS_UI}"]),
}
BASE_FRAMEWORKS = ["AppKit", "Metal", "QuartzCore"]
PLIST = """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>{name}</string>
<key>CFBundleIdentifier</key><string>dev.quill.launch-floor.{name}</string>
<key>CFBundleName</key><string>{name}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>NSPrincipalClass</key><string>NSApplication</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
"""


def linked_frameworks(executable):
    """The system frameworks `executable` links, by name."""
    listing = subprocess.run(["otool", "-L", executable], check=True, capture_output=True, text=True)
    names = []
    for line in listing.stdout.splitlines()[1:]:
        path = line.strip().split(" ", 1)[0]
        if path.startswith("/System/Library/Frameworks/"):
            names.append(path.split("/")[4].removesuffix(".framework"))
    return names


def bind_table(executable, out):
    """A C file holding a pointer to each symbol `executable` binds."""
    listing = subprocess.run(["dyld_info", "-fixups", executable], check=True, capture_output=True,
                             text=True)
    symbols = []
    for line in listing.stdout.splitlines():
        fields = line.split()
        if len(fields) >= 5 and fields[3] == "bind":
            symbol = fields[4].partition("/")[2]
            if symbol not in symbols:
                symbols.append(symbol)
    lines = [f'extern char s{i} __asm__("{symbol}");' for i, symbol in enumerate(symbols)]
    lines.append("__attribute__((used)) void *const quill_binds[] = {")
    lines += [f"    &s{i}," for i in range(len(symbols))]
    lines.append("};")
    out.write_text("\n".join(lines) + "\n")
    return out


def build(name, frameworks, sources=()):
    app = bench.WORK / f"{name}.app"
    binary = app / "Contents/MacOS" / name
    binary.parent.mkdir(parents=True, exist_ok=True)
    flags = [arg for framework in frameworks for arg in ("-framework", framework)]
    subprocess.run(["clang", "-O2", "-fobjc-arc", "-mmacosx-version-min=14.0", HERE / "floor.m",
                    *sources, *flags, "-liconv", "-lc++", "-o", binary], check=True)
    (app / "Contents/Info.plist").write_text(PLIST.format(name=name))
    identity = os.environ.get("QUILL_CODESIGN_IDENTITY", "-")
    subprocess.run(["codesign", "-f", "-s", identity, app], check=True, capture_output=True)
    return app


def launch_floor(app, env, cold):
    if cold:
        subprocess.run(["sudo", "-n", "/usr/sbin/purge"], check=True)
        time.sleep(1)
    trace = bench.WORK / "floor-trace.txt"
    seen = bench.WORK / "floor.onscreen"
    trace.unlink(missing_ok=True)
    seen.unlink(missing_ok=True)
    watcher = subprocess.Popen([bench.winwatch(), app.stem, seen, "20"])
    time.sleep(0.2)
    args = ["/usr/bin/open", "-n", "-a", app, "--env", f"FLOOR_TRACE={trace}"]
    for pair in env:
        args += ["--env", pair]
    start = time.time()
    subprocess.run(args, check=True)
    watcher.wait(timeout=25)
    time.sleep(1.5)
    subprocess.run(["pkill", "-x", app.stem])
    time.sleep(1.5)
    marks = {}
    for path in (trace, seen):
        if path.exists():
            for line in path.read_text().splitlines():
                name, _, micros = line.partition(" ")
                if micros.strip().isdigit():
                    marks[name] = (int(micros) / 1e6 - start) * 1000.0
    return marks


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--runs", type=int, default=8)
    parser.add_argument("--cold", action="store_true", help="empty the file cache before every launch")
    parser.add_argument("--quill", action="append", default=[], metavar="LABEL=BUNDLE")
    parser.add_argument("variants", nargs="*", default=["titled"], choices=sorted(VARIANTS))
    opts = parser.parse_args()
    apps = {"FloorApp": build("FloorApp", BASE_FRAMEWORKS)}
    if "opnew" in opts.variants:
        apps["FloorOpNew"] = build("FloorOpNew", BASE_FRAMEWORKS, [HERE / "opnew.cpp"])
    if {"linked", "binds"} & set(opts.variants):
        if not opts.quill:
            parser.error("linked and binds need a --quill bundle to copy the frameworks from")
        executable = pathlib.Path(opts.quill[0].partition("=")[2]) / "Contents/MacOS/quill"
        frameworks = sorted(set(BASE_FRAMEWORKS + linked_frameworks(executable)))
        apps["FloorLinked"] = build("FloorLinked", frameworks)
        table = bind_table(executable, bench.WORK / "floor-binds.c")
        apps["FloorBinds"] = build("FloorBinds", frameworks, [table])
    creds = bench.credentials()
    sides = []
    for name in opts.variants:
        app, env = VARIANTS[name]
        sides.append((name, lambda cold, app=apps[app], env=env: launch_floor(app, env, cold)))
    for pair in opts.quill:
        label, _, bundle = pair.partition("=")
        trace = bench.WORK / f"trace-{label}.txt"
        sides.append((label, lambda cold, app=pathlib.Path(bundle), trace=trace:
                      bench.launch(app, trace, creds, cold, onscreen=True)))
    for _, run in sides:
        run(False)
    results = {label: [] for label, _ in sides}
    for index in range(opts.runs):
        for label, run in sides:
            marks = run(opts.cold)
            results[label].append(marks)
            shown = f"{marks['onscreen']:.0f} ms" if "onscreen" in marks else "NOT SEEN"
            print(f"{label} run {index}: onscreen {shown}, load {os.getloadavg()[0]:.1f}", flush=True)
            time.sleep(2)
    for label, runs in results.items():
        print(f"\n{label}:")
        names = {name for marks in runs for name in marks}
        order = sorted(names, key=lambda n: statistics.median(m[n] for m in runs if n in m))
        for name in order:
            values = [m[name] for m in runs if name in m]
            print(f"  {name:20s} median {statistics.median(values):6.0f}"
                  f"  min {min(values):6.0f}  max {max(values):6.0f}  n={len(values)}")


if __name__ == "__main__":
    main()
