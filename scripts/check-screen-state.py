#!/usr/bin/env python3
"""Every `QuillApp` field is the process's, or the screen's.

docs/decisions/account-scoped-ui-state.md: the UI of the account on screen
lives in one value, `OnScreen` (src/ui/on_screen.rs), declared through
`on_screen_fields!` so that every field names its goodbye. `QuillApp`
keeps only the process's state, and each such field is listed in
scripts/app-wide-state.txt with the reason it survives an account switch.

This check fails when a `QuillApp` field is neither infrastructure nor
listed, when the list names a field that is gone, or when `OnScreen` is
declared outside the macro. Run from the repository root:

    python3 scripts/check-screen-state.py
"""

import re
import sys

APP = "src/ui/app.rs"
SCREEN = "src/ui/on_screen.rs"
LIST = "scripts/app-wide-state.txt"

# The fields that make the design work, rather than hold state of their own.
INFRASTRUCTURE = {"screen", "live", "demo_session", "credentials"}

FIELD = re.compile(r"^\s{4}pub(?:\([^)]*\))?\s+([a-z_][a-z0-9_]*)\s*:", re.M)


def struct_body(text, name):
    start = text.find(f"struct {name} {{")
    if start < 0:
        return None
    depth = 0
    for i in range(start, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[start:i]
    return None


def app_fields():
    with open(APP, encoding="utf-8") as f:
        body = struct_body(f.read(), "QuillApp")
    if body is None:
        sys.exit(f"{APP}: no `struct QuillApp`")
    return FIELD.findall(body)


def listed_fields():
    out = {}
    with open(LIST, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            name, _, reason = line.partition("|")
            name, reason = name.strip(), reason.strip()
            if not reason:
                sys.exit(f"{LIST}:{n}: `{name}` needs a reason after `|`")
            out[name] = reason
    return out


def screen_declared_by_macro():
    with open(SCREEN, encoding="utf-8") as f:
        text = f.read()
    if "on_screen_fields! {" not in text:
        return "no `on_screen_fields! {` call"
    # The struct's only declaration is the one inside the macro's body.
    outside = [
        m.start()
        for m in re.finditer(r"^pub(?:\([^)]*\))?\s+struct OnScreen\b", text, re.M)
    ]
    if outside:
        return "`struct OnScreen` is declared outside `on_screen_fields!`"
    return None


def main():
    failed = False
    fields = app_fields()
    listed = listed_fields()
    for name in fields:
        if name in INFRASTRUCTURE or name in listed:
            continue
        print(
            f"{APP}: `{name}` is a QuillApp field but neither infrastructure nor "
            f"listed in {LIST}. The account's UI goes in OnScreen "
            f"({SCREEN}); the process's state is listed with its reason."
        )
        failed = True
    for name in listed:
        if name not in fields:
            print(f"{LIST}: `{name}` is listed but QuillApp has no such field")
            failed = True
    problem = screen_declared_by_macro()
    if problem:
        print(f"{SCREEN}: {problem}")
        failed = True
    if failed:
        sys.exit(1)
    print(f"screen state: OK ({len(fields)} QuillApp fields, {len(listed)} listed)")


if __name__ == "__main__":
    main()
