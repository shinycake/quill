#!/usr/bin/env python3
"""An answer is matched to the request that asked, by its `RequestId`.

docs/contributing/state-machines.md, rule 3. A stage that waits for an
answer holds the `RequestId` of its request and takes only that answer
(`Fetch::sent` / `answered` / `failed`, `ListOp::begin`, `RequestAcks`).
Four shortcuts kept coming back in the state sweeps (about 175 entries in
the re-sweep of 2026-11-11, more in round 3), and this check counts them:

- `purpose`: a call to `Requests::has_purpose*`, `take_purpose*` or
  `has_auth_submit` outside src/state/requests.rs. "Some request with
  this purpose is in flight" is not "my request is in flight": a second
  window, an older request or the same request for another chat answers
  the question too. The busy state is then wrong, a late answer is taken
  for the current one, and an error ends the wrong stage.
- `unmatched`: a call to `Fetch::succeed`, which sets the answer whatever
  request it came from. An older request's late answer overwrites a newer
  one.

- `generation`: a call to `matches_generation`, which takes an answer
  because it carries the generation counter (`u64`) of the query on screen
  instead of the `RequestId` its stage holds. A second ask inside one
  generation (a retry, a paged answer, a supplement) is taken for the first.
- `stamped`: a `RequestKey::` outside the request constructors
  (src/state/requests.rs, src/state/session_requests.rs,
  src/state/request_key.rs). What an id-less answer is about is recorded by
  `Session::request_for_*`, one key per request; a sender that stamps its
  own key (or forgets to) leaves the answer to be dropped silently.

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
    "with that purpose is in flight or whether it carries the query's generation "
    "(docs/contributing/state-machines.md, rule 3). A request's key is set only by "
    "Session::request_for_*."
)
OWNER = "src/state/requests.rs"
PURPOSE = re.compile(r"\.\s*(?:has_purpose|take_purpose|has_auth_submit)\s*\(")
SUCCEED = re.compile(r"\.\s*succeed\s*\(")
GENERATION = re.compile(r"\.\s*matches_generation\s*\(")
KEY_OWNERS = {
    "src/state/requests.rs",
    "src/state/session_requests.rs",
    "src/state/request_key.rs",
}
STAMP = re.compile(r"\bRequestKey\s*::")


def hits(root):
    for path, text in shape_guard.source(root):
        if path == OWNER or shape_guard.is_seeder(path):
            continue
        purpose = "_purpose" in text or "has_auth_submit" in text
        succeed = ".succeed" in text
        generation = "matches_generation" in text
        stamped = "RequestKey" in text and path not in KEY_OWNERS
        if not (purpose or succeed or generation or stamped):
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
        if generation:
            for match in GENERATION.finditer(text):
                line = shape_guard.line_of(text, match.start())
                yield f"generation:{scopes.at(match.start())}", path, line
        if stamped:
            for match in STAMP.finditer(text):
                line = shape_guard.line_of(text, match.start())
                yield f"stamped:{scopes.at(match.start())}", path, line


if __name__ == "__main__":
    shape_guard.main("check-request-match", hits, BASELINE, ALLOW, HEADER, ADVICE)
