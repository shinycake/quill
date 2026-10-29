#!/usr/bin/env bash
# Build libquill_calls: Telegram's call engine (tgcalls, LGPL-3.0) on
# Telegram's WebRTC fork (tg_owt, BSD-3), behind Quill's own C API
# (native/quill-calls). It replaces the prebuilt ntgcalls library
# (docs/decisions/tgcalls-engine.md).
#
# Everything is built from source at the commits and tags Telegram Desktop
# pins (tdesktop Telegram/build/prepare/prepare.py and the
# Telegram/ThirdParty/tgcalls submodule), with the same tg_owt options:
#   tg_owt   07fb35bc  static, no audio backends (Quill brings its own device module)
#   tgcalls  1c236c09
#   Opus 1.5.2, libvpx 1.14.1, OpenH264 2.6.0, mozjpeg 4.1.5, rnnoise d8ea2b0, all static
# FFmpeg comes from scripts/build-ffmpeg.sh (shared, LGPL) and OpenSSL 3 from
# the system or OPENSSL_ROOT_DIR, both linked dynamically and bundled as today.
#
# Runs on Linux and macOS for the host architecture. Output:
#   vendor/quill-calls/lib/libquill_calls.{so,dylib}
#   vendor/quill-calls/include/quill_calls.h
#   vendor/quill-calls/share/  exact source pins for the LGPL source offer
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${QUILL_CALLS_DEST:-$ROOT/vendor/quill-calls}"
WORK="${QUILL_CALLS_WORK:-$DEST/work}"
DEPS="$WORK/deps"
FFMPEG_PREFIX="${QUILL_FFMPEG_PREFIX:-$ROOT/vendor/ffmpeg/prefix}"
JOBS="${JOBS:-$( (nproc || sysctl -n hw.ncpu) 2>/dev/null || echo 4)}"

TG_OWT_COMMIT=07fb35bc33b3e79125a1beab7690e830f183f5d8
TGCALLS_COMMIT=1c236c09f8d8569fead14bd68000618a52051225
# Tags as tdesktop names them, pinned to the commits they point at.
OPUS_TAG=v1.5.2 OPUS_COMMIT=ddbe48383984d56acd9e1ab6a090c54ca6b735a6
LIBVPX_TAG=v1.14.1 LIBVPX_COMMIT=12f3a2ac603e8f10742105519e0cd03c3b8f71dd
OPENH264_TAG=v2.6.0 OPENH264_COMMIT=652bdb7719f30b52b08e506645a7322ff1b2cc6f
MOZJPEG_TAG=v4.1.5 MOZJPEG_COMMIT=6c9f0897afa1c2738d7222a0a9ab49e8b536a267
RNNOISE_COMMIT=d8ea2b0ec6a88f8a68d6904d75e33b4f09cb2987
# Linux: PipeWire's headers only (tg_owt dlopens the library), at the
# release tdesktop's Linux build uses (Telegram/build/docker/centos_env).
PIPEWIRE_TAG=0.3.62 PIPEWIRE_COMMIT=3a443b4e1c9730675c7de0453a6279ab9ee263fd

case "$(uname -s)" in
  Linux) OS=linux ;;
  Darwin) OS=macos ;;
  *) echo "error: unsupported host $(uname -s); Windows builds with scripts/build-tgcalls-windows.ps1" >&2; exit 2 ;;
esac
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64|amd64) ARCH=x86_64 ;;
  arm64|aarch64) ARCH=arm64 ;;
  *) echo "error: unsupported architecture $ARCH" >&2; exit 2 ;;
esac

[[ -d "$FFMPEG_PREFIX/include/libavcodec" ]] || {
  echo "error: FFmpeg headers not found in $FFMPEG_PREFIX; run scripts/build-ffmpeg.sh first or set QUILL_FFMPEG_PREFIX" >&2
  exit 2
}

if [[ -z "${OPENSSL_ROOT_DIR:-}" && "$OS" == macos ]]; then
  OPENSSL_ROOT_DIR="$(brew --prefix openssl@3)"
