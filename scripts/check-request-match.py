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
- `counter`: a `generation`, `epoch` or `token` number (`uN` or
  `Option<uN>`) in a variant of a `*Purpose` enum, listed by enum and
  variant. A counter in the purpose stands in for the `RequestId`: the
  stage has to bump it by hand to tell its own answer from an older one,
  a stage that forgets answers the wrong request, and a duplicate request
  for the same stamp cannot be told apart (r3 sweep: shared-media tabs,
  deep links and chat-search jumps all carried one). Hold the `RequestId`
  in the stage instead (`Loading { request }`; `LanguageSwitch` holds one
  per ask) and compare the answer's.
- `unsent`: a call to `Fetch::begin()` with no `.sent(request)` after it in
  the same function. The stage reads Loading but holds no request, so
  `answered` and `failed` can never take its own answer and every answer is
  matched by purpose instead (round 3 found it again in six senders).

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
# In UI code the per-chat variant is the same shortcut: a box reading "a
# request of this purpose for my chat is out" is true for another window's.
# The driver's own double-send refusal (`has_purpose_for_chat` in src/connect)
# is not an answer match.
PURPOSE_FOR_CHAT = re.compile(r"\.\s*(?:has_purpose|take_purpose)_for_chat\s*\(")
SUCCEED = re.compile(r"\.\s*succeed\s*\(")
BEGIN = re.compile(r"\.\s*begin\s*\(\s*\)")
SENT = re.compile(r"\.\s*sent\s*\(")
AFTER = 400
PURPOSE_ENUM = re.compile(r"\benum\s+(\w*Purpose)\b[^{;]*\{")
VARIANT = re.compile(r"\b([A-Z]\w*)\s*[{(]")
COUNTER = re.compile(
    r"\b(?:generation|epoch|token)\s*:\s*(?:Option\s*<\s*)?u(?:8|16|32|64)\b"
)


def counter_hits(text):
    """(enum, variant, offset) of every counter field in a `*Purpose` enum's variants."""
    for enum in PURPOSE_ENUM.finditer(text):
        end = shape_guard.closing(text, enum.end() - 1)
        pos = enum.end()
        while True:
            variant = VARIANT.search(text, pos, end)
            if not variant:
                break
            close = shape_guard.closing(text, variant.end() - 1)
            for match in COUNTER.finditer(text, variant.end(), close):
                yield enum.group(1), variant.group(1), match.start()
            pos = close + 1


def hits(root):
    for path, text in shape_guard.source(root):
        if path == OWNER or shape_guard.is_seeder(path):
            continue
        purpose = "_purpose" in text or "has_auth_submit" in text
        succeed = ".succeed" in text
        counter = "Purpose" in text and COUNTER.search(text)
        begin = ".begin(" in text
        if not purpose and not succeed and not counter and not begin:
            continue
        scopes = shape_guard.Scopes(path, text)
        if counter:
            for enum, variant, offset in counter_hits(text):
                yield f"counter:{enum}::{variant}", path, shape_guard.line_of(text, offset)
        if purpose:
            patterns = [PURPOSE]
            if path.startswith("src/ui/"):
                patterns.append(PURPOSE_FOR_CHAT)
            for pattern in patterns:
                for match in pattern.finditer(text):
                    line = shape_guard.line_of(text, match.start())
                    yield f"purpose:{scopes.at(match.start())}", path, line
        if succeed:
            for match in SUCCEED.finditer(text):
                line = shape_guard.line_of(text, match.start())
                yield f"unmatched:{scopes.at(match.start())}", path, line
        if begin:
            for match in BEGIN.finditer(text):
                if SENT.search(text, match.end(), match.end() + AFTER):
                    continue
                line = shape_guard.line_of(text, match.start())
                yield f"unsent:{scopes.at(match.start())}", path, line


def self_test():
    def found(variants):
        text = "pub enum ChatsPurpose {\n" + variants + "}\n"
        return [(v, shape_guard.line_of(text, o)) for _, v, o in counter_hits(text)]

    assert found("    DeepLinkInfo { generation: u64 },\n") == [("DeepLinkInfo", 2)]
    assert found("    Jump {\n        generation: u64,\n    },\n") == [("Jump", 3)]
    assert found("    Labels { epoch: Option<u32> },\n    Link { token: u8 },\n") == [
        ("Labels", 2),
        ("Link", 3),
    ]
    assert not found("    Plain { request: u64 },\n    Id(u64),\n    Unit,\n")
    assert not list(counter_hits("pub enum Stage { Jump { generation: u64 } }\n"))


if __name__ == "__main__":
    self_test()
    shape_guard.main("check-request-match", hits, BASELINE, ALLOW, HEADER, ADVICE)
