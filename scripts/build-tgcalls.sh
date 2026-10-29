#!/usr/bin/env bash
# Build libquill_calls: Telegram's call engine (tgcalls, LGPL-3.0) on
# Telegram's WebRTC fork (tg_owt, BSD-3), behind Quill's own C API
# (native/quill-calls), the library Quill's calls run on
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
# Windows (x86_64) builds the same parts with MSVC and the static C runtime
# (/MT), as tdesktop's prepare.py does on win64: libvpx through its vs17
# solution, OpenH264 with its MSVC makefile, the CMake projects with cl, and
# OpenSSL 3 and zlib static from vcpkg (x64-windows-static, as
# scripts/build-tdlib-windows.ps1 takes them). FFmpeg stays the MinGW-built
# DLLs Quill already ships; the library links them through MSVC import
# libraries made here from the DLLs' exports. Run it from MSYS2 bash
# (UCRT64, for make and nasm) with an MSVC developer environment inherited
# (MSYS2_PATH_TYPE=inherit after vcvars64 or ilammy/msvc-dev-cmd), and vcpkg
# at VCPKG_INSTALLATION_ROOT.
#
# Runs on Linux, macOS and Windows for the host architecture. Output:
#   vendor/quill-calls/lib/libquill_calls.{so,dylib} or quill_calls.dll
#   vendor/quill-calls/include/quill_calls.h
#   vendor/quill-calls/share/  exact source pins for the LGPL source offer
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
case "$(uname -s)" in
  Linux) OS=linux ;;
  Darwin) OS=macos ;;
  MINGW*|MSYS*) OS=windows ;;
  *) echo "error: unsupported host $(uname -s)" >&2; exit 2 ;;
esac
# On Windows every path handed to a native tool is C:/style, which MSYS2
# leaves alone when it passes arguments on.
winpath() { if [[ "$OS" == windows ]]; then cygpath -m "$1"; else printf '%s\n' "$1"; fi; }
ROOT="$(winpath "$ROOT")"
DEST="$(winpath "${QUILL_CALLS_DEST:-$ROOT/vendor/quill-calls}")"
WORK="$(winpath "${QUILL_CALLS_WORK:-$DEST/work}")"
DEPS="$WORK/deps"
# The C API library's own build tree; separate trees let checkouts share $WORK.
BUILD="$(winpath "${QUILL_CALLS_BUILD:-$WORK/quill-calls}")"
FFMPEG_PREFIX="$(winpath "${QUILL_FFMPEG_PREFIX:-$ROOT/vendor/ffmpeg/prefix}")"
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

ARCH="$(uname -m)"
case "$ARCH" in
  x86_64|amd64) ARCH=x86_64 ;;
  arm64|aarch64) ARCH=arm64 ;;
  *) echo "error: unsupported architecture $ARCH" >&2; exit 2 ;;
esac
if [[ "$OS" == windows && "$ARCH" != x86_64 ]]; then
  echo "error: the Windows build is x86_64 only" >&2
  exit 2
fi

