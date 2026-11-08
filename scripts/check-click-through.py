#!/usr/bin/env python3
"""Reject hand-built click-to-dismiss layers in the UI.

GPUI sends a click to every hitbox under the pointer down to the first one
that occludes. A full-window element with its own click or press handler
(a backdrop that closes something) therefore also fires when the panel drawn
over it is clicked, unless that panel occludes, and it lets the click reach
what is behind it unless it occludes itself. `DismissLayer`
(src/ui/dismiss_layer.rs) arranges both; this check makes sure new layers use
it.

It flags any `div()` builder chain under src/ui that covers its parent
(`.absolute()` with `.inset_0()`, `.size_full()` or all four edges at 0) and
takes a click or a press itself (`.on_click(`, `.on_mouse_down(`,
`.on_mouse_up(`). A chain that is not a dismiss layer (a drag catcher, a
resize handle) is allowed with a `// click-through-ok: <reason>` comment on
one of the three lines above the `div()`.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent / "src" / "ui"
SKIP = {"dismiss_layer.rs"}
MARK = "click-through-ok:"
HANDLERS = {"on_click", "on_mouse_down", "on_mouse_up"}
START = re.compile(r"\bdiv\(\)")
CALL = re.compile(r"\s*(?://[^\n]*\n\s*)*\.([a-z_0-9]+)\s*(?:::<[^>]*>)?\(")


def skip_args(text, i):
    """`text[i]` is `(`: the index after its matching `)`."""
    depth = 0
    while i < len(text):
        c = text[i]
        if c == '"':
            i += 1
            while i < len(text) and text[i] != '"':
                i += 2 if text[i] == "\\" else 1
        elif c == "/" and text.startswith("//", i):
            i = text.find("\n", i)
            if i < 0:
                return len(text)
        elif c == "'" and re.match(r"'(\\.|[^\\'])'", text[i:]):
            i += len(re.match(r"'(\\.|[^\\'])'", text[i:]).group(0)) - 1
        elif c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return len(text)


def chain_calls(text, i):
    """The method names called directly on the builder starting at `i`."""
    names = []
    while True:
        m = CALL.match(text, i)
        if not m:
            return names
        names.append(m.group(1))
        i = skip_args(text, m.end() - 1)


def covers(names):
    if "absolute" not in names:
        return False
    if "inset_0" in names or "size_full" in names:
        return True
    return {"top_0", "left_0", "right_0", "bottom_0"} <= set(names)


def is_test(path):
    return path.name.endswith("_tests.rs") or "tests" in path.parts or path.name == "tests.rs"


def main():
    problems = []
    for path in sorted(ROOT.rglob("*.rs")):
        if path.name in SKIP or is_test(path):
            continue
        text = path.read_text()
        # Production code only: stop at the first test module.
        cut = re.search(r"\n#\[cfg\(test\)\]", text)
        if cut:
            text = text[: cut.start()]
        lines = text.split("\n")
        for m in START.finditer(text):
            names = chain_calls(text, m.end())
            if not covers(names) or not HANDLERS & set(names):
                continue
            line = text.count("\n", 0, m.start())
            above = lines[max(0, line - 3) : line + 1]
            if any(MARK in l for l in above):
                continue
            rel = path.relative_to(ROOT.parent.parent)
            problems.append(f"{rel}:{line + 1}")
    if problems:
        print("Hand-built click-to-dismiss layers (use DismissLayer, src/ui/dismiss_layer.rs):")
        for p in problems:
            print(f"  {p}")
        print(f"Not a dismiss layer? Add `// {MARK} <reason>` above the div().")
        return 1
    print("click-through check OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
