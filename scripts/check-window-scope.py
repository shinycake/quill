#!/usr/bin/env python3
"""Async work in src/ui must say which window's state it belongs to.

While a chat window is drawn or handles input, its state (composer,
history, viewer, pickers, message menu, share box, the session's view) is
swapped into the shared `QuillApp` fields (`src/ui/chat_window/slot.rs`).
A timer, picker or background job that finishes later finds whichever
window's state is in place by then, so a plain `cx.spawn` / `cx.defer`
whose closure reads or writes that state can land in the wrong window.

This check flags `cx.spawn`, `spawn_in`, `cx.defer`, `defer_in` and
`on_next_frame` calls in src/ui whose closure touches per-window state,
directly or through a `QuillApp` method that does (matched by name, so a
false hit is possible). Fix one by starting the work with
`spawn_scoped` / `spawn_scoped_in` / `defer_scoped` (it finishes in the
window it started in, and not at all once that window closed),
`QuillApp::spawn_main` / `spawn_main_in` (it belongs to no chat window),
or by calling `in_window_scope` inside the closure. Work that is really
the app's, with no window state involved, says why in a comment on one
of the three lines above the call:

    // window-scope: <why this needs no window>

Usage: python3 scripts/check-window-scope.py [src/ui directory]
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent / "src" / "ui"

START = re.compile(r"\b(cx\.spawn|\.spawn_in|cx\.defer|\.defer_in|on_next_frame)\s*\(")
# The fields `swap_slot` and `Session::swap_view` exchange, and the
# accessors that read the session's view.
WINDOW_STATE = re.compile(
    r"\.(history|composer|composer_ui|message_ui|pickers|viewer|share)\b(?!_)"
    r"|search_ui\.chat_input|\bopen_chat\b|\bopen_topic\b|threads\.thread"
    r"|saved\.(sublist|tag_search)|chat_search|shared_media"
    r"|\bopen_chat_id\(|\bviewed_chat\(|\bviewed_topic\("
)
METHOD = re.compile(r"\bfn\s+([a-z_0-9]+)\s*(<[^>]*>)?\s*\(\s*&(mut\s+)?self")
CALL = re.compile(r"\b(?:self|this|app)\.([a-z_0-9]+)\s*\(")
SCOPED = re.compile(r"\bin_window_scope\b|\bset_window_scope\b")
MARKER = re.compile(r"//\s*window-scope:\s*\S")
# The scope machinery itself and GPUI's entity methods, which every
# closure calls.
NOT_WINDOW_STATE = {
    "update",
    "update_in",
    "read_with",
    "read",
    "in_window_scope",
    "set_window_scope",
    "note_window_input",
    "swap_slot",
}


def block_after(text: str, at: int) -> str:
    start = text.find("{", at)
    if start < 0:
        return ""
    depth = 0
    for i in range(start, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[start : i + 1]
    return text[start:]


def call_args(text: str, at: int) -> str:
    depth = 1
    i = at
    while i < len(text) and depth:
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
        i += 1
    return text[at:i]


def is_test(path: pathlib.Path) -> bool:
    name = path.name
    return (
        "tests" in path.parts
        or name == "tests.rs"
        or name.endswith("_tests.rs")
        or "_tests_" in name
    )


def main() -> int:
    root = pathlib.Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT
    files = [p for p in sorted(root.rglob("*.rs")) if not is_test(p)]
    texts = {p: p.read_text() for p in files}
    bodies: dict[str, list[str]] = {}
    for text in texts.values():
        for m in METHOD.finditer(text):
            bodies.setdefault(m.group(1), []).append(block_after(text, m.end()))
    touching = {
        name
        for name, blocks in bodies.items()
        if name not in NOT_WINDOW_STATE and any(WINDOW_STATE.search(b) for b in blocks)
    }
    grew = True
    while grew:
        grew = False
        for name, blocks in bodies.items():
            if name in touching or name in NOT_WINDOW_STATE:
                continue
            if any(c in touching for b in blocks for c in CALL.findall(b)):
                touching.add(name)
                grew = True
    failures = []
    for path, text in texts.items():
        lines = text.split("\n")
        for m in START.finditer(text):
            body = call_args(text, m.end())
            if SCOPED.search(body):
                continue
            direct = sorted({x.group(0) for x in WINDOW_STATE.finditer(body)})
            via = sorted({c for c in CALL.findall(body) if c in touching})
            if not direct and not via:
                continue
            line = text.count("\n", 0, m.start()) + 1
            above = lines[max(0, line - 4) : line]
            if any(MARKER.search(l) for l in above):
                continue
            rel = path.relative_to(root.parent.parent)
            what = ", ".join(direct + [f"{c}()" for c in via][:4])
            failures.append(f"{rel}:{line}: async work touches window state ({what})")
    if failures:
        print("\n".join(failures))
        print(
            f"\n{len(failures)} async call(s) in src/ui touch per-window state without a scope.\n"
            "Start them with spawn_scoped / spawn_scoped_in / defer_scoped (the window that\n"
            "started the work), QuillApp::spawn_main / spawn_main_in (no chat window), call\n"
            "in_window_scope inside the closure, or, when no window state is involved,\n"
            "add `// window-scope: <why>` on one of the three lines above the call.\n"
            "See src/ui/chat_window/scoped.rs."
        )
        return 1
    print("window scope: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