sha256() { if command -v sha256sum >/dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
# slib <name>: a static library's file name, as the build below installs it.
slib() { if [[ "$OS" == windows ]]; then echo "$1.lib"; else echo "lib$1.a"; fi; }

[[ -d "$FFMPEG_PREFIX/include/libavcodec" ]] || {
  echo "error: FFmpeg headers not found in $FFMPEG_PREFIX; run scripts/build-ffmpeg.sh first or set QUILL_FFMPEG_PREFIX" >&2
  exit 2
}

# Extra arguments for every CMake configure below.
CMAKE_EXTRA=()
if [[ "$OS" == windows ]]; then
  command -v cl >/dev/null || {
    echo "error: cl.exe not found; run from MSYS2 with an MSVC developer environment (MSYS2_PATH_TYPE=inherit)" >&2
    exit 2
  }
  # MSVC's directory first: MSYS2's /usr/bin has a link.exe of its own.
  PATH="$(dirname "$(command -v cl)"):$PATH"
  # libvpx's solution is built by Visual Studio's MSBuild, which libvpx's
  # makefile looks up by name. vcvars lists its directory, but a lookup of
  # "msbuild" from MSYS2 bash misses it there, and the Windows directory has
  # an older .NET Framework MSBuild that can't build VS 2022 projects. So
  # find Visual Studio's own (VSINSTALLDIR, else vswhere), put its directory
  # first and name it exactly as it is spelled (MSBUILD_TOOL below).
  msbuild_dir=""
  vs_dirs=()
  [[ -n "${VSINSTALLDIR:-}" ]] && vs_dirs+=("$(cygpath -u "$VSINSTALLDIR")")
  vswhere="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
  if [[ -x "$vswhere" ]]; then
    while IFS= read -r dir; do
      [[ -n "$dir" ]] && vs_dirs+=("$(cygpath -u "$dir")")
    done < <(MSYS2_ARG_CONV_EXCL='*' "$vswhere" -latest -products '*' \
      -requires Microsoft.Component.MSBuild -property installationPath | tr -d '\r')
  fi
  for dir in ${vs_dirs[@]+"${vs_dirs[@]}"}; do
    if [[ -f "$dir/MSBuild/Current/Bin/amd64/MSBuild.exe" ]]; then
      msbuild_dir="$dir/MSBuild/Current/Bin/amd64"
      break
    fi
  done
  [[ -n "$msbuild_dir" ]] || {
    echo "error: Visual Studio's MSBuild.exe not found (VSINSTALLDIR or vswhere)" >&2
    exit 2
  }
  PATH="$msbuild_dir:$PATH"
  # The SDK's rc and mt, which CMake's MSVC links use, by the same route.
  if [[ -n "${WindowsSdkVerBinPath:-}" ]]; then
    sdk_bin="$(cygpath -u "$WindowsSdkVerBinPath")"
    PATH="${sdk_bin%/}/x64:$PATH"
  fi
  for tool in cmake ninja nasm make MSBuild.exe which lib dumpbin rc mt; do
    command -v "$tool" >/dev/null || { echo "error: $tool not found on PATH" >&2; exit 2; }
  done
  VCPKG_DIR="$(winpath "${VCPKG_INSTALLATION_ROOT:-C:/vcpkg}")"
  [[ -x "$VCPKG_DIR/vcpkg.exe" ]] || { echo "error: vcpkg not found at $VCPKG_DIR (set VCPKG_INSTALLATION_ROOT)" >&2; exit 2; }
  # cl, and the static C runtime everywhere, whatever CMake version a
  # project asks for. CC stays unset: libvpx's configure only skips probing
  # the compiler with gcc-style flags when it is.
  CMAKE_EXTRA=(-DCMAKE_C_COMPILER=cl -DCMAKE_CXX_COMPILER=cl
    -DCMAKE_POLICY_DEFAULT_CMP0091=NEW -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded
    -DCMAKE_POLICY_VERSION_MINIMUM=3.5)
  # Filled from vcpkg by build_vcpkg_deps.
  OPENSSL_INCLUDE="$DEPS/include"
else
  if [[ -z "${OPENSSL_ROOT_DIR:-}" && "$OS" == macos ]]; then
    OPENSSL_ROOT_DIR="$(brew --prefix openssl@3)"
  fi
  OPENSSL_INCLUDE="${OPENSSL_ROOT_DIR:+$OPENSSL_ROOT_DIR/include}"
  OPENSSL_INCLUDE="${OPENSSL_INCLUDE:-/usr/include}"
  [[ -f "$OPENSSL_INCLUDE/openssl/ssl.h" ]] || { echo "error: OpenSSL 3 headers not found in $OPENSSL_INCLUDE" >&2; exit 2; }
  export CFLAGS="${CFLAGS:-} -fPIC -O2"
  export CXXFLAGS="${CXXFLAGS:-} -fPIC -O2"
fi

mkdir -p "$WORK" "$DEPS"
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
    git -C "$dir" -c advice.detachedHead=false checkout -q -f FETCH_HEAD
  fi
  local have
  have="$(git -C "$dir" rev-parse HEAD)"
  if [[ "$have" != "$commit" ]]; then
    echo "error: $dir is at $have, expected $commit" >&2
    exit 1
  fi
}

# apply_patches <dir> <patch>...: applies each patch to the checkout in
# $WORK/<dir> unless it is already applied.
apply_patches() {
  local dir="$WORK/$1" patch
  shift
  for patch in "$@"; do
    if git -C "$dir" apply --check "$patch" 2>/dev/null; then
      git -C "$dir" apply "$patch"
    elif ! git -C "$dir" apply --reverse --check "$patch" 2>/dev/null; then
      echo "error: $patch does not apply to $dir" >&2
      exit 1
    fi
  done
}

