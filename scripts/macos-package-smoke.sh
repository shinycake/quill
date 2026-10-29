#!/usr/bin/env bash
# macOS compile/package smoke: produce a layout that does not depend on Homebrew.
set -euo pipefail
# Say which command failed instead of exiting silently under set -e.
trap 'echo "error: macos-package-smoke.sh line $LINENO failed: $BASH_COMMAND" >&2' ERR

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist/Quill.app"
BIN="${QUILL_BIN:-$ROOT/target/release/quill}"

if [[ -z "${QUILL_BIN:-}" ]]; then
  echo "Building current release binary…"
  cargo build --features ui --release --locked --manifest-path "$ROOT/Cargo.toml"
fi

# Bundle version = the binary's own version (Cargo.toml), e.g. "Quill 0.2.0".
VERSION="$("$BIN" --version | sed -n 's/^Quill \([0-9][0-9A-Za-z.+-]*\)$/\1/p')"
[[ -n "$VERSION" ]] || { echo "error: $BIN --version did not print 'Quill <version>'" >&2; exit 1; }

rm -rf "$DIST"
mkdir -p "$DIST/Contents/MacOS" "$DIST/Contents/Frameworks" "$DIST/Contents/Resources"

cat > "$DIST/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Quill</string>
  <key>CFBundleIdentifier</key><string>org.shinycake.quill</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleExecutable</key><string>quill</string>
  <key>CFBundleIconFile</key><string>Quill</string>
  <key>NSMicrophoneUsageDescription</key><string>Quill uses your microphone for voice and video calls.</string>
  <key>NSCameraUsageDescription</key><string>Quill uses your camera for video calls, video messages, and scanning login QR codes to link devices.</string>
  <key>NSBluetoothAlwaysUsageDescription</key><string>Quill uses Bluetooth to connect to your phone when you add or use a passkey from it with a QR code.</string>
  <key>NSLocationUsageDescription</key><string>Quill shows where you are on the map when you open Send Location in a chat.</string>
  <key>NSLocationWhenInUseUsageDescription</key><string>Quill shows where you are on the map when you open Send Location in a chat.</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>CFBundleURLTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>CFBundleURLName</key><string>org.shinycake.quill</string>
      <key>CFBundleURLSchemes</key>
      <array>
        <string>tg</string>
      </array>
    </dict>
  </array>
</dict>
</plist>
PLIST

cp "$BIN" "$DIST/Contents/MacOS/quill"
cp "$ROOT/assets/icons/Quill.icns" "$DIST/Contents/Resources/Quill.icns"
# LICENSE, third-party notices and license texts (Settings → Open-source licenses).
bash "$ROOT/scripts/stage-licenses.sh" "$DIST/Contents/Resources"
bash "$ROOT/scripts/build-qr-scanner.sh"
cp "$ROOT/target/qr-scanner/quill-qr-scanner" "$DIST/Contents/MacOS/quill-qr-scanner"
# Mini apps run in a helper process (docs/decisions/codex-miniapp-webview.md);
# Quill looks for it next to its own executable.
WEBVIEW_BIN="${QUILL_WEBVIEW_BIN:-$ROOT/target/release/quill-webview}"
if [[ ! -f "$WEBVIEW_BIN" ]]; then
  cargo build --release --locked -p quill-webview --manifest-path "$ROOT/Cargo.toml"
fi
cp "$WEBVIEW_BIN" "$DIST/Contents/MacOS/quill-webview"

if [[ -n "${QUILL_TDJSON_PATH:-}" && -f "${QUILL_TDJSON_PATH}" ]]; then
  cp "${QUILL_TDJSON_PATH}" "$DIST/Contents/Frameworks/libtdjson.dylib"
  if command -v install_name_tool >/dev/null; then
    install_name_tool -id @rpath/libtdjson.dylib "$DIST/Contents/Frameworks/libtdjson.dylib" || true
    install_name_tool -add_rpath @executable_path/../Frameworks "$DIST/Contents/MacOS/quill" || true
  fi
