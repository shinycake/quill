#!/bin/bash
# The hero tour: Cap Studio records the demo window while cliclick drives the
# real mouse and keyboard and `scroll` posts smooth trackpad scrolls.
# usage: tour.sh <out.cap>   (record.sh builds `scroll` into $H first)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
H="${TMPDIR:-/tmp}/quill-hero"
CAP=/Applications/Cap.app/Contents/MacOS/cap-cli
OUT="$1"
# The window sits just under the menu bar of a 1512x982-point laptop screen.
OX=56; OY=60
P() { echo "$((OX + $1)),$((OY + $2))"; }
mv_() { cliclick -e 420 m:"$(P "$1" "$2")"; }
clk() { cliclick -e 420 m:"$(P "$1" "$2")" w:220 c:.; }
scrl() { "$H/scroll" "$1" "$2"; }

pkill -f 'screenshot-demo ready-showcase' || true
sleep 1
cd "$ROOT"
QUILL_DEMO_HIDE_STATUS=1 QUILL_DEMO_WINDOW_ORIGIN=$OX,$OY QUILL_DEMO_WINDOW_SIZE=1400x900 \
  QUILL_DEMO_SHOWCASE=chat QUILL_DEMO_THEME=dark QUILL_DEMO_LINGER_MS=600000 \
  target/release/quill --screenshot-demo ready-showcase "$H/out" >/dev/null 2>&1 &
sleep 4
cliclick c:"$(P 700 12)" # bring the window to the front
WIN=$("$CAP" record windows --json | python3 -c "
import json, sys
d = json.load(sys.stdin)
ws = d if isinstance(d, list) else d.get('windows', d)
print([w['id'] for w in ws if w['ownerName'] == 'quill' and w['bounds']['x'] == $OX][0])")
rm -rf "$OUT"
"$CAP" record start --window "$WIN" --fps 60 --duration 58 --path "$OUT" >"$H/cap-record.log" 2>&1 &
REC=$!
sleep 2.5
# Window points in a 1400x900 window.
# 1. Weekend Hikers: scroll back through the album and replies.
mv_ 850 470; sleep 0.3; scrl 520 1.2; sleep 1.2; scrl -520 1.0; sleep 0.5
# 2. Maya Chen: type a reply and send it.
clk 161 338; sleep 1.3
clk 700 871; sleep 0.3
cliclick -w 55 t:"Adding Night Drive to the road trip playlist"; sleep 0.4
clk 1372 871; sleep 0.3
mv_ 900 640; sleep 0.2; scrl -300 0.5; sleep 1.2
# 3. Lunch Crew: scroll through the polls.
clk 161 658; sleep 1.0
mv_ 850 470; sleep 0.2; scrl 760 1.4; sleep 1.3; scrl -760 1.1; sleep 0.5
# 4. Search for the photo channel and open it.
clk 150 150; sleep 0.3
cliclick -w 70 t:"photo"; sleep 0.9
clk 150 364; sleep 1.2
# 5. Scroll the channel, then open a photo, flip to the next one, close.
mv_ 850 480; sleep 0.2; scrl 640 1.2; sleep 1.0; scrl -640 1.0; sleep 0.5
clk 526 252; sleep 1.7
clk 1366 410; sleep 1.7
clk 1366 28; sleep 0.9
# 6. Back to Weekend Hikers; rest on the header, where the tour started.
clk 161 274; sleep 1.1
mv_ 900 58; sleep 2.5
"$CAP" record stop >/dev/null 2>&1 || true
wait $REC || true
pkill -f 'screenshot-demo ready-showcase' || true
