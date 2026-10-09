#!/usr/bin/env bash
# Corresponding source for the LGPL components Quill's packages ship, attached to
# every GitHub release (docs/decisions/codex-release-pipeline.md):
#
#   quill-source-ffmpeg-<tag>.tar.xz               FFmpeg as built by scripts/build-ffmpeg.sh
#                                                  (Linux, Windows), plus its configure line
#   quill-source-ntgcalls-<tag>.tar.xz             ntgcalls at the tag the prebuilt library
#                                                  comes from (scripts/vendor-ntgcalls.sh)
#   quill-source-ntgcalls-ffmpeg-<tag>.tar.xz      the FFmpeg release ntgcalls links statically
#   quill-source-ntgcalls-glib-<ver>.tar.xz        the GLib release ntgcalls links (Linux)
#   quill-source-ntgcalls-build-scripts.tar.xz     pytgcalls' FFmpeg/GLib build scripts at
#                                                  the tags ntgcalls pins
#   LGPL-SOURCES.txt                               exact URLs, commits and the written offer
#
#   scripts/release-lgpl-sources.sh <out-dir>
# Every archive is `git archive` of a commit that is checked against the pin, so
# the archive is exactly that commit's tree. Needs git, xz and network access.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:?usage: release-lgpl-sources.sh <out-dir>}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Pins read from the scripts that build or fetch the shipped binaries.
FFMPEG_TAG="$(sed -n 's/^TAG=\(n[0-9.]*\)$/\1/p' "$ROOT/scripts/build-ffmpeg.sh")"
FFMPEG_COMMIT="$(sed -n 's/^COMMIT=\([0-9a-f]\{40\}\)$/\1/p' "$ROOT/scripts/build-ffmpeg.sh")"
NTG_TAG="$(sed -n 's/^NTGCALLS_VERSION="\(v[0-9.]*\)"$/\1/p' "$ROOT/scripts/vendor-ntgcalls.sh")"
[[ -n "$FFMPEG_TAG" && -n "$FFMPEG_COMMIT" && -n "$NTG_TAG" ]] || { echo "error: could not read pins" >&2; exit 1; }

# archive <name> <url> <ref> [expected-commit]: shallow-clone <ref>, check the
# commit, write <name>.tar.xz with a <name>/ prefix. Prints the commit.
archive() {
  local name="$1" url="$2" ref="$3" want="${4:-}" dir commit
  dir="$WORK/$name"
  git -c advice.detachedHead=false clone -q --depth 1 --branch "$ref" "$url" "$dir"
  commit="$(git -C "$dir" rev-parse HEAD)"
  if [[ -n "$want" && "$commit" != "$want" ]]; then
    echo "error: $url $ref is $commit, expected $want" >&2
    exit 1
  fi
  git -C "$dir" archive --format=tar --prefix="$name/" HEAD | xz -T0 -6 > "$OUT/$name.tar.xz"
  echo "$commit"
}

echo "FFmpeg $FFMPEG_TAG (Quill's build)" >&2
FF_COMMIT="$(archive "quill-source-ffmpeg-$FFMPEG_TAG" https://github.com/FFmpeg/FFmpeg.git "$FFMPEG_TAG" "$FFMPEG_COMMIT")"

echo "ntgcalls $NTG_TAG" >&2
NTG_COMMIT="$(archive "quill-source-ntgcalls-$NTG_TAG" https://github.com/pytgcalls/ntgcalls.git "$NTG_TAG")"
PROPS="$WORK/quill-source-ntgcalls-$NTG_TAG/version.properties"
prop() { sed -n "s/^version\.$1=\([^[:space:]]*\).*/\1/p" "$PROPS"; }
NTG_FFMPEG="$(prop ffmpeg)"
NTG_GLIB="$(prop glib)"
[[ -n "$NTG_FFMPEG" && -n "$NTG_GLIB" ]] || { echo "error: ntgcalls version.properties has no ffmpeg/glib pin" >&2; exit 1; }

echo "FFmpeg n$NTG_FFMPEG (inside ntgcalls)" >&2
NTG_FF_COMMIT="$(archive "quill-source-ntgcalls-ffmpeg-n$NTG_FFMPEG" https://github.com/FFmpeg/FFmpeg.git "n$NTG_FFMPEG")"
echo "GLib $NTG_GLIB (inside ntgcalls, Linux)" >&2
NTG_GLIB_COMMIT="$(archive "quill-source-ntgcalls-glib-$NTG_GLIB" https://github.com/GNOME/glib.git "$NTG_GLIB")"

