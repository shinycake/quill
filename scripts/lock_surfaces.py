"""Every surface the passcode lock must reach is closed by it or refused under it.

Imported by scripts/check-screen-state.py. The lock had no single owner:
each review round found one more surface it did not reach (recordings, a
mini app opened after it, deep links). `close_for_lock`
(src/ui/passcode/lock_gate.rs) is the one place that closes or stops them,
and `refused_while_locked` is the one gate that stops new ones.

A production function in src/ui that opens an OS window (`.open_window(`),
starts a helper process (`Command::new(`) or starts a recording
(`.begin_capture_start(`) must either call `refused_while_locked()` itself
or be listed in scripts/lock-surfaces.txt as `fn | closer | reason`.
`closer` is a method `close_for_lock` calls; `-` says, in the reason, why
the lock does not need to reach it; `refuses` pins a function that must
keep calling `refused_while_locked()` (link and launch paths no pattern
finds).
"""

import os
import re

LIST = "scripts/lock-surfaces.txt"
GATE_FILE = "src/ui/passcode/lock_gate.rs"
STARTS = re.compile(r"\.open_window\(|\bCommand::new\(|\.begin_capture_start\(")
GATE = "refused_while_locked()"
TEST_MOD = re.compile(r"^\s*#\[cfg\(test\)\]\s*\n(?:\s*#\[[^\n]*\]\s*\n)*\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{", re.M)
FN = re.compile(r"\bfn ([a-z_][a-z0-9_]*)\s*(?:<[^>(]*>)?\(")


def balanced(text, start):
    depth = 1
    for i in range(start, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return start, i
    return None


def production(text):
    match = TEST_MOD.search(text)
    return text[: match.start()] if match else text


def functions(text):
    """(name, start, end, body) of every function with a body."""
    out = []
    for match in FN.finditer(text):
        brace = text.find("{", match.end())
        semi = text.find(";", match.end())
        if brace == -1 or (semi != -1 and semi < brace):
            continue
        span = balanced(text, brace + 1)
        if span:
            out.append((match.group(1), span[0], span[1], text[span[0] : span[1]]))
    return out


def ui_sources():
    for root, _, names in os.walk("src/ui"):
        if os.sep + "tests" in root or root.endswith("tests"):
            continue
        for name in sorted(names):
            if name.endswith(".rs") and not name.endswith("tests.rs") and "_tests_" not in name:
                path = os.path.join(root, name)
                with open(path, encoding="utf-8") as f:
                    yield path, production(f.read())


def listed(path=LIST):
    out, problems = {}, []
    if not os.path.exists(path):
        return out, [f"{path}: missing"]
    with open(path, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = [p.strip() for p in line.split("|", 2)]
            if len(parts) != 3 or not parts[2]:
                problems.append(f"{path}:{n}: want `fn | closer | reason`")
                continue
            out[parts[0]] = (parts[1], n)
    return out, problems


def problems():
    entries, out = listed()
    starters, bodies, gate_body = {}, {}, None
    for path, text in ui_sources():
        fns = functions(text)
        for name, start, end, body in fns:
            bodies.setdefault(name, []).append(body)
            if path == GATE_FILE and name == "close_for_lock":
                gate_body = body
        for match in STARTS.finditer(text):
            inner = [f for f in fns if f[1] <= match.start() < f[2]]
            if not inner:
                continue
            # The innermost function holding the call.
            name = min(inner, key=lambda f: f[2] - f[1])[0]
            starters.setdefault(name, (path, match.group(0)))
    if gate_body is None:
        return out + [f"{GATE_FILE}: no `close_for_lock`"], len(starters)
    for name, (path, what) in sorted(starters.items()):
        if any(GATE in body for body in bodies.get(name, [])):
            continue
        if name not in entries:
            out.append(
                f"{path}: `{name}` opens a surface (`{what}`) the passcode lock does not "
                f"know about. Refuse it while locked (`{GATE}`), close it in "
                f"`close_for_lock`, or list it in {LIST} as `{name} | <closer or -> | <reason>`."
            )
    for name, (closer, n) in sorted(entries.items()):
        if name not in bodies:
            out.append(f"{LIST}:{n}: `{name}` is not a function in src/ui")
            continue
        if closer == "refuses":
            if not any(GATE in body for body in bodies[name]):
                out.append(f"{LIST}:{n}: `{name}` must call `{GATE}`")
            continue
        if name not in starters:
            out.append(f"{LIST}:{n}: `{name}` opens no window, helper or recording")
        if closer != "-" and not re.search(rf"\bself\.{re.escape(closer)}\(", gate_body):
            out.append(f"{LIST}:{n}: `{name}` names `{closer}`, which `close_for_lock` does not call")
    return out, len(starters)


if __name__ == "__main__":
    found, count = problems()
    for line in found:
        print(line)
    print(f"lock surfaces: {'FAIL' if found else 'OK'} ({count} surface starts)")
