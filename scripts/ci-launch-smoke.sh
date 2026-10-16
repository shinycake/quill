#!/usr/bin/env bash
# Linux launch smoke (docs/decisions/ci-launch-smoke.md): open the real UI window
# of a demo-capture build on a virtual X display with software Vulkan (Mesa
# lavapipe), screenshot it, and fail on a non-zero exit, a missing ready marker
# or a blank picture. Usage: ci-launch-smoke.sh <quill-binary> <out-dir> [kind...]
set -euo pipefail

bin=${1:?usage: ci-launch-smoke.sh <quill-binary> <out-dir> [kind...]}
out=${2:?usage: ci-launch-smoke.sh <quill-binary> <out-dir> [kind...]}
shift 2
kinds=("$@")
[ "${#kinds[@]}" -gt 0 ] || kinds=(ready-chats ready-chats-composer ready-chat-list-search)

mkdir -p "$out"
out=$(cd "$out" && pwd)
bin=$(cd "$(dirname "$bin")" && pwd)/$(basename "$bin")
here=$(cd "$(dirname "$0")" && pwd)

icd=${VK_ICD_FILENAMES:-}
if [ -z "$icd" ]; then
  for f in /usr/share/vulkan/icd.d/lvp_icd*.json; do
    if [ -f "$f" ]; then icd=$f; break; fi
  done
fi
[ -n "$icd" ] || { echo "no lavapipe ICD found (install mesa-vulkan-drivers)" >&2; exit 1; }
export VK_ICD_FILENAMES=$icd

display=:97
Xvfb "$display" -screen 0 1280x800x24 >"$out/xvfb.log" 2>&1 &
xvfb=$!
app=
cleanup() {
  if [ -n "$app" ]; then kill "$app" 2>/dev/null || true; fi
  kill "$xvfb" 2>/dev/null || true
}
trap cleanup EXIT
for _ in $(seq 1 50); do
  [ -S "/tmp/.X11-unix/X${display#:}" ] && break
  sleep 0.2
done

failed=0
for kind in "${kinds[@]}"; do
  echo "::group::launch smoke: $kind"
  rm -f "$out/.quill-ready-$kind"
  DISPLAY=$display QUILL_DEMO_LINGER_MS=8000 QUILL_REDUCED_MOTION=1 \
    QUILL_DEMO_WINDOW_SIZE=1200x740 \
    "$bin" --screenshot-demo "$kind" "$out" >"$out/$kind.log" 2>&1 &
  app=$!
  ready=0
  for _ in $(seq 1 180); do
    if [ -f "$out/.quill-ready-$kind" ]; then ready=1; break; fi
    kill -0 "$app" 2>/dev/null || break
    sleep 1
  done
  if [ "$ready" = 1 ]; then
    sleep 2
    DISPLAY=$display import -window root -depth 8 "ppm:$out/$kind.ppm" || true
  fi
  status=0
  wait "$app" || status=$?
  app=
  tail -n 20 "$out/$kind.log" || true
  if [ "$ready" != 1 ]; then
    echo "FAIL $kind: window never became ready (exit $status)"
    failed=1
  elif [ "$status" != 0 ]; then
    echo "FAIL $kind: exit code $status"
    failed=1
  elif [ ! -f "$out/$kind.ppm" ]; then
    echo "FAIL $kind: screen capture failed"
    failed=1
  elif ! python3 -I "$here/ci-image-check.py" "$out/$kind.ppm"; then
    echo "FAIL $kind: blank or flat picture"
    failed=1
  else
    echo "ok $kind"
  fi
  if [ -f "$out/$kind.ppm" ]; then
    convert "$out/$kind.ppm" "$out/$kind.png" && rm -f "$out/$kind.ppm"
  fi
  echo "::endgroup::"
done
exit "$failed"