fi

# The call library (scripts/build-tgcalls.sh, docs/decisions/tgcalls-engine.md).
# It links FFmpeg and OpenSSL as @rpath/<name> and finds them through its
# @loader_path rpath, so each goes into Frameworks beside it: FFmpeg from the
# prefix it was built against, OpenSSL from OPENSSL_ROOT_DIR (Homebrew's
# openssl@3 by default, the one tdjson links too). quill-calls-sys looks for
# the library in Frameworks.
CALLS_DIR="${QUILL_CALLS_DIR:-$ROOT/vendor/quill-calls}"
FFMPEG_PREFIX="${QUILL_FFMPEG_PREFIX:-$ROOT/vendor/ffmpeg/prefix}"
OPENSSL_DIR="${OPENSSL_ROOT_DIR:-$(brew --prefix openssl@3 2>/dev/null || true)}"
CALLS_LIB="$CALLS_DIR/lib/libquill_calls.dylib"
if [[ -f "$CALLS_LIB" ]]; then
  FW="$DIST/Contents/Frameworks"
  cp -L "$CALLS_LIB" "$FW/libquill_calls.dylib"
  chmod u+w "$FW/libquill_calls.dylib"
  install_name_tool -id @rpath/libquill_calls.dylib "$FW/libquill_calls.dylib"
  while IFS= read -r dep; do
    name="${dep#@rpath/}"
    [[ "$dep" == @rpath/* && "$name" != libquill_calls.dylib && ! -f "$FW/$name" ]] || continue
    if [[ -f "$FFMPEG_PREFIX/lib/$name" ]]; then
      src="$FFMPEG_PREFIX/lib/$name"
    elif [[ -n "$OPENSSL_DIR" && -f "$OPENSSL_DIR/lib/$name" ]]; then
      src="$OPENSSL_DIR/lib/$name"
    else
      echo "error: libquill_calls.dylib needs $name, found in neither $FFMPEG_PREFIX/lib nor OpenSSL" >&2
      exit 1
    fi
    cp -L "$src" "$FW/$name"
    chmod u+w "$FW/$name"
    install_name_tool -id "@rpath/$name" "$FW/$name"
    echo "  bundled $src -> Frameworks/$name (for libquill_calls)"
  done < <(otool -L "$FW/libquill_calls.dylib" | tail -n +2 | sed -E 's/^[[:space:]]+//; s/ \(compatibility version.*$//')
  mkdir -p "$DIST/Contents/Resources/licenses/quill-calls" "$DIST/Contents/Resources/licenses/ffmpeg"
  cp "$CALLS_DIR/share/SOURCES.txt" "$DIST/Contents/Resources/licenses/quill-calls/SOURCES.txt"
  cp -R "$CALLS_DIR/share/licenses" "$DIST/Contents/Resources/licenses/quill-calls/licenses"
  if [[ -d "$FFMPEG_PREFIX/share/quillvideo" ]]; then
    cp "$FFMPEG_PREFIX"/share/quillvideo/* "$DIST/Contents/Resources/licenses/ffmpeg/"
  fi
  chmod -R u=rwX,go=rX "$DIST/Contents/Resources/licenses/quill-calls" "$DIST/Contents/Resources/licenses/ffmpeg"
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
# (e.g. an "Apple Development: ..." identity) for a stable signature. A
# certificate signature is timestamped so it stays valid after the
# certificate expires.
SIGN_ID="${QUILL_CODESIGN_IDENTITY:--}"
SIGN_FLAGS=(--force --sign "$SIGN_ID")
[[ "$SIGN_ID" == - ]] || SIGN_FLAGS+=(--timestamp)
for item in "$DIST"/Contents/Frameworks/*.dylib "$DIST/Contents/MacOS/quill-qr-scanner" "$DIST/Contents/MacOS/quill-webview" "$DIST"; do
  [[ -e "$item" ]] && codesign "${SIGN_FLAGS[@]}" "$item"
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
