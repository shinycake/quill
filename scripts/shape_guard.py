"""Shared parts of the shape guards.

The guards (`check-refcell-borrows.py`, `check-owned-timers.py`,
`check-request-match.py`, `check-stage-enums.py`) each find one shape that
the state sweeps kept reporting, and hold it with a ratchet: the hits in
the tree when the guard was added are in a baseline file that may only
shrink, and a hit that is not there fails.

This module reads Rust source well enough for that:

- `source(root)` yields every non-test file as text with comments, string
  and char literals blanked and `#[cfg(test)]` items removed, offsets and
  line numbers kept;
- `Scopes` names the `Type::function` a position is in, which is the key a
  hit is listed under. A key has no path and no line number, so moving or
  splitting a file changes nothing;
- `run` compares hits with the baseline and the allow list.

A baseline line is `<key> <count>`. An allow line is `<key> <reason>`.
"""

import os
import re
import sys

SCAN = ("src", "crates")

_TOKEN = re.compile(
    r"""
      //[^\n]*
    | /\*.*?\*/
    | \bb?r(\#*)".*?"\1
    | b?"(?:[^"\\]|\\.)*"
    | b?'(?:[^'\\\n]|\\[^\n][^'\n]*?)'
    """,
    re.S | re.X,
)
_NOT_NEWLINE = re.compile(r"[^\n]")
_CFG_TEST = re.compile(r"#\[cfg\(test\)\]")
_ATTRS = re.compile(r"(?:\s*#\[[^\]]*\])*\s*")
_ITEM = re.compile(
    r"(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|unsafe|const|extern(?:\s+\"[^\"]*\")?)\s+)*"
    r"(?:mod|fn|impl|use|const|static|struct|enum|type|trait|union|thread_local!|macro_rules!)(?![\w])"
)
_TEST_MOD = re.compile(
    r"#\[cfg\(test\)\]((?:\s*#\[[^\]]*\])*)\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;"
)
_PLAIN_MOD = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;", re.M)
_PATH_ATTR = re.compile(r'#\[path\s*=\s*"([^"]+)"\]')
_TEST_NAME = re.compile(r"(?:^|/)(?:tests?(?:_\w+)?|\w+_tests?)(?:\.rs$|/)")
_IMPL = re.compile(r"^[ \t]*(?:unsafe\s+)?impl\b([^{;]*)\{", re.M)
_FN = re.compile(r"\bfn\s+([A-Za-z_]\w*)")
_SEEDER = re.compile(r"demo|fixture")
_OPEN = {")": "(", "]": "[", "}": "{"}
_CLOSE = {"(": ")", "[": "]", "{": "}"}


def blank(text):
    """`text` with comments and the insides of literals turned to spaces."""
    return _TOKEN.sub(lambda m: _NOT_NEWLINE.sub(" ", m.group(0)), text)


def closing(text, start):
    """Index of the bracket that closes the one at `start`."""
    open_ch = text[start]
    close_ch = _CLOSE[open_ch]
    depth = 0
    for i in range(start, len(text)):
        ch = text[i]
        if ch == open_ch:
            depth += 1
        elif ch == close_ch:
            depth -= 1
            if depth == 0:
                return i
    return len(text) - 1


def opening(text, end):
    """Index of the bracket that opens the one closed at `end`."""
    close_ch = text[end]
    open_ch = _OPEN[close_ch]
    depth = 0
    for i in range(end, -1, -1):
        ch = text[i]
        if ch == close_ch:
            depth += 1
        elif ch == open_ch:
            depth -= 1
            if depth == 0:
                return i
    return 0


def _body_start(text, start):
    """The `{` that opens an item's body after `start`, or None at a `;`."""
    depth = 0
    for i in range(start, len(text)):
        ch = text[i]
        if ch in "([":
            depth += 1
        elif ch in ")]":
            depth -= 1
        elif depth == 0 and ch == "{":
            return i
        elif depth == 0 and ch == ";":
            return None
    return None


def _statement_end(text, start):
    """One past the end of the field, argument or statement at `start`."""
    depth = 0
    for i in range(start, len(text)):
        ch = text[i]
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            if depth == 0:
                return i
            depth -= 1
            if depth == 0 and ch == "}":
                return i + 1
        elif depth == 0 and ch in ",;":
            return i + 1
    return len(text)


