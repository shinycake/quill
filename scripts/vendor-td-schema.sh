#!/usr/bin/env bash
# Replace schema/td_api.tl with the official file at the pinned TDLib commit.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PIN_COMMIT="$(sed -n 's/.*TDLIB_GIT_COMMIT: \&str = "\([0-9a-f]*\)".*/\1/p' "$ROOT/src/pins.rs")"
URL="https://raw.githubusercontent.com/tdlib/td/${PIN_COMMIT}/td/generate/scheme/td_api.tl"
DEST="$ROOT/schema/td_api.tl"
TMP="$(mktemp)"

echo "Fetching $URL"
curl -fsSL --retry 4 --retry-delay 2 -o "$TMP" "$URL"
BYTES="$(wc -c < "$TMP" | tr -d ' ')"
HASH="$(sha256sum "$TMP" | awk '{print $1}')"
echo "official size=$BYTES sha256=$HASH"
if ! grep -q 'vector<' "$TMP"; then
  echo "error: fetched schema is missing typed vector<T> parameters" >&2
  exit 1
fi
mv "$TMP" "$DEST"
echo "Wrote $DEST"
echo "Update src/pins.rs TD_API_TL_SHA256 / TD_API_TL_BYTES if they do not match."
