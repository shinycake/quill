#!/usr/bin/env bash
# apply-parity-fragment.sh — merge-pipeline helper. Run on a main checkout
# AFTER a PR's merge is confirmed, with the fragment files the merge added:
#   bash scripts/apply-parity-fragment.sh parity-fragments/<slice-id>.txt [...]
# For each declared ID it flips the matching STATUS.md Status checkbox to [x],
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
    # Accept bare IDs ("premium-promo-page") as well as "parity:premium-promo-page".
    case "$id" in parity:*) ;; *) id="parity:$id" ;; esac
    if grep -q -F "<!-- $id -->" STATUS.md; then
      # flip "- [ ] ... <!-- id -->" to "- [x] ..." (only unchecked lines)
      if grep -q -E "^- \[ \] .*<!--[ ]*$id[ ]*-->" STATUS.md; then
        sed -E "s/^- \[ \](.*<!--[ ]*${id}[ ]*-->)/- [x]\1/" STATUS.md > STATUS.md.tmp
        mv STATUS.md.tmp STATUS.md
        checked=$((checked + 1))
      fi
    else
      echo "apply: '$id' has no STATUS.md anchor (checklist changed?) — skipped" >&2
      skipped=$((skipped + 1))
    fi
  done < "$frag"
  rm -f "$frag"
  echo "apply: consumed $frag"
done

# One-time migration: the old contract sentence becomes stale once fragments land.
if grep -q "Every merged feature PR checks its boxes in this list\." STATUS.md; then
  sed 's|Every merged feature PR checks its boxes in this list\.|Feature PRs declare completed items in parity-fragments/<slice-id>.txt; the merge pipeline checks the boxes here after each merge (parity may lag a merge by a few minutes).|' STATUS.md > STATUS.md.tmp
  mv STATUS.md.tmp STATUS.md
  echo "apply: updated STATUS.md contract sentence (one-time migration)"
fi

echo "apply: done — $checked box(es) checked, $skipped skipped"
