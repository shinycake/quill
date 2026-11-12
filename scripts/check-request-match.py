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

- `generation`: a request purpose variant that carries `generation: u64`.
  A hand-bumped counter stamped on the purpose stands in for the
  `RequestId` the stage should hold (r3 sweep: shared-media tabs, deep
  links and chat-search jumps all carried one, and a duplicate request for
  the same stamp could not be told apart). Put the `RequestId` in the
  stage (`Loading { request }`) and compare the answer's.

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
PURPOSE_FILE = re.compile(r"^src/state/(?:request_purpose\.rs|domains/[^/]+/purpose\.rs)$")
VARIANT = re.compile(r"^\s{4}([A-Z]\w*)\s*\{")
GENERATION_FIELD = re.compile(r"^\s+generation\s*:\s*u64\b")
GENERATION_INLINE = re.compile(r"\{[^}]*\bgeneration\s*:\s*u64\b")


def generation_hits(text):
    """(variant, line number) of every purpose variant carrying `generation: u64`."""
    variant = ""
    for number, line in enumerate(text.splitlines(), 1):
        found = VARIANT.match(line)
        if found:
            variant = found.group(1)
            if GENERATION_INLINE.search(line):
                yield variant, number
        elif GENERATION_FIELD.match(line):
            yield variant, number


def hits(root):
    for path, text in shape_guard.source(root):
        if PURPOSE_FILE.match(path):
            for variant, number in generation_hits(text):
                yield f"generation:{variant}", path, number
        if path == OWNER or shape_guard.is_seeder(path):
            continue
        purpose = "_purpose" in text or "has_auth_submit" in text
        succeed = ".succeed" in text
        if not purpose and not succeed:
            continue
        scopes = shape_guard.Scopes(path, text)
        if purpose:
            for match in PURPOSE.finditer(text):
                line = shape_guard.line_of(text, match.start())
                yield f"purpose:{scopes.at(match.start())}", path, line
        if succeed:
            for match in SUCCEED.finditer(text):
                line = shape_guard.line_of(text, match.start())
                yield f"unmatched:{scopes.at(match.start())}", path, line


def self_test():
    one_line = "    DeepLinkInfo { generation: u64 },\n"
    multi = "    Jump {\n        generation: u64,\n    },\n"
    assert list(generation_hits(one_line)) == [("DeepLinkInfo", 1)]
    assert list(generation_hits(multi)) == [("Jump", 2)]
    assert not list(generation_hits("    Plain { request: u64 },\n"))


if __name__ == "__main__":
    self_test()
    shape_guard.main("check-request-match", hits, BASELINE, ALLOW, HEADER, ADVICE)
