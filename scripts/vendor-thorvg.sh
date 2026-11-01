#!/usr/bin/env bash
# Copy the part of ThorVG that Quill builds (CPU engine, Lottie loader without
# expressions, raw loader, C API) into third_party/thorvg/, apply Quill's patch,
# and record the tag and commit in third_party/thorvg/VERSION. build.rs compiles
# it with `cc`.
#
# Usage: scripts/vendor-thorvg.sh <tag> [thorvg-checkout]
#   With no checkout, the tag is shallow-cloned from github.com/thorvg/thorvg.
#   A checkout must have <tag> checked out (no local changes are checked).
# Bumping ThorVG = rerun with the new tag, then follow docs/dependency-updates.md.
set -euo pipefail

TAG="${1:?usage: scripts/vendor-thorvg.sh <tag> [thorvg-checkout]}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/third_party/thorvg"
PATCH="$ROOT/native/patches/thorvg-sticker-limits.patch"

if [[ $# -ge 2 ]]; then
  SRC="$(cd "$2" && pwd)"
else
  TMP="$(mktemp -d)"
  trap 'rm -rf "$TMP"' EXIT
  git clone --quiet --depth 1 --branch "$TAG" https://github.com/thorvg/thorvg.git "$TMP/thorvg"
  SRC="$TMP/thorvg"
fi

COMMIT="$(git -C "$SRC" rev-parse HEAD)"
TAGGED="$(git -C "$SRC" rev-parse "refs/tags/$TAG^{commit}")"
if [[ "$COMMIT" != "$TAGGED" ]]; then
  echo "error: $SRC is at $COMMIT, not $TAG ($TAGGED)" >&2
  exit 1
fi
VERSION="$(sed -n -E "s/^ *version *: *'([0-9.]+)'.*/\1/p" "$SRC/meson.build" | head -1)"
[[ -n "$VERSION" ]] || { echo "error: no version in $SRC/meson.build" >&2; exit 1; }

CHANGES="$(cat "$DEST/QUILL-CHANGES.md")"
rm -rf "$DEST"
mkdir -p "$DEST"
printf '%s\n' "$CHANGES" >"$DEST/QUILL-CHANGES.md"

# copy <dir> <find args...>: the matching files of $SRC/<dir>, same layout.
copy() {
  local dir="$1"
  shift
  (cd "$SRC" && find "$dir" -type f "$@" -print0) |
    while IFS= read -r -d '' file; do
      mkdir -p "$DEST/$(dirname "$file")"
      cp "$SRC/$file" "$DEST/$file"
    done
}

cp "$SRC/LICENSE" "$DEST/LICENSE"
copy inc -maxdepth 1 -name '*.h'
copy src/common -maxdepth 1 \( -name '*.cpp' -o -name '*.h' \)
copy src/renderer -maxdepth 1 \( -name '*.cpp' -o -name '*.h' \)
copy src/renderer/cpu_engine -maxdepth 1 \( -name '*.cpp' -o -name '*.h' \)
copy src/loaders/raw -maxdepth 1 \( -name '*.cpp' -o -name '*.h' \)
# The expressions header stays: with THORVG_LOTTIE_EXPRESSIONS_SUPPORT unset it
# holds the no-op stubs the parser and builder call. Its .cpp and jerryscript go.
copy src/loaders/lottie -maxdepth 1 \( -name '*.cpp' -o -name '*.h' \) ! -name 'tvgLottieExpressions.cpp'
copy src/loaders/lottie/rapidjson
copy src/bindings/capi -maxdepth 1 \( -name '*.cpp' -o -name '*.h' \)

[ -n "${THORVG_NO_PATCH:-}" ] || patch --quiet --fuzz=0 --no-backup-if-mismatch -p1 -d "$DEST" <"$PATCH"

printf 'tag %s\ncommit %s\nversion %s\n' "$TAG" "$COMMIT" "$VERSION" >"$DEST/VERSION"
echo "Vendored ThorVG $TAG ($COMMIT) into third_party/thorvg: $(find "$DEST" -type f | wc -l | tr -d ' ') files"
