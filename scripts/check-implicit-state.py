#!/usr/bin/env python3
"""No new implicit state machines.

docs/contributing/state-machines.md: a thing that moves through stages
(asked, in flight, answered, failed; opening, open, closing) is one enum
value, not a set of flags and options that code keeps in step by hand.
Fourteen review rounds (#1392 to #1405 and on) found hundreds of bugs of
the same few shapes: a `loading`/`pending`/`busy` bool set before a request
and cleared only on success, so it sticks when the answer fails or its
continuation is dropped; a `loading` bool next to an `error` field that
can both be set; flags that are reset by hand in some places and not
others.

This check finds the shapes a field list shows:

- a `bool` field whose name says it is a stage of something
  (`pending`, `loading`, `busy`, `in_flight`, `requested`, `sending`,
  `failed`, `done`, ...);
- a struct that holds such a flag and an `error` field beside it (that
  pair is `Fetch<T>`, src/state/fetch.rs);
- an integer field that counts acknowledgements (`*_oks`, `*_acks`,
  `*_acked`): a counter is not matched to what asked, so another request's
  answer (or a dialog closed and reopened) is counted for this one. Register
  the request with `Session::acks` (`RequestAcks`, src/state/acks.rs) and
  settle each on its own answer.

Every hit in the tree when the check was added is in
scripts/implicit-state-baseline.txt. Those are debt: the list may only
shrink, and a listed field that is gone must be removed from it. A new
hit fails. Model it as an enum (or `Fetch<T>`) instead, or, when the bool
truly is not a stage of anything (a user setting, a capability), list it
in scripts/implicit-state-allow.txt with the reason.

Run from the repository root:

    python3 scripts/check-implicit-state.py [--root DIR] [--write-baseline]
"""

import importlib.util
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location(
    "check_screen_state", os.path.join(HERE, "check-screen-state.py")
)
_screen = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_screen)
struct_fields = _screen.struct_fields

BASELINE = "scripts/implicit-state-baseline.txt"
ALLOW = "scripts/implicit-state-allow.txt"
SCAN = ("src", "crates")

STAGE_WORDS = (
    "pending|loading|loaded|busy|in_flight|inflight|requested|requesting|"
    "waiting|awaiting|fetching|fetched|sending|sent|saving|saved|submitting|"
    "submitted|uploading|downloading|opening|closing|starting|started|"
    "stopping|failed|failing|done|finished|completed|retrying|connecting|"
    "joining|leaving|resolving|resolved|refreshing|syncing|queued|armed|"
    "in_progress|ongoing|working|asked|answered"
)
STAGE_BOOL = re.compile(rf"(?:^|_)(?:{STAGE_WORDS})(?:$|_)")
COUNTER_TYPES = ("u8", "u16", "u32", "u64", "usize", "i32", "i64", "isize")
ACK_COUNTER = re.compile(r"(?:^|_)(?:oks|acks|acked|ack_count)(?:$|_)")
ERROR_FIELD = re.compile(r"(?:^|_)(?:error|err|failure)(?:$|_)")
STRUCT = re.compile(r"\bstruct\s+([A-Z][A-Za-z0-9_]*)\s*(?:<[^{;]*>)?\s*(?:where[^{;]*)?\{")


def bodies(text):
    """`(name, body)` for every brace struct in `text`."""
    for match in STRUCT.finditer(text):
        start = match.end()
        depth = 1
        for i in range(start, len(text)):
            ch = text[i]
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    yield match.group(1), text[start:i]
                    break


