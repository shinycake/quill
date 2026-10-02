#!/usr/bin/env bash
# Prepare the standalone binary asset expected by GitHub release checks.
set -euo pipefail
BIN="${1:-target/release/quill}"
OUT="${2:-dist/update}"
NAME="$("$BIN" --release-asset-name)"
[[ "$("$BIN" --build-info)" == ui ]]
[[ "$NAME" =~ ^quill-(macos|linux)-(aarch64|x86_64)$ ]]
"$BIN" --version
mkdir -p "$OUT"
cp "$BIN" "$OUT/$NAME"
chmod 755 "$OUT/$NAME"
shasum -a 256 "$OUT/$NAME" > "$OUT/$NAME.sha256"
echo "Prepared $OUT/$NAME (upload this binary; GitHub supplies its SHA-256 asset digest)."
