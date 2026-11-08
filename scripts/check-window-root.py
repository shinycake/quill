#!/usr/bin/env python3
"""Reject windows opened without the kit's Root.

App-wide handlers run for whichever window an event lands in: keystroke
interceptors ask that window whether a kit dialog is open, and kit toasts
and dialogs live in its per-window state. A window whose root view is not
`gpui_kit::component::Root` has no such state, and the first of those calls
panics (issue #1401: a click on a notification window crashed Quill on
Windows). So every `cx.open_window(` under src builds its root with
`Root::new`.

A call that cannot (it hands over a root made elsewhere) is allowed with a
`// window-root-ok: <reason>` comment on one of the three lines above it.
Test files (`tests.rs`, `*_tests.rs`, anything under a `tests/` folder) and
code after a file's `#[cfg(test)]` module start are skipped.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent / "src"
MARK = "window-root-ok:"
CALL = re.compile(r"\bcx\s*\.open_window\(")
TEST_MOD = re.compile(r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*mod\s+\w+\s*\{")


def call_args(text, i):
    """`text[i]` is `(`: the text up to its matching `)`."""
    depth = 0
    start = i
    while i < len(text):
        c = text[i]
        if c == '"':
            i += 1
            while i < len(text) and text[i] != '"':
                i += 2 if text[i] == "\\" else 1
        elif c == "/" and text.startswith("//", i):
            i = text.find("\n", i)
            if i < 0:
                return text[start:]
        elif c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return text[start : i + 1]
        i += 1
    return text[start:]


def is_test_file(path):
    rel = path.relative_to(ROOT)
    return (
        path.name == "tests.rs"
        or path.name.endswith("_tests.rs")
        or "tests" in rel.parts[:-1]
    )


def main():
    failures = []
    for path in sorted(ROOT.rglob("*.rs")):
        if is_test_file(path):
            continue
        text = path.read_text(encoding="utf-8")
        test_mod = TEST_MOD.search(text)
        end = test_mod.start() if test_mod else len(text)
        for match in CALL.finditer(text, 0, end):
            args = call_args(text, match.end() - 1)
            if "Root::new" in args:
                continue
            line = text.count("\n", 0, match.start()) + 1
            above = text.splitlines()[max(0, line - 4) : line]
            if any(MARK in l for l in above):
                continue
            failures.append(f"{path.relative_to(ROOT.parent)}:{line}")
    if failures:
        print("Windows opened without gpui_kit::component::Root:")
        for failure in failures:
            print(f"  {failure}")
        print(f"Wrap the root view in Root::new, or add `// {MARK} <reason>`.")
        return 1
    print("check-window-root: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