def _test_end(text, item):
    """One past the end of what a `#[cfg(test)]` before `item` covers."""
    word = _ITEM.match(text, item)
    if word is None:
        return _statement_end(text, item)
    brace = _body_start(text, word.end())
    if brace is None:
        end = text.find(";", item)
        return len(text) if end < 0 else end + 1
    return closing(text, brace) + 1


def strip_tests(text):
    """`text` (already blanked) without what `#[cfg(test)]` covers: an
    item (a module, function, impl, ...) or one field or statement."""
    out = []
    at = 0
    for match in _CFG_TEST.finditer(text):
        if match.start() < at:
            continue
        end = _test_end(text, _ATTRS.match(text, match.end()).end())
        out.append(text[at : match.start()])
        out.append(_NOT_NEWLINE.sub(" ", text[match.start() : end]))
        at = end
    out.append(text[at:])
    return "".join(out)


def _rust_files(root):
    for top in SCAN:
        for dirpath, dirnames, names in os.walk(os.path.join(root, top)):
            dirnames[:] = sorted(d for d in dirnames if d not in ("target", "vendor"))
            for name in sorted(names):
                if name.endswith(".rs"):
                    path = os.path.join(dirpath, name)
                    yield os.path.relpath(path, root).replace(os.sep, "/")


def _module_dir(rel):
    folder, name = os.path.split(rel)
    if name in ("mod.rs", "lib.rs", "main.rs"):
        return folder
    return os.path.join(folder, name[:-3])


def _declared(rel, attrs, name):
    """The files a `mod name;` in `rel` can mean."""
    path = _PATH_ATTR.search(attrs)
    if path:
        return [os.path.normpath(os.path.join(os.path.dirname(rel), path.group(1)))]
    base = os.path.join(_module_dir(rel), name)
    return [base + ".rs", os.path.join(base, "mod.rs")]


def test_files(root, raw):
    """Paths of files that only build under test: named so, or declared by
    a `#[cfg(test)] mod x;`, or a module of such a file."""
    tests = {rel for rel in raw if _TEST_NAME.search(rel) or "#![cfg(test)]" in raw[rel]}
    queue = []
    for rel, text in raw.items():
        for match in _TEST_MOD.finditer(text):
            queue.extend(_declared(rel, match.group(1), match.group(2)))
    queue.extend(tests)
    seen = set()
    while queue:
        rel = queue.pop().replace(os.sep, "/")
        if rel in seen or rel not in raw:
            continue
        seen.add(rel)
        tests.add(rel)
        for match in _PLAIN_MOD.finditer(raw[rel]):
            queue.extend(_declared(rel, "", match.group(1)))
    return tests


def source(root="."):
    """`(path, text)` for every non-test Rust file, ready to search."""
    raw = {}
    for rel in _rust_files(root):
        with open(os.path.join(root, rel), encoding="utf-8", errors="replace") as handle:
            raw[rel] = handle.read()
    skip = test_files(root, raw)
    for rel in sorted(raw):
        if rel not in skip:
            yield rel, strip_tests(blank(raw[rel]))


def is_seeder(path):
    """A screenshot-demo or fixture file: it seeds state, it does not run
    the app."""
    return bool(_SEEDER.search(path))


def body_start(text, start):
    """The `{` that opens an item's body after `start`, or None at a `;`."""
    return _body_start(text, start)


def line_of(text, offset):
    return text.count("\n", 0, offset) + 1


def _impl_type(header):
    """`Type` from the text between `impl` and `{`."""
    header = header.split(" where ")[0]
    if " for " in header:
        header = header.rsplit(" for ", 1)[1]
    header = re.sub(r"^\s*<[^>]*>", "", header)
    name = re.match(r"\s*&?\s*(?:mut\s+)?(?:dyn\s+)?((?:\w+::)*\w+)", header)
    return name.group(1).rsplit("::", 1)[-1] if name else "?"