# pytgcalls' build scripts at the tags matching those versions: they hold the
# configure options and patches used for the static FFmpeg/GLib in ntgcalls.
echo "pytgcalls build scripts" >&2
SCRIPTS="$WORK/quill-source-ntgcalls-build-scripts"
mkdir -p "$SCRIPTS"
BUILD_FF_COMMIT="$(archive build-ffmpeg https://github.com/pytgcalls/ffmpeg.git "v$NTG_FFMPEG")"
BUILD_GLIB_COMMIT="$(archive build-glib https://github.com/pytgcalls/glib.git "v$NTG_GLIB")"
for n in build-ffmpeg build-glib; do
  xz -dc "$OUT/$n.tar.xz" | tar -x -C "$SCRIPTS"
  rm "$OUT/$n.tar.xz"
done
tar -C "$WORK" -cf - quill-source-ntgcalls-build-scripts | xz -T0 -6 > "$OUT/quill-source-ntgcalls-build-scripts.tar.xz"

cp "$ROOT/licenses/ntgcalls-components.txt" "$WORK/components.txt"
cat > "$OUT/LGPL-SOURCES.txt" <<EOF
Corresponding source for the LGPL components shipped with Quill

Quill itself is MIT-licensed; its full source is the git tag of this release at
https://github.com/shinycake/quill. The packages also ship these LGPL libraries
as separate, replaceable shared libraries (Quill loads them at runtime):

FFmpeg $FFMPEG_TAG (LGPL-2.1-or-later), Linux and Windows packages
  libavcodec, libavformat, libavutil, libswscale, libswresample
  Source:  https://github.com/FFmpeg/FFmpeg/tree/$FFMPEG_TAG  (commit $FF_COMMIT)
  Archive: quill-source-ffmpeg-$FFMPEG_TAG.tar.xz
  Built unmodified by scripts/build-ffmpeg.sh in the Quill tree; the exact
  configure line is in licenses/ffmpeg/FFMPEG-SOURCE.txt inside each package.

ntgcalls $NTG_TAG (LGPL-3.0), all packages (libntgcalls.so / .dylib, ntgcalls.dll)
  Official prebuilt library from https://github.com/pytgcalls/ntgcalls/releases/tag/$NTG_TAG,
  unmodified and SHA-256 pinned by scripts/vendor-ntgcalls.sh.
  Source:  https://github.com/pytgcalls/ntgcalls/tree/$NTG_TAG  (commit $NTG_COMMIT)
  Archive: quill-source-ntgcalls-$NTG_TAG.tar.xz (git submodules pybind11 and
  oboe are not included: they are used only by the Python and Android builds)

  ntgcalls links these LGPL libraries statically (versions from its
  version.properties):
  FFmpeg n$NTG_FFMPEG (LGPL-2.1-or-later), all platforms
    Source:  https://github.com/FFmpeg/FFmpeg/tree/n$NTG_FFMPEG  (commit $NTG_FF_COMMIT)
    Archive: quill-source-ntgcalls-ffmpeg-n$NTG_FFMPEG.tar.xz
    Build:   https://github.com/pytgcalls/ffmpeg/tree/v$NTG_FFMPEG  (commit $BUILD_FF_COMMIT)
  GLib $NTG_GLIB (LGPL-2.1-or-later), Linux only
    Source:  https://github.com/GNOME/glib/tree/$NTG_GLIB  (commit $NTG_GLIB_COMMIT)
    Archive: quill-source-ntgcalls-glib-$NTG_GLIB.tar.xz
    Build:   https://github.com/pytgcalls/glib/tree/v$NTG_GLIB  (commit $BUILD_GLIB_COMMIT)
  Archive of both build-script repositories: quill-source-ntgcalls-build-scripts.tar.xz

Written offer: for at least three years after this release, the Quill
maintainers will provide the complete corresponding source of any LGPL
component listed above, for the exact version shipped here, to anyone who asks
by opening an issue at https://github.com/shinycake/quill/issues. You may
replace any of these libraries in the package with your own compatible build.

Other components inside the ntgcalls library and their licenses:

$(cat "$WORK/components.txt")
EOF

ls -l "$OUT"
