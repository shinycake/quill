#!/usr/bin/env python3
"""A drain of one-shot notes shows every note, not the last.

The reducers push their failure lines to a `notes: Vec<String>` that the UI
takes once per frame. Two failures between two frames are two lines, and
`set_note` already queues each one in the toast queue (`ui/toast_queue.rs`).
`notes.into_iter().last()` shows the last and silently drops the others, so
a failed Decline can go unreported behind a failed Add Offer (sweep
s4-payments-2: 09-069, 10-013, 13-108).

This check fails when a file in src/ui keeps only the last of a `notes`
list. Loop over the notes and call `set_note` for each one.

Usage: python3 scripts/check-drained-notes.py   (from the repository root)
"""
import os
import re
import sys

LAST_ONLY = re.compile(r'\bnotes\s*\.into_iter\(\)\s*\.last\(\)')


def ui_files():
    for root, _, names in os.walk('src/ui'):
        for name in sorted(names):
            path = os.path.join(root, name)
            if name.endswith('.rs') and 'test' not in path:
                yield path


def main():
    problems = []
    for path in ui_files():
        with open(path, encoding='utf-8') as handle:
            for number, line in enumerate(handle, 1):
                if LAST_ONLY.search(line):
                    problems.append(f'{path}:{number}: only the last note is shown')
    if problems:
        print('\n'.join(problems))
        print('Show every note: `for note in notes { self.set_note(..) }`.')
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
