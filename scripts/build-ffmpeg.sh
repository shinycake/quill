#!/usr/bin/env bash
# Build the pinned FFmpeg (LGPL-2.1+, shared libraries, decoders only) and the
# quillvideo shim (native/quillvideo) that Quill loads at runtime for in-process
# video playback on Linux and Windows (docs/decisions/codex-video-cross-platform.md).
#
# Runs on Linux, macOS (development only: Quill.app keeps AVPlayer) and Windows
# under MSYS2 UCRT64 (MinGW-w64 GCC). Output, all under vendor/ffmpeg/prefix:
#   Linux    lib/libquillvideo.so  lib/libav{codec,format,util}.so.N  lib/libsw{scale,resample}.so.N
#   macOS    lib/libquillvideo.dylib + the FFmpeg dylibs
#   Windows  bin/quillvideo.dll    bin/av{codec,format,util}-N.dll  bin/sw{scale,resample}-N.dll
#   share/quillvideo/  FFmpeg's license texts and the exact source + configure line
#
# LGPL posture: no --enable-gpl / --enable-nonfree, no external libraries
# (--disable-autodetect), dynamic linking only. Telegram Desktop pins the same
# FFmpeg release (Telegram/build/prepare/prepare.py).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${QUILL_FFMPEG_DEST:-$ROOT/vendor/ffmpeg}"
PREFIX="$DEST/prefix"
TAG=n8.1.3
COMMIT=1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7
JOBS="${JOBS:-$( (nproc || sysctl -n hw.ncpu) 2>/dev/null || echo 4)}"

case "$(uname -s)" in
  Linux) OS=linux ;;
  Darwin) OS=macos ;;
  MINGW*|MSYS*) OS=windows ;;
  *) echo "error: unsupported host $(uname -s)" >&2; exit 2 ;;
esac

SRC="$DEST/source"
if [[ ! -d "$SRC/.git" ]]; then
  git clone --depth 1 --branch "$TAG" https://github.com/FFmpeg/FFmpeg.git "$SRC"
fi
actual="$(git -C "$SRC" rev-parse HEAD)"
if [[ "$actual" != "$COMMIT" ]]; then
  echo "error: FFmpeg $TAG is $actual, expected $COMMIT" >&2
  exit 1
fi

# Everything Telegram serves: H.264/HEVC/VP8/VP9/MPEG-4 video and AAC/MP3/
# Opus/Vorbis/FLAC/PCM sound in MP4/MOV, Matroska/WebM, plus GIF and Ogg.
DECODERS=h264,hevc,vp8,vp9,mpeg4,gif,aac,aac_latm,mp3,mp3float,opus,vorbis,flac,alac,pcm_s16le,pcm_s16be,pcm_s24le,pcm_f32le,pcm_u8,pcm_alaw,pcm_mulaw
DEMUXERS=mov,matroska,gif,ogg,mp3,aac,flac,wav
PARSERS=h264,hevc,vp8,vp9,mpeg4video,aac,aac_latm,mpegaudio,opus,vorbis,flac,gif

CONFIG=(
  --prefix="$PREFIX"
  --enable-shared --disable-static
  --disable-programs --disable-doc --disable-network --disable-autodetect
  --disable-avdevice --disable-avfilter
  --disable-everything
  --enable-protocol=file
  --enable-decoder="$DECODERS"
  --enable-demuxer="$DEMUXERS"
  --enable-parser="$PARSERS"
  --enable-swscale --enable-swresample
  --extra-cflags=-DCONFIG_SAFE_BITSTREAM_READER=1
)
case "$OS" in
  linux) CONFIG+=(--enable-pic --enable-pthreads) ;;
  macos) CONFIG+=(--enable-pthreads) ;;
  # Native Windows threads, and the MinGW runtime (libgcc, and winpthread,
  # which still backs clock_gettime/nanosleep) linked statically: the DLLs
  # import only the system (UCRT/KERNEL32/bcrypt) — check-bundle-pe.ps1.
  windows) CONFIG+=(--target-os=mingw32 --disable-pthreads --enable-w32threads "--extra-ldflags=-static-libgcc -static") ;;
esac
if [[ "$OS" != macos ]] && ! command -v nasm >/dev/null; then
  echo "error: nasm is required for FFmpeg's x86 assembly (apt install nasm / pacman -S nasm)" >&2
  exit 2
fi

BUILD="$DEST/build"
rm -rf "$BUILD" "$PREFIX"
mkdir -p "$BUILD"
(
  cd "$BUILD"
  "$SRC/configure" "${CONFIG[@]}"
  make -j"$JOBS"
  make install
)

# The shim, linked against the libraries above.
SHIM="$ROOT/native/quillvideo/quillvideo.c"
LIBS=(-lavformat -lavcodec -lswscale -lswresample -lavutil)
case "$OS" in
  linux)
    cc -O2 -fPIC -shared -fvisibility=hidden -Wall -Wextra -Werror \
      -I"$PREFIX/include" "$SHIM" -L"$PREFIX/lib" -Wl,--no-as-needed "${LIBS[@]}" \
      -Wl,-rpath,'$ORIGIN' -Wl,-soname,libquillvideo.so -o "$PREFIX/lib/libquillvideo.so"
    ;;
  macos)
    cc -O2 -dynamiclib -fvisibility=hidden -Wall -Wextra -Werror \
      -I"$PREFIX/include" "$SHIM" -L"$PREFIX/lib" "${LIBS[@]}" \
      -install_name @rpath/libquillvideo.dylib -o "$PREFIX/lib/libquillvideo.dylib"
    ;;
  windows)
    gcc -O2 -shared -Wall -Wextra -Werror -static-libgcc \
      -I"$PREFIX/include" "$SHIM" -L"$PREFIX/lib" "${LIBS[@]}" -o "$PREFIX/bin/quillvideo.dll"
    ;;
esac

# License texts and the corresponding-source pointer, shipped with the libraries.
SHARE="$PREFIX/share/quillvideo"
mkdir -p "$SHARE"
cp "$SRC/COPYING.LGPLv2.1" "$SRC/LICENSE.md" "$SHARE/"
{
  echo "FFmpeg $TAG (commit $COMMIT), unmodified, from https://github.com/FFmpeg/FFmpeg"
  echo "Built by Quill's scripts/build-ffmpeg.sh as shared libraries, LGPL-2.1-or-later."
  echo "Source: https://github.com/FFmpeg/FFmpeg/tree/$TAG"
  echo "        https://ffmpeg.org/releases/ffmpeg-${TAG#n}.tar.xz"
  echo "Configure: ${CONFIG[*]}"
  echo
  echo "The quillvideo shim (quillvideo.c, MIT) is Quill source: native/quillvideo/."
  echo "Replace the FFmpeg libraries with a compatible build of your own to relink."
} >"$SHARE/FFMPEG-SOURCE.txt"
echo "Built FFmpeg $TAG and quillvideo into $PREFIX"
