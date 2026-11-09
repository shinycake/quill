"""Every kit dialog open names the window it belongs to.

Each window shows its own kit dialogs (src/ui/chat_window/dialog_hosts.rs).
A box whose open flag lives in the window's own state (`chat_ui`,
`message_ui`, `composer_ui`, `pickers`, `viewer`, `share`, `history`, all
swapped in with the slot) is per window: each window shows its own. Every
other box has one flag for the account and shows in the one window that
asked for it, which the open names as it sets the flag. Round 13 (area B)
found the old rule, "the window of the last input", sending a Mute box to
the window that showed the first one and an invite box to the window the
pointer had crossed.

Checked here:

1. A `register_dialogs!` spec says `.per_window()` exactly when its open
   flag reads the window's own state (directly, or through `self.` /
   `app.` helpers and `fn x(app: &QuillApp)` helpers up to three calls
   deep).
2. In src/ui, every write that opens a session-wide box (an assignment to
   its flag that is not `None` / `false`, or `.replace(` /
   `.get_or_insert*(` / `.insert(` on it) sits in a function that names the
   window: `open_box(DialogKind::<Name>)` (the window in place) or
   `open_box_in(DialogKind::<Name>, window)` (an answer's flow window). A
   write that only refreshes a box where it already shows, or a demo that
   stages one, says so on the line or one of the three above:

       // box-host: <why this write names no window>

3. A session-wide box whose flag only the session sets (src/state,
   src/connect: an answer) has an `open_box*` naming it in src/ui (the
   request, which knows its window), or a `Name | answer | <reason>` line in
   scripts/dialog-hosts.txt.
4. An `open_box*` names a session-wide kind (a per-window one needs none).

A flag is found by the field paths its predicate reads (the last two
names, `notify.mute_menu_open`). A predicate that hides them (a closure
argument, a negation) gives its paths in scripts/dialog-hosts.txt as
`Name | keys | a.b c.d`; `a.b()` there is a method that opens the box
(`gift_market.push()`).

Known limits: a write through a local binding (`let boxes = &mut
self.call_boxes; boxes.confirm = ..`) is not seen, and the enclosing
function is the nearest `fn` above the write.

Run by scripts/check-window-scope.py.
"""

import os
import re

LIST = "scripts/dialog-hosts.txt"
# The fields `swap_slot` exchanges (src/ui/chat_window/slot.rs), as the
# predicates name them.
WINDOW_FIELDS = r"history|composer|composer_ui|chat_ui|message_ui|pickers|viewer|share"
WINDOW_READ = re.compile(rf"\b(?:app|self)\s*\.\s*(?:{WINDOW_FIELDS})\b")
SELF_CALL = re.compile(r"\b(?:self|app)\.([a-z_][a-z0-9_]*)\(")
FREE_CALL = re.compile(r"\b([a-z_][a-z0-9_]*)\(\s*(?:self|app)\s*\)")
OPEN_CALL = re.compile(r"\bopen_box(?:_in)?\(\s*(?:[A-Za-z_][\w:]*::)?DialogKind::([A-Za-z0-9_]+)\b")
MARKER = re.compile(r"//\s*box-host:\s*\S")
METHODS = {
    "is_some", "is_none", "is_some_and", "is_none_or", "as_ref", "as_deref", "is_empty",
    "map", "and_then", "contains", "get", "iter", "any", "then", "unwrap_or",
}


def balanced(text, start, open_ch, close_ch):
    depth = 1
    for i in range(start, len(text)):
        if text[i] == open_ch:
            depth += 1
        elif text[i] == close_ch:
            depth -= 1
            if depth == 0:
                return i
    return None


def is_test(path):
    name = os.path.basename(path)
    return "test" in name or "/tests/" in path or "tests" in path.split(os.sep)


def is_demo(path):
    return "demo" in os.path.basename(path) or "/demos/" in path


def sources(root, top):
    for base, _, names in os.walk(os.path.join(root, top)):
        for name in sorted(names):
            path = os.path.join(base, name)
            if name.endswith(".rs") and not is_test(path):
                with open(path, encoding="utf-8") as f:
                    yield os.path.relpath(path, root), f.read()


