#!/usr/bin/env bash
# Run the ffmpeg and ffprobe that ship in a Quill package the way Quill does,
# with no library search path set, and fail if they are missing, GPL/nonfree,
# or cannot do what the app's command lines need (stills, MPEG-4/AAC clips,
# raw camera-preview frames). Used by package-linux.yml and package-windows.yml.
#
#   scripts/check-ffmpeg-tools.sh <dir holding ffmpeg and ffprobe> [--expect-encoder NAME]...
set -euo pipefail

DIR="${1:?usage: check-ffmpeg-tools.sh <dir> [--expect-encoder NAME]...}"
shift
EXPECT=()
while (( $# )); do
  case "$1" in
    --expect-encoder) EXPECT+=("${2:?--expect-encoder needs a name}"); shift 2 ;;
    *) echo "error: unknown argument $1" >&2; exit 2 ;;
  esac
done

FFMPEG="$DIR/ffmpeg"
FFPROBE="$DIR/ffprobe"
for tool in "$FFMPEG" "$FFPROBE"; do
  [[ -x "$tool" || -x "$tool.exe" ]] || { echo "FAIL: $tool is missing from the package" >&2; exit 1; }
done

"$FFPROBE" -version | head -n 1
version="$("$FFMPEG" -version)"
head -n 1 <<<"$version"
if grep -q -e '--enable-gpl' -e '--enable-nonfree' -e '--enable-version3' <<<"$version"; then
  echo "FAIL: ffmpeg was configured with GPL, version3 or nonfree options" >&2
  exit 1
fi
license="$("$FFMPEG" -hide_banner -L 2>&1 | head -n 8)"
grep -q 'Lesser General Public' <<<"$(tr '\n' ' ' <<<"$license")" || { echo "FAIL: ffmpeg reports a non-LGPL license: $license" >&2; exit 1; }

encoders="$("$FFMPEG" -hide_banner -encoders 2>&1)"
if grep -q -w libx264 <<<"$encoders"; then
  echo "FAIL: libx264 (GPL) is linked into the bundled ffmpeg" >&2
  exit 1
fi
for name in "${EXPECT[@]+"${EXPECT[@]}"}"; do
  grep -q -w "$name" <<<"$encoders" || { echo "FAIL: encoder $name is missing" >&2; exit 1; }
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"
run() { "$FFMPEG" -y -hide_banner -loglevel error "$@"; }

# A still, downscaled to JPEG and PNG as the photo send does, then probed.
run -f lavfi -i testsrc=duration=1:size=320x240:rate=10 -frames:v 1 still.png
run -i still.png -vf scale=100:80:flags=lanczos -frames:v 1 -q:v 2 small.jpg
run -i still.png -vf scale=100:80:flags=lanczos -frames:v 1 small.png
size="$("$FFPROBE" -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 small.jpg | tr -d '\r')"
[[ "$size" == "100,80" ]] || { echo "FAIL: ffprobe read '$size' for the downscaled photo, want 100,80" >&2; exit 1; }

# A clip with sound (MPEG-4 + AAC in MP4), its duration, the camera-preview
# pipe and the editor's cut/turn/scale filters.
run -f lavfi -i testsrc=duration=2:size=320x240:rate=10 -f lavfi -i sine=duration=2 \
  -c:v mpeg4 -c:a aac -shortest clip.mp4
duration="$("$FFPROBE" -v error -show_entries format=duration -of default=nw=1:nk=1 clip.mp4 | tr -d '\r')"
[[ "$duration" == 2.* ]] || { echo "FAIL: ffprobe read duration '$duration' for the 2 s clip" >&2; exit 1; }
bytes="$(run -i clip.mp4 -vf "hflip,crop=min(iw\,ih):min(iw\,ih),scale=64:64,fps=5" -f rawvideo -pix_fmt rgb24 - | wc -c | tr -d ' ')"
(( bytes == 64 * 64 * 3 * 10 )) || { echo "FAIL: preview pipe produced $bytes bytes, want $((64 * 64 * 3 * 10))" >&2; exit 1; }
run -ss 0.5 -t 1 -i clip.mp4 -vf "transpose=1,scale=-2:100,format=yuv420p" -an -c:v mpeg4 -movflags +faststart cut.mp4
[[ -s cut.mp4 ]] || { echo "FAIL: the cut/turn/scale encode wrote nothing" >&2; exit 1; }
run -ss 0.5 -i clip.mp4 -frames:v 1 -vf scale=-2:60 -q:v 4 frame.jpg
run -i clip.mp4 -vf "fps=2,scale=32:32:force_original_aspect_ratio=decrease" -frames:v 2 frames-%03d.png
[[ -s frames-002.png ]] || { echo "FAIL: frame extraction wrote nothing" >&2; exit 1; }
echo "OK: bundled ffmpeg/ffprobe in $DIR start without a library path and encode, probe and filter"