# Each dependency writes a stamp naming what it was built from once it is
# installed, and is skipped while that stamp still matches: a rerun after a
# failure resumes where it stopped, and a changed pin rebuilds.
# STAMP_LAYOUT changes when what a build leaves in $DEPS changes.
STAMP_LAYOUT=3
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
  built opus "$OPUS_COMMIT" "$(slib opus)" && return
  fetch opus https://github.com/xiph/opus.git "$OPUS_COMMIT"
  local extra=()
  [[ "$OS" == windows ]] && extra=(-DOPUS_STATIC_RUNTIME=ON)
  cmake -S "$WORK/opus" -B "$WORK/opus/out" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$DEPS" \
    -DCMAKE_POSITION_INDEPENDENT_CODE=ON -DBUILD_SHARED_LIBS=OFF \
    -DOPUS_BUILD_PROGRAMS=OFF -DOPUS_BUILD_TESTING=OFF \
    ${extra[@]+"${extra[@]}"} ${CMAKE_EXTRA[@]+"${CMAKE_EXTRA[@]}"}
  cmake --build "$WORK/opus/out" --parallel "$JOBS"
  cmake --install "$WORK/opus/out"
  license opus "$WORK/opus/COPYING"
  stamp opus "$OPUS_COMMIT"
}

build_libvpx() {
  local key="$LIBVPX_COMMIT"
  if [[ "$OS" == windows ]]; then
    key+=" patches-$(cat "$ROOT"/native/quill-calls/patches/libvpx-win-*.patch | sha256 | cut -c1-16)"
  fi
  built libvpx "$key" "$(slib vpx)" && return
  fetch libvpx https://github.com/webmproject/libvpx.git "$LIBVPX_COMMIT"
  if [[ "$OS" == windows ]]; then
    # tdesktop's patches/build_libvpx_win.sh: a Visual Studio 2022 solution
    # that make drives through msbuild, with the static runtime, and the
    # patches of tdesktop's that a Windows x64 build needs. make install
    # copies the headers; the library is taken from msbuild's output. The
    # makefile skips the solution, without failing, when it can't find
    # MSBUILD_TOOL; the cp below then fails.
    apply_patches libvpx "$ROOT"/native/quill-calls/patches/libvpx-win-*.patch
    (cd "$WORK/libvpx" && ./configure --prefix="$DEPS" --target=x86_64-win64-vs17 --as=nasm \
      --enable-static-msvcrt \
      --disable-examples --disable-unit-tests --disable-tools --disable-docs \
      --enable-vp8 --enable-vp9 --enable-webm-io --size-limit=4096x4096 \
      && make -j"$JOBS" MSBUILD_TOOL=MSBuild.exe \
      && make install MSBUILD_TOOL=MSBuild.exe)
    cp "$WORK/libvpx/x64/Release/vpxmt.lib" "$DEPS/lib/vpx.lib"
    license libvpx "$WORK/libvpx/LICENSE" "$WORK/libvpx/PATENTS"
    stamp libvpx "$key"
    return
  fi
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
  stamp libvpx "$key"
}

build_openh264() {
  built openh264 "$OPENH264_COMMIT" "$(slib openh264)" && return
  fetch openh264 https://github.com/cisco/openh264.git "$OPENH264_COMMIT"
  if [[ "$OS" == windows ]]; then
    # OpenH264's own MSVC makefile (cl, nasm, -MT); installs openh264.lib.
    make -C "$WORK/openh264" -j"$JOBS" PREFIX="$DEPS" OS=msvc ARCH=x86_64 install-static
  else
    local arch=()
    [[ "$ARCH" == arm64 ]] && arch=(ARCH="$([[ $OS == macos ]] && echo arm64 || echo aarch64)")
    make -C "$WORK/openh264" -j"$JOBS" PREFIX="$DEPS" ${arch[@]+"${arch[@]}"} \
      CFLAGS_OPT=-O2 install-static
  fi
  license openh264 "$WORK/openh264/LICENSE"
  stamp openh264 "$OPENH264_COMMIT"
}

