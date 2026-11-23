#!/usr/bin/env bash
# ready-showcase on a virtual X display: Vulkan (lavapipe) and OpenGL (llvmpipe,
# Vulkan hidden), glass on and off, light and dark. Usage: glass-shots.sh <bin> <out>
set -uo pipefail
bin=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
out=$2
mkdir -p "$out"
lvp=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json | head -1)
Xvfb :97 -screen 0 1280x860x24 >"$out/xvfb.log" 2>&1 &
xvfb=$!
sleep 2
export DISPLAY=:97
VK_ICD_FILENAMES=$lvp vulkaninfo 2>/dev/null | grep -B2 -A12 "supportedUsageFlags" | head -40 >"$out/vulkan-surface.txt" || true
glxinfo -B >"$out/glxinfo.txt" 2>&1 || true
shot() {
  local name=$1; shift
  local dir="$out/$name"
  mkdir -p "$dir/data"
  rm -f "$dir/.quill-ready-ready-showcase"
  env XDG_DATA_HOME="$dir/data" QUILL_DEMO_LINGER_MS=9000 QUILL_REDUCED_MOTION=1 \
    QUILL_DEMO_WINDOW_SIZE=1200x800 QUILL_DEMO_CLICK='r:480,250;p:600' "$@" \
    "$bin" --screenshot-demo ready-showcase "$dir" >"$dir/run.log" 2>&1 &
  local app=$!
  for _ in $(seq 1 180); do
    [ -f "$dir/.quill-ready-ready-showcase" ] && break
    kill -0 "$app" 2>/dev/null || break
    sleep 1
  done
  sleep 4
  import -window root -depth 8 "$out/$name.png" || true
  wait "$app"; echo "$name exit $?" | tee -a "$out/summary.txt"
  grep -rh "glass\|GPU\|adapter\|wgpu\|panic" "$dir/run.log" "$dir/data" 2>/dev/null | head -40 >"$dir/grep.txt" || true
}
shot vulkan-light VK_ICD_FILENAMES=$lvp QUILL_GLASS=force
shot vulkan-dark VK_ICD_FILENAMES=$lvp QUILL_DEMO_THEME=dark QUILL_GLASS=force
shot vulkan-software-on VK_ICD_FILENAMES=$lvp QUILL_GLASS=on
shot vulkan-off VK_ICD_FILENAMES=$lvp QUILL_GLASS=off
shot gl-light VK_ICD_FILENAMES=/nonexistent.json VK_DRIVER_FILES=/nonexistent.json QUILL_GLASS=force
shot gl-off VK_ICD_FILENAMES=/nonexistent.json VK_DRIVER_FILES=/nonexistent.json QUILL_GLASS=off
kill "$xvfb" 2>/dev/null || true
exit 0
