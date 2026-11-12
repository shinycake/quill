#!/usr/bin/env python3
"""A timer belongs to the stage that waits for it.

docs/contributing/state-machines.md, rule 2 ("every stage has a way out")
and rule 6. Two shapes kept coming back in the state sweeps:

- `detached`: a task that sleeps on a timer or runs a loop (a watcher)
  and is then `.detach()`ed. Nothing can cancel it. The stage it serves (a toast, a linger, a
  watchdog) only names the timer, so the code grows a generation counter
  to tell an old timer's wake-up from the current one, and the timer still
  fires after its owner (the box, the chat, the account, the window) is
  gone. Hold the `Task` in the stage instead
  (`Shown { toast, _hide: Task<()> }`): replacing or leaving the stage
  drops the task, and the timer with it.
- `poll`: a `loop`/`while` that sleeps on a bare timer and goes round
  again. It keeps waking while the window is hidden, the app is locked or
  the account is logged out. Wait for the event instead: a subscription or
  observer, a channel, `wake_wait::sleep_or_wake` on a `quill::wake`
  signal (it sleeps until poked), or the frame clock
  (`QuillApp::request_animation_tick`) for anything that only matters
  while it is drawn.

The sites in the tree when the check was added are in
scripts/owned-timers-baseline.txt, by rule and function. That list is
debt: it only shrinks. A new site fails. A poll that must run on a clock
(a protocol keep-alive, a native API with no event) goes in
scripts/owned-timers-allow.txt with what ends it and why it has no event.
Screenshot-demo and fixture files are not checked.

Run from the repository root:

    python3 scripts/check-owned-timers.py [--root DIR] [--list] [--prune]
"""

import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import shape_guard  # noqa: E402

BASELINE = "scripts/owned-timers-baseline.txt"
ALLOW = "scripts/owned-timers-allow.txt"
HEADER = (
    "# Timers no stage owns (scripts/check-owned-timers.py).\n"
    "# <rule>:<Type::function> <count>. Only remove lines or lower counts.\n"
)
ADVICE = (
    "detached: hold the Task in the stage that waits for the timer, so leaving the stage "
    "cancels it. poll: wait for the event (a subscription, wake_wait::sleep_or_wake, the "
    "frame clock) instead of a timer in a loop (docs/contributing/state-machines.md)."
)
TIMER = re.compile(r"\.\s*timer\s*\(|\bTimer::(?:after|at|interval)\b|\bsleep\s*\(")
# In src/ui a thread that sleeps in a loop is a timer in a loop too (the
# capability probe of 19-011 was one): `.await` or not.
BLOCKING_SLEEP = re.compile(r"\bthread::sleep\s*\(")
DETACH = re.compile(r"\.\s*detach\s*\(\s*\)")
LOOP = re.compile(r"\b(?:loop|while)\b")
WATCH = re.compile(r"\b(?:loop|while)\b|\bsleep_or_wake\b|\brun_poll_loop\b")
NAME = re.compile(r"([A-Za-z_]\w*)\s*$")


def detached_body(text, dot, scopes):
    """The source of the task whose `.detach()` starts at `dot`."""
    before = text[:dot].rstrip()
    if before.endswith(")"):
        return text[shape_guard.opening(text, len(before) - 1) : dot]
    name = NAME.search(before)
    span = scopes.fn_span(dot)
    if not name or not span:
        return ""
    made = None
    for made in re.finditer(rf"\blet\s+(?:mut\s+)?{name.group(1)}\b", text[span[0] : dot]):
        pass
    return text[span[0] + made.start() : dot] if made else ""


def hits(root):
    for path, text in shape_guard.source(root):
        if shape_guard.is_seeder(path) or not (TIMER.search(text) or DETACH.search(text)):
            continue
        scopes = shape_guard.Scopes(path, text)
        in_ui = path.replace(os.sep, "/").startswith("src/ui/")
        for match in DETACH.finditer(text):
            body = detached_body(text, match.start(), scopes)
            if TIMER.search(body) or WATCH.search(body):
                line = shape_guard.line_of(text, match.start())
                yield f"detached:{scopes.at(match.start())}", path, line
        for match in LOOP.finditer(text):
            brace = shape_guard.body_start(text, match.end())
            if brace is None:
                continue
            body = text[brace : shape_guard.closing(text, brace)]
            slept = in_ui and BLOCKING_SLEEP.search(body)
            if (".await" in body or slept) and TIMER.search(body):
                line = shape_guard.line_of(text, match.start())
                yield f"poll:{scopes.at(match.start())}", path, line


if __name__ == "__main__":
    shape_guard.main("check-owned-timers", hits, BASELINE, ALLOW, HEADER, ADVICE)
