#!/usr/bin/env python3
"""An answer is matched to the request that asked, by its `RequestId`.

docs/contributing/state-machines.md, rule 3. A stage that waits for an
answer holds the `RequestId` of its request and takes only that answer
(`Fetch::sent` / `answered` / `failed`, `ListOp::begin`, `RequestAcks`).
Two shortcuts kept coming back in the state sweeps (about 175 entries in
the re-sweep of 2026-11-11), and this check counts both:

- `purpose`: a call to `Requests::has_purpose*`, `take_purpose*` or
  `has_auth_submit` outside src/state/requests.rs. "Some request with
  this purpose is in flight" is not "my request is in flight": a second
  window, an older request or the same request for another chat answers
  the question too. The busy state is then wrong, a late answer is taken
  for the current one, and an error ends the wrong stage.
- `unmatched`: a call to `Fetch::succeed`, which sets the answer whatever
  request it came from. An older request's late answer overwrites a newer
  one.
- `ticket`: a request purpose (any `enum *Purpose` under src/) that carries a `u64` counter (`token`,
  `generation`) to tell its answer from an older one. The counter stands
  in for the `RequestId`: every start path must remember to bump it, and
  nothing ties it to the request that is out. The stage that waits holds
  the `RequestId` (`ResolvingChat { request }`, `Loading { request }`) and
  compares it with the answer's.

Screenshot-demo and fixture files seed state this way and are not checked.

The calls in the tree when the check was added are in
scripts/request-match-baseline.txt, by rule and function. That list is
debt: it only shrinks. A new call fails. Keep the `RequestId` in the stage
(`Fetch::sent(request)`, `Asking { request }`) and compare it:
`fetch.answered(Some(pending.id), value)`, `fetch.waits_for(id)`. A call
that truly needs no matching goes in scripts/request-match-allow.txt with
the reason.

Run from the repository root:

    python3 scripts/check-request-match.py [--root DIR] [--list] [--prune]
"""

import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import shape_guard  # noqa: E402

BASELINE = "scripts/request-match-baseline.txt"
ALLOW = "scripts/request-match-allow.txt"
HEADER = (
    "# Answers not matched by RequestId (scripts/check-request-match.py).\n"
    "# <rule>:<Type::function> <count>. Only remove lines or lower counts.\n"
)
ADVICE = (
    "Match the answer to its request: hold the RequestId in the stage and compare it "
    "(Fetch::sent + answered/failed/waits_for), instead of asking whether any request "
    "with that purpose is in flight (docs/contributing/state-machines.md, rule 3)."
)
OWNER = "src/state/requests.rs"
PURPOSE = re.compile(r"\.\s*(?:has_purpose|take_purpose|has_auth_submit)\s*\(")
SUCCEED = re.compile(r"\.\s*succeed\s*\(")
TICKET = re.compile(r"\b(?:token|generation|ticket|seq)\s*:\s*u64\b")


def variant_of(text, at):
    """`Enum::Variant` the field at `at` belongs to (a purpose variant)."""
    enum = None
    for enum in re.finditer(r"\benum\s+(\w+)", text[:at]):
        pass
    if enum is None:
        return "@purpose"
    variant = None
    for variant in re.finditer(r"\b([A-Z]\w*)\s*\{", text[enum.end() : at]):
        pass
    return f"{enum.group(1)}::{variant.group(1) if variant else '?'}"


def purpose_enum_bodies(text):
    """`(start, end)` of the body of every `enum <Name>Purpose { ... }`."""
    for head in re.finditer(r"\benum\s+\w*Purpose\b[^{;]*\{", text):
        depth, at = 1, head.end()
        while at < len(text) and depth:
            depth += {"{": 1, "}": -1}.get(text[at], 0)
            at += 1
        yield head.end(), at


def hits(root):
    for path, text in shape_guard.source(root):
        if path == OWNER or shape_guard.is_seeder(path):
            continue
        purpose = "_purpose" in text or "has_auth_submit" in text
        succeed = ".succeed" in text
        ticket = path.startswith("src/") and bool(re.search(r"\benum\s+\w*Purpose\b", text))
        if not purpose and not succeed and not ticket:
            continue
        scopes = shape_guard.Scopes(path, text)
        if ticket:
            for start, end in purpose_enum_bodies(text):
                for match in TICKET.finditer(text, start, end):
                    line = shape_guard.line_of(text, match.start())
                    yield f"ticket:{variant_of(text, match.start())}", path, line
        if purpose:
            for match in PURPOSE.finditer(text):
                line = shape_guard.line_of(text, match.start())
                yield f"purpose:{scopes.at(match.start())}", path, line
        if succeed:
            for match in SUCCEED.finditer(text):
                line = shape_guard.line_of(text, match.start())
                yield f"unmatched:{scopes.at(match.start())}", path, line


if __name__ == "__main__":
    shape_guard.main("check-request-match", hits, BASELINE, ALLOW, HEADER, ADVICE)
