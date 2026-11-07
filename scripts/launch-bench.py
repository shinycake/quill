#!/usr/bin/env python3
"""Time Quill's macOS launch to a painted chat list.

    scripts/launch-bench.py [--runs N] [--cold] [--template dist/Quill.app] LABEL=BINARY ...

Each BINARY (a `--features ui --release` quill) is copied into its own
signed Quill.app under ~/Library/Caches/quill-launch-bench/LABEL, built from
the template bundle (its Frameworks hold TDLib and the other libraries). The
bundles are launched N times each, alternating, through LaunchServices with a
clean environment (as a Finder double click is), against the real account in
the default data folder, with QUILL_STARTUP_TRACE set (src/startup_trace.rs).
Every milestone is printed as the median, min and max ms after `open` was
called; `chat_list` is the first frame that draws the chat list.

`--onscreen` adds `onscreen`, the moment WindowServer first lists Quill's
window as on screen, seen from outside by scripts/winwatch.swift (built
with swiftc on first use). It polls every 2 ms.

`--cold` empties the file cache (`sudo -n /usr/sbin/purge`, so sudo must
allow it without a password) before every launch, as after a reboot. The
shared system libraries stay mapped, so this is close to, not exactly, a
first launch after boot.

Environment: QUILL_CODESIGN_IDENTITY signs the bundles (use the identity the
keychain already trusts for Quill's database key, or the keychain asks on
every launch). TELEGRAM_API_ID and TELEGRAM_API_HASH are passed to the app;
QUILL_ENV_FILE (default: quill.local.env next to this repository) fills them
when unset.

Quit any running Quill first: a second Quill hands its launch to the first.
Bundles and trace files stay on the internal disk: an app that LaunchServices
starts from a removable volume waits on a consent prompt.
"""
import argparse
import json
import os
import pathlib
import shutil
import statistics
import subprocess
import sys
import time

WORK = pathlib.Path.home() / "Library/Caches/quill-launch-bench"
REPO = pathlib.Path(__file__).resolve().parent.parent
DEFAULT_DATA = pathlib.Path.home() / "Library/Application Support/org.shinycake.Quill"


def credentials():
    found = {k: os.environ.get(k) for k in ("TELEGRAM_API_ID", "TELEGRAM_API_HASH")}
    env_file = pathlib.Path(os.environ.get("QUILL_ENV_FILE", REPO / "quill.local.env"))
    if not all(found.values()) and env_file.is_file():
        for line in env_file.read_text().splitlines():
            key, _, value = line.partition("=")
            if key.strip() in found and not found[key.strip()]:
                found[key.strip()] = value.strip()
    return {k: v for k, v in found.items() if v}


def make_bundle(label, binary, template):
    app = WORK / label / "Quill.app"
    shutil.rmtree(app.parent, ignore_errors=True)
    app.parent.mkdir(parents=True)
    subprocess.run(["ditto", template, app], check=True)
    exe = app / "Contents/MacOS/quill"
    shutil.copy2(binary, exe)
    subprocess.run(["install_name_tool", "-add_rpath", "@executable_path/../Frameworks", exe],
                   capture_output=True)
    identity = os.environ.get("QUILL_CODESIGN_IDENTITY", "-")
    subprocess.run(["codesign", "--force", "--deep", "-s", identity, app],
                   check=True, capture_output=True)
    return app


def running(pattern=None):
    """Whether a process matches `pattern`, or with none, whether a Quill app runs.

    `quill mcp` and `quill agent` are command-line clients of the app (src/dev_cli.rs),
    not a second app a launch would hand off to, so they do not count."""
    if pattern:
        return subprocess.run(["pgrep", "-f", pattern], capture_output=True).returncode == 0
    pids = subprocess.run(["pgrep", "-x", "quill"], capture_output=True, text=True).stdout.split()
    for pid in pids:
        exe = subprocess.run(["ps", "-o", "comm=", "-p", pid], capture_output=True, text=True).stdout.strip()
        args = subprocess.run(["ps", "-o", "args=", "-p", pid], capture_output=True, text=True).stdout.strip()
        argv = args.removeprefix(exe).split()
        if exe and not (argv and argv[0] in ("mcp", "agent")):
            return True
    return False


