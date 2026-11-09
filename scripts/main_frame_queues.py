"""A queue only the main window's frame drains brings that window forward.

Some requests wait in a field that only the main window's frame reads
(`frame_upkeep`, or the main window as `resolved_link_frame`): a mini
app's switch-inline, Open Bot and checkout (`mini_apps.render_action`), a
resolved chat to open (`links.pending_deep_link_open`). A minimized or
hidden main window draws no frame, and `cx.activate(true)` does not
restore it, so a request filled from a chat window, a mini app or a call
window waited there unseen (round 17: mini app actions, the video chat's
"Share to").

Checked here, in src/ui outside tests: every `<queue> = Some(` write is
followed, within the next 12 lines, by `raise_main_window_later(` or
`hold_resolved_link(` (which raises it when another window asked), or
says why it needs neither on the line or one of the three above:

    // main-frame: <why the main window's frame runs without help>

A resolve started from src/ui goes through the same seam (round 18 B
#8): `driver.resolve_deep_link(` is called only in src/ui/deep_links.rs,
by `QuillApp::resolve_deep_link_here`, which names the asking window as
the flow's window so the resolved chat opens there and the main window
comes forward for it. A bare call left the flow with no window: the chat
opened in a minimized main window, behind the window that asked.

Run by scripts/check-window-scope.py.
"""

import os
import re

QUEUES = re.compile(
    r"\b(?:mini_apps\s*\.\s*render_action|links\s*\.\s*pending_deep_link_open)\s*=\s*Some\s*\("
)
BARE_RESOLVE = re.compile(r"\.\s*resolve_deep_link\s*\(")
RESOLVE_SEAM = "src/ui/deep_links.rs"
RAISE = re.compile(r"\b(?:raise_main_window_later|hold_resolved_link)\s*\(")
MARKER = re.compile(r"//\s*main-frame:\s*\S")
WITHIN = 12


def is_test(path):
    name = os.path.basename(path)
    return name.endswith("_tests.rs") or name == "tests.rs" or "/tests/" in path


def strip_test_modules(text):
    at = re.search(r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*mod\s+\w+\s*\{", text)
    return text if at is None else text[: at.start()]


def problems(root="."):
    out = []
    top = os.path.join(root, "src/ui")
    for dirpath, _, names in os.walk(top):
        for name in sorted(names):
            if not name.endswith(".rs"):
                continue
            path = os.path.join(dirpath, name)
            rel = os.path.relpath(path, root)
            if is_test(rel):
                continue
            with open(path, encoding="utf-8") as handle:
                text = strip_test_modules(handle.read())
            lines = text.split("\n")
            if rel != RESOLVE_SEAM:
                for m in BARE_RESOLVE.finditer(text):
                    line = text.count("\n", 0, m.start()) + 1
                    out.append(
                        f"{rel}:{line}: a deep link resolved outside its seam; call "
                        "resolve_deep_link_here (or queue_deep_link), so the flow names "
                        "the window that asked"
                    )
            for m in QUEUES.finditer(text):
                line = text.count("\n", 0, m.start()) + 1
                above = lines[max(0, line - 4) : line]
                after = "\n".join(lines[line - 1 : line - 1 + WITHIN])
                if any(MARKER.search(l) for l in above) or RAISE.search(after):
                    continue
                out.append(
                    f"{rel}:{line}: a main-frame queue filled without bringing the main window "
                    "forward; call raise_main_window_later (or hold_resolved_link) after it, or "
                    "say why with `// main-frame: <why>`"
                )
    return out
