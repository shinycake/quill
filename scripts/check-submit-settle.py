#!/usr/bin/env python3
"""A box that holds a `Submit` must have it settled.

`quill::state::Submit` is the stage of a box's Save: `Sending` leaves on
TDLib's answer (the box closes) or on its error (the box stays with the
reason). Nothing leaves it unless a `settle_*` function reads it, so a box
that sets `Submit::sent(..)` and has no settle call would show Save busy
for good (docs/contributing/state-machines.md, rule 2).

This check fails when a file in src/ui calls `Submit::sent(` but no
`fn settle_*` of that file is called from `src/ui/box_submits.rs` (the pump
`poll_live` runs) or from the per-window upkeep
(`src/ui/app_render/window_upkeep.rs`, for a box that needs its window).

Usage: python3 scripts/check-submit-settle.py   (from the repository root)
"""
import os
import re
import sys

PUMPS = ['src/ui/box_submits.rs', 'src/ui/app_render/window_upkeep.rs']
SETTLE = re.compile(r'fn (settle_\w+)')


def ui_files():
    for root, _, names in os.walk('src/ui'):
        for name in sorted(names):
            path = os.path.join(root, name)
            if name.endswith('.rs') and 'test' not in path and 'demo' not in path:
                yield path


def main():
    pumped = ''
    for path in PUMPS:
        with open(path, encoding='utf-8') as handle:
            pumped += handle.read()
    problems = []
    for path in ui_files():
        with open(path, encoding='utf-8') as handle:
            text = handle.read()
        if 'Submit::sent(' not in text:
            continue
        settles = SETTLE.findall(text)
        if not any(re.search(r'\.%s\(' % name, pumped) for name in settles):
            problems.append(
                f'{path}: calls Submit::sent but no settle_* function of it is '
                f'called from {PUMPS[0]} or {PUMPS[1]}'
            )
    for problem in problems:
        print(problem)
    if problems:
        print('A Sending box with nothing to settle it stays busy for good; '
              'add a settle_* fn and call it from src/ui/box_submits.rs.')
        return 1
    print('submit settle OK')
    return 0


if __name__ == '__main__':
    sys.exit(main())
