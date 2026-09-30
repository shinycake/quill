#!/usr/bin/env bash
# check-parity-fragments.sh — CI gate for the fragment-based parity contract.
# Usage: scripts/check-parity-fragments.sh [base-ref]   (default: origin/main)
# Fails the PR when:
#   1. it hand-edits README.md or DECISIONS.md (only the merge pipeline writes those),
#   2. a parity-fragments/*.txt line is not a valid parity ID,
#   3. a declared ID has no matching <!-- parity:... --> anchor in README.md,
#   4. the same ID is declared twice across fragments in this PR.
set -euo pipefail

BASE="${1:-origin/main}"
fail() { echo "parity-fragments: ERROR: $*" >&2; exit 1; }

CHANGED="$(git diff --name-only "$BASE"...HEAD -- || true)"

# 1. README.md / DECISIONS.md are pipeline-owned now.
if printf '%s\n' "$CHANGED" | grep -qx -e 'README\.md' -e 'DECISIONS\.md'; then
  fail "this PR edits README.md or DECISIONS.md — forbidden. Declare completed items in parity-fragments/<slice-id>.txt and slice notes in docs/decisions/<slice-id>.md; the merge pipeline updates README.md after merge."
fi

# Fragments added or modified by this PR.
FRAGMENTS="$(printf '%s\n' "$CHANGED" | grep -x -e 'parity-fragments/[^/]*\.txt' || true)"
[ -z "$FRAGMENTS" ] && { echo "parity-fragments: no fragments changed — OK"; exit 0; }

tmpfile="$(mktemp)"
trap 'rm -f "$tmpfile"' EXIT

while IFS= read -r frag; do
  [ -f "$frag" ] || continue  # deleted fragment: nothing to validate
  lineno=0
  while IFS= read -r line || [ -n "$line" ]; do
    lineno=$((lineno + 1))
    # strip comments and whitespace
    id="$(printf '%s' "$line" | sed 's/#.*//; s/^[[:space:]]*//; s/[[:space:]]*$//')"
    [ -z "$id" ] && continue
    # 2. schema: parity:<area>-<slug>, lowercase alnum + hyphens
    printf '%s' "$id" | grep -q -x -E 'parity:[a-z0-9]+(-[a-z0-9]+)*' \
      || fail "$frag:$lineno: malformed parity ID '$id' (want parity:<area>-<slug>)"
    # 3. the ID must exist as a README anchor
    grep -q -F "<!-- $id -->" README.md \
      || fail "$frag:$lineno: '$id' has no matching <!-- $id --> anchor in README.md"
    printf '%s\n' "$id" >> "$tmpfile"
  done < "$frag"
done <<< "$FRAGMENTS"

# 4. no duplicate IDs across fragments in this PR
dups="$(sort "$tmpfile" | uniq -d || true)"
[ -n "$dups" ] && fail "duplicate parity IDs declared in this PR: $(printf '%s' "$dups" | tr '\n' ' ')"

count="$(wc -l < "$tmpfile" | tr -d ' ')"
echo "parity-fragments: OK ($count declared ID(s), all anchored in README.md)"
