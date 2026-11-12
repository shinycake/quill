#!/usr/bin/env python3
"""An in-flight stage holds what it waits for.

docs/contributing/state-machines.md, rules 2 and 3. Turning
`loading: bool` into `enum Load { Idle, Loading }` renames the flag and
fixes nothing: the stage still cannot tell its own answer from another
request's, and still cannot cancel its work when it is replaced. The
re-sweep of 2026-11-11 recorded 205 such renames.

This check reads every enum outside test code. A variant whose name says
work is in flight (`Loading`, `Asking`, `Sending`, `Sent`, `Waiting`,
`Pending`, `Running`, `Resolving`, ...: the `IN_FLIGHT` list below) must
hold what ends it:

- `bare`: the variant has no data at all;
- `unkeyed`: the variant has data, but no `RequestId`, `Task`,
  `JoinHandle`, `Subscription`, channel end or deadline in it, or in the
  types it names;
- `optional`: the only request in it is an `Option<RequestId>`: a stage
  that is `Loading(None)` until a second call fills the request in is the
  "asked but not sent" flag again. Hold `Idle` until the request exists
  and write `Loading { request }` together with the registry entry.

Any variant, in flight or not, is also checked for `rawid`: a field named
`request`, `extra`, `request_id`, `generation` or `serial` typed `u64`,
a counter standing in for the `RequestId` of the request the stage waits
for. A stage holds the `RequestId` itself, so a request the session's table
dropped can be tied to the stage that waits for it. (A `..Purpose` enum is
the request's own data and keeps its generation.)

The variants in the tree when the check was added are in
scripts/stage-enums-baseline.txt as `rule:Enum::Variant`. That list is
debt: it only shrinks. A new one fails. Put the request in the variant
(`Loading { request: RequestId }`, or use `Fetch<T>`, which has it) and
compare it when the answer comes; for local work hold the `Task`, so
replacing the stage cancels it. A variant that waits for nothing Quill
started (a TDLib state mirrored as is, a stage the user leaves) goes in
scripts/stage-enums-allow.txt with the reason. Renaming the variant to
dodge the word list is not a fix; reviewers reject it.

Enums under src/telegram/envelope mirror TDLib's own types and are not
checked, nor are enums named as a message, a reason or a derived view
(`..Action`, `..Event`, `..Outcome`, `..View`, ...: `NOT_A_STAGE` below).

Run from the repository root:

    python3 scripts/check-stage-enums.py [--root DIR] [--list] [--prune]
"""

import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import shape_guard  # noqa: E402

BASELINE = "scripts/stage-enums-baseline.txt"
ALLOW = "scripts/stage-enums-allow.txt"
HEADER = (
    "# In-flight stages that hold nothing to match or cancel (scripts/check-stage-enums.py).\n"
    "# <rule>:<Enum>::<Variant> <count>. Only remove lines.\n"
)
ADVICE = (
    "An in-flight variant holds what ends it: the RequestId of its request (compared when "
    "the answer comes) or the Task doing the work (docs/contributing/state-machines.md, "
    "rules 2 and 3). Fetch<T> already does."
)
MIRRORS = ("src/telegram/envelope",)
IN_FLIGHT = set(
    "Asking Asked Loading Sending Sent Fetching Resolving Working Checking Uploading "
    "Downloading Saving Pending Waiting Awaiting Requesting Requested Submitting Submitted "
    "Joining Leaving Connecting Probing Preparing Starting Stopping Opening Closing Running "
    "Busy InFlight Applying Verifying Refreshing Syncing Creating Deleting Looking Searching "
    "Importing Exporting Queued InProgress Mutating Processing Retrying Reading Writing "
    "Installing Paying Buying Transferring Upgrading Lingering Decoding Clearing "
    "Building Cutting Encoding Activating Rendering Listing Jumping Capturing".split()
)
CARRIER = re.compile(
    r"\b(?:RequestId|Task|TaskHold|RenderJob|JoinHandle|Subscription|Receiver|Sender|Instant|SystemTime)\b"
)
OPTIONAL_REQUEST = re.compile(r"Option\s*<\s*(?:[\w:]*::)?RequestId\s*>")
# Enums that name a message, a reason or a derived view, not a stored stage.
NOT_A_STAGE = re.compile(
    r"(?:View|Action|Event|Command|Refusal|Reason|Error|Outcome|Result|Label|Line|Icon|Demo|Reply)$"
)
RAW_ID = re.compile(r"\b(?:request|extra|request_id|generation|serial)\s*:\s*(?:Option\s*<\s*)?[ui]64\b")
TYPE = re.compile(r"\b(?:struct|enum)\s+([A-Z]\w*)\s*(?:<[^{;(]*>)?\s*(?:where[^{;(]*)?([({])")
ENUM = re.compile(r"\benum\s+([A-Z]\w*)\s*(?:<[^{;]*>)?\s*(?:where[^{;]*)?\{")
ATTR = re.compile(r"#\[[^\]]*\]")
VARIANT = re.compile(r"([A-Z]\w*)\s*(.*)$", re.S)
NAME = re.compile(r"\b[A-Z]\w*\b")


def variants(body):
    """`(name, data)` of each variant in an enum body."""
    depth, start = 0, 0
    parts = []
    for i, ch in enumerate(body):
        if ch in "({[<":
            depth += 1
        elif ch in ")}]>":
            depth -= 1
        elif ch == "," and depth == 0:
            parts.append(body[start:i])
            start = i + 1
    parts.append(body[start:])
    for part in parts:
        match = VARIANT.match(ATTR.sub("", part).strip())
        if match:
            data = match.group(2).strip()
            yield match.group(1), "" if data.startswith("=") else data


def carries(data, types, depth=3):
    """Whether `data` holds a request, a task or a deadline, itself or in a
    type it names."""
    if CARRIER.search(data):
        return True
    if depth == 0:
        return False
    return any(
        carries(body, types, depth - 1)
        for name in set(NAME.findall(data))
        for body in types.get(name, ())
    )


def hits(root):
    files = list(shape_guard.source(root))
    types = {}
    for _, text in files:
        for match in TYPE.finditer(text):
            brace = match.end() - 1
            body = text[brace : shape_guard.closing(text, brace) + 1]
            types.setdefault(match.group(1), []).append(body)
    for path, text in files:
        if path.startswith(MIRRORS):
            continue
        for match in ENUM.finditer(text):
            if NOT_A_STAGE.search(match.group(1)):
                continue
            brace = match.end() - 1
            body = text[brace + 1 : shape_guard.closing(text, brace)]
            offset = brace + 1
            for name, data in variants(body):
                found = re.search(rf"\b{name}\b", text[offset:])
                at = offset + (found.start() if found else 0)
                line = shape_guard.line_of(text, at)
                if RAW_ID.search(data) and not match.group(1).endswith("Purpose"):
                    yield f"rawid:{match.group(1)}::{name}", path, line
                if name not in IN_FLIGHT:
                    continue
                optional = OPTIONAL_REQUEST.findall(data)
                if not data:
                    yield f"bare:{match.group(1)}::{name}", path, line
                elif optional and len(optional) == len(re.findall(r"\bRequestId\b", data)):
                    yield f"optional:{match.group(1)}::{name}", path, line
                elif not carries(data, types):
                    yield f"unkeyed:{match.group(1)}::{name}", path, line


if __name__ == "__main__":
    shape_guard.main("check-stage-enums", hits, BASELINE, ALLOW, HEADER, ADVICE)
