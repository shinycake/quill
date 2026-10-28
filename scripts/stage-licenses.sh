#!/usr/bin/env bash
# Copy Quill's license and third-party notice files into a package directory.
#   scripts/stage-licenses.sh <dest-dir>
# Writes <dest>/LICENSE, THIRD_PARTY.md, THIRD_PARTY_LICENSES.md and
# <dest>/licenses/ (native-library license texts, ThorVG's notices, the Unicode
# emoji data license, the spellcheck word-list attribution). The app's
# "Open-source licenses" entry opens <dest>/THIRD_PARTY.md. FFmpeg's texts are
# staged separately by the Linux/Windows package scripts (licenses/ffmpeg/).
# scripts/windows-package.ps1 mirrors this list; keep the two in sync.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${1:?usage: stage-licenses.sh <dest-dir>}"

mkdir -p "$DEST/licenses/thorvg" "$DEST/licenses/unicode-emoji"
cp "$ROOT/LICENSE" "$ROOT/THIRD_PARTY.md" "$ROOT/THIRD_PARTY_LICENSES.md" "$DEST/"
cp "$ROOT"/licenses/*.txt "$DEST/licenses/"
# ThorVG is compiled into the binary; its texts come from the vendored tree.
cp "$ROOT/third_party/thorvg/LICENSE" "$DEST/licenses/thorvg/LICENSE"
cp "$ROOT/third_party/thorvg/src/loaders/lottie/rapidjson/LICENSE" "$DEST/licenses/thorvg/rapidjson-LICENSE"
cp "$ROOT/assets/emoji/LICENSE.txt" "$DEST/licenses/unicode-emoji/LICENSE.txt"
chmod 644 "$DEST/LICENSE" "$DEST/THIRD_PARTY.md" "$DEST/THIRD_PARTY_LICENSES.md" \
  "$DEST"/licenses/*.txt "$DEST"/licenses/thorvg/* "$DEST/licenses/unicode-emoji/LICENSE.txt"