def quit_quill(app):
    # Only the bundle this script launched: another Quill (the user's, or
    # one started outside this script) is left alone. It is asked to quit
    # the way the Dock asks, so it records a clean exit: a signal would
    # leave a crash notice for the account's next launch.
    pattern = str(app / "Contents/MacOS/quill")
    pids = subprocess.run(["pgrep", "-f", pattern], capture_output=True, text=True).stdout.split()
    for pid in pids:
        script = ("ObjC.import('AppKit'); "
                  f"$.NSRunningApplication.runningApplicationWithProcessIdentifier({int(pid)}).terminate")
        subprocess.run(["osascript", "-l", "JavaScript", "-e", script], capture_output=True)
    for _ in range(80):
        if not running(pattern):
            return
        time.sleep(0.25)
    subprocess.run(["pkill", "-TERM", "-f", pattern])
    for _ in range(80):
        if not running(pattern):
            return
        time.sleep(0.25)
    subprocess.run(["pkill", "-KILL", "-f", pattern])
    time.sleep(2)


def evict(paths):
    """Drop the files under `paths` from the file cache and leave the rest of the
    system (the dyld shared cache, other apps) cached, as on a Dock click a while
    after Quill last ran. `--cold` runs `purge`, which empties everything."""
    import ctypes
    libc = ctypes.CDLL(None, use_errno=True)
    libc.mmap.restype = ctypes.c_void_p
    libc.mmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int, ctypes.c_int,
                          ctypes.c_int, ctypes.c_longlong]
    libc.msync.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int]
    libc.munmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    prot_read, map_shared, ms_invalidate = 0x1, 0x1, 0x2
    for root in paths:
        for dirpath, _, names in os.walk(root):
            for name in names:
                path = os.path.join(dirpath, name)
                if os.path.islink(path):
                    continue
                try:
                    fd = os.open(path, os.O_RDONLY)
                except OSError:
                    continue
                try:
                    size = os.fstat(fd).st_size
                    if size == 0:
                        continue
                    base = libc.mmap(None, size, prot_read, map_shared, fd, 0)
                    if base in (None, ctypes.c_void_p(-1).value):
                        continue
                    libc.msync(base, size, ms_invalidate)
                    libc.munmap(base, size)
                finally:
                    os.close(fd)


def winwatch():
    """The outside-in window timer, built from scripts/winwatch.swift."""
    binary = WORK / "winwatch"
    source = REPO / "scripts/winwatch.swift"
    if not binary.exists() or binary.stat().st_mtime < source.stat().st_mtime:
        WORK.mkdir(parents=True, exist_ok=True)
        subprocess.run(["swiftc", "-O", source, "-o", binary], check=True)
    return binary


