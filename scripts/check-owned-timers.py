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
  stored deadline (`due.saturating_duration_since(now)`), and the call that
  re-arms must hand on a deadline too, not `now() + PERIOD` or a constant;
  otherwise the site goes in the allow list with its reason.
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
    r"\.\s*timer\s*\(|\bTimer::(?:after|at|interval)\b|\bsleep\s*\("
)
# A wait computed from a deadline (`due - now`) is not a fixed period. Only the
# timer's own delay is tested (never the words elsewhere in the function), and
# a `*_wait(` call or a `remaining` field is no proof: either may hand back a
# constant. Every name in the delay is read through its `let`, and the call
# that re-arms is tested too (`rearm_fixed`).
DEADLINE = re.compile(
    r"duration_since|saturating_sub|checked_sub|\bdeadline\b|\bdue\b|\bnext_wake\b"
)
CONSTANT = re.compile(r"\b[A-Z][A-Z0-9_]{2,}\b|Duration::from_\w+\s*\(\s*[0-9]")
IDENT = re.compile(r"\s*([A-Za-z_]\w*)\s*$")
NAMES = re.compile(r"(?<![\w.:])([a-z_]\w*)\b(?!\s*[(:!])")
# A name the wait is taken from (`period.saturating_sub(spent)`), whose
# `let` is read to see whether it holds a constant.
RECEIVER = re.compile(r"(?<![\w.:])([a-z_]\w*)(?=\s*\.\s*(?:saturating_sub|checked_sub)\b)")
FLOOR = re.compile(r"\.\s*max\s*\(")
MILLIS = re.compile(r"Duration::from_millis\s*\(\s*([0-9_]+)\s*\)")
FRAME_MS = 16
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


def last_let(name, body, before):
    """The expression `name` was last bound to before offset `before`, or None."""
    made = None
    for made in re.finditer(rf"\blet\s+(?:mut\s+)?{name}\b[^=;]*=(?!=)", body[:before]):
        pass
    if not made:
        return None
    end = body.find(";", made.end())
    return body[made.end() : end if end >= 0 else len(body)].strip(), made.start()


def resolved(expr, body, before, names=NAMES, depth=4):
    """`expr` with each name `names` finds that a `let` bound replaced by what it holds."""
    if depth == 0:
        return expr

    def swap(found):
        bound = last_let(found.group(1), body, before)
        if not bound:
            return found.group(0)
        return "(" + resolved(bound[0], body, bound[1], names, depth - 1) + ")"

    return names.sub(swap, expr)


def long_floor(expr, body, before):
    """Whether a `.max(..)` in `expr` floors the wait at a constant longer than a frame."""
    for found in FLOOR.finditer(expr):
        arg = expr[found.end() : shape_guard.closing(expr, found.end() - 1)]
        bound = last_let(arg.strip(), body, before) if IDENT.fullmatch(arg) else None
        if bound:
            arg = bound[0]
        if not CONSTANT.search(arg) or re.fullmatch(r"\s*FRAME\s*", arg):
            continue
        millis = MILLIS.fullmatch(arg.strip())
        if not millis or int(millis.group(1).replace("_", "")) > FRAME_MS:
            return True
    return False


def delay_from_deadline(expr, body, before):
    """Whether the delay `expr` comes from a stored deadline on every path.

    Every name is read through its last `let` in `body`; an `if` or `match`
    must be driven by a deadline in each of its branches; a constant floor
    (`.max(SECOND)`) turns the wait into a fixed period.
    """
    if IDENT.fullmatch(expr):
        bound = last_let(expr.strip(), body, before)
        return bool(bound) and delay_from_deadline(bound[0], body, bound[1])
    expr = resolved(expr, body, before, RECEIVER)
    if re.search(r"\bif\b|\bmatch\b", expr):
        arms = []
        at = 0
        while (brace := expr.find("{", at)) >= 0:
            close = shape_guard.closing(expr, brace)
            arms.append(expr[brace + 1 : close])
            at = close + 1
        return bool(arms) and all(delay_from_deadline(arm.strip(), body, before) for arm in arms)
    if long_floor(expr, body, before):
        return False
    return bool(DEADLINE.search(expr)) and not CONSTANT.search(re.split(r"\.(?:min|max)\(", expr)[0])


def rearm_fixed(body, name, after):
    """Whether a self-call of `name` after offset `after` hands on a fixed time.

    The argument must come from a stored deadline: `Instant::now() + X`, a
    name bound to one, or a bare constant is a period in disguise.
    """
    call = re.compile(rf"(?:\.|\bSelf::)\s*{re.escape(name)}\s*\(")
    for found in call.finditer(body, after):
        paren = found.end() - 1
        args = resolved(body[paren + 1 : shape_guard.closing(body, paren)], body, found.start())
        if re.search(r"\bnow\b[^;]*\+", args) or (
            CONSTANT.search(args) and not DEADLINE.search(args)
        ):
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
            if not call.search(body, timer.end()):
                continue
            paren = body.find("(", timer.start())
            if (
                paren >= 0
                and delay_from_deadline(
                    body[paren + 1 : shape_guard.closing(body, paren)].strip(), body, timer.start()
                )
                and not rearm_fixed(body, name, timer.end())
            ):
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
            if (".await" in body or slept) and TIMER.search(body):
                line = shape_guard.line_of(text, match.start())
                yield f"poll:{scopes.at(match.start())}", path, line


if __name__ == "__main__":
    shape_guard.main("check-owned-timers", hits, BASELINE, ALLOW, HEADER, ADVICE)
