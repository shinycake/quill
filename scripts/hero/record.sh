#!/bin/bash
# Records the product page's hero clip (site/media/hero-*.mp4) on macOS.
#
# 1. Runs the `ready-showcase` demo (invented chats, generated art, no live
#    Telegram) with a scripted tour: QUILL_DEMO_CLICK hovers and clicks
#    in-process and logs when each step ran (QUILL_DEMO_CLICK_LOG).
# 2. Records only that window with ScreenCaptureKit (sckrec.swift): no
#    cursor, no other windows, 60 fps at the display's pixel scale.
# 3. composite.py draws a synthetic cursor that glides between the logged
#    hover points on eased arcs, with motion blur, a press dip and a ripple
#    on clicks, and pastes back the traffic lights macOS swaps for a
#    "being recorded" badge (lights.png, cut from an ordinary capture).
# 4. The loop seam gets a 0.5 s crossfade, then H.264 at 2240 and 1280 px.
#
# Needs: a release build with demo-capture, Xcode command line tools,
# ffmpeg with libx264, cwebp, python3 with numpy and pillow, and Screen
# Recording permission for the terminal.
#
#   cargo build --release --features ui,demo-capture
#   scripts/hero/record.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HERE="$ROOT/scripts/hero"
TMP=$(mktemp -d)
PY=${PYTHON:-python3}
swiftc -O "$HERE/sckrec.swift" -o "$TMP/sckrec"
resvg -w 160 "$HERE/cursor.svg" "$TMP/cursor@4x.png" 2>/dev/null \
  || rsvg-convert -w 160 "$HERE/cursor.svg" -o "$TMP/cursor@4x.png"
cp "$HERE/lights.png" "$TMP/"

# Window points in a 1400x900 window: m: hover, x,y click, p: pause (ms).
# It ends hovering the empty chat header, where the cursor rests at the seam.
TOUR="p:2200;m:161,413;p:450;161,413;p:2300;m:161,725;p:450;161,725;p:2300;m:154,668;p:450;154,668;p:1900;m:493,365;p:600;493,365;p:2200;m:1364,414;p:350;1364,414;p:2000;m:1364,22;p:350;1364,22;p:1300;m:161,350;p:450;161,350;p:1500;m:900,58;p:3000"

cd "$ROOT"
QUILL_DEMO_WINDOW_ORIGIN=20,30 QUILL_DEMO_WINDOW_SIZE=1400x900 QUILL_DEMO_SHOWCASE=chat \
  QUILL_DEMO_THEME=dark QUILL_DEMO_LINGER_MS=60000 QUILL_DEMO_CLICK="$TOUR" \
  QUILL_DEMO_CLICK_LOG="$TMP/steps.log" \
  target/release/quill --screenshot-demo ready-showcase "$TMP/out" >/dev/null 2>&1 &
APP=$!
"$TMP/sckrec" $APP 32 "$TMP/take.mov" | tee "$TMP/rec.log"
kill $APP 2>/dev/null || true

# Seconds between the start of the recording and the first logged step.
OFFSET=$("$PY" - "$TMP" <<'PY'
import sys
d = sys.argv[1]
first = int(open(f"{d}/steps.log").readline().split()[0]) / 1000
start = float(open(f"{d}/rec.log").read().split("recording started ")[1].split()[0])
print(round(first - start, 3))
PY
)
# Cut from shortly before the first hover to shortly after the last one.
read START END < <("$PY" - "$TMP" "$OFFSET" <<'PY'
import sys
d, off = sys.argv[1], float(sys.argv[2])
lines = [l.split(" ", 1) for l in open(f"{d}/steps.log")]
t0 = int(lines[0][0]) / 1000
hovers = [int(ms) / 1000 - t0 + off for ms, s in lines if s.startswith("m:")]
print(round(hovers[0] - 1.3, 2), round(hovers[-1] + 0.46, 2))
PY
)
(cd "$TMP" && "$PY" "$HERE/composite.py" take.mov steps.log "$OFFSET" "$START" "$END" master.mp4 cursor@4x.png)
D=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$TMP/master.mp4")
FADE_AT=$("$PY" -c "print(round($D - 1.0, 3))")
for SIZE in 2240 1280; do
  CRF=$([ $SIZE = 2240 ] && echo 21 || echo 23)
  ffmpeg -v error -y -i "$TMP/master.mp4" -filter_complex \
    "[0:v]split[a][b];[a]trim=0.5:$D,setpts=PTS-STARTPTS[main];[b]trim=0:0.5,setpts=PTS-STARTPTS[head];[main][head]xfade=transition=fade:duration=0.5:offset=$FADE_AT,scale=$SIZE:-2:flags=lanczos,format=yuv420p[v]" \
    -map "[v]" -an -c:v libx264 -crf $CRF -preset slower -tune animation -profile:v high \
    -g 300 -movflags +faststart "site/media/hero-$SIZE.mp4"
done
ffmpeg -v error -y -i site/media/hero-2240.mp4 -frames:v 1 "$TMP/poster.png"
cwebp -quiet -q 80 "$TMP/poster.png" -o site/img/hero-poster.webp
echo "wrote site/media/hero-2240.mp4, hero-1280.mp4 and site/img/hero-poster.webp"
