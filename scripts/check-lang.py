#!/usr/bin/env python3
"""Interface strings check (docs/decisions/language-packs.md).

Fails when:
- a UI file has more hard-coded English text than its entry in the
  baseline (scripts/lang-baseline.txt, "<count> <path>"); files not listed
  must have none;
- a key in lang/en/*.strings is not used anywhere in src/.

Text is a string literal of at least two words that starts with a capital
letter, outside test files and the lines of logs, panics and attributes.
The media viewer and player files are skipped until they are converted.

--update lowers baseline entries to the current counts and drops the ones
now at zero; it never adds or raises an entry.

Usage: python3 scripts/check-lang.py [--update]   (from the repository root)
"""
import os
import re
import sys

BASELINE = 'scripts/lang-baseline.txt'
SKIP_PATH = re.compile(r'test|demo|fixture|video|player|media_viewer|group_call')
SKIP_LINE = re.compile(
    r'^\s*(#\[|//)|\b(debug|info|warn|error|trace|log|eprintln|println|panic|unreachable|todo|assert\w*)!'
    r'|\.expect\(|\bQUILL_|concat!|include_str!'
)
LITERAL = re.compile(r'"((?:[^"\\]|\\.)*)"')
TEXT = re.compile(r"^[A-Z][a-z']+[,]? [A-Za-z'{}]+")


def ui_files():
    for root, _, names in os.walk('src/ui'):
        for name in sorted(names):
            path = os.path.join(root, name)
            if name.endswith('.rs') and not SKIP_PATH.search(path):
                yield path


def english_count(path):
    count = 0
    in_tests = False
    for line in open(path, encoding='utf-8'):
        if line.startswith('#[cfg(test)]'):
            in_tests = True
        if in_tests or SKIP_LINE.search(line):
            continue
        for lit in LITERAL.findall(line):
            if TEXT.match(lit) and '::' not in lit:
                count += 1
    return count


def load_baseline():
    out = {}
    if os.path.exists(BASELINE):
        for line in open(BASELINE, encoding='utf-8'):
            parts = line.split()
            if len(parts) == 2 and not line.startswith('#'):
                out[parts[1]] = int(parts[0])
    return out


def unused_keys():
    used = set()
    for root, _, names in os.walk('src'):
        for name in names:
            if name.endswith('.rs'):
                text = open(os.path.join(root, name), encoding='utf-8').read()
                used.update(re.findall(r'keys::([A-Z0-9_]+)', text))
    out = []
    for name in sorted(os.listdir('lang/en')):
        if not name.endswith('.strings'):
            continue
        for key in re.findall(r'^"([a-z0-9_]+)(?:#[a-z]+)?"\s*=', open(os.path.join('lang/en', name), encoding='utf-8').read(), re.M):
            const = 'Q_' + key[4:].upper() if key.startswith('qll_') else key[4:].upper()
            if const not in used:
                out.append('%s (lang/en/%s)' % (key, name))
    return sorted(set(out))


def main():
    update = '--update' in sys.argv
    baseline = load_baseline()
    counts = {path: english_count(path) for path in ui_files()}
    failed = False
    for path, count in sorted(counts.items()):
        allowed = baseline.get(path, 0)
        if count > allowed:
            failed = True
            print('ENGLISH %s: %d hard-coded strings (allowed %d). Use tr(keys::…) and add the text to lang/en/*.strings.' % (path, count, allowed))
    if update:
        lines = ['# Hard-coded English strings per UI file (scripts/check-lang.py). May only shrink.']
        for path in sorted(baseline):
            count = min(counts.get(path, 0), baseline[path])
            if count:
                lines.append('%d %s' % (count, path))
        open(BASELINE, 'w', encoding='utf-8').write('\n'.join(lines) + '\n')
    for key in unused_keys():
        failed = True
        print('UNUSED %s: no keys:: use in src/. Remove it.' % key)
    if failed:
        sys.exit(1)
    print('interface strings OK')


if __name__ == '__main__':
    main()