fi
OPENSSL_INCLUDE="${OPENSSL_ROOT_DIR:+$OPENSSL_ROOT_DIR/include}"
OPENSSL_INCLUDE="${OPENSSL_INCLUDE:-/usr/include}"
[[ -f "$OPENSSL_INCLUDE/openssl/ssl.h" ]] || { echo "error: OpenSSL 3 headers not found in $OPENSSL_INCLUDE" >&2; exit 2; }

mkdir -p "$WORK" "$DEPS"
export CFLAGS="${CFLAGS:-} -fPIC -O2"
export CXXFLAGS="${CXXFLAGS:-} -fPIC -O2"
if [[ "$OS" == macos ]]; then
  export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-14.0}"
fi

# fetch <dir> <url> <commit>: shallow checkout of one pinned commit, verified.
fetch() {
  local dir="$WORK/$1" url="$2" commit="$3"
  if [[ ! -d "$dir/.git" ]]; then
    git init -q "$dir"
    git -C "$dir" remote add origin "$url"
  fi
  if [[ "$(git -C "$dir" rev-parse -q --verify HEAD 2>/dev/null)" != "$commit" ]]; then
    git -C "$dir" fetch -q --depth 1 origin "$commit"
    git -C "$dir" -c advice.detachedHead=false checkout -q FETCH_HEAD
  fi
  local have
  have="$(git -C "$dir" rev-parse HEAD)"
  if [[ "$have" != "$commit" ]]; then
    echo "error: $dir is at $have, expected $commit" >&2
    exit 1
  fi
}

# Each dependency writes a stamp naming what it was built from once it is
# installed, and is skipped while that stamp still matches: a rerun after a
# failure resumes where it stopped, and a changed pin rebuilds.
# STAMP_LAYOUT changes when what a build leaves in $DEPS changes.
STAMP_LAYOUT=2
# built <name> <key> <library>: the stamp matches and the library is still there.
built() { [[ "$(cat "$DEPS/.stamp-$1" 2>/dev/null)" == "$STAMP_LAYOUT $2" && -f "$DEPS/lib/$3" ]]; }
stamp() { printf '%s\n' "$STAMP_LAYOUT $2" > "$DEPS/.stamp-$1"; }

# license <name> <file>...: keeps the notices a part asks to travel with it
# in binary form. They live in $DEPS so a cached build still has them.
license() {
  local name="$1"
  shift
  rm -rf "$DEPS/licenses/$name"
  mkdir -p "$DEPS/licenses/$name"
  cp "$@" "$DEPS/licenses/$name/"
}

build_opus() {
  built opus "$OPUS_COMMIT" libopus.a && return
  fetch opus https://github.com/xiph/opus.git "$OPUS_COMMIT"
  cmake -S "$WORK/opus" -B "$WORK/opus/out" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$DEPS" \
    -DCMAKE_POSITION_INDEPENDENT_CODE=ON -DBUILD_SHARED_LIBS=OFF \
    -DOPUS_BUILD_PROGRAMS=OFF -DOPUS_BUILD_TESTING=OFF
  cmake --build "$WORK/opus/out" --parallel "$JOBS"
  cmake --install "$WORK/opus/out"
  license opus "$WORK/opus/COPYING"
  stamp opus "$OPUS_COMMIT"
}

build_libvpx() {
  built libvpx "$LIBVPX_COMMIT" libvpx.a && return
  fetch libvpx https://github.com/webmproject/libvpx.git "$LIBVPX_COMMIT"
  local target=()
  if [[ "$OS" == macos ]]; then
    target=(--target="$([[ $ARCH == arm64 ]] && echo arm64 || echo x86_64)-darwin20-gcc")
  fi
  (cd "$WORK/libvpx" && ./configure --prefix="$DEPS" ${target[@]+"${target[@]}"} \
    --enable-pic --enable-static --disable-shared \
    --disable-examples --disable-unit-tests --disable-tools --disable-docs \
    --enable-vp8 --enable-vp9 --enable-webm-io --size-limit=4096x4096 \
    && make -j"$JOBS" && make install)
  license libvpx "$WORK/libvpx/LICENSE" "$WORK/libvpx/PATENTS"
  stamp libvpx "$LIBVPX_COMMIT"
}

