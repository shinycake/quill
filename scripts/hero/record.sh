#!/bin/bash
# Records the product page's hero clip with Cap (https://cap.so), the open
# source screen recorder, in Studio mode. Cap records the window and the real
# cursor track, then renders a smoothed cursor with motion blur, click
# ripples and auto zooms on a wallpaper background.
#
# 1. tour.sh runs the `ready-showcase` demo (invented chats, generated art,
#    no live Telegram) and drives the real mouse and keyboard with cliclick,
#    plus smooth trackpad scrolls from scroll.swift, while `cap record` runs.
# 2. cap-config.py applies the look: trim, zooms, cursor, wallpaper.
# 3. `cap export` renders it; ffmpeg crossfades the loop seam and encodes the
#    web versions.
#
# Needs Cap Desktop (brew install --cask cap), cliclick, the Xcode command
# line tools, ffmpeg with libx264, cwebp, python3 with numpy and pillow, a
# release build (cargo build --release --features ui,demo-capture), and
# Screen Recording and Accessibility permission for the terminal. Don't touch
# the mouse while it records (about a minute). The demo's timestamps follow
# the local clock: late at night, run it with a TZ where it is mid-day (e.g.
# TZ=Asia/Tokyo) so they read naturally and no date divider shows up.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
H="${TMPDIR:-/tmp}/quill-hero"
mkdir -p "$H"
PY=${PYTHON:-python3}
CAP=/Applications/Cap.app/Contents/MacOS/cap-cli
swiftc -O "$HERE/scroll.swift" -o "$H/scroll"
"$PY" "$HERE/wallpaper.py" "$H/wallpaper.jpg"
bash "$HERE/tour.sh" "$H/tour.cap"
"$PY" "$HERE/cap-config.py" "$H/tour.cap" "$H/wallpaper.jpg"
"$CAP" export "$H/tour.cap" -o "$H/export.mp4" --fps 60 --resolution 2560x1440 --quality maximum
D=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$H/export.mp4")
FADE_AT=$("$PY" -c "print(round($D - 1.0, 3))")
cd "$ROOT"
for SIZE in 1920 1280; do
  ffmpeg -v error -y -i "$H/export.mp4" -filter_complex \
    "[0:v]split[a][b];[a]trim=0.5:$D,setpts=PTS-STARTPTS[main];[b]trim=0:0.5,setpts=PTS-STARTPTS[head];[main][head]xfade=transition=fade:duration=0.5:offset=$FADE_AT,scale=$SIZE:-2:flags=lanczos,format=yuv420p[v]" \
    -map "[v]" -an -c:v libx264 -crf 24 -preset slower -tune animation -profile:v high \
    -g 300 -movflags +faststart "site/media/hero-$SIZE.mp4"
done
ffmpeg -v error -y -i site/media/hero-1920.mp4 -frames:v 1 "$H/poster.png"
cwebp -quiet -q 80 "$H/poster.png" -o site/img/hero-poster.webp
echo "wrote site/media/hero-1920.mp4, hero-1280.mp4 and site/img/hero-poster.webp"
