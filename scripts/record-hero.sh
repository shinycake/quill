#!/bin/bash
# Records the product page's hero clip (site/media/hero-*.mp4) on macOS from
# the `ready-showcase` demo: invented chats, generated art, no live Telegram.
# Needs a release build with the demo-capture feature, ffmpeg (with libx264),
# and Screen Recording permission for the terminal. Keep the mouse pointer
# outside the top-left 1420x930 points of the main display while it records.
#
#   cargo build --release --features ui,demo-capture
#   scripts/record-hero.sh
set -euo pipefail
cd "$(dirname "$0")/.."
TMP=$(mktemp -d)
# Window points in a 1400x900 window: hover, click, pause (ms), close viewer.
SCRIPT="p:2200;m:161,413;p:450;161,413;p:2300;m:161,725;p:450;161,725;p:2300;m:154,668;p:450;154,668;p:1900;m:493,365;p:600;493,365;p:2200;m:1364,414;p:350;1364,414;p:2000;m:1364,22;p:350;1364,22;p:1300;m:161,350;p:450;161,350;p:4000"
SCREEN=$(ffmpeg -hide_banner -f avfoundation -list_devices true -i "" 2>&1 | sed -n 's/.*\[\([0-9]*\)\] Capture screen 0.*/\1/p')
# The demo window opens at (20,30) points; the capture is in pixels (2x).
ffmpeg -hide_banner -loglevel error -y -f avfoundation -capture_cursor 0 -framerate 60 \
  -pixel_format nv12 -i "$SCREEN:none" -t 28 -vf "crop=2800:1800:40:60" \
  -c:v libx264 -preset veryfast -crf 10 -pix_fmt yuv420p "$TMP/take.mp4" &
FF=$!
sleep 1
QUILL_DEMO_WINDOW_SIZE=1400x900 QUILL_DEMO_SHOWCASE=chat QUILL_DEMO_THEME=dark \
  QUILL_DEMO_LINGER_MS=60000 QUILL_DEMO_CLICK="$SCRIPT" \
  target/release/quill --screenshot-demo ready-showcase "$TMP/out" >/dev/null 2>&1 &
APP=$!
wait $FF
kill $APP 2>/dev/null || true
# Cut from shortly before the first click to the same moment after the
# return to the first chat, so the loop has no visible seam. Check the cut
# points with: ffmpeg -i take.mp4 -vf "select='gt(scene,0.01)',showinfo" -f null -
START=3.55 END=25.77
for W in 2240 1280; do
  CRF=$([ $W = 2240 ] && echo 21 || echo 23)
  ffmpeg -v error -y -ss $START -to $END -i "$TMP/take.mp4" -vf "scale=$W:-2:flags=lanczos" -an \
    -c:v libx264 -crf $CRF -preset slower -tune animation -pix_fmt yuv420p -profile:v high \
    -g 300 -movflags +faststart "site/media/hero-$W.mp4"
done
ffmpeg -v error -y -ss $START -i "$TMP/take.mp4" -frames:v 1 -vf "scale=2240:-2:flags=lanczos" "$TMP/poster.png"
cwebp -quiet -q 80 "$TMP/poster.png" -o site/img/hero-poster.webp
echo "wrote site/media/hero-2240.mp4, hero-1280.mp4 and site/img/hero-poster.webp"
