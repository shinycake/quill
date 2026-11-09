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
5. Where no window asked (round 14), an open names the window the request
   carried. The poll loop (`poll_live`), the main window's own upkeep (the
   steps of `frame_upkeep` but `per_window_upkeep`) and the toast drain
   (`drain_suggested_post_notes`) run with the main window in place
   whichever window the user acted in, and run as `Asker::Nobody`
   (`QuillApp::unasked`, which their callers must use). A function they
   reach (through `self.` / `this.` / `app.` method calls, by name) must
   not call `open_box(` or `queue_deep_link(` outside an
   `as_asked_by(window, ..)` call: carry the asking window with the
   request (the chat it was about, the box it follows up) and open under
   `as_asked_by`, or say why with `// box-host: <why>`.

A flag is found by the field paths its predicate reads (the last two
names, `notify.mute_menu_open`). A predicate that hides them (a closure
argument, a negation) gives its paths in scripts/dialog-hosts.txt as
`Name | keys | a.b c.d`; `a.b()` there is a method that opens the box
(`gift_market.push()`).

Known limits: a write through a local binding (`let boxes = &mut
self.call_boxes; boxes.confirm = ..`) is not seen, and the enclosing
function is the nearest `fn` above the write. Rule 5 follows methods by
name (a false hit is possible: mark it), and not through closures handed
to other types.

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
    out += unasked_problems(ui)
    return out


# Rule 5: the functions no window asks in, by file and name, and the
# frame whose `self.x(` steps are the main window's own.
UNASKED_ROOTS = [
    ("src/ui/notifications/mod.rs", "poll_live"),
    ("src/ui/suggested_posts_ui.rs", "drain_suggested_post_notes"),
]
MAIN_FRAME = ("src/ui/app_render/frame_upkeep.rs", "frame_upkeep")
# The window's own frame inside the main frame: the window in place asks.
IN_PLACE_STEPS = {"per_window_upkeep", "asked_in_place", "lock_waits"}
ASKER_SENSITIVE = re.compile(r"\b(open_box|queue_deep_link)\(")
METHOD_CALL = re.compile(r"\b(?:self|this|app)\s*\.\s*([a-z_][a-z0-9_]*)\s*\(")
NAMED_ASKER = re.compile(r"\b(?:as_asked_by|asked_in_place)\s*\(")
# Calls whose closures run later, in input or in a task (a click handler, a
# toast's link, a spawned job): what they do is not this pass's.
LATER = re.compile(
    r"\b(?:listener|marked_toast|on_[a-z_]+|spawn\w*|defer\w*|subscribe\w*|observe\w*)\s*\("
)
# Calls of a root, and the wrapper that runs it as nobody.
UNASKED_CALL = re.compile(r"\bunasked\s*\(")
# Tasks no window asked for: their `ScopedApp` updates run as nobody
# (`chat_window/scoped.rs`), so the bodies of these calls are roots too.
UNASKED_TASK = re.compile(
    r"\b(?:spawn_screen|spawn_screen_in|spawn_app|spawn_app_in|defer_screen|defer_screen_in)\s*\("
)


def blank_calls(body, pattern):
    """`body` with the arguments of the calls `pattern` finds blanked."""
    out, at = body, 0
    while True:
        m = pattern.search(out, at)
        if not m:
            return out
        end = balanced(out, m.end(), "(", ")")
        if end is None:
            return out
        out = out[: m.start()] + " " * (end + 1 - m.start()) + out[end + 1 :]
        at = end + 1


def without_named_askers(body):
    """`body` with what names its asker (`as_asked_by(..)`,
    `asked_in_place(..)`) and what runs later (`LATER`) blanked."""
    return blank_calls(blank_calls(body, NAMED_ASKER), LATER)


