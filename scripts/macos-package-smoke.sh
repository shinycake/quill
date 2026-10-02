#!/usr/bin/env bash
# macOS compile/package smoke: produce a layout that does not depend on Homebrew.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist/Quill.app"
BIN="$ROOT/target/release/quill"

echo "Building current release binary…"
cargo build --features ui --release --locked --manifest-path "$ROOT/Cargo.toml"

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
  <key>NSCameraUsageDescription</key><string>Quill uses your camera for video calls and video messages.</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
</dict>
</plist>
PLIST

cp "$BIN" "$DIST/Contents/MacOS/quill"

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

echo "Assembled $DIST"
if command -v otool >/dev/null; then
  echo "otool -L (must not list /opt/homebrew for tdjson):"
  otool -L "$DIST/Contents/MacOS/quill" || true
  if [[ -f "$DIST/Contents/Frameworks/libtdjson.dylib" ]]; then
    otool -L "$DIST/Contents/Frameworks/libtdjson.dylib" || true
  else
    echo "tdjson not bundled in this smoke (set QUILL_TDJSON_PATH to include it)."
  fi
fi
