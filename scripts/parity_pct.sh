#!/usr/bin/env bash
# parity_pct.sh — compute the Telegram-parity percentage from the STATUS.md checklist.
# Usage: scripts/parity_pct.sh [path-to-STATUS.md]
# Counts only the "## Status" section checkboxes: checked / total.
set -euo pipefail

STATUS="${1:-STATUS.md}"
if [ ! -f "$STATUS" ]; then
  echo "error: $STATUS not found" >&2
  exit 1
fi

section="$(awk '/^## Status/{flag=1;next} /^## /{flag=0} flag' "$STATUS")"
total="$(printf '%s\n' "$section" | grep -c -- '^- \[[ x]\]' || true)"
done="$(printf '%s\n' "$section" | grep -c -- '^- \[x\]' || true)"
pct="$(awk -v d="$done" -v t="$total" 'BEGIN { printf "%.1f", (t > 0 ? 100 * d / t : 0) }')"
echo "parity: $done/$total ($pct%)"
