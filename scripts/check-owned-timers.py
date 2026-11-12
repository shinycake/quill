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
- `poll` also covers a re-arm: a function that spawns a task which waits a constant time and
  then calls the same function again from its update. It is `poll` written
  without a `loop`: the task is held, but the wake-up rate and the wakes
  while hidden, locked or idle are the same. The wait must come from a
  deadline (`due.saturating_duration_since(now)`, a `*_wait(..)` call), or
  the site goes in the allow list with its reason.
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
    "poll: a task that waits a fixed time and re-arms itself is a poll too. "
    "detached: hold the Task in the stage that waits for the timer, so leaving the stage "
    "cancels it. poll: wait for the event (a subscription, wake_wait::sleep_or_wake, the "
    "frame clock) instead of a timer in a loop (docs/contributing/state-machines.md)."
)
TIMER = re.compile(
    r"\.\s*timer\s*\(|\bTimer::(?:after|at|interval)\b|\bsleep\s*\(|\bsleep_or_wake_paced\s*\("
)
# A wait computed from a deadline (`due - now`) is not a fixed period.
DEADLINE = re.compile(
    r"duration_since|saturating_sub|checked_sub|\bdeadline\b|\bdue\b|\bremaining\b|\bnext_wake\b"
    r"|\b\w+_wait\w*\s*\("
)
# Receivers that are not a call of the function itself (`cx.new(..)`).
GENERIC = {"new", "update", "spawn", "default", "from", "into", "sleep"}
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


def fixed_timer(body):
    """Whether `body` waits on a timer whose time is not computed from a deadline."""
    for timer in TIMER.finditer(body):
        paren = body.find("(", timer.start())
        if paren < 0:
            return True
        if not DEADLINE.search(body[paren : shape_guard.closing(body, paren) + 1]):
            return True
    return False


def in_loop(body, offset):
    """Whether a `loop`/`while` body around `offset` holds it (that is `poll`)."""
    for match in LOOP.finditer(body, 0, offset):
        brace = shape_guard.body_start(body, match.end())
        if brace is not None and brace < offset < shape_guard.closing(body, brace):
            return True
    return False


def rearm_hits(text, scopes):
    """Functions whose timer-then-self-call chain re-arms at a fixed period."""
    for start, end, name in scopes.fns:
        if name in GENERIC:
            continue
        body = text[start:end]
        call = re.compile(rf"(?:\.|\bSelf::)\s*{re.escape(name)}\s*\(")
        for timer in TIMER.finditer(body):
            if DEADLINE.search(body) or not call.search(body, timer.end()):
                continue
            if in_loop(body, timer.start()):
                continue
            yield f"poll:{scopes.at(start)}", shape_guard.line_of(text, start + timer.start())
            break


def hits(root):
    for path, text in shape_guard.source(root):
        if shape_guard.is_seeder(path) or not (TIMER.search(text) or DETACH.search(text)):
            continue
        scopes = shape_guard.Scopes(path, text)
        in_ui = path.replace(os.sep, "/").startswith("src/ui/")
        for key, line in rearm_hits(text, scopes):
            yield key, path, line
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
            if (".await" in body or slept) and fixed_timer(body):
                line = shape_guard.line_of(text, match.start())
                yield f"poll:{scopes.at(match.start())}", path, line


if __name__ == "__main__":
    shape_guard.main("check-owned-timers", hits, BASELINE, ALLOW, HEADER, ADVICE)
