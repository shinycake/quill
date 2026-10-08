#!/usr/bin/env bash
# macOS compile/package smoke: produce a layout that does not depend on Homebrew.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist/Quill.app"
BIN="${QUILL_BIN:-$ROOT/target/release/quill}"

if [[ -z "${QUILL_BIN:-}" ]]; then
  echo "Building current release binary…"
  cargo build --features ui --release --locked --manifest-path "$ROOT/Cargo.toml"
fi

rm -rf "$DIST"
mkdir -p "$DIST/Contents/MacOS" "$DIST/Contents/Frameworks" "$DIST/Contents/Resources"

cat > "$DIST/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Quill</string>
  <key>CFBundleIdentifier</key><string>org.shinycake.quill</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleExecutable</key><string>quill</string>
  <key>NSMicrophoneUsageDescription</key><string>Quill uses your microphone for voice and video calls.</string>
  <key>NSCameraUsageDescription</key><string>Quill uses your camera for video calls, video messages, and scanning login QR codes to link devices.</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
</dict>
</plist>
PLIST

cp "$BIN" "$DIST/Contents/MacOS/quill"
bash "$ROOT/scripts/build-qr-scanner.sh"
cp "$ROOT/target/qr-scanner/quill-qr-scanner" "$DIST/Contents/MacOS/quill-qr-scanner"

if [[ -n "${QUILL_TDJSON_PATH:-}" && -f "${QUILL_TDJSON_PATH}" ]]; then
  cp "${QUILL_TDJSON_PATH}" "$DIST/Contents/Frameworks/libtdjson.dylib"
  if command -v install_name_tool >/dev/null; then
    install_name_tool -id @rpath/libtdjson.dylib "$DIST/Contents/Frameworks/libtdjson.dylib" || true
    install_name_tool -add_rpath @executable_path/../Frameworks "$DIST/Contents/MacOS/quill" || true
  fi
fi

NTG_LIB="${QUILL_NTGCALLS_LIB:-$ROOT/vendor/ntgcalls/lib/libntgcalls.dylib}"
if [[ -f "$NTG_LIB" ]]; then
  cp "$NTG_LIB" "$DIST/Contents/Frameworks/libntgcalls.dylib"
fi

RLOTTIE="${QUILL_RLOTTIE_PATH:-$ROOT/vendor/rlottie/prefix/lib/librlottie.dylib}"
if [[ -f "$RLOTTIE" ]]; then
  cp -L "$RLOTTIE" "$DIST/Contents/Frameworks/librlottie.dylib"
  install_name_tool -id @rpath/librlottie.dylib "$DIST/Contents/Frameworks/librlottie.dylib"
  if [[ -d "$ROOT/vendor/rlottie/source/licenses" ]]; then
    mkdir -p "$DIST/Contents/Resources/rlottie-licenses"
    cp "$ROOT/vendor/rlottie/source/COPYING" "$ROOT/vendor/rlottie/source/licenses/"* "$DIST/Contents/Resources/rlottie-licenses/"
  fi
fi

# Bundle every non-system dylib the bundled libraries link (OpenSSL for tdjson,
# and anything else a Homebrew-built library drags in), rewrite their ids to
# @rpath/<name> and every reference to @loader_path/<name>, so the app depends
# only on the OS. Runs to a fixed point because copied libs may have deps too.
is_os_ref() { [[ "$1" == /usr/lib/* || "$1" == /System/Library/* || "$1" == @* ]]; }

vendor_deps() {
  local FW="$DIST/Contents/Frameworks" changed=1 lib dep real name
  while (( changed )); do
    changed=0
    for lib in "$FW"/*.dylib; do
      [[ -f "$lib" ]] || continue
      while IFS= read -r dep; do
        is_os_ref "$dep" && continue
        real="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$dep")"
        name="$(basename "$dep")"
        if [[ ! -f "$real" ]]; then
          echo "error: $lib needs $dep which does not exist on this machine" >&2
          exit 1
        fi
        if [[ ! -f "$FW/$name" ]]; then
          cp "$real" "$FW/$name"
          chmod u+w "$FW/$name"
          install_name_tool -id "@rpath/$name" "$FW/$name"
          echo "  bundled $dep -> Frameworks/$name"
        fi
        install_name_tool -change "$dep" "@loader_path/$name" "$lib"
        changed=1
      done < <(otool -L "$lib" | tail -n +2 | sed -E 's/^[[:space:]]+//; s/ \(compatibility version.*$//')
    done
  done
}
vendor_deps

# install_name_tool invalidates signatures. Re-sign inside-out: nested dylibs
# and helpers first, then the app. Ad-hoc by default; set QUILL_CODESIGN_IDENTITY
# (e.g. an "Apple Development: ..." identity) for a stable local signature.
SIGN_ID="${QUILL_CODESIGN_IDENTITY:--}"
for item in "$DIST"/Contents/Frameworks/*.dylib "$DIST/Contents/MacOS/quill-qr-scanner" "$DIST"; do
  [[ -e "$item" ]] && codesign --force --sign "$SIGN_ID" "$item"
done

echo "Assembled $DIST"
bash "$ROOT/scripts/check-bundle-macho.sh" "$DIST"
if command -v otool >/dev/null; then
  echo "otool -L (must not list /opt/homebrew for tdjson):"
  otool -L "$DIST/Contents/MacOS/quill" || true
  if [[ -f "$DIST/Contents/Frameworks/libtdjson.dylib" ]]; then
    otool -L "$DIST/Contents/Frameworks/libtdjson.dylib" || true
  else
    echo "tdjson not bundled in this smoke (set QUILL_TDJSON_PATH to include it)."
  fi
fi