class Scopes:
    """Names the `Type::function` that holds a position of one file."""

    def __init__(self, path, text):
        self.top = "@" + os.path.basename(path)[:-3]
        self.impls = []
        for match in _IMPL.finditer(text):
            brace = match.end() - 1
            self.impls.append((brace, closing(text, brace), _impl_type(match.group(1))))
        self.fns = []
        for match in _FN.finditer(text):
            brace = _body_start(text, match.end())
            if brace is not None:
                self.fns.append((brace, closing(text, brace), match.group(1)))

    @staticmethod
    def _inner(spans, offset):
        best = None
        for start, end, name in spans:
            if start <= offset <= end and (best is None or start > best[0]):
                best = (start, name)
        return best

    def fn_span(self, offset):
        """`(start, end)` of the innermost function body around `offset`."""
        best = None
        for start, end, _ in self.fns:
            if start <= offset <= end and (best is None or start > best[0]):
                best = (start, end)
        return best

    def at(self, offset):
        function = self._inner(self.fns, offset)
        owner = self._inner(self.impls, offset)
        if function and owner:
            return f"{owner[1]}::{function[1]}"
        if function:
            return function[1]
        if owner:
            return owner[1]
        return self.top


def listed(path):
    """`{key: (line number, rest of line)}` of a list file."""
    out = {}
    if not os.path.exists(path):
        return out
    with open(path, encoding="utf-8") as handle:
        for number, line in enumerate(handle, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            key, _, rest = line.partition(" ")
            out[key] = (number, rest.strip())
    return out


def _count(rest):
    return int(rest) if rest.isdigit() else 1


def _write(path, header, counts):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(header)
        for key in sorted(counts):
            handle.write(f"{key} {counts[key]}\n")


def run(name, hits, baseline, allow, header, advice, argv):
    """Compare `hits` (`(key, path, line)`) with the lists; the exit code.

    `--write-baseline` lists today's hits, `--prune` lowers the baseline to
    what is left (it never adds), `--list` prints every hit, `--strict`
    also fails on a baseline line that is too high.
    """
    root = argv[argv.index("--root") + 1] if "--root" in argv else "."
    baseline_path = os.path.join(root, baseline)
    allow_path = os.path.join(root, allow)
    allowed = listed(allow_path)
    sites = {}
    for key, path, line in hits(root):
        sites.setdefault(key, []).append(f"{path}:{line}")
    if "--list" in argv:
        for key in sorted(sites):
            for site in sites[key]:
                print(f"{site} {key}")
        return 0
    counts = {key: len(found) for key, found in sites.items() if key not in allowed}
    if "--write-baseline" in argv:
        _write(baseline_path, header, counts)
        print(f"{name}: baseline written, {sum(counts.values())} hits under {len(counts)} keys")
        return 0
    known = {key: (number, _count(rest)) for key, (number, rest) in listed(baseline_path).items()}
    if "--prune" in argv:
        kept = {key: min(count, counts[key]) for key, (_, count) in known.items() if key in counts}
        _write(baseline_path, header, kept)
        print(f"{name}: baseline pruned to {sum(kept.values())} hits under {len(kept)} keys")
        return 0
    problems = []
    stale = []
    for key, (number, reason) in allowed.items():
        if not reason:
            problems.append(f"{allow}:{number}: {key} needs a reason after the name")
        if key not in sites:
            stale.append(f"{allow}:{number}: {key} is gone; remove the line")
    for key in sorted(counts):
        have = counts[key]
        base = known.get(key, (0, 0))[1]
        if have > base:
            where = ", ".join(sites[key])
            was = f"{base} in the baseline" if base else "not in the baseline"
            problems.append(f"{key}: {have} here, {was} ({where})")
    for key, (number, base) in sorted(known.items()):
        have = counts.get(key, 0)
        if have == 0:
            stale.append(f"{baseline}:{number}: {key} is gone; remove the line")
        elif have < base:
            stale.append(f"{baseline}:{number}: {key} is down to {have}; lower the count")
    if stale:
        for line in stale:
            print(line)
        print(f"{name}: {len(stale)} baseline line(s) can go: run with --prune and commit")
        if "--strict" in argv:
            problems.extend(stale)
    if problems:
        for problem in problems:
            if problem not in stale:
                print(problem)
        print(advice)
        print(f"{name}: {len(problems)} problem(s)")
        return 1
    total = sum(count for _, count in known.values())
    print(f"{name}: ok ({total} baseline hits under {len(known)} keys, {len(allowed)} allowed)")
    return 0


def main(name, hits, baseline, allow, header, advice):
    sys.exit(run(name, hits, baseline, allow, header, advice, sys.argv[1:]))
