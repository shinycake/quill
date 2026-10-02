#!/usr/bin/env bash
# apply-parity-fragment.sh — merge-pipeline helper. Run on a main checkout
# AFTER a PR's merge is confirmed, with the fragment files the merge added:
#   bash scripts/apply-parity-fragment.sh parity-fragments/<slice-id>.txt [...]
# For each declared ID it flips the matching README.md Status checkbox to [x],
# then deletes the fragment files. Prints a summary; the pipeline commits.
# Idempotent: already-checked boxes and unknown IDs are skipped, never fatal.
set -euo pipefail

[ "$#" -ge 1 ] || { echo "usage: $0 <fragment> [...]" >&2; exit 1; }

checked=0
skipped=0
for frag in "$@"; do
  [ -f "$frag" ] || { echo "apply: $frag not found, skipping" >&2; continue; }
  while IFS= read -r line || [ -n "$line" ]; do
    id="$(printf '%s' "$line" | sed 's/#.*//; s/^[[:space:]]*//; s/[[:space:]]*$//')"
    [ -z "$id" ] && continue
    if grep -q -F "<!-- $id -->" README.md; then
      # flip "- [ ] ... <!-- id -->" to "- [x] ..." (only unchecked lines)
      if grep -q -E "^- \[ \] .*<!--[ ]*$id[ ]*-->" README.md; then
        sed -E "s/^- \[ \](.*<!--[ ]*${id}[ ]*-->)/- [x]\1/" README.md > README.md.tmp
        mv README.md.tmp README.md
        checked=$((checked + 1))
      fi
    else
      echo "apply: '$id' has no README anchor (checklist changed?) — skipped" >&2
      skipped=$((skipped + 1))
    fi
  done < "$frag"
  rm -f "$frag"
  echo "apply: consumed $frag"
done

# One-time migration: the old contract sentence becomes stale once fragments land.
if grep -q "Every merged feature PR checks its boxes in this list\." README.md; then
  sed 's|Every merged feature PR checks its boxes in this list\.|Feature PRs declare completed items in parity-fragments/<slice-id>.txt; the merge pipeline checks the boxes here after each merge (parity may lag a merge by a few minutes).|' README.md > README.md.tmp
  mv README.md.tmp README.md
  echo "apply: updated README contract sentence (one-time migration)"
fi

echo "apply: done — $checked box(es) checked, $skipped skipped"