build_mozjpeg() {
  built mozjpeg "$MOZJPEG_COMMIT" "$(slib jpeg)" && return
  fetch mozjpeg https://github.com/mozilla/mozjpeg.git "$MOZJPEG_COMMIT"
  cmake -S "$WORK/mozjpeg" -B "$WORK/mozjpeg/out" -G Ninja \
    -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$DEPS" -DCMAKE_INSTALL_LIBDIR="$DEPS/lib" \
    -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
    -DWITH_JPEG8=ON -DENABLE_SHARED=OFF -DPNG_SUPPORTED=OFF -DWITH_TURBOJPEG=OFF \
    ${CMAKE_EXTRA[@]+"${CMAKE_EXTRA[@]}"}
  cmake --build "$WORK/mozjpeg/out" --parallel "$JOBS"
  cmake --install "$WORK/mozjpeg/out"
  # MSVC builds name the static library jpeg-static.lib.
  if [[ "$OS" == windows ]]; then
    cp "$DEPS/lib/jpeg-static.lib" "$DEPS/lib/jpeg.lib"
  fi
  license mozjpeg "$WORK/mozjpeg/LICENSE.md"
  stamp mozjpeg "$MOZJPEG_COMMIT"
}

build_rnnoise() {
  built rnnoise "$RNNOISE_COMMIT" "$(slib rnnoise)" && return
  fetch rnnoise https://github.com/desktop-app/rnnoise.git "$RNNOISE_COMMIT"
  # On Windows rnnoise builds with DLL_EXPORT, which marks its functions
  # dllexport and would export them from quill_calls.dll; an empty
  # RNNOISE_EXPORT keeps them inside.
  local flags=()
  [[ "$OS" == windows ]] && flags=("-DCMAKE_C_FLAGS=-DWIN32 -D_WINDOWS -DRNNOISE_EXPORT=")
  cmake -S "$WORK/rnnoise" -B "$WORK/rnnoise/out" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
    ${flags[@]+"${flags[@]}"} ${CMAKE_EXTRA[@]+"${CMAKE_EXTRA[@]}"}
  cmake --build "$WORK/rnnoise/out" --parallel "$JOBS"
  mkdir -p "$DEPS/lib" "$DEPS/include"
  cp "$WORK/rnnoise/out/$(slib rnnoise)" "$DEPS/lib/"
  cp "$WORK/rnnoise/include/rnnoise.h" "$DEPS/include/"
  license rnnoise "$WORK/rnnoise/COPYING"
  stamp rnnoise "$RNNOISE_COMMIT"
}

