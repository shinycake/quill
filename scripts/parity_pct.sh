#!/usr/bin/env bash
# parity_pct.sh — compute the Telegram-parity percentage from the README.md checklist.
# Usage: scripts/parity_pct.sh [path-to-README.md]
# Counts only the "## Status" section checkboxes: checked / total.
set -euo pipefail

README="${1:-README.md}"
if [ ! -f "$README" ]; then
  echo "error: $README not found" >&2
  exit 1
fi

section="$(awk '/^## Status/{flag=1;next} /^## /{flag=0} flag' "$README")"
total="$(printf '%s\n' "$section" | grep -c -- '^- \[[ x]\]' || true)"
done="$(printf '%s\n' "$section" | grep -c -- '^- \[x\]' || true)"
pct="$(awk -v d="$done" -v t="$total" 'BEGIN { printf "%.1f", (t > 0 ? 100 * d / t : 0) }')"
echo "parity: $done/$total ($pct%)"
