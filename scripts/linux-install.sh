#!/usr/bin/env bash
# Install this Quill package for the current user (no root needed).
#   ./install.sh [prefix]     default prefix: ~/.local
# Copies the package to <prefix>/opt/quill, links <prefix>/bin/quill, and
# installs the desktop entry and icon. Remove with: ./install.sh --uninstall [prefix]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODE=install
if [[ "${1:-}" == --uninstall ]]; then MODE=uninstall; shift; fi
PREFIX="${1:-$HOME/.local}"
DEST="$PREFIX/opt/quill"
APPS="$PREFIX/share/applications"
ICONS="$PREFIX/share/icons/hicolor"

if [[ "$MODE" == uninstall ]]; then
  rm -rf "$DEST"
  rm -f "$PREFIX/bin/quill" "$APPS/quill.desktop" "$ICONS"/*/apps/quill.png
  echo "Removed Quill from $PREFIX (account data under ~/.local/share is untouched)."
  exit 0
fi

mkdir -p "$PREFIX/opt" "$PREFIX/bin" "$APPS" "$ICONS"
rm -rf "$DEST.new"
cp -a "$HERE" "$DEST.new"
rm -rf "$DEST"
mv "$DEST.new" "$DEST"
ln -sf "$DEST/quill" "$PREFIX/bin/quill"
sed "s|^Exec=.*|Exec=$DEST/quill %u|" "$HERE/share/applications/quill.desktop" > "$APPS/quill.desktop"
for icon in "$HERE"/share/icons/hicolor/*/apps/quill.png; do
  size="$(basename "$(dirname "$(dirname "$icon")")")"
  mkdir -p "$ICONS/$size/apps"
  cp "$icon" "$ICONS/$size/apps/quill.png"
done
command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$PREFIX/share/icons/hicolor" || true
echo "Installed Quill to $DEST (command: $PREFIX/bin/quill)."