# Windows: OpenSSL 3 and zlib as static /MT libraries from vcpkg, copied into
# $DEPS so they are cached and stamped like the rest. The key is vcpkg's
# commit, which fixes the port versions.
build_vcpkg_deps() {
  local key triplet=x64-windows-static
  key="vcpkg-$(git -c safe.directory="*" -C "$VCPKG_DIR" rev-parse HEAD 2>/dev/null || echo unknown)"
  built openssl "$key" libcrypto.lib && built zlib "$key" zlib.lib && return
  # Without MSYS2's directories on PATH: OpenSSL's Configure refuses MSYS2's
  # perl for MSVC targets, and vcpkg sets up the MSVC tools itself.
  local dir native_path=()
  local -a dirs
  IFS=: read -r -a dirs <<<"$PATH"
  for dir in "${dirs[@]}"; do
    case "$dir" in
      /usr/*|/bin|/ucrt64/*|/mingw*|/clang*|/opt/*) ;;
      *) native_path+=("$dir") ;;
    esac
  done
  (IFS=:; PATH="${native_path[*]}" \
    "$VCPKG_DIR/vcpkg.exe" install "openssl:$triplet" "zlib:$triplet" --clean-after-build)
  local installed="$VCPKG_DIR/installed/$triplet"
  mkdir -p "$DEPS/lib" "$DEPS/include"
  cp "$installed/lib/libssl.lib" "$installed/lib/libcrypto.lib" "$DEPS/lib/"
  # The static zlib's file name differs between zlib and vcpkg versions
  # (zlib.lib, zs.lib); its pkg-config file names it, and the link step
  # takes it as zlib.lib.
  local zlib_name zlib_lib
  zlib_name="$(sed -n 's/^Libs:.*-l\([^ ]*\).*$/\1/p' "$installed/lib/pkgconfig/zlib.pc" | tr -d '\r')"
  for zlib_lib in ${zlib_name:+"$zlib_name.lib"} zlib.lib zs.lib zlibstatic.lib; do
    [[ -f "$installed/lib/$zlib_lib" ]] && break
  done
  [[ -f "$installed/lib/$zlib_lib" ]] || { echo "error: no zlib static library in $installed/lib:" >&2; ls "$installed/lib" >&2; exit 1; }
  cp "$installed/lib/$zlib_lib" "$DEPS/lib/zlib.lib"
  rm -rf "$DEPS/include/openssl"
  cp -R "$installed/include/openssl" "$DEPS/include/"
  cp "$installed/include/zlib.h" "$installed/include/zconf.h" "$DEPS/include/"
  # `vcpkg list` takes one filter at most.
  "$VCPKG_DIR/vcpkg.exe" list | tr -d '\r' | grep -E "^(openssl|zlib):$triplet" > "$DEPS/vcpkg-versions.txt"
  # Linked in statically, so their notices travel with the DLL.
  license openssl "$installed/share/openssl/copyright"
  license zlib "$installed/share/zlib/copyright"
  stamp openssl "$key"
  stamp zlib "$key"
}

# Windows: MSVC import libraries for the MinGW-built FFmpeg DLLs, made from
# the names each DLL exports, so quill_calls.dll imports avcodec-N.dll and
# the rest by the names Quill ships them under.
ffmpeg_import_libs() {
  local out="$WORK/ffmpeg-msvc" name dll
  rm -rf "$out"
  mkdir -p "$out"
  for name in avcodec avformat avutil swresample swscale; do
    dll="$(find "$FFMPEG_PREFIX/bin" -maxdepth 1 -name "$name-*.dll" | head -n 1)"
    [[ -n "$dll" ]] || { echo "error: $name-*.dll not found in $FFMPEG_PREFIX/bin" >&2; exit 2; }
    {
      echo "LIBRARY $(basename "$dll")"
      echo "EXPORTS"
      # "ordinal hint RVA name" rows.
      dumpbin -nologo -exports "$dll" | tr -d '\r' \
        | awk 'NF == 4 && $1 ~ /^[0-9]+$/ && $2 ~ /^[0-9A-Fa-f]+$/ && length($3) == 8 { print "  " $4 }'
    } > "$out/$name.def"
    grep -q -E '^  (av|sw)' "$out/$name.def" || { echo "error: no exports read from $dll" >&2; exit 1; }
    # No MSYS2 argument conversion: it would read -def:C:/... as a path list.
    MSYS2_ARG_CONV_EXCL='*' lib -nologo -machine:x64 -def:"$out/$name.def" -out:"$out/$name.lib"
  done
}

build_tg_owt() {
  built tg_owt "$TG_OWT_KEY" "$(slib tg_owt)" && return
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
  # tdesktop's special target names the platform; on Windows, win64.
  local extra=() special="$OS"
  [[ "$OS" == macos ]] && extra=(-DCMAKE_OSX_ARCHITECTURES="$ARCH")
  [[ "$OS" == linux ]] && extra=(-DTG_OWT_DLOPEN_PIPEWIRE=ON)
  [[ "$OS" == windows ]] && special=win64
  cmake -S "$WORK/tg_owt" -B "$WORK/tg_owt/out" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$DEPS" -DCMAKE_INSTALL_LIBDIR=lib \
    -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
    -DTG_OWT_BUILD_AUDIO_BACKENDS=OFF \
    -DTG_OWT_SPECIAL_TARGET="$special" \
    -DTG_OWT_LIBJPEG_INCLUDE_PATH="$DEPS/include" \
    -DTG_OWT_OPENSSL_INCLUDE_PATH="$OPENSSL_INCLUDE" \
    -DTG_OWT_OPUS_INCLUDE_PATH="$DEPS/include/opus" \
    -DTG_OWT_LIBVPX_INCLUDE_PATH="$DEPS/include" \
    -DTG_OWT_OPENH264_INCLUDE_PATH="$DEPS/include" \
    -DTG_OWT_FFMPEG_INCLUDE_PATH="$FFMPEG_PREFIX/include" \
    ${extra[@]+"${extra[@]}"} ${CMAKE_EXTRA[@]+"${CMAKE_EXTRA[@]}"}
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
  # WebRTC's modules/third_party code that tg_owt compiles in (the G.711
  # and G.722 codecs, iSAC's FFT, PortAudio's ring buffer).
  local part
  for part in fft g711 g722 portaudio; do
    license "$part" "$owt/src/modules/third_party/$part/LICENSE"
  done
  if [[ "$OS" == linux ]]; then
    license pipewire "$owt/src/third_party/pipewire/COPYING"
  fi
  stamp tg_owt "$TG_OWT_KEY"
}

# tg_owt compiles against the codecs' and FFmpeg's headers, so a new pin of
# any of them rebuilds it.
TG_OWT_KEY="$TG_OWT_COMMIT pipewire-$PIPEWIRE_COMMIT opus-$OPUS_COMMIT libvpx-$LIBVPX_COMMIT"
TG_OWT_KEY+=" openh264-$OPENH264_COMMIT mozjpeg-$MOZJPEG_COMMIT"
TG_OWT_KEY+=" ffmpeg-$(cat "$FFMPEG_PREFIX"/include/libav*/version*.h | sha256 | cut -c1-16)"

