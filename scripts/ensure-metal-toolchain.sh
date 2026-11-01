#!/usr/bin/env bash
# Xcode 26 and later ship the Metal compiler as a separate download, and a build
# with QUILL_REQUIRE_METALLIB fails without it.
set -euo pipefail

if xcrun metal --version >/dev/null 2>&1; then
  exit 0
fi
xcodebuild -downloadComponent MetalToolchain
xcrun metal --version