build_openh264() {
  built openh264 "$OPENH264_COMMIT" libopenh264.a && return
  fetch openh264 https://github.com/cisco/openh264.git "$OPENH264_COMMIT"
  local arch=()
  [[ "$ARCH" == arm64 ]] && arch=(ARCH="$([[ $OS == macos ]] && echo arm64 || echo aarch64)")
  make -C "$WORK/openh264" -j"$JOBS" PREFIX="$DEPS" ${arch[@]+"${arch[@]}"} \
    CFLAGS_OPT=-O2 install-static
  license openh264 "$WORK/openh264/LICENSE"
  stamp openh264 "$OPENH264_COMMIT"
}

build_mozjpeg() {
  built mozjpeg "$MOZJPEG_COMMIT" libjpeg.a && return
  fetch mozjpeg https://github.com/mozilla/mozjpeg.git "$MOZJPEG_COMMIT"
  cmake -S "$WORK/mozjpeg" -B "$WORK/mozjpeg/out" -G Ninja \
    -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$DEPS" -DCMAKE_INSTALL_LIBDIR="$DEPS/lib" \
    -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
    -DWITH_JPEG8=ON -DENABLE_SHARED=OFF -DPNG_SUPPORTED=OFF -DWITH_TURBOJPEG=OFF
  cmake --build "$WORK/mozjpeg/out" --parallel "$JOBS"
  cmake --install "$WORK/mozjpeg/out"
  license mozjpeg "$WORK/mozjpeg/LICENSE.md"
  stamp mozjpeg "$MOZJPEG_COMMIT"
}

build_rnnoise() {
  built rnnoise "$RNNOISE_COMMIT" librnnoise.a && return
  fetch rnnoise https://github.com/desktop-app/rnnoise.git "$RNNOISE_COMMIT"
  cmake -S "$WORK/rnnoise" -B "$WORK/rnnoise/out" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_POSITION_INDEPENDENT_CODE=ON
  cmake --build "$WORK/rnnoise/out" --parallel "$JOBS"
  mkdir -p "$DEPS/lib" "$DEPS/include"
  cp "$WORK/rnnoise/out/librnnoise.a" "$DEPS/lib/"
  cp "$WORK/rnnoise/include/rnnoise.h" "$DEPS/include/"
  license rnnoise "$WORK/rnnoise/COPYING"
  stamp rnnoise "$RNNOISE_COMMIT"
}