if [[ "$OS" == windows ]]; then
  build_vcpkg_deps
  ffmpeg_import_libs
fi
build_opus
build_libvpx
build_openh264
build_mozjpeg
build_rnnoise
build_tg_owt
fetch tgcalls https://github.com/TelegramMessenger/tgcalls.git "$TGCALLS_COMMIT"
# Quill's fixes to tgcalls (native/quill-calls/patches), each applied once.
apply_patches tgcalls "$ROOT"/native/quill-calls/patches/tgcalls-*.patch

cmake -S "$ROOT/native/quill-calls" -B "$BUILD" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$DEST" \
  -DQUILL_CALLS_DEPS="$DEPS" \
  -DQUILL_CALLS_TGCALLS="$WORK/tgcalls" \
  -DQUILL_CALLS_FFMPEG="$FFMPEG_PREFIX" \
  -DQUILL_CALLS_OPENSSL_INCLUDE="$OPENSSL_INCLUDE" \
  ${OPENSSL_ROOT_DIR:+-DOPENSSL_ROOT_DIR="$OPENSSL_ROOT_DIR"} \
  $([[ "$OS" == macos ]] && echo "-DCMAKE_OSX_ARCHITECTURES=$ARCH") \
  $([[ "$OS" == windows ]] && echo "-DQUILL_CALLS_FFMPEG_LIB=$WORK/ffmpeg-msvc") \
  ${CMAKE_EXTRA[@]+"${CMAKE_EXTRA[@]}"}
cmake --build "$BUILD" --parallel "$JOBS"
cmake --install "$BUILD"

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
if [[ "$OS" == windows ]]; then
  {
    echo "Windows: OpenSSL and zlib, linked statically, as vcpkg builds them (x64-windows-static)"
    echo "  at https://github.com/microsoft/vcpkg $(git -c safe.directory="*" -C "$VCPKG_DIR" rev-parse HEAD 2>/dev/null || echo unknown):"
    sed 's/^/  /' "$DEPS/vcpkg-versions.txt"
  } >> "$DEST/share/SOURCES.txt"
fi

# The licenses the library's parts ask to travel with it in binary form:
# tgcalls' LGPL-3.0 (with the GPL-3.0 it extends) and the BSD, MIT and
# Apache notices of tg_owt, the libraries tg_owt carries and the codecs.
license tgcalls "$WORK/tgcalls/LICENSE" "$ROOT/licenses/GPL-3.0.txt"
rm -rf "$DEST/share/licenses"
cp -R "$DEPS/licenses" "$DEST/share/licenses"
# tgcalls carries json11 without a license file; its notice heads the source.
mkdir -p "$DEST/share/licenses/json11"
sed -n '1,/\*\//p' "$WORK/tgcalls/tgcalls/third-party/json11.cpp" > "$DEST/share/licenses/json11/LICENSE"

case "$OS" in
  linux) LIB="$DEST/lib/libquill_calls.so" ;;
  macos) LIB="$DEST/lib/libquill_calls.dylib" ;;
  windows) LIB="$DEST/lib/quill_calls.dll" ;;
esac
[[ -f "$LIB" ]] || { echo "error: $LIB was not built" >&2; exit 1; }
if [[ "$OS" == windows ]]; then
  # The C runtime, OpenSSL and zlib are linked in: the DLL imports only
  # Windows' own DLLs and the FFmpeg DLLs shipped beside it.
  imports="$(dumpbin -nologo -dependents "$LIB" | tr -d '\r' | awk 'tolower($1) ~ /\.dll$/ { print tolower($1) }')"
  echo "$LIB imports:" $imports
  if grep -E '^(vcruntime|msvcp|concrt|libssl|libcrypto|zlib)' <<<"$imports"; then
    echo "error: $LIB imports a library that should be linked in statically" >&2
    exit 1
  fi
fi
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
sha256 "$LIB" | tee "$DEST/share/libquill_calls.sha256"
echo "Built $LIB"