def strip_tests(text):
    """`text` without `#[cfg(test)] mod ... { ... }` blocks."""
    out, i = [], 0
    pattern = re.compile(r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*mod\s+\w+\s*\{")
    while True:
        match = pattern.search(text, i)
        if match is None:
            out.append(text[i:])
            return "".join(out)
        out.append(text[i : match.start()])
        depth, j = 1, match.end()
        while j < len(text) and depth:
            if text[j] == "{":
                depth += 1
            elif text[j] == "}":
                depth -= 1
            j += 1
        i = j


def is_test_file(path):
    name = os.path.basename(path)
    return name.endswith("_tests.rs") or name == "tests.rs" or "/tests/" in path


def hits(root):
    """Sorted `Struct::field` keys for every stage flag and fetch pair."""
    found = set()
    for top in SCAN:
        for dirpath, _, files in os.walk(os.path.join(root, top)):
            if "/target" in dirpath or "/vendor" in dirpath:
                continue
            for name in files:
                path = os.path.join(dirpath, name)
                if not name.endswith(".rs") or is_test_file(path):
                    continue
                with open(path, encoding="utf-8", errors="replace") as handle:
                    text = strip_tests(handle.read())
                for struct, body in bodies(text):
                    fields = struct_fields(body)
                    flags = [
                        f
                        for f, t in fields
                        if t == "bool" and STAGE_BOOL.search(f) and not f.startswith("can_")
                    ]
                    for flag in flags:
                        found.add(f"{struct}::{flag}")
                    for f, t in fields:
                        if t in COUNTER_TYPES and ACK_COUNTER.search(f):
                            found.add(f"{struct}::{f}")
                    errors = [f for f, t in fields if ERROR_FIELD.search(f) and t != "bool"]
                    if flags and errors:
                        found.add(f"{struct}::{flags[0]}+{errors[0]}")
    return sorted(found)


def listed(path):
    """Keys in a list file: the first word of each line, `#` comments skipped."""
    if not os.path.exists(path):
        return {}
    out = {}
    with open(path, encoding="utf-8") as handle:
        for number, line in enumerate(handle, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            key, _, reason = line.partition(" ")
            out[key] = (number, reason.strip())
    return out


def main():
    args = sys.argv[1:]
    root = "."
    if "--root" in args:
        root = args[args.index("--root") + 1]
    found = hits(root)
    if "--write-baseline" in args:
        allow = listed(os.path.join(root, ALLOW))
        with open(os.path.join(root, BASELINE), "w", encoding="utf-8") as handle:
            handle.write(
                "# Implicit-state debt (scripts/check-implicit-state.py). Only remove lines.\n"
            )
            for key in found:
                if key not in allow:
                    handle.write(key + "\n")
        print(f"baseline written: {len(found)} hits")
        return 0
    baseline = listed(os.path.join(root, BASELINE))
    allow = listed(os.path.join(root, ALLOW))
    problems = []
    for key, (number, reason) in allow.items():
        if not reason:
            problems.append(f"{ALLOW}:{number}: {key} needs a reason after the name")
    found_set = set(found)
    for key in found:
        if key in baseline or key in allow:
            continue
        if ACK_COUNTER.search(key.rsplit("::", 1)[-1]):
            problems.append(
                f"{key}: an acknowledgement counter is not matched to what asked; register "
                "the request with Session::acks (RequestAcks) and settle it on its own answer"
            )
        elif "+" in key:
            problems.append(
                f"{key}: a stage flag beside an error field is a hand-rolled Fetch<T> "
                "(src/state/fetch.rs); use it, or an enum"
            )
        else:
            problems.append(
                f"{key}: a bool that names a stage is an implicit state machine; make the "
                f"stages one enum value (docs/contributing/state-machines.md), or list it in {ALLOW} "
                "with why it is not a stage"
            )
    for path, entries in ((BASELINE, baseline), (ALLOW, allow)):
        for key, (number, _) in entries.items():
            if key not in found_set:
                problems.append(f"{path}:{number}: {key} is gone; remove the line")
    if problems:
        for problem in problems:
            print(problem)
        print(f"check-implicit-state: {len(problems)} problem(s)")
        return 1
    print(f"check-implicit-state: ok ({len(baseline)} baseline, {len(allow)} allowed)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
