#!/usr/bin/env python3
"""No new unmatched `Fetch` answers.

docs/contributing/state-machines.md rule 3: an answer is applied only when
it matches what asked. `Fetch::succeed` / `fail` / `fail_dropping_data`
(src/state/fetch.rs) take no request id, so an older request's late answer
lands on a newer ask after `begin()` or `outdate()`. A fetch that is asked
from a sender takes its answer with `Fetch::answered(request, ..)` and
`Fetch::failed(request, ..)`, after the sender recorded the request with
`Fetch::begin()` and `Fetch::sent(request)`.

This check counts the textual `.succeed(` / `.fail(` / `.fail_dropping_data(`
calls per source file (test files and src/state/fetch.rs excluded). The
count of each file at the time the check was added is in
scripts/fetch-unmatched-baseline.txt. Those are debt: a count may only
shrink: a file that gains a call fails, and a file that migrates lowers its
line when it can. Pushed updates carry no request and may call `succeed`;
say so in the review.

Run from the repository root:

    python3 scripts/check-fetch-answers.py [--root DIR] [--write-baseline]
"""

import os
import re
import sys

BASELINE = "scripts/fetch-unmatched-baseline.txt"
CALL = re.compile(r"\.(succeed|fail|fail_dropping_data)\(")


def is_test_file(path):
    name = os.path.basename(path)
    return (
        name.endswith("_tests.rs")
        or name == "tests.rs"
        or "/tests/" in path
        or path == "src/state/fetch.rs"
    )


def count_calls(root):
    counts = {}
    for scan in ("src", "crates"):
        for base, _dirs, files in os.walk(os.path.join(root, scan)):
            for name in files:
                if not name.endswith(".rs"):
                    continue
                full = os.path.join(base, name)
                rel = os.path.relpath(full, root).replace(os.sep, "/")
                if is_test_file(rel):
                    continue
                try:
                    text = open(full, encoding="utf-8").read()
                except OSError:
                    continue
                found = len(CALL.findall(text))
                if found:
                    counts[rel] = found
    return counts


def read_baseline(root):
    path = os.path.join(root, BASELINE)
    baseline = {}
    if os.path.exists(path):
        for line in open(path, encoding="utf-8"):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            name, _, count = line.rpartition(" ")
            baseline[name] = int(count)
    return baseline


def main():
    root = "."
    args = sys.argv[1:]
    if "--root" in args:
        root = args[args.index("--root") + 1]
    counts = count_calls(root)
    if "--write-baseline" in args:
        with open(os.path.join(root, BASELINE), "w", encoding="utf-8") as out:
            out.write(
                "# Files that call Fetch::succeed / fail / fail_dropping_data (no request id).\n"
                "# Debt: counts only shrink. Migrate a file to answered / failed with the\n"
                "# request (scripts/check-fetch-answers.py), then lower or drop its line.\n"
            )
            for name in sorted(counts):
                out.write(f"{name} {counts[name]}\n")
        return 0
    baseline = read_baseline(root)
    problems = []
    for name, count in sorted(counts.items()):
        allowed = baseline.get(name, 0)
        if count > allowed:
            problems.append(
                f"{name}: {count} unmatched Fetch answer call(s), baseline {allowed}: "
                "use Fetch::answered(request, ..) / Fetch::failed(request, ..) with the "
                "request the sender recorded via Fetch::sent(request)"
            )
    if problems:
        print("\n".join(problems))
        return 1
    total = sum(counts.values())
    print(f"check-fetch-answers: ok ({total} baseline calls in {len(counts)} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
