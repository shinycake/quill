#!/usr/bin/env bash
# Capture NeedTdjson + WaitPhoneNumber GPUI surfaces under Xvfb + lavapipe.
# Usage: bash scripts/capture-connect-screenshots.sh [outdir]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-$ROOT/docs/screenshots}"
mkdir -p "$OUT"

DISPLAY_NUM="${QUILL_XVFB_DISPLAY:-99}"
export DISPLAY=":${DISPLAY_NUM}"
export LIBGL_ALWAYS_SOFTWARE=1
export WGPU_BACKEND=vulkan
export VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/usr/share/vulkan/icd.d/lvp_icd.json}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp/quill-xdg-runtime}"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

# Fontconfig (GPUI text)
export FONTCONFIG_PATH="${FONTCONFIG_PATH:-/etc/fonts}"

if ! pgrep -f "Xvfb :${DISPLAY_NUM}" >/dev/null 2>&1; then
  Xvfb ":${DISPLAY_NUM}" -screen 0 1280x800x24 -ac +extension GLX +render -noreset &
  XVFB_PID=$!
  cleanup() { kill "$XVFB_PID" 2>/dev/null || true; }
  trap cleanup EXIT
  sleep 0.5
fi

capture_one() {
  local kind="$1"
  local png="$2"
  local marker="$OUT/.quill-ready-${kind}"
  rm -f "$marker"
  echo "==> demo ${kind} → ${png}"
  cargo run --features ui --quiet -- --screenshot-demo "$kind" "$OUT" &
  APP_PID=$!
  for _ in $(seq 1 60); do
    if [[ -f "$marker" ]]; then
      break
    fi
    if ! kill -0 "$APP_PID" 2>/dev/null; then
      echo "quill exited before ready marker" >&2
      wait "$APP_PID" || true
      return 1
    fi
    sleep 0.25
  done
  if [[ ! -f "$marker" ]]; then
    echo "timed out waiting for ${marker}" >&2
    kill "$APP_PID" 2>/dev/null || true
    return 1
  fi
  # Window origin is (20,20); size 1200x740 — grab with a little padding.
  ffmpeg -y -hide_banner -loglevel error \
    -f x11grab -draw_mouse 0 -video_size 1240x780 -i "${DISPLAY}+0,0" \
    -frames:v 1 "$png"
  wait "$APP_PID" || true
  rm -f "$marker"
  echo "wrote $png ($(wc -c < "$png") bytes)"
}

cd "$ROOT"
capture_one need-tdjson "$OUT/connect-need-tdjson.png"
capture_one wait-phone "$OUT/connect-wait-phone.png"
capture_one wait-code "$OUT/connect-wait-code.png"
capture_one wait-password "$OUT/connect-wait-password.png"
echo "done"