build_tg_owt() {
  built tg_owt "$TG_OWT_KEY" libtg_owt.a && return
  fetch tg_owt https://github.com/desktop-app/tg_owt.git "$TG_OWT_COMMIT"
  git -C "$WORK/tg_owt" submodule update -q --init --recursive --depth 1
  if [[ "$OS" == linux ]]; then
    # tg_owt reads these ahead of the system's, which are too old on
    # Ubuntu 22.04 (0.3.48) for its camera code.
    fetch tg_owt/src/third_party/pipewire https://github.com/PipeWire/pipewire.git "$PIPEWIRE_COMMIT"
    local major minor micro
    IFS=. read -r major minor micro <<<"$PIPEWIRE_TAG"
    mkdir -p "$WORK/tg_owt/src/third_party/pipewire/build/src/pipewire"
    sed -e "s/@PIPEWIRE_VERSION_MAJOR@/$major/g" -e "s/@PIPEWIRE_VERSION_MINOR@/$minor/g" \
      -e "s/@PIPEWIRE_VERSION_MICRO@/$micro/g" -e 's/@PIPEWIRE_API_VERSION@/"0.3"/g' \
      "$WORK/tg_owt/src/third_party/pipewire/src/pipewire/version.h.in" \
      > "$WORK/tg_owt/src/third_party/pipewire/build/src/pipewire/version.h"
  fi
  # TG_OWT_SPECIAL_TARGET selects tdesktop's non-packaged mode: include
  # paths only, the libraries are linked once into libquill_calls.
  local extra=()
  [[ "$OS" == macos ]] && extra=(-DCMAKE_OSX_ARCHITECTURES="$ARCH")
  [[ "$OS" == linux ]] && extra=(-DTG_OWT_DLOPEN_PIPEWIRE=ON)
  cmake -S "$WORK/tg_owt" -B "$WORK/tg_owt/out" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$DEPS" -DCMAKE_INSTALL_LIBDIR=lib \
    -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
    -DTG_OWT_BUILD_AUDIO_BACKENDS=OFF \
    -DTG_OWT_SPECIAL_TARGET="$OS" \
    -DTG_OWT_LIBJPEG_INCLUDE_PATH="$DEPS/include" \
    -DTG_OWT_OPENSSL_INCLUDE_PATH="$OPENSSL_INCLUDE" \
    -DTG_OWT_OPUS_INCLUDE_PATH="$DEPS/include/opus" \
    -DTG_OWT_LIBVPX_INCLUDE_PATH="$DEPS/include" \
    -DTG_OWT_OPENH264_INCLUDE_PATH="$DEPS/include" \
    -DTG_OWT_FFMPEG_INCLUDE_PATH="$FFMPEG_PREFIX/include" \
    ${extra[@]+"${extra[@]}"}
  cmake --build "$WORK/tg_owt/out" --parallel "$JOBS"
  cmake --install "$WORK/tg_owt/out"
  local owt="$WORK/tg_owt"
  license tg_owt "$owt/LICENSE" "$owt/src/PATENTS"
  license abseil-cpp "$owt/src/third_party/abseil-cpp/LICENSE"
  license crc32c "$owt/src/third_party/crc32c/src/LICENSE"
  license libsrtp "$owt/src/third_party/libsrtp/LICENSE"
  license libyuv "$owt/src/third_party/libyuv/LICENSE" "$owt/src/third_party/libyuv/PATENTS"
  license pffft "$owt/src/third_party/pffft/LICENSE"
  license rnnoise-tg_owt "$owt/src/third_party/rnnoise/COPYING"
  license ooura "$owt/src/common_audio/third_party/ooura/LICENSE"
  license spl_sqrt_floor "$owt/src/common_audio/third_party/spl_sqrt_floor/LICENSE"
  license base64 "$owt/src/rtc_base/third_party/base64/LICENSE"
  license sigslot "$owt/src/rtc_base/third_party/sigslot/LICENSE"
  if [[ "$OS" == linux ]]; then
    license pipewire "$owt/src/third_party/pipewire/COPYING"
  fi
  stamp tg_owt "$TG_OWT_KEY"
}

# tg_owt compiles against the codecs' and FFmpeg's headers, so a new pin of
# any of them rebuilds it.
TG_OWT_KEY="$TG_OWT_COMMIT pipewire-$PIPEWIRE_COMMIT opus-$OPUS_COMMIT libvpx-$LIBVPX_COMMIT"
TG_OWT_KEY+=" openh264-$OPENH264_COMMIT mozjpeg-$MOZJPEG_COMMIT"
TG_OWT_KEY+=" ffmpeg-$(cat "$FFMPEG_PREFIX"/include/libav*/version*.h | shasum -a 256 | cut -c1-16)"

build_opus
build_libvpx
build_openh264
build_mozjpeg
build_rnnoise
build_tg_owt
fetch tgcalls https://github.com/TelegramMessenger/tgcalls.git "$TGCALLS_COMMIT"

