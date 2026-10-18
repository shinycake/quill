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
import os
import pathlib
import shutil
import statistics
import subprocess
import sys
import time

BUNDLE_ID = "org.shinycake.quill"
WORK = pathlib.Path.home() / "Library/Caches/quill-launch-bench"
REPO = pathlib.Path(__file__).resolve().parent.parent


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


def running():
    return subprocess.run(["pgrep", "-x", "quill"], capture_output=True).returncode == 0


def quit_quill():
    subprocess.run(["osascript", "-e", f'tell application id "{BUNDLE_ID}" to quit'],
                   capture_output=True)
    for _ in range(80):
        if not running():
            return
        time.sleep(0.25)
    subprocess.run(["pkill", "-TERM", "-x", "quill"])
    time.sleep(2)


def launch(app, trace, creds, cold=False, timeout=40.0):
    trace.unlink(missing_ok=True)
    if cold:
        subprocess.run(["sudo", "-n", "/usr/sbin/purge"], check=True)
        time.sleep(1)
    args = ["/usr/bin/open", "-n", "-a", app, "--env", f"QUILL_STARTUP_TRACE={trace}"]
    for key, value in creds.items():
        args += ["--env", f"{key}={value}"]
    env = {"HOME": os.environ["HOME"], "PATH": "/usr/bin:/bin:/usr/sbin:/sbin"}
    start = time.time()
    subprocess.run(args, env=env, check=True)
    marks = {}
    while time.time() < start + timeout and "chat_list" not in marks:
        if trace.exists():
            for line in trace.read_text().splitlines():
                name, _, micros = line.partition(" ")
                if micros.strip().isdigit():
                    marks[name] = (int(micros) / 1e6 - start) * 1000.0
        time.sleep(0.02)
    # Let the launch's tail (first syncs, avatars) finish before quitting.
    time.sleep(3)
    quit_quill()
    return marks


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--runs", type=int, default=7)
    parser.add_argument("--cold", action="store_true", help="empty the file cache before every launch")
    parser.add_argument("--template", default=str(REPO / "dist/Quill.app"))
    parser.add_argument("sides", nargs="+", metavar="LABEL=BINARY")
    opts = parser.parse_args()
    if running():
        sys.exit("a Quill is running; quit it first")
    creds = credentials()
    sides = []
    for side in opts.sides:
        label, _, binary = side.partition("=")
        sides.append((label, make_bundle(label, binary, opts.template)))
    results = {label: [] for label, _ in sides}
    # One unrecorded launch each: a newly signed binary is assessed by the
    # system on its first launch (~0.7 s), which is not Quill's launch time.
    for label, app in sides:
        launch(app, WORK / f"trace-{label}.txt", creds)
    for run in range(opts.runs):
        for label, app in sides:
            marks = launch(app, WORK / f"trace-{label}.txt", creds, opts.cold)
            results[label].append(marks)
            shown = f"{marks['chat_list']:.0f} ms" if "chat_list" in marks else "NOT REACHED"
            print(f"{label} run {run}: chat_list {shown}, load {os.getloadavg()[0]:.1f}",
                  flush=True)
            time.sleep(2)
    for label, runs in results.items():
        reached = sum("chat_list" in m for m in runs)
        print(f"\n{label}: {reached}/{len(runs)} runs reached chat_list")
        names = {name for m in runs for name in m}
        order = sorted(names, key=lambda n: statistics.median(m[n] for m in runs if n in m))
        for name in order:
            values = [m[name] for m in runs if name in m]
            print(f"  {name:14s} median {statistics.median(values):6.0f}"
                  f"  min {min(values):6.0f}  max {max(values):6.0f}  n={len(values)}")


if __name__ == "__main__":
    main()
