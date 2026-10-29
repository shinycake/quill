#!/usr/bin/env bash
# Corresponding source for the LGPL components Quill's packages ship, attached to
# every GitHub release (docs/decisions/codex-release-pipeline.md):
#
#   quill-source-ffmpeg-<tag>.tar.xz               FFmpeg as built by scripts/build-ffmpeg.sh
#                                                  (Linux, Windows; on macOS for the call
#                                                  library), plus its configure line
#   quill-source-tgcalls-<commit>.tar.xz           tgcalls at the commit libquill_calls is
#                                                  built from (scripts/build-tgcalls.sh)
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

# Pins read from the scripts that build the shipped binaries.
FFMPEG_TAG="$(sed -n 's/^TAG=\(n[0-9.]*\)$/\1/p' "$ROOT/scripts/build-ffmpeg.sh")"
FFMPEG_COMMIT="$(sed -n 's/^COMMIT=\([0-9a-f]\{40\}\)$/\1/p' "$ROOT/scripts/build-ffmpeg.sh")"
TGCALLS_COMMIT="$(sed -n 's/^TGCALLS_COMMIT=\([0-9a-f]\{40\}\)$/\1/p' "$ROOT/scripts/build-tgcalls.sh")"
[[ -n "$FFMPEG_TAG" && -n "$FFMPEG_COMMIT" && -n "$TGCALLS_COMMIT" ]] || { echo "error: could not read pins" >&2; exit 1; }

# archive <name> <url> <ref> [expected-commit]: shallow-fetch <ref> (a tag or a
# commit), check the commit, write <name>.tar.xz with a <name>/ prefix. Prints
# the commit.
archive() {
  local name="$1" url="$2" ref="$3" want="${4:-}" dir commit
  dir="$WORK/$name"
  git init -q "$dir"
  git -C "$dir" fetch -q --depth 1 "$url" "$ref"
  commit="$(git -C "$dir" rev-parse 'FETCH_HEAD^{commit}')"
  if [[ -n "$want" && "$commit" != "$want" ]]; then
    echo "error: $url $ref is $commit, expected $want" >&2
    exit 1
  fi
  git -C "$dir" archive --format=tar --prefix="$name/" "$commit" | xz -T0 -6 > "$OUT/$name.tar.xz"
  echo "$commit"
}

echo "FFmpeg $FFMPEG_TAG (Quill's build)" >&2
FF_COMMIT="$(archive "quill-source-ffmpeg-$FFMPEG_TAG" https://github.com/FFmpeg/FFmpeg.git "$FFMPEG_TAG" "$FFMPEG_COMMIT")"

TGCALLS_SHORT="${TGCALLS_COMMIT:0:8}"
echo "tgcalls $TGCALLS_COMMIT" >&2
archive "quill-source-tgcalls-$TGCALLS_SHORT" https://github.com/TelegramMessenger/tgcalls.git \
  "$TGCALLS_COMMIT" "$TGCALLS_COMMIT" >/dev/null

cat > "$OUT/LGPL-SOURCES.txt" <<EOF
Corresponding source for the LGPL components shipped with Quill

Quill itself is MIT-licensed; its full source is the git tag of this release at
https://github.com/shinycake/quill. The packages also ship these LGPL libraries
as separate, replaceable shared libraries (Quill loads them at runtime):

FFmpeg $FFMPEG_TAG (LGPL-2.1-or-later), all packages
  libavcodec, libavformat, libavutil, libswscale, libswresample
  Source:  https://github.com/FFmpeg/FFmpeg/tree/$FFMPEG_TAG  (commit $FF_COMMIT)
  Archive: quill-source-ffmpeg-$FFMPEG_TAG.tar.xz
  Built unmodified by scripts/build-ffmpeg.sh in the Quill tree; the exact
  configure line is in licenses/ffmpeg/FFMPEG-SOURCE.txt inside each package.

tgcalls (LGPL-3.0), inside libquill_calls (libquill_calls.so, libquill_calls.dylib)
  Telegram's call library, built with Quill's C API by scripts/build-tgcalls.sh
  in the Quill tree, which applies Quill's patches from native/quill-calls/patches.
  Source:  https://github.com/TelegramMessenger/tgcalls/tree/$TGCALLS_COMMIT
  Archive: quill-source-tgcalls-$TGCALLS_SHORT.tar.xz
  libquill_calls links FFmpeg (above) and OpenSSL dynamically. Its other parts
  (tg_owt, Opus, libvpx, OpenH264, mozjpeg, rnnoise) are under permissive
  licenses: licenses/quill-calls/SOURCES.txt inside each package names the
  exact commit of each, and licenses/quill-calls/licenses/ holds their notices.

Written offer: for at least three years after this release, the Quill
maintainers will provide the complete corresponding source of any LGPL
component listed above, for the exact version shipped here, to anyone who asks
by opening an issue at https://github.com/shinycake/quill/issues. You may
replace any of these libraries in the package with your own compatible build.
EOF

ls -l "$OUT"