def fn_bodies(ui):
    """name -> [(path, offset of body, body)] for every `fn` in src/ui."""
    out = {}
    for path, text in ui:
        for m in re.finditer(r"\bfn ([a-z_][a-z0-9_]*)\s*(?:<[^>(]*>)?\(", text):
            brace = text.find("{", m.end())
            semi = text.find(";", m.end())
            if brace == -1 or (semi != -1 and semi < brace):
                continue
            end = balanced(text, brace + 1, "{", "}")
            if end is not None:
                out.setdefault(m.group(1), []).append((path, text, brace + 1, text[brace + 1 : end]))
    return out


def unasked_problems(ui):
    """Rule 5."""
    bodies = fn_bodies(ui)
    texts = dict(ui)
    out = []
    starts = []
    for path, name in UNASKED_ROOTS:
        if name not in {n for n, entries in bodies.items() if any(p == path for p, *_ in entries)}:
            out.append(f"{path}: no `fn {name}` (UNASKED_ROOTS in scripts/dialog_hosts.py)")
            continue
        starts.append((name, name))
    frame = [e for e in bodies.get(MAIN_FRAME[1], []) if e[0] == MAIN_FRAME[0]]
    if not frame:
        out.append(f"{MAIN_FRAME[0]}: no `fn {MAIN_FRAME[1]}` (MAIN_FRAME in scripts/dialog_hosts.py)")
    for _, _, _, body in frame:
        for step in METHOD_CALL.findall(without_named_askers(body)):
            if step not in IN_PLACE_STEPS:
                starts.append((step, f"{MAIN_FRAME[1]} > {step}"))
    # The bodies of tasks no window asked for.
    for path, text in ui:
        for m in UNASKED_TASK.finditer(text):
            if text[max(0, m.start() - 3) : m.start()].endswith("fn "):
                continue
            end = balanced(text, m.end(), "(", ")")
            if end is None:
                continue
            line_no = text.count("\n", 0, m.start()) + 1
            name = f"{m.group(0).rstrip('( ')} at {path}:{line_no}"
            bodies.setdefault(name, []).append((path, text, m.end(), text[m.end() : end]))
            starts.append((name, name))
    # Each root's callers run it as nobody.
    roots = [name for _, name in UNASKED_ROOTS] + [MAIN_FRAME[1]]
    for path, text in ui:
        for root in roots:
            for m in re.finditer(rf"\.\s*{root}\s*\(", text):
                line_no = text.count("\n", 0, m.start()) + 1
                line = text.split("\n")[line_no - 1]
                if not UNASKED_CALL.search(line):
                    out.append(
                        f"{path}:{line_no}: `{root}` runs where no window asks: call it inside "
                        f"`unasked(|this| this.{root}(..))`"
                    )
    lines_of = {}
    seen = set()
    queue = list(starts)
    hits = {}
    while queue:
        name, via = queue.pop(0)
        if name in seen:
            continue
        seen.add(name)
        for path, text, base, body in bodies.get(name, []):
            open_body = without_named_askers(body)
            for m in ASKER_SENSITIVE.finditer(open_body):
                if open_body[max(0, m.start() - 3) : m.start()].endswith("fn "):
                    continue
                line_no = text.count("\n", 0, base + m.start()) + 1
                lines = lines_of.setdefault(path, text.split("\n"))
                above = lines[max(0, line_no - 4) : line_no]
                if any(MARKER.search(l) for l in above):
                    continue
                hits.setdefault((path, line_no), (m.group(1), via))
            for callee in METHOD_CALL.findall(open_body):
                if callee not in seen:
                    queue.append((callee, f"{via} > {callee}"))
    for (path, line_no), (call, via) in sorted(hits.items()):
        out.append(
            f"{path}:{line_no}: `{call}` runs where no window asked ({via}): carry the asking "
            f"window with the request and run it under `as_asked_by(window, ..)`, or say why "
            f"with `// box-host: <why>`"
        )
    return out


if __name__ == "__main__":
    found = problems()
    print("\n".join(found))
    print(len(found))
