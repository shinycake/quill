#!/usr/bin/env bash
# Assemble the Linux package: a relocatable directory + tarball holding `quill`
# and the native libraries Quill loads at runtime, so it needs no env vars.
#
#   quill-linux-<arch>/
#     quill                      RUNPATH $ORIGIN/lib
#     quill-webview              RUNPATH $ORIGIN/lib  (mini apps; needs the system's WebKitGTK 4.1)
#     lib/libtdjson.so           RUNPATH $ORIGIN
#     lib/libquill_calls.so      RUNPATH $ORIGIN  (calls: tgcalls behind Quill's C API, LGPL-3.0)
#     lib/libquillvideo.so       RUNPATH $ORIGIN  (in-process video, native/quillvideo)
#     lib/libav{codec,format,util,filter,device}.so.N, libsw{scale,resample}.so.N   FFmpeg, LGPL-2.1+
#     ffmpeg, ffprobe            RUNPATH $ORIGIN/lib  (same FFmpeg build, no libx264; video send/edit, photos, camera)
#     licenses/ffmpeg/           FFmpeg license texts + exact source and configure line
#     licenses/quill-calls/      libquill_calls' source pins and its parts' notices
#     lib/libssl.so.3, libcrypto.so.3   (bundled OpenSSL, see decision doc)
#     share/applications/org.shinycake.quill.desktop, share/icons/hicolor/<size>/apps/quill.png
#     install.sh, README.txt, LICENSE, THIRD_PARTY.md, THIRD_PARTY_LICENSES.md
#     licenses/                  native-library license texts (scripts/stage-licenses.sh)
#
# Inputs (env, all optional):
#   QUILL_BIN            release binary            (default target/release/quill)
#   QUILL_WEBVIEW_BIN    mini-app helper           (default target/release/quill-webview)
#   QUILL_TDJSON_PATH    libtdjson.so              (default native/prefix/lib/libtdjson.so)
#   QUILL_CALLS_DIR      scripts/build-tgcalls.sh  (default vendor/quill-calls)
#   QUILL_FFMPEG_PREFIX  scripts/build-ffmpeg.sh   (default vendor/ffmpeg/prefix)
#   OUT                  output directory          (default dist/linux)
# Requires patchelf, readelf, ldd, tar.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
[[ "$(uname -s)" == Linux ]] || { echo "error: linux-package.sh must run on Linux" >&2; exit 2; }
for tool in patchelf readelf ldd tar; do
  command -v "$tool" >/dev/null || { echo "error: missing required tool: $tool" >&2; exit 2; }
done

BIN="${QUILL_BIN:-target/release/quill}"
WEBVIEW_BIN="${QUILL_WEBVIEW_BIN:-target/release/quill-webview}"
TDJSON="${QUILL_TDJSON_PATH:-native/prefix/lib/libtdjson.so}"
CALLS="${QUILL_CALLS_DIR:-vendor/quill-calls}"
FFMPEG="${QUILL_FFMPEG_PREFIX:-vendor/ffmpeg/prefix}"
QUILLVIDEO="$FFMPEG/lib/libquillvideo.so"
OUT="${OUT:-dist/linux}"
for f in "$BIN" "$WEBVIEW_BIN" "$TDJSON" "$CALLS/lib/libquill_calls.so" "$CALLS/share/SOURCES.txt" "$QUILLVIDEO" "$FFMPEG/bin/ffmpeg" "$FFMPEG/bin/ffprobe"; do
  [[ -f "$f" ]] || { echo "error: missing input $f" >&2; exit 2; }
done

NAME="$("$BIN" --release-asset-name)"
[[ "$NAME" =~ ^quill-linux-(x86_64|aarch64)$ ]] || { echo "error: unexpected asset name $NAME" >&2; exit 2; }
PKG="$OUT/$NAME"
TARBALL="$OUT/$NAME-bundle.tar.gz"

rm -rf "$PKG" "$TARBALL" "$TARBALL.sha256"
mkdir -p "$PKG/lib" "$PKG/share/applications" "$PKG/share/icons/hicolor"