def specs(ui):
    """name -> (path, predicate, per_window)"""
    out = {}
    for path, text in ui:
        for m in re.finditer(r"^\s*([A-Z][A-Za-z0-9_]*)\s*=>\s*DialogSpec::new\(", text, re.M):
            if text[: m.start()].rstrip().endswith("///"):
                continue
            end = balanced(text, m.end(), "(", ")")
            if end is None:
                continue
            args = text[m.end() : end]
            depth, parts, cur = 0, [], []
            for ch in args:
                if ch in "([{":
                    depth += 1
                elif ch in ")]}":
                    depth -= 1
                if ch == "," and depth == 0:
                    parts.append("".join(cur))
                    cur = []
                else:
                    cur.append(ch)
            parts.append("".join(cur))
            if len(parts) < 2:
                continue
            # The chained calls after `new(..)`, up to the entry's comma.
            rest, i = "", end + 1
            while i < len(text):
                tail = text[i:]
                stripped = tail.lstrip()
                skip = len(tail) - len(stripped)
                if stripped.startswith("."):
                    paren = stripped.find("(")
                    close = balanced(stripped, paren + 1, "(", ")") if paren != -1 else None
                    if close is None:
                        break
                    rest += stripped[: close + 1]
                    i += skip + close + 1
                    continue
                break
            if "//" in m.group(0):
                continue
            out[m.group(1)] = (path, parts[1], ".per_window()" in rest)
    return out


def functions(ui):
    """fn name -> bodies, for `self` methods and `fn x(app: &QuillApp)` helpers."""
    out = {}
    for _, text in ui:
        for m in re.finditer(r"\bfn ([a-z_][a-z0-9_]*)\s*(?:<[^>(]*>)?\(", text):
            brace = text.find("{", m.end())
            semi = text.find(";", m.end())
            if brace == -1 or (semi != -1 and semi < brace):
                continue
            end = balanced(text, brace + 1, "{", "}")
            if end is not None:
                out.setdefault(m.group(1), []).append(text[brace + 1 : end])
    return out


BARE = re.compile(r"^\s*(?:[A-Za-z_][\w]*::)*([a-z_][a-z0-9_]*)\s*$")


def reads_window(code, fns, depth=0, seen=None):
    bare = BARE.match(code)
    if bare and depth == 0:
        code = "\n".join(fns.get(bare.group(1), []))
    if WINDOW_READ.search(code):
        return True
    if depth >= 3:
        return False
    seen = set() if seen is None else seen
    for name in SELF_CALL.findall(code) + FREE_CALL.findall(code):
        if name in seen or name in METHODS:
            continue
        seen.add(name)
        if any(reads_window(body, fns, depth + 1, seen) for body in fns.get(name, [])):
            return True
    return False


CHAIN = re.compile(r"\b(?:app|self|s|session)((?:\s*\.\s*[a-z_][a-z0-9_]*(?:\(\))?)+)")


def keys_of(code, fns, depth=0):
    """The flag paths (`a.b`) a predicate reads."""
    bare = BARE.match(code)
    if bare and depth == 0:
        code = "\n".join(fns.get(bare.group(1), []))
    out = set()
    for m in CHAIN.finditer(code):
        names = [p.strip() for p in m.group(1).split(".") if p.strip()]
        fields = []
        for p in names:
            if p.endswith("()"):
                name = p[:-2]
                if name in ("session", "session_mut"):
                    continue
                if name not in METHODS and depth < 2:
                    for body in fns.get(name, []):
                        out |= keys_of(body, fns, depth + 1)
                break
            if p in METHODS:
                break
            fields.append(p)
        if len(fields) >= 2:
            out.add(".".join(fields[-2:]))
    return out


