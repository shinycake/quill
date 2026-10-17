#!/usr/bin/env bash
# Build the pinned FFmpeg (LGPL-2.1+, shared libraries) with the ffmpeg and
# ffprobe programs, and the quillvideo shim (native/quillvideo) that Quill loads
# at runtime for in-process video playback on Linux and Windows
# (docs/decisions/codex-video-cross-platform.md). The programs are what Quill's
# video editor, photo downscale, round-video recording and camera preview run
# (src/media_tools.rs; docs/decisions/sweep-ffmpeg-tools-bundled.md).
#
# Runs on Linux, macOS (development only: Quill.app keeps AVPlayer) and Windows
# under MSYS2 UCRT64 (MinGW-w64 GCC). Output, all under vendor/ffmpeg/prefix:
#   Linux    lib/libquillvideo.so  lib/libav{codec,format,util,filter,device}.so.N  lib/libsw{scale,resample}.so.N  bin/ffmpeg bin/ffprobe
#   macOS    lib/libquillvideo.dylib + the FFmpeg dylibs + bin/ffmpeg bin/ffprobe (development only)
#   Windows  bin/quillvideo.dll    bin/av{codec,format,util,filter,device}-N.dll  bin/sw{scale,resample}-N.dll  bin/ffmpeg.exe bin/ffprobe.exe
#   share/quillvideo/  FFmpeg's license texts and the exact source + configure line
#
# LGPL posture: no --enable-gpl / --enable-nonfree, no external libraries
# (--disable-autodetect), dynamic linking only. Telegram Desktop pins the same
# FFmpeg release (Telegram/build/prepare/prepare.py).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${QUILL_FFMPEG_DEST:-$ROOT/vendor/ffmpeg}"
PREFIX="$DEST/prefix"
TAG=n9.0.2
COMMIT=946fcce07b6dcd0331c8cc609192aeff5e1924f8
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
# Opus/Vorbis/FLAC/PCM sound in MP4/MOV, Matroska/WebM, plus GIF and Ogg; and,
# for the programs, the photos and older camera/phone formats users send.
DECODERS=h264,hevc,vp8,vp9,mpeg4,gif,aac,aac_latm,mp3,mp3float,opus,vorbis,flac,alac,pcm_s16le,pcm_s16be,pcm_s24le,pcm_f32le,pcm_u8,pcm_alaw,pcm_mulaw,mjpeg,png,apng,webp,bmp,tiff,rawvideo,mpeg1video,mpeg2video,h263,msmpeg4v3,wmv2,wmv3,vc1,prores,ac3,mp2,mp2float,wmav2,amrnb,amrwb,wrapped_avframe
DEMUXERS=mov,matroska,gif,ogg,mp3,aac,flac,wav,image2,image2pipe,image_jpeg_pipe,image_png_pipe,image_webp_pipe,image_bmp_pipe,image_tiff_pipe,avi,asf,flv,mpegts,mpegps,ac3,amr,lavfi
PARSERS=h264,hevc,vp8,vp9,mpeg4video,aac,aac_latm,mpegaudio,opus,vorbis,flac,gif,mjpeg,png,webp,bmp,h263,mpegvideo,ac3
# What Quill's command lines write and filter: JPEG/PNG stills, MPEG-4 Part 2
# and AAC (FFmpeg's own, LGPL), MP4/MOV, raw frames for the previews. No H.264
# encoder here: libx264 is GPL, so the programs use the operating system's
# (h264_mf on Windows), else an ffmpeg already on the machine (src/video_encoder.rs).
ENCODERS=mjpeg,png,gif,aac,mpeg4,rawvideo,pcm_s16le,wrapped_avframe
MUXERS=mp4,mov,image2,image2pipe,rawvideo,null,gif,matroska,webm,wav,mjpeg
FILTERS=scale,crop,hflip,vflip,transpose,fps,split,format,null,anull,aformat,aresample,trim,atrim,setpts,asetpts,pad,palettegen,paletteuse,testsrc,color,sine,anullsrc
BSFS=aac_adtstoasc,extract_extradata,null,h264_mp4toannexb

