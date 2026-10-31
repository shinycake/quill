#!/usr/bin/env bash
# Build the pinned OpenSSL (shared libraries) for the macOS app and call
# library. Homebrew's bottles target the macOS they were built on, and Quill
# supports older releases than the hosted runner runs, so the libraries bundled
# into Quill.app are built here for MACOSX_DEPLOYMENT_TARGET instead.
#
# Output: native/openssl/prefix/{include,lib}. Point OPENSSL_ROOT_DIR at it.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${QUILL_OPENSSL_DEST:-$ROOT/native/openssl}"
PREFIX="$DEST/prefix"
VERSION=3.5.9
SHA256=603f5602e2eef00d77fbd429d34dcd5822bb301757a1bc9cdb24c670f1eb859a
JOBS="${JOBS:-$(sysctl -n hw.ncpu)}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-14.0}"

case "$(uname -m)" in
  arm64) TARGET=darwin64-arm64-cc ;;
  x86_64) TARGET=darwin64-x86_64-cc ;;
  *) echo "error: unsupported architecture $(uname -m)" >&2; exit 2 ;;
esac

mkdir -p "$DEST"
TARBALL="$DEST/openssl-$VERSION.tar.gz"
curl -fsSL --retry 3 -o "$TARBALL" \
  "https://github.com/openssl/openssl/releases/download/openssl-$VERSION/openssl-$VERSION.tar.gz"
echo "$SHA256  $TARBALL" | shasum -a 256 -c -

SRC="$DEST/source"
rm -rf "$SRC" "$PREFIX"
mkdir -p "$SRC"
tar -xzf "$TARBALL" -C "$SRC" --strip-components=1

cd "$SRC"
./Configure "$TARGET" shared no-tests no-docs --prefix="$PREFIX" --libdir=lib
make -j"$JOBS"
make install_sw
rm -rf "$SRC" "$TARBALL"