cmake -S "$ROOT/native/quill-calls" -B "$WORK/quill-calls" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$DEST" \
  -DQUILL_CALLS_DEPS="$DEPS" \
  -DQUILL_CALLS_TGCALLS="$WORK/tgcalls" \
  -DQUILL_CALLS_FFMPEG="$FFMPEG_PREFIX" \
  -DQUILL_CALLS_OPENSSL_INCLUDE="$OPENSSL_INCLUDE" \
  ${OPENSSL_ROOT_DIR:+-DOPENSSL_ROOT_DIR="$OPENSSL_ROOT_DIR"} \
  $([[ "$OS" == macos ]] && echo "-DCMAKE_OSX_ARCHITECTURES=$ARCH")
cmake --build "$WORK/quill-calls" --parallel "$JOBS"
cmake --install "$WORK/quill-calls"

mkdir -p "$DEST/share"
cat > "$DEST/share/SOURCES.txt" <<EOF
libquill_calls is built from these sources (scripts/build-tgcalls.sh):
tgcalls  https://github.com/TelegramMessenger/tgcalls  $TGCALLS_COMMIT  (LGPL-3.0)
tg_owt   https://github.com/desktop-app/tg_owt         $TG_OWT_COMMIT  (BSD-3-Clause)
opus     https://github.com/xiph/opus                  $OPUS_TAG ($OPUS_COMMIT)
libvpx   https://github.com/webmproject/libvpx         $LIBVPX_TAG ($LIBVPX_COMMIT)
openh264 https://github.com/cisco/openh264             $OPENH264_TAG ($OPENH264_COMMIT)
mozjpeg  https://github.com/mozilla/mozjpeg            $MOZJPEG_TAG ($MOZJPEG_COMMIT)
rnnoise  https://github.com/desktop-app/rnnoise        $RNNOISE_COMMIT
pipewire https://github.com/PipeWire/pipewire        $PIPEWIRE_TAG ($PIPEWIRE_COMMIT), headers only, Linux
quill-calls (the C API)  native/quill-calls in https://github.com/shinycake/quill  $(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)
EOF

# The licenses the library's parts ask to travel with it in binary form:
# tgcalls' LGPL-3.0 (with the GPL-3.0 it extends) and the BSD, MIT and
# Apache notices of tg_owt, the libraries tg_owt carries and the codecs.
license tgcalls "$WORK/tgcalls/LICENSE" "$ROOT/licenses/GPL-3.0.txt"
rm -rf "$DEST/share/licenses"
cp -R "$DEPS/licenses" "$DEST/share/licenses"
# tgcalls carries json11 without a license file; its notice heads the source.
mkdir -p "$DEST/share/licenses/json11"
sed -n '1,/\*\//p' "$WORK/tgcalls/tgcalls/third-party/json11.cpp" > "$DEST/share/licenses/json11/LICENSE"

LIB="$DEST/lib/libquill_calls.$([[ $OS == macos ]] && echo dylib || echo so)"
[[ -f "$LIB" ]] || { echo "error: $LIB was not built" >&2; exit 1; }
if [[ "$OS" == macos ]]; then
  # FFmpeg and OpenSSL are linked by their absolute build paths; refer to
  # them by name through the rpath (@loader_path) so the copies shipped next
  # to the library are the ones loaded. The edit voids the linker's ad-hoc
  # signature, so sign again.
  while read -r dep; do
    case "$dep" in
      "$FFMPEG_PREFIX"/*|"${OPENSSL_ROOT_DIR:-/nonexistent}"/*|/opt/homebrew/*|/usr/local/opt/*)
        install_name_tool -change "$dep" "@rpath/$(basename "$dep")" "$LIB" ;;
    esac
  done < <(otool -L "$LIB" | tail -n +2 | sed -E 's/^[[:space:]]+//; s/ \(compatibility version.*$//')
  codesign --force --sign - "$LIB"
  if otool -L "$LIB" | tail -n +2 | grep -E -v '^[[:space:]]+(@rpath/|/usr/lib/|/System/)'; then
    echo "error: $LIB still links a library by a build path" >&2
    exit 1
  fi
fi
shasum -a 256 "$LIB" | tee "$DEST/share/libquill_calls.sha256"
echo "Built $LIB"
