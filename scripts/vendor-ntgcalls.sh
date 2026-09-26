#!/usr/bin/env bash
# Vendor the ntgcalls prebuilt shared library (Linux x86_64) for Quill's
# Phase C2a media engine. Quill NEVER links this library at build time:
# the `ntgcalls-sys` crate loads it at runtime via dlopen (LGPLv3 sidecar
# compliance — see THIRD_PARTY.md). The extracted files live under
# vendor/ntgcalls/ and are git-ignored; this script is the reproducible
# procurement step.
#
# Re-runnable: skips the download when the local file matches the pinned
# checksum. Fails loudly on any checksum mismatch.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="$REPO_ROOT/vendor/ntgcalls"
# Note: do NOT stage in $TMPDIR — the sandbox's /tmp is a small tmpfs that a
# 40MB zip can fill. ~/.cache persists and has room.
STAGE_DIR="${QUILL_VENDOR_STAGE:-$HOME/.cache/quill-vendor}/ntgcalls"

# Pinned release + integrity. v3.0.0 (2026-09-25) publishes no checksums on
# the release page, so this SHA256 was observed by the implementer on
# 2026-09-26 and is pinned here. If upstream ever publishes checksums,
# prefer theirs.
NTGCALLS_VERSION="v3.0.0"
NTGCALLS_URL="https://github.com/pytgcalls/ntgcalls/releases/download/${NTGCALLS_VERSION}/ntgcalls.linux-x86_64-shared_libs.zip"
NTGCALLS_SHA256="b28f99eec39ae62a9c612da1e16b2884c5662f32c52effc0d985a6918f2831f0"

need() {
    command -v "$1" >/dev/null 2>&1 || { echo "vendor-ntgcalls: missing required tool: $1" >&2; exit 1; }
}
need curl
need unzip
need sha256sum

mkdir -p "$STAGE_DIR"
ZIP="$STAGE_DIR/ntgcalls.linux-x86_64-shared_libs.zip"

if [ -f "$ZIP" ]; then
    echo "vendor-ntgcalls: zip already present in stage dir, re-verifying checksum"
else
    echo "vendor-ntgcalls: downloading $NTGCALLS_URL"
    curl -fsSL -o "$ZIP" "$NTGCALLS_URL"
fi

echo "vendor-ntgcalls: verifying SHA256"
ACTUAL="$(sha256sum "$ZIP" | awk '{print $1}')"
if [ "$ACTUAL" != "$NTGCALLS_SHA256" ]; then
    echo "vendor-ntgcalls: CHECKSUM MISMATCH" >&2
    echo "  expected: $NTGCALLS_SHA256" >&2
    echo "  actual:   $ACTUAL" >&2
    echo "  Refusing to vendor. If upstream re-cut the release, verify the new" >&2
    echo "  artifact out-of-band and update NTGCALLS_SHA256 in this script." >&2
    rm -f "$ZIP"
    exit 1
fi
echo "vendor-ntgcalls: checksum OK"

# Re-extract idempotently: wipe + unzip so a stale vendor dir can't linger.
rm -rf "$VENDOR_DIR"
mkdir -p "$VENDOR_DIR"
unzip -q -o "$ZIP" -d "$VENDOR_DIR"

LIB="$VENDOR_DIR/lib/libntgcalls.so"
HDR="$VENDOR_DIR/include/ntgcalls.h"
[ -f "$LIB" ] || { echo "vendor-ntgcalls: expected $LIB after extraction" >&2; exit 1; }
[ -f "$HDR" ] || { echo "vendor-ntgcalls: expected $HDR after extraction" >&2; exit 1; }

echo "vendor-ntgcalls: exported ntg_* symbols: $(nm -D --defined-only "$LIB" | grep -c ' T ntg_')"
echo "vendor-ntgcalls: vendored into $VENDOR_DIR"
echo "vendor-ntgcalls: DONE (not committed to git by design; see .gitignore)"
