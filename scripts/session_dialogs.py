"""Dialogs whose open flag lives in the `Session` close on a switch.

Imported by scripts/check-screen-state.py. The `Session` goes to the
background with its account and comes back with it, so a dialog whose
`DialogSpec` predicate reads the session would be open again on
switch-back unless a farewell closes it (phase 3 did this for payments;
round 10 found eight more boxes with no closer).

A predicate that reads the session (`session()`, `demo_session`, `live`,
directly or through `self.` / `app.` helpers up to three calls deep) must
be listed in scripts/session-dialogs.txt as `Name | closer | reason`.
`closer` is the method a farewell (`impl .. Farewell for ..`) calls to
close it; `-` says why none is needed, in the reason.
"""

import os
import re

LIST = "scripts/session-dialogs.txt"
SESSION_READ = re.compile(r"\bsession\(\)|\bdemo_session\b|\b(?:self|app)\.live\b|driver\.session")
CALL = re.compile(r"\b(?:self|app)\.([a-z_][a-z0-9_]*)\(")


def balanced(text, start, open_ch, close_ch):
    """The text after `start` up to the bracket closing the one before it."""
    depth = 1
    for i in range(start, len(text)):
        if text[i] == open_ch:
            depth += 1
        elif text[i] == close_ch:
            depth -= 1
            if depth == 0:
                return text[start:i]
    return None


def ui_sources():
    for root, _, names in os.walk("src/ui"):
        for name in sorted(names):
            if name.endswith(".rs") and not name.endswith("tests.rs"):
                path = os.path.join(root, name)
                with open(path, encoding="utf-8") as f:
                    yield path, f.read()


def split_args(text):
    parts, depth, part = [], 0, []
    for i, ch in enumerate(text):
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append("".join(part).strip())
            part = []
        else:
            part.append(ch)
    parts.append("".join(part).strip())
    return [p for p in parts if p]


def collect(sources):
    dialogs, functions, farewells = {}, {}, []
    for path, text in sources:
        for match in re.finditer(r"\b([A-Z][A-Za-z0-9_]*)\s*=>\s*DialogSpec::new\(", text):
            args = balanced(text, match.end(), "(", ")")
            parts = split_args(args or "")
            if len(parts) >= 2:
                dialogs[match.group(1)] = (path, parts[1])
        for match in re.finditer(r"\bfn ([a-z_][a-z0-9_]*)\s*(?:<[^>(]*>)?\(\s*&(?:mut )?self\b", text):
            brace = text.find("{", match.end())
            semi = text.find(";", match.end())
            if brace == -1 or (semi != -1 and semi < brace):
                continue
            body = balanced(text, brace + 1, "{", "}")
            if body is not None:
                functions.setdefault(match.group(1), []).append(body)
        for match in re.finditer(r"\bimpl\b[^{;]*\bFarewell for [^{]*\{", text):
            body = balanced(text, match.end(), "{", "}")
            if body is not None:
                farewells.append(body)
    return dialogs, functions, farewells


def reads_session(code, functions, depth=0, seen=None):
    if SESSION_READ.search(code):
        return True
    if depth >= 3:
        return False
    seen = set() if seen is None else seen
    for name in CALL.findall(code):
        if name in seen:
            continue
        seen.add(name)
        for body in functions.get(name, []):
            if reads_session(body, functions, depth + 1, seen):
                return True
    return False


def listed(path=LIST):
    out, problems = {}, []
    if not os.path.exists(path):
        return out, problems
    with open(path, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = [p.strip() for p in line.split("|", 2)]
            if len(parts) != 3 or not parts[2]:
                problems.append(f"{path}:{n}: want `Name | closer | reason`")
                continue
            out[parts[0]] = (parts[1], n)
    return out, problems


def problems():
    dialogs, functions, farewells = collect(ui_sources())
    entries, out = listed()
    farewell_text = "\n".join(farewells)
    backed = {
        name: path
        for name, (path, predicate) in dialogs.items()
        if reads_session(predicate, functions)
    }
    for name, path in sorted(backed.items()):
        if name not in entries:
            out.append(
                f"{path}: dialog `{name}` reads the Session in its predicate, so it "
                f"comes back on switch-back. Close it in a farewell and list it in "
                f"{LIST} as `{name} | <closer> | <reason>`."
            )
    for name, (closer, n) in sorted(entries.items()):
        if name not in backed:
            out.append(f"{LIST}:{n}: `{name}` is not a dialog whose predicate reads the Session")
            continue
        if closer != "-" and not re.search(rf"\b{re.escape(closer)}\(", farewell_text):
            out.append(
                f"{LIST}:{n}: `{name}` names `{closer}`, which no `impl .. Farewell` calls"
            )
    return out, len(backed)
