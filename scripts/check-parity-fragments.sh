#!/usr/bin/env bash
# check-parity-fragments.sh — CI gate for the fragment-based parity contract.
# Usage: scripts/check-parity-fragments.sh [base-ref]   (default: origin/main)
# Fails the PR when:
#   1. it hand-edits STATUS.md or DECISIONS.md (only the merge pipeline writes those),
#   2. a parity-fragments/*.txt line is not a valid parity ID,
#   3. a declared ID has no matching <!-- parity:... --> anchor in STATUS.md,
#   4. the same ID is declared twice across fragments in this PR.
set -euo pipefail

BASE="${1:-origin/main}"
fail() { echo "parity-fragments: ERROR: $*" >&2; exit 1; }

# Fail closed: if the base diff can't be computed, the gate must not pass.
CHANGED="$(git diff --name-only "$BASE"...HEAD --)" \
  || fail "cannot compute diff against base ref '$BASE' (bad ref or incomplete fetch)"

# 1. Pipeline output must exactly match declarations already merged into the base.
if printf '%s\n' "$CHANGED" | grep -qx 'DECISIONS\.md'; then
  fail "this PR edits DECISIONS.md — forbidden. Declare completed items in parity-fragments/<slice-id>.txt and slice notes in docs/decisions/<slice-id>.md; the merge pipeline updates STATUS.md after merge."
fi
if printf '%s\n' "$CHANGED" | grep -qx 'STATUS\.md'; then
  MERGE_BASE="$(git merge-base "$BASE" HEAD)"
  # The checklist moved from README.md to STATUS.md; the move itself has no
  # generated base to compare with.
  if ! git cat-file -e "$MERGE_BASE:STATUS.md" 2>/dev/null; then
    echo "parity-fragments: STATUS.md is new on this branch (checklist move) — OK"
    exit 0
  fi
  printf '%s\n' "$CHANGED" | grep -qvE '^(STATUS\.md|parity-fragments/[^/]+\.txt)$' \
    && fail "checklist reconciliation must contain only generated STATUS.md and consumed fragments"
  CONSUMED="$(git diff --name-only --diff-filter=D "$MERGE_BASE" HEAD -- 'parity-fragments/*.txt')"
  [ -n "$CONSUMED" ] || fail "STATUS.md changed without consuming merged declarations"
  [ -z "$(git diff --name-only --diff-filter=AM "$MERGE_BASE" HEAD -- 'parity-fragments/*.txt')" ] \
    || fail "reconciliation cannot add or modify declarations"
  GENERATED="$(mktemp -d)"
  trap 'rm -rf "$GENERATED"' EXIT
  git show "$MERGE_BASE:STATUS.md" > "$GENERATED/STATUS.md"
  mkdir "$GENERATED/parity-fragments"
  FRAGMENT_ARGS=()
  while IFS= read -r frag; do
    git show "$MERGE_BASE:$frag" > "$GENERATED/$frag"
    FRAGMENT_ARGS+=("$frag")
  done <<< "$CONSUMED"
  HELPER="$(pwd)/scripts/apply-parity-fragment.sh"
  (cd "$GENERATED" && bash "$HELPER" "${FRAGMENT_ARGS[@]}")
  cmp -s STATUS.md "$GENERATED/STATUS.md" \
    || fail "STATUS.md differs from verified merged-fragment output"
  echo "parity-fragments: verified generated checklist reconciliation"
  exit 0
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
    # 3. the ID must exist as a STATUS.md anchor
    grep -q -F "<!-- $id -->" STATUS.md \
      || fail "$frag:$lineno: '$id' has no matching <!-- $id --> anchor in STATUS.md"
    printf '%s\n' "$id" >> "$tmpfile"
  done < "$frag"
done <<< "$FRAGMENTS"

# 4. no duplicate IDs across fragments in this PR
dups="$(sort "$tmpfile" | uniq -d || true)"
[ -n "$dups" ] && fail "duplicate parity IDs declared in this PR: $(printf '%s' "$dups" | tr '\n' ' ')"

count="$(wc -l < "$tmpfile" | tr -d ' ')"
echo "parity-fragments: OK ($count declared ID(s), all anchored in STATUS.md)"
