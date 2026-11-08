"""Process-wide state in src/ui must say why it is not per window.

A chat window draws with the same `QuillApp` and the same thread as the
main window, so a `thread_local!` or `static` that paint or a frame writes
is shared by every window: the last window drawn wins. Round 12 (area B)
found two of these: the text selection a copy checks (the main window
repainting after a chat window's selection let a protected chat's text be
copied) and the spoiler specks that ask for the next tick.

Every production `thread_local!` static and every `static` atomic or lock
in src/ui is either keyed by window (its type names `WindowId`) or listed
in scripts/ui-statics.txt with the reason it is safe to share:

    src/ui/paragraph_cache.rs CACHE | a shaping cache: any window's text

A listed name that is gone fails too, so the list stays true. Test files,
statics marked `#[cfg(test)]` and write-once `OnceLock`s are skipped.

Run by scripts/check-window-scope.py.
"""

import pathlib
import re

LIST = "scripts/ui-statics.txt"
STATIC = re.compile(r"^\s*(?:pub(?:\([a-z]+\))?\s+)?static\s+([A-Z][A-Z0-9_]*)\s*:\s*(.*)$")
ONCE = re.compile(r"^\s*(?:std::sync::)?Once(?:Lock|Cell)?\b")
SHARED = re.compile(r"^\s*(?:std::sync::(?:atomic::)?|parking_lot::)?(?:Atomic|Mutex|RwLock)")


def is_test_file(path: pathlib.Path) -> bool:
    return "test" in path.name or "tests" in path.parts


def statics(text: str):
    """Yields (name, type) of each production static the check covers."""
    lines = text.split("\n")
    depth_in_tl = None  # brace depth inside a thread_local! block
    tl_is_test = False
    for i, line in enumerate(lines):
        prev = [l.strip() for l in lines[max(0, i - 3) : i]]
        marked_test = any(p.startswith("#[cfg(test)]") for p in prev[-2:])
        if depth_in_tl is None and "thread_local!" in line:
            depth_in_tl = line.count("{") - line.count("}")
            tl_is_test = marked_test
            if depth_in_tl <= 0 and "static" not in line:
                depth_in_tl = None
            continue
        if depth_in_tl is not None:
            m = STATIC.match(line)
            if m and not tl_is_test and not marked_test and not ONCE.match(m.group(2)):
                yield m.group(1), m.group(2)
            depth_in_tl += line.count("{") - line.count("}")
            if depth_in_tl <= 0:
                depth_in_tl = None
            continue
        m = STATIC.match(line)
        if m and SHARED.match(m.group(2)) and not marked_test:
            yield m.group(1), m.group(2)


def listed(repo: pathlib.Path):
    entries = {}
    problems = []
    path = repo / LIST
    if not path.exists():
        return entries, [f"{LIST} is missing"]
    for n, raw in enumerate(path.read_text().split("\n"), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        head, sep, why = line.partition("|")
        parts = head.split()
        if not sep or not why.strip() or len(parts) != 2:
            problems.append(f"{LIST}:{n}: want `path NAME[,NAME] | why`")
            continue
        for name in parts[1].split(","):
            entries[(parts[0], name)] = n
    return entries, problems


def problems(repo: pathlib.Path) -> list[str]:
    entries, out = listed(repo)
    seen = set()
    for path in sorted((repo / "src/ui").rglob("*.rs")):
        if is_test_file(path):
            continue
        rel = str(path.relative_to(repo))
        for name, ty in statics(path.read_text()):
            if "WindowId" in ty:
                continue
            seen.add((rel, name))
            if (rel, name) not in entries:
                out.append(
                    f"{rel}: `static {name}` is shared by every window; key it by "
                    f"WindowId or list it in {LIST} with why it may be shared"
                )
    for (rel, name), n in sorted(entries.items()):
        if (rel, name) not in seen:
            out.append(f"{LIST}:{n}: `{rel} {name}` is no longer a shared static; remove it")
    return out
