"""Every toast names its window through the Asker.

A toast shows in one window (src/ui/toast_queue.rs). Round 15 (area B)
found toasts going to a copy of the last press (`ConnectionUi::note_window`)
while the boxes of the same flow went to the window that asked
(`QuillApp::asking_window`): a bot's answer to a button pressed in a chat
window toasted in the main window when the user had clicked there since.
Now `QuillApp::set_note` takes the window from the Asker, as `open_box`
does, and that is the only way a note gets its window.

Checked here, in src/ui outside tests:

1. `set_note_in(` and `toasts.push_in(` (a note or a toast given its window
   by hand) appear only in src/ui/connection_state.rs, or say why on the
   line or one of the three above:

       // note-host: <why this toast names its own window>

2. `windows.last_press` (the window of the last press, read past the
   Asker) is read only in src/ui/chat_window/, or says why the same way.
   Code that runs away from the window that asked carries that window and
   runs under `as_asked_by(window, ..)`; where no window asked, the Asker
   falls back to the last press itself (`asking_window_or_last_press`).

Run by scripts/check-window-scope.py.
"""

import os
import re

HAND_HOST = re.compile(r"\b(?:set_note_in|toasts\s*\.\s*push_in)\s*\(")
LAST_PRESS = re.compile(r"\bwindows\s*\.\s*last_press\b")
MARKER = re.compile(r"//\s*note-host:\s*\S")
SEAM = "src/ui/connection_state.rs"
WINDOWS_DIR = "src/ui/chat_window/"


def is_test(path):
    name = os.path.basename(path)
    return name.endswith("_tests.rs") or name == "tests.rs" or "/tests/" in path


def sources(root):
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
                yield rel, handle.read()


def strip_test_modules(text):
    """The text before the first `#[cfg(test)] mod tests {` block."""
    at = re.search(r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*mod\s+\w+\s*\{", text)
    return text if at is None else text[: at.start()]


def marked(lines, line):
    above = lines[max(0, line - 4) : line]
    return any(MARKER.search(l) for l in above)


def problems(root="."):
    out = []
    for rel, text in sources(root):
        text = strip_test_modules(text)
        lines = text.split("\n")
        if rel != SEAM:
            for m in HAND_HOST.finditer(text):
                line = text.count("\n", 0, m.start()) + 1
                if not marked(lines, line):
                    out.append(
                        f"{rel}:{line}: a toast given its window by hand; set the note with "
                        "QuillApp::set_note (the window that asked), under as_asked_by(window, ..) "
                        "for a flow that carried its window"
                    )
        if not rel.startswith(WINDOWS_DIR):
            for m in LAST_PRESS.finditer(text):
                line = text.count("\n", 0, m.start()) + 1
                if not marked(lines, line):
                    out.append(
                        f"{rel}:{line}: reads the window of the last press past the Asker; use "
                        "asking_window() / asking_window_or_last_press()"
                    )
    return out


if __name__ == "__main__":
    import sys

    found = problems(sys.argv[1] if len(sys.argv) > 1 else ".")
    print("\n".join(found) if found else "note hosts: OK")
    sys.exit(1 if found else 0)
