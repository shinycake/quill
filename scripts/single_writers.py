"""A value replaced whole has one writer.

Imported by scripts/check-screen-state.py. Some values carry what their
replacement must do: replacing `ChatUi` ends its chat visit and drops the
visit's parked plays (`replace_chat_ui`), and replacing the Group Settings
box clears the invite pages of the chat it showed
(`set_group_settings_dialog`). Round 15 of the review found both done by
hand at some writers and missed at others.

scripts/single-writers.txt lists `field | path::fn | reason`. Outside
tests, a whole write of `field` (an `=` assignment, `mem::replace` or
`mem::take` of it, or `field.take()`) is allowed only inside `fn` in
`path`. Reads and writes of a sub-field (`field.x = ..`) are not whole
writes.
"""

import os
import re

LIST = "scripts/single-writers.txt"
FN = re.compile(r"\bfn\s+([a-z_][a-z0-9_]*)")


def listed(path=LIST):
    entries = []
    with open(path, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = [part.strip() for part in line.split("|")]
            if len(parts) != 3 or "::" not in parts[1] or not parts[2]:
                entries.append((n, None, None, None))
                continue
            file, fn = parts[1].rsplit("::", 1)
            entries.append((n, parts[0], file, fn))
    return entries


def write_pattern(field):
    """A whole write of `field`: `<..>.field =` (not `==`), `mem::replace` /
    `mem::take` of it, or `<..>.field.take()`."""
    name = re.escape(field)
    return re.compile(
        rf"\.{name}\s*=(?!=)"
        rf"|mem::(?:replace|take)\(\s*&mut\s+\*?[A-Za-z_][A-Za-z0-9_.]*\.{name}\s*[,)]"
        rf"|\.{name}\.take\(\)"
    )


def enclosing_fn(text, pos):
    found = None
    for match in FN.finditer(text, 0, pos):
        found = match.group(1)
    return found


def sources(root="src"):
    for base, _, names in os.walk(root):
        for name in sorted(names):
            if not name.endswith(".rs") or name.endswith("tests.rs"):
                continue
            path = os.path.join(base, name)
            if f"{os.sep}tests{os.sep}" in path:
                continue
            yield path


def strip_test_mods(text):
    """Drop `#[cfg(test)] mod .. { .. }` blocks so in-file tests may write."""
    out = text
    for match in reversed(list(re.finditer(r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*mod\s+\w+\s*\{", text))):
        depth, i = 1, match.end()
        while i < len(text) and depth:
            depth += {"{": 1, "}": -1}.get(text[i], 0)
            i += 1
        out = out[: match.start()] + " " * (i - match.start()) + out[i:]
    return out


def writer_problems(files, entries):
    """`files`: {path: text}. `entries`: from `listed`."""
    out = []
    for n, field, file, fn in entries:
        if field is None:
            out.append(f"{LIST}:{n}: expected `field | path::fn | reason`")
            continue
        if file not in files:
            out.append(f"{LIST}:{n}: `{file}` does not exist")
            continue
        if not re.search(rf"\bfn\s+{re.escape(fn)}\b", files[file]):
            out.append(f"{LIST}:{n}: `{file}` has no `fn {fn}`")
        pattern = write_pattern(field)
        for path, text in sorted(files.items()):
            body = strip_test_mods(text)
            for match in pattern.finditer(body):
                if path == file and enclosing_fn(body, match.start()) == fn:
                    continue
                line = body.count("\n", 0, match.start()) + 1
                out.append(
                    f"{path}:{line}: `{field}` is replaced whole outside its one writer "
                    f"`{fn}` ({file}); call the writer, so what the replacement must do runs"
                )
    return out


MENU_CLOSE = re.compile(r"\bmessage_ui\.menu\s*=\s*None\b")


def menu_close_problems(files):
    """The message menu closes through `close_message_menu` only: it also
    drops the reaction strip's request (`reset_message_reactions`), which a
    bare `message_ui.menu = None` leaves for a late answer to land on."""
    out = []
    for path, text in sorted(files.items()):
        if "/demos/" in path:
            continue
        body = strip_test_mods(text)
        for match in MENU_CLOSE.finditer(body):
            if enclosing_fn(body, match.start()) == "close_message_menu":
                continue
            line = body.count("\n", 0, match.start()) + 1
            out.append(
                f"{path}:{line}: `message_ui.menu = None` skips the reaction reset; "
                "call `close_message_menu()`"
            )
    return out


def problems():
    files = {}
    for path in sources():
        with open(path, encoding="utf-8") as f:
            files[path] = f.read()
    entries = listed()
    return writer_problems(files, entries) + menu_close_problems(files), len(entries)
