#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/vendor/rlottie"
PIN=ea06d2f29ba01b8d06c00a838d107f5e484ae59b
CXX_FLAGS=""
if [[ "$(uname -s)" == Darwin && "$(uname -m)" == arm64 ]]; then
  # Upstream's ARM pixman assembly targets ELF; use its scalar renderer on macOS.
  CXX_FLAGS="-U__ARM_NEON__"
fi
if [[ ! -d "$DEST/source/.git" ]]; then
  git clone https://github.com/Samsung/rlottie.git "$DEST/source"
fi
git -C "$DEST/source" checkout --detach "$PIN"
cmake -S "$DEST/source" -B "$DEST/build" -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
  "-DCMAKE_CXX_FLAGS=$CXX_FLAGS" \
  -DBUILD_SHARED_LIBS=ON -DLOTTIE_MODULE=OFF -DLOTTIE_TEST=OFF \
  -DLIB_INSTALL_DIR="$DEST/prefix/lib" -DCMAKE_INSTALL_PREFIX="$DEST/prefix"
cmake --build "$DEST/build" --parallel 4
cmake --install "$DEST/build"