def listed(root):
    path = os.path.join(root, LIST)
    out, problems = {}, []
    if not os.path.exists(path):
        return out, problems
    with open(path, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = [p.strip() for p in line.split("|", 2)]
            if len(parts) != 3 or parts[1] not in ("keys", "answer") or not parts[2]:
                problems.append(f"{LIST}:{n}: want `Name | keys | a.b c.d` or `Name | answer | <reason>`")
                continue
            out.setdefault(parts[0], {})[parts[1]] = (parts[2], n)
    return out, problems


def write_pattern(key):
    if key.endswith("()"):
        a, b = (re.escape(p) for p in key[:-2].split("."))
        return re.compile(rf"\b{a}\s*\.\s*{b}\(")
    a, b = (re.escape(p) for p in key.split("."))
    path = rf"\b{a}\s*\.\s*{b}"
    return re.compile(
        rf"{path}\s*=(?![=>])(?!\s*(?:None\b|false\b|Default::default\(\)))"
        rf"|{path}\s*\.\s*(?:replace|get_or_insert\w*|insert)\("
    )


def enclosing_fn(text, pos):
    """The body of the innermost `fn` around `pos`."""
    best = None
    for m in re.finditer(r"\bfn [a-z_][a-z0-9_]*\s*(?:<[^>(]*>)?\(", text[:pos]):
        brace = text.find("{", m.end())
        if brace == -1 or brace > pos:
            continue
        end = balanced(text, brace + 1, "{", "}")
        if end is not None and end >= pos:
            best = text[brace + 1 : end]
    return best or ""


def problems(root="."):
    ui = list(sources(root, "src/ui"))
    core = list(sources(root, "src/state")) + list(sources(root, "src/connect"))
    all_specs = specs(ui)
    fns = functions(ui)
    entries, out = listed(root)
    opened = {}
    for path, text in ui:
        for m in OPEN_CALL.finditer(text):
            opened.setdefault(m.group(1), []).append(f"{path}:{text.count(chr(10), 0, m.start()) + 1}")
    for name, (path, predicate, per_window) in sorted(all_specs.items()):
        window = reads_window(predicate, fns)
        if window and not per_window:
            out.append(
                f"{path}: dialog `{name}` reads the window's own state in its flag; "
                f"declare it `.per_window()` so each window shows its own"
            )
        if per_window and not window:
            out.append(
                f"{path}: dialog `{name}` is `.per_window()` but its flag reads no window state"
            )
        if per_window:
            for site in opened.get(name, []):
                out.append(f"{site}: `{name}` is per window; its open names no window")
            continue
        entry = entries.get(name, {})
        keys = set(entry["keys"][0].split()) if "keys" in entry else keys_of(predicate, fns)
        ui_writes, core_writes = [], []
        for key in sorted(keys):
            if key.count(".") != 1:
                continue
            pattern = write_pattern(key)
            for files, sink in ((ui, ui_writes), (core, core_writes)):
                for wpath, text in files:
                    if is_demo(wpath):
                        continue
                    lines = text.split("\n")
                    for m in pattern.finditer(text):
                        line = text.count("\n", 0, m.start()) + 1
                        sink.append((wpath, line, text, lines, m.start()))
        for wpath, line, text, lines, pos in ui_writes:
            around = lines[max(0, line - 4) : line]
            if any(MARKER.search(l) for l in around):
                continue
            body = enclosing_fn(text, pos)
            if name in OPEN_CALL.findall(body) or name in [
                m.group(1) for m in OPEN_CALL.finditer(body)
            ]:
                continue
            out.append(
                f"{wpath}:{line}: opens `{name}` without naming its window; call "
                f"open_box(DialogKind::{name}) here (or open_box_in for an answer's "
                f"flow window), or say why with `// box-host: <why>`"
            )
        if core_writes and not opened.get(name) and "answer" not in entry:
            where = ", ".join(sorted({f"{p}:{l}" for p, l, *_ in core_writes})[:3])
            out.append(
                f"{where}: the session opens `{name}` on an answer, and no request in src/ui "
                f"names its window (open_box / open_box_in); or list it in {LIST} as "
                f"`{name} | answer | <reason>`"
            )
    for name, entry in sorted(entries.items()):
        for rule, (_, n) in entry.items():
            if name not in all_specs:
                out.append(f"{LIST}:{n}: `{name}` is not a registered dialog")
            elif all_specs[name][2]:
                out.append(f"{LIST}:{n}: `{name}` is per window and needs no entry")
    for name, sites in sorted(opened.items()):
        if name not in all_specs:
            out.append(f"{sites[0]}: open_box names `{name}`, which is not a registered dialog")
    return out


if __name__ == "__main__":
    found = problems()
    print("\n".join(found))
    print(len(found))
