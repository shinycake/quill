#!/usr/bin/env python3
"""Every library a Quill launch loads, merged with its startup trace marks.

    python3 -I scripts/launch-floor/dyld-timeline.py EXE OUT [--cold]

Runs EXE (a bench bundle's Contents/MacOS/quill: those have no hardened
runtime, so dyld reads DYLD_PRINT_LIBRARIES) with QUILL_STARTUP_TRACE, stamps
each image dyld reports as it arrives, and writes OUT as one line per load or
mark, in ms after exec. Loads arrive through a pipe, so a burst shares one
stamp. Quill is asked to quit after 7 s through NSRunningApplication, the way
launch-bench.py quits it: a signal would leave a "closed unexpectedly" report
for the account's next launch. Set TELEGRAM_API_ID and TELEGRAM_API_HASH (from
quill.local.env) or the chats never load.
"""
import os
import pathlib
import subprocess
import sys
import tempfile
import threading
import time


def main():
    exe, out = sys.argv[1], pathlib.Path(sys.argv[2])
    trace = pathlib.Path(tempfile.mkdtemp()) / "trace.txt"
    env = dict(os.environ, DYLD_PRINT_LIBRARIES="1", QUILL_STARTUP_TRACE=str(trace))
    if "--cold" in sys.argv:
        subprocess.run(["sudo", "-n", "/usr/sbin/purge"], check=True)
        time.sleep(2)
    loads = []
    proc = subprocess.Popen([exe], env=env, stderr=subprocess.PIPE, stdout=subprocess.DEVNULL)

    def read():
        for raw in proc.stderr:
            line = raw.decode(errors="replace").rstrip()
            if line.startswith("dyld["):
                loads.append((time.time() * 1e6, line.split("> ", 1)[-1]))

    threading.Thread(target=read, daemon=True).start()
    time.sleep(7)
    script = ("ObjC.import('AppKit'); "
              f"$.NSRunningApplication.runningApplicationWithProcessIdentifier({proc.pid}).terminate")
    subprocess.run(["osascript", "-l", "JavaScript", "-e", script], capture_output=True)
    try:
        proc.wait(timeout=20)
    except subprocess.TimeoutExpired:
        proc.kill()
    marks = []
    for line in trace.read_text().splitlines():
        name, _, micros = line.partition(" ")
        if micros.strip().isdigit():
            marks.append((int(micros), name))
    exec_at = next(micros for micros, name in marks if name == "exec")
    rows = sorted([(t, "MARK " + name) for t, name in marks] + loads)
    with out.open("w") as file:
        for t, label in rows:
            file.write(f"{(t - exec_at) / 1000:8.1f} {label}\n")
    print(f"{out}: {len(loads)} loads, {len(marks)} marks")


if __name__ == "__main__":
    main()