install -m 755 "$BIN" "$PKG/quill"
install -m 755 "$WEBVIEW_BIN" "$PKG/quill-webview"
# cp -L: tdjson installs may be symlink chains; ship the real file under the name the loader opens.
install -m 755 "$(readlink -f "$TDJSON")" "$PKG/lib/libtdjson.so"
# quill-calls-sys looks for the call library in lib/ beside the executable.
install -m 755 "$(readlink -f "$CALLS/lib/libquill_calls.so")" "$PKG/lib/libquill_calls.so"
# In-process video: the shim plus FFmpeg's shared libraries under their
# sonames (dynamic linking keeps FFmpeg replaceable, as the LGPL asks).
install -m 755 "$(readlink -f "$QUILLVIDEO")" "$PKG/lib/libquillvideo.so"
ffmpeg_libs=0
for lib in "$FFMPEG"/lib/lib{avcodec,avformat,avutil,avfilter,avdevice,swscale,swresample}.so.*; do
  base="$(basename "$lib")"
  [[ "$base" =~ ^lib[a-z]+\.so\.[0-9]+$ ]] || continue
  install -m 755 "$(readlink -f "$lib")" "$PKG/lib/$base"
  ffmpeg_libs=$((ffmpeg_libs + 1))
done
(( ffmpeg_libs == 7 )) || { echo "error: expected 7 FFmpeg libraries in $FFMPEG/lib, found $ffmpeg_libs" >&2; exit 2; }
# The command-line tools Quill runs (src/media_tools.rs finds them next to `quill`).
install -m 755 "$FFMPEG/bin/ffmpeg" "$PKG/ffmpeg"
install -m 755 "$FFMPEG/bin/ffprobe" "$PKG/ffprobe"
mkdir -p "$PKG/licenses/ffmpeg"
install -m 644 "$FFMPEG"/share/quillvideo/* "$PKG/licenses/ffmpeg/"
mkdir -p "$PKG/licenses/quill-calls"
install -m 644 "$CALLS/share/SOURCES.txt" "$PKG/licenses/quill-calls/SOURCES.txt"
cp -R "$CALLS/share/licenses" "$PKG/licenses/quill-calls/licenses"
chmod -R u=rwX,go=rX "$PKG/licenses/quill-calls"

# Bundle OpenSSL: libssl.so.3 / libcrypto.so.3 as resolved for the shipped
# libraries (libtdjson needs them; libssl in turn needs libcrypto).
vendor_openssl() {
  local changed=1 lib dep path
  while (( changed )); do
    changed=0
    for lib in "$PKG"/lib/*.so*; do
      while read -r dep path; do
        case "$dep" in
          libssl.so.*|libcrypto.so.*)
            if [[ ! -e "$PKG/lib/$dep" ]]; then
              [[ -n "$path" && -f "$path" ]] || { echo "error: cannot resolve $dep for $lib" >&2; exit 1; }
              install -m 755 "$(readlink -f "$path")" "$PKG/lib/$dep"
              changed=1
            fi ;;
        esac
      done < <(env -u LD_LIBRARY_PATH ldd "$lib" | awk '$2 == "=>" {print $1, $3}')
    done
  done
}
vendor_openssl

# Only drop symbols from libraries we built ourselves; the distro OpenSSL is
# left as shipped.
strip --strip-unneeded "$PKG/lib/libtdjson.so" "$PKG/lib/libquill_calls.so" \
  "$PKG"/lib/libquillvideo.so "$PKG"/lib/libav*.so.* "$PKG"/lib/libsw*.so.* \
  "$PKG/ffmpeg" "$PKG/ffprobe" || true

patchelf --set-rpath '$ORIGIN/lib' "$PKG/quill"
patchelf --set-rpath '$ORIGIN/lib' "$PKG/quill-webview"
patchelf --set-rpath '$ORIGIN/lib' "$PKG/ffmpeg"
patchelf --set-rpath '$ORIGIN/lib' "$PKG/ffprobe"
for lib in "$PKG"/lib/*.so*; do
  patchelf --set-rpath '$ORIGIN' "$lib"
done

install -m 644 assets/org.shinycake.quill.desktop "$PKG/share/applications/org.shinycake.quill.desktop"
for icon in assets/icons/hicolor/*/apps/quill.png; do
  size="$(basename "$(dirname "$(dirname "$icon")")")"
  install -D -m 644 "$icon" "$PKG/share/icons/hicolor/$size/apps/quill.png"
done
install -m 755 scripts/linux-install.sh "$PKG/install.sh"
install -m 644 scripts/linux-package-README.txt "$PKG/README.txt"
bash scripts/stage-licenses.sh "$PKG"

bash scripts/check-bundle-elf.sh "$PKG"

tar -C "$OUT" --owner=0 --group=0 --numeric-owner -czf "$TARBALL" "$NAME"
(cd "$OUT" && sha256sum "$(basename "$TARBALL")" > "$(basename "$TARBALL").sha256")
echo "Packaged $TARBALL ($(du -h "$TARBALL" | cut -f1))"