def launch(app, trace, creds, cold=False, timeout=40.0, data_root=None, until="chat_list", extra_env=(),
           evict_paths=(), onscreen=False):
    trace.unlink(missing_ok=True)
    seen = trace.with_suffix(".onscreen")
    seen.unlink(missing_ok=True)
    if cold:
        subprocess.run(["sudo", "-n", "/usr/sbin/purge"], check=True)
        time.sleep(1)
    elif evict_paths:
        evict([app, *evict_paths])
        time.sleep(1)
    args = ["/usr/bin/open", "-n", "-a", app, "--env", f"QUILL_STARTUP_TRACE={trace}"]
    for key, value in creds.items():
        args += ["--env", f"{key}={value}"]
    if data_root:
        args += ["--env", f"QUILL_DATA_ROOT={data_root}"]
    for pair in extra_env:
        args += ["--env", pair]
    env = {"HOME": os.environ["HOME"], "PATH": "/usr/bin:/bin:/usr/sbin:/sbin"}
    watcher = None
    if onscreen:
        watcher = subprocess.Popen([winwatch(), app.stem, seen, str(timeout)])
        time.sleep(0.2)
    start = time.time()
    subprocess.run(args, env=env, check=True)
    marks = {}

    def read_marks():
        if trace.exists():
            for line in trace.read_text().splitlines():
                name, _, micros = line.partition(" ")
                if micros.strip().isdigit():
                    marks[name] = (int(micros) / 1e6 - start) * 1000.0

    while time.time() < start + timeout and until not in marks:
        read_marks()
        time.sleep(0.02)
    if watcher:
        try:
            watcher.wait(timeout=max(1.0, start + timeout - time.time()))
        except subprocess.TimeoutExpired:
            watcher.kill()
        if seen.exists():
            name, _, micros = seen.read_text().strip().partition(" ")
            if micros.isdigit():
                marks[name] = (int(micros) / 1e6 - start) * 1000.0
    # Let the launch's tail (first syncs, avatars) finish before quitting,
    # and keep the marks it wrote (did_finish comes after chat_list).
    time.sleep(3)
    read_marks()
    quit_quill(app)
    return marks


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--runs", type=int, default=7)
    parser.add_argument("--cold", action="store_true", help="empty the file cache before every launch")
    parser.add_argument("--evict", action="store_true",
                        help="drop only the bundle and the data folder from the file cache before every launch")
    parser.add_argument("--template", default=str(REPO / "dist/Quill.app"))
    parser.add_argument("--data-root", help="QUILL_DATA_ROOT for the launches (default: the real account)")
    parser.add_argument("--until", default="chat_list",
                        help="the milestone that ends a launch (first_render for a signed-out data root)")
    parser.add_argument("--onscreen", action="store_true",
                        help="also time when the window is first on screen, from outside (scripts/winwatch.swift)")
    parser.add_argument("--json", metavar="PATH", help="also write every run's marks to PATH")
    parser.add_argument("sides", nargs="+", metavar="LABEL=BINARY")
    opts = parser.parse_args()
    if running() and not opts.data_root:
        sys.exit("a Quill is running; quit it first (or pass --data-root)")
    creds = credentials()
    sides = []
    envs = {}
    for side in opts.sides:
        label, _, binary = side.partition("=")
        binary, *extra = binary.split("@")
        envs[label] = extra
        sides.append((label, make_bundle(label, binary, opts.template)))
    results = {label: [] for label, _ in sides}
    evict_paths = [opts.data_root or DEFAULT_DATA] if opts.evict else []
    # One unrecorded launch each: a newly signed binary is assessed by the
    # system on its first launch (~0.7 s), which is not Quill's launch time.
    for label, app in sides:
        launch(app, WORK / f"trace-{label}.txt", creds, data_root=opts.data_root, until=opts.until, extra_env=envs[label],
               onscreen=opts.onscreen)
    for run in range(opts.runs):
        for label, app in sides:
            marks = launch(app, WORK / f"trace-{label}.txt", creds, opts.cold,
                           data_root=opts.data_root, until=opts.until, extra_env=envs[label],
                           evict_paths=evict_paths, onscreen=opts.onscreen)
            results[label].append(marks)
            shown = f"{marks[opts.until]:.0f} ms" if opts.until in marks else "NOT REACHED"
            print(f"{label} run {run}: {opts.until} {shown}, load {os.getloadavg()[0]:.1f}",
                  flush=True)
            time.sleep(2)
    if opts.json:
        pathlib.Path(opts.json).write_text(json.dumps(results, indent=1))
    for label, runs in results.items():
        reached = sum(opts.until in m for m in runs)
        print(f"\n{label}: {reached}/{len(runs)} runs reached {opts.until}")
        names = {name for m in runs for name in m}
        order = sorted(names, key=lambda n: statistics.median(m[n] for m in runs if n in m))
        for name in order:
            values = [m[name] for m in runs if name in m]
            print(f"  {name:14s} median {statistics.median(values):6.0f}"
                  f"  min {min(values):6.0f}  max {max(values):6.0f}  n={len(values)}")


if __name__ == "__main__":
    main()
