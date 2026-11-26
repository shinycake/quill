#!/usr/bin/env python3
"""Reject feature code that asks which window layout is on.

The window layout (Classic or Floating, `src/layout_style.rs`) is decided in
one place. Render sites read numbers from `chrome().metrics` or pass their
element through a frame helper in `src/ui/chrome/`; they never branch on the
layout. So outside the files below, naming `LayoutStyle::` or one of the
metrics' enums fails: a new bar calls `chrome().bar(..)`, a new card calls
`chrome().list_card(..)` or `panel_card(..)`, and is right in both layouts
without knowing either (docs/decisions/floating-layout.md).

Test files (`tests.rs`, `*_tests.rs`, anything under a `tests/` folder) are
skipped.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "src"

NAMES = [
    "LayoutStyle",
    "Look",
    "SelectedRow",
    "RowScale",
    "MenuEnter",
    "PageSwitch",
    "Elevation",
    "Edge",
]
USE = re.compile(r"\b(" + "|".join(NAMES) + r")::")
# `LayoutStyle` names nothing else; the metrics' enum names (`Edge`, `Look`)
# are common, so they count only in a file that reaches `layout_style`.
ONLY_CHOICE = re.compile(r"\b(LayoutStyle)::")

# Files and folders that decide the look (paths relative to the repo root).
ALLOW = [
    "src/layout_style.rs",
    "src/settings/appearance.rs",
    "src/density.rs",
    "src/ui/chrome/",
    "src/ui/shell_layout/",
    "src/ui/app_slice.rs",
    "src/ui/app_slice/",
    "src/ui/conversation/conversation_part1.rs",
    "src/ui/conversation/top_bars.rs",
    "src/ui/chat_row/row_ink.rs",
    "src/ui/appearance/",
    "src/ui/wallpaper.rs",
    "src/ui/sidebar_reveal.rs",
    "src/ui/panel_reveal.rs",
    "src/ui/chat_list/folder_slide.rs",
]


def is_test_file(path):
    rel = path.relative_to(SRC)
    return (
        path.name == "tests.rs"
        or path.name.endswith("_tests.rs")
        or "tests" in rel.parts[:-1]
    )


def allowed(rel):
    return any(rel == a or (a.endswith("/") and rel.startswith(a)) for a in ALLOW)


def main():
    hits = []
    for path in sorted(SRC.rglob("*.rs")):
        if is_test_file(path):
            continue
        rel = path.relative_to(ROOT).as_posix()
        if allowed(rel):
            continue
        text = path.read_text(encoding="utf-8")
        pattern = USE if "layout_style" in text else ONLY_CHOICE
        for n, line in enumerate(text.splitlines(), 1):
            code = line.split("//", 1)[0]
            m = pattern.search(code)
            if m:
                hits.append(f"{rel}:{n}: names {m.group(1)}::")
    if hits:
        print("Feature code asks which layout is on. Use chrome().metrics or a")
        print("frame helper in src/ui/chrome/ instead (docs/decisions/floating-layout.md):")
        for hit in hits:
            print("  " + hit)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
