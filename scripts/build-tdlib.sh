#!/usr/bin/env bash
# Build pinned TDLib (tdjson) with the reviewed Quill export extension.
# Does not download prebuilt binaries. Does not use Homebrew library paths
# as the runtime search path for the app.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PIN_COMMIT="$(sed -n 's/.*TDLIB_GIT_COMMIT: \&str = "\([0-9a-f]*\)".*/\1/p' "$ROOT/src/pins.rs")"
PIN_VERSION="$(sed -n 's/.*TDLIB_CMAKE_VERSION: \&str = "\([^"]*\)".*/\1/p' "$ROOT/src/pins.rs")"
SRC="${TDLIB_SRC:-$ROOT/native/td}"
BUILD="${TDLIB_BUILD:-$ROOT/native/build}"
PREFIX="${TDLIB_PREFIX:-$ROOT/native/prefix}"

echo "Pinned TDLib ${PIN_VERSION} @ ${PIN_COMMIT}"

if [[ ! -d "$SRC/.git" ]]; then
  git clone --depth 1 https://github.com/tdlib/td.git "$SRC"
  git -C "$SRC" fetch --depth 1 origin "$PIN_COMMIT"
  git -C "$SRC" checkout "$PIN_COMMIT"
fi

HEAD="$(git -C "$SRC" rev-parse HEAD)"
if [[ "$HEAD" != "$PIN_COMMIT" ]]; then
  echo "error: $SRC is $HEAD, expected $PIN_COMMIT" >&2
  exit 1
fi

# Keep the upstream schema pin intact in schema/. The native extension is
# reproducible, and unrelated local TDLib edits must not enter a packaged build.
PATCH="$ROOT/native/patches/tdlib-quill-takeout-contacts.patch"
if git -C "$SRC" diff --quiet && git -C "$SRC" diff --cached --quiet; then
  git -C "$SRC" apply --check "$PATCH"
  git -C "$SRC" apply "$PATCH"
elif ! git -C "$SRC" diff | cmp -s - "$PATCH" || ! git -C "$SRC" diff --cached --quiet; then
  echo "error: $SRC has changes other than the pinned Quill export patch" >&2
  exit 1
fi

# Linux: let libtdjson.so find bundled libssl/libcrypto/libz next to itself
# ($ORIGIN) instead of only the system search path. No effect on macOS, where
# scripts/macos-package-smoke.sh rewrites install names after the fact.
EXTRA_CMAKE=()
if [[ "$(uname -s)" == Linux ]]; then
  EXTRA_CMAKE+=('-DCMAKE_INSTALL_RPATH=$ORIGIN' -DCMAKE_BUILD_WITH_INSTALL_RPATH=ON)
fi

cmake -S "$SRC" -B "$BUILD" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$PREFIX" \
  -DTD_ENABLE_LTO=ON \
  ${EXTRA_CMAKE[@]+"${EXTRA_CMAKE[@]}"}

cmake --build "$BUILD" --target install --parallel "${JOBS:-$(nproc 2>/dev/null || sysctl -n hw.ncpu)}"

LIB=""
for candidate in \
  "$PREFIX/lib/libtdjson.dylib" \
  "$PREFIX/lib/libtdjson.so" \
  "$PREFIX/bin/tdjson.dll"
do
  if [[ -f "$candidate" ]]; then
    LIB="$candidate"
    break
  fi
done

if [[ -z "$LIB" ]]; then
  echo "error: tdjson was not installed into $PREFIX" >&2
  exit 1
fi

shasum -a 256 "$LIB" | tee "$PREFIX/tdjson.sha256"
echo "Built $LIB"
echo "Set QUILL_TDJSON_PATH=$LIB for a developer run, or copy into the app bundle (see docs/native-bundle.md)."
