#!/usr/bin/env python3
"""No new `RefCell` borrows that can panic.

docs/contributing/panic-safety.md, rule 6: `borrow()` and `borrow_mut()`
panic when the cell is already borrowed, and a panic on the UI thread ends
the process. The guard that looks short today is held across a call into
GPUI, a callback or another module tomorrow. The re-sweep of 2026-11-11
recorded 89 such sites as crash risks, among them 33 copies of one dialog
body cell (`Rc<RefCell<Option<_>>>` with `borrow_mut().take()`).

This check counts every `.borrow()`, `.borrow_mut()`, `.with_borrow(..)`
and `.with_borrow_mut(..)` call outside test code, by the function it is in
and the name it is called on. The calls in
the tree when the check was added are in
scripts/refcell-borrows-baseline.txt. That list is debt: it only shrinks.
A new call fails. Write one of these instead:

- `Cell<T>` (`get`, `set`, `take`, `replace`) for a value that is copied
  or handed over whole. A one-shot body is `Rc<Cell<Option<T>>>` and
  `cell.take()`: there is no guard to hold.
- `try_borrow()` / `try_borrow_mut()` with an early return, so a re-entry
  skips one frame instead of ending the process.
- state on an entity, read and written through `cx`.

A `.borrow()` that is not a `RefCell` (the `Borrow` trait, a channel's
`borrow()`) goes in scripts/refcell-borrows-allow.txt with the reason.

Run from the repository root:

    python3 scripts/check-refcell-borrows.py [--root DIR] [--list] [--prune]
"""

import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import shape_guard  # noqa: E402

BASELINE = "scripts/refcell-borrows-baseline.txt"
ALLOW = "scripts/refcell-borrows-allow.txt"
HEADER = (
    "# RefCell borrows that can panic (scripts/check-refcell-borrows.py).\n"
    "# <Type::function>::<receiver>.<call> <count>. Only remove lines or lower counts.\n"
)
ADVICE = (
    "A RefCell borrow panics when the cell is already borrowed. Use Cell (take/set/replace), "
    "try_borrow()/try_borrow_mut() with an early return, or entity state "
    "(docs/contributing/panic-safety.md, rule 6)."
)
BORROW = re.compile(r"\.\s*(?:(borrow|borrow_mut)\s*\(\s*\)|(with_borrow|with_borrow_mut)\s*\()")
RECEIVER = re.compile(r"([A-Za-z_]\w*)\s*(?:\([^()]*\))?\s*$")


def receiver(text, dot):
    """The last name before the `.` at `dot`: `body` in `self.body.borrow()`."""
    match = RECEIVER.search(text[max(0, dot - 160) : dot])
    return match.group(1) if match else "_"


def hits(root):
    for path, text in shape_guard.source(root):
        if ".borrow" not in text:
            continue
        scopes = shape_guard.Scopes(path, text)
        for match in BORROW.finditer(text):
            call = match.group(1) or match.group(2)
            key = f"{scopes.at(match.start())}::{receiver(text, match.start())}.{call}"
            yield key, path, shape_guard.line_of(text, match.start())


if __name__ == "__main__":
    shape_guard.main("check-refcell-borrows", hits, BASELINE, ALLOW, HEADER, ADVICE)