CONFIG=(
  --prefix="$PREFIX"
  --enable-shared --disable-static
  --disable-ffplay --disable-doc --disable-network --disable-autodetect
  --enable-avdevice --enable-avfilter
  --disable-everything
  --enable-protocol=file,pipe
  --enable-decoder="$DECODERS"
  --enable-encoder="$ENCODERS"
  --enable-demuxer="$DEMUXERS"
  --enable-muxer="$MUXERS"
  --enable-parser="$PARSERS"
  --enable-filter="$FILTERS"
  --enable-bsf="$BSFS"
  --enable-indev=lavfi
  --enable-swscale --enable-swresample --enable-zlib
  --extra-cflags=-DCONFIG_SAFE_BITSTREAM_READER=1
)
# Hardware decoding (Settings > Advanced > Hardware accelerated video
# decoding; docs/decisions/hw-decode-toggle.md). The system frameworks and
# DLLs are part of the OS: VideoToolbox on macOS, and D3D11 / DXVA2, which
# FFmpeg loads at run time, on Windows. Linux stays software-only: FFmpeg
# links libva/libvdpau directly, so the package would stop loading on
# systems without them.
case "$OS" in
  # Capture: V4L2 cameras, ALSA and PulseAudio (PipeWire serves it) microphones.
  linux) CONFIG+=(--enable-pic --enable-pthreads --enable-alsa --enable-libpulse --enable-indev=v4l2,alsa,pulse) ;;
  macos) CONFIG+=(--enable-pthreads
    --enable-videotoolbox --enable-hwaccel=h264_videotoolbox,hevc_videotoolbox,vp9_videotoolbox) ;;
  # Native Windows threads, and the MinGW runtime (libgcc, and winpthread,
  # which still backs clock_gettime/nanosleep) linked statically: the DLLs
  # import only the system (UCRT/KERNEL32/bcrypt) — check-bundle-pe.ps1.
  # DirectShow cameras and microphones; the H.264 encoder is Media Foundation's.
  windows) CONFIG+=(--target-os=mingw32 --disable-pthreads --enable-w32threads --enable-mediafoundation --enable-d3d11va --enable-encoder=h264_mf --enable-indev=dshow "--extra-ldflags=-static-libgcc -static"
    --enable-dxva2
    --enable-hwaccel=h264_d3d11va2,hevc_d3d11va2,vp9_d3d11va2,h264_dxva2,hevc_dxva2,vp9_dxva2) ;;
esac
if [[ "$OS" == linux ]] && ! command -v pkg-config >/dev/null; then
  echo "error: pkg-config is required to find libpulse/libasound (apt install pkg-config libpulse-dev libasound2-dev)" >&2
  exit 2
fi
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

# The shim, linked against the libraries above: the video player's
# quillvideo.c and the WebM sticker decoder quillsticker.c (VP9/VP8 alpha via
# a second native decoder; docs/decisions/sweep-webm-stickers.md).
SHIM=("$ROOT/native/quillvideo/quillvideo.c" "$ROOT/native/quillvideo/quillsticker.c")
LIBS=(-lavformat -lavcodec -lswscale -lswresample -lavutil)
case "$OS" in
  linux)
    cc -O2 -fPIC -shared -fvisibility=hidden -Wall -Wextra -Werror \
      -I"$PREFIX/include" "${SHIM[@]}" -L"$PREFIX/lib" -Wl,--no-as-needed "${LIBS[@]}" \
      -Wl,-rpath,'$ORIGIN' -Wl,-soname,libquillvideo.so -o "$PREFIX/lib/libquillvideo.so"
    ;;
  macos)
    cc -O2 -dynamiclib -fvisibility=hidden -Wall -Wextra -Werror \
      -I"$PREFIX/include" "${SHIM[@]}" -L"$PREFIX/lib" "${LIBS[@]}" \
      -install_name @rpath/libquillvideo.dylib -o "$PREFIX/lib/libquillvideo.dylib"
    ;;
  windows)
    gcc -O2 -shared -Wall -Wextra -Werror -static-libgcc \
      -I"$PREFIX/include" "${SHIM[@]}" -L"$PREFIX/lib" "${LIBS[@]}" -o "$PREFIX/bin/quillvideo.dll"
    ;;
esac

# License texts and the corresponding-source pointer, shipped with the libraries.
SHARE="$PREFIX/share/quillvideo"
mkdir -p "$SHARE"
cp "$SRC/COPYING.LGPLv2.1" "$SRC/LICENSE.md" "$SHARE/"
{
  echo "FFmpeg $TAG (commit $COMMIT), unmodified, from https://github.com/FFmpeg/FFmpeg"
  echo "Built by Quill's scripts/build-ffmpeg.sh as shared libraries plus the ffmpeg and ffprobe"
  echo "programs, LGPL-2.1-or-later (no --enable-gpl, no --enable-nonfree)."
  echo "Source: https://github.com/FFmpeg/FFmpeg/tree/$TAG"
  echo "        https://ffmpeg.org/releases/ffmpeg-${TAG#n}.tar.xz"
  echo "Configure: ${CONFIG[*]}"
  echo
  echo "The quillvideo shim (quillvideo.c, MIT) is Quill source: native/quillvideo/."
  echo "Replace the FFmpeg libraries with a compatible build of your own to relink."
} >"$SHARE/FFMPEG-SOURCE.txt"
# Both programs must exist (the package scripts copy them).
for tool in ffmpeg ffprobe; do
  [[ -x "$PREFIX/bin/$tool" || -x "$PREFIX/bin/$tool.exe" ]] || { echo "error: $tool was not built" >&2; exit 1; }
done
echo "Built FFmpeg $TAG, ffmpeg/ffprobe and quillvideo into $PREFIX"
