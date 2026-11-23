#!/usr/bin/env bash
# Captures the _XSETTINGS_SETTINGS property that real XSETTINGS managers
# publish, for xsettings_scale.rs's tests. Each capture runs the manager on
# its own Xvfb with a 1920x1080 core screen at the given DPI.
#
# Needs: Xvfb, dbus-launch, xfsettingsd + xfconf-query (xfce4-settings),
# xsettingsd, python3-xlib. Run from this directory:
#   bash capture.sh
set -euo pipefail

display=90

dump() {
  python3 -I -c '
import sys
from Xlib import X, display
d = display.Display()
owner = d.get_selection_owner(d.intern_atom("_XSETTINGS_S0", only_if_exists=True))
prop = owner.get_full_property(d.intern_atom("_XSETTINGS_SETTINGS"), X.AnyPropertyType)
sys.stdout.buffer.write(bytes(prop.value))
'
}

# xfsettingsd <out> <core dpi> <window scale> [Xft/DPI]
# Without an Xft/DPI argument the setting keeps Xfce's shipped default
# (empty). -1 is what the Appearance dialog stores when Custom DPI is off.
xfce() {
  local out=$1 dpi=$2 scale=$3 xft_dpi=${4:-}
  display=$((display + 1))
  (
    export HOME DISPLAY=":$display"
    HOME=$(mktemp -d)
    Xvfb "$DISPLAY" -screen 0 1920x1080x24 -dpi "$dpi" >/dev/null 2>&1 &
    xvfb=$!
    trap 'kill $xvfb ${DBUS_SESSION_BUS_PID:-} ${manager:-} 2>/dev/null; rm -rf "$HOME"' EXIT
    sleep 1
    eval "$(dbus-launch --sh-syntax)"
    xfconf-query -c xsettings -p /Gdk/WindowScalingFactor -s "$scale"
    if [ -n "$xft_dpi" ]; then
      xfconf-query -c xsettings -p /Xft/DPI -n -t int -s "$xft_dpi"
    fi
    xfsettingsd --no-daemon >/dev/null 2>&1 &
    manager=$!
    sleep 3
    dump >"$out"
  )
}

# xsettingsd <out> <config line>...
xsettingsd_() {
  local out=$1
  shift
  display=$((display + 1))
  (
    export DISPLAY=":$display"
    config=$(mktemp)
    printf '%s\n' "$@" >"$config"
    Xvfb "$DISPLAY" -screen 0 1920x1080x24 -dpi 96 >/dev/null 2>&1 &
    xvfb=$!
    trap 'kill $xvfb ${manager:-} 2>/dev/null; rm -f "$config"' EXIT
    sleep 1
    xsettingsd -c "$config" >/dev/null 2>&1 &
    manager=$!
    sleep 1
    dump >"$out"
  )
}

xfce xfce-defaults.bin 96 1
xfce xfce-measured-165dpi.bin 166 1 -1
xfce xfce-2x-measured-281dpi.bin 282 2 -1
xsettingsd_ xsettingsd-theme-only.bin 'Net/ThemeName "Adwaita"'
xsettingsd_ xsettingsd-dpi-144.bin 'Net/ThemeName "Adwaita"' 'Xft/DPI 147456'
