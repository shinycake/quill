#!/bin/bash
# Install or update Quill on macOS from the latest GitHub release:
#   curl -fsSL https://raw.githubusercontent.com/shinycake/quill/main/scripts/macos-install.sh | bash
#
# Quill has no Apple Developer ID, so the app is only ad-hoc signed and not
# notarized. Gatekeeper checks only files that carry the com.apple.quarantine
# attribute, which browsers add to downloads and curl does not. Downloading
# here with curl, and clearing the attribute in case something added it, lets
# Quill open without the "move to Bin" warning. The archive is checked against
# the release's SHA-256 and its code signature is verified before install.
#
# Environment:
#   QUILL_INSTALL_DIR  folder to install into (default /Applications, else ~/Applications)
#   QUILL_VERSION      release tag such as v2026.1017.2948 (default: latest)
#   QUILL_ZIP_URL      archive URL (curl syntax, file:// works); skips the checksum
#                      unless QUILL_SHA256 is set
#   QUILL_NO_OPEN=1    don't open Quill when done
set -euo pipefail

# Braces: piped into bash, nothing runs until the whole script has arrived.
{
REPO=shinycake/quill
ASSET=quill-macos-aarch64.zip

say() { printf '%s\n' "$*"; }
die() { printf 'quill install: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = Darwin ] || die "this installer is for macOS"
[ "$(uname -m)" = arm64 ] || die "Quill needs a Mac with Apple Silicon"
major="$(sw_vers -productVersion | cut -d. -f1)"
[ "$major" -ge 14 ] || die "Quill needs macOS 14 or later (this Mac has $(sw_vers -productVersion))"

if [ -n "${QUILL_INSTALL_DIR:-}" ]; then
  dest_dir="$QUILL_INSTALL_DIR"
elif [ -w /Applications ]; then
  dest_dir=/Applications
else
  dest_dir="$HOME/Applications"
fi
mkdir -p "$dest_dir"
# Resolved, so /var and /private/var name the same app when matching processes.
dest_dir="$(cd "$dest_dir" && pwd -P)"
app="$dest_dir/Quill.app"

if [ -n "${QUILL_ZIP_URL:-}" ]; then
  zip_url="$QUILL_ZIP_URL"
  sums_url=""
else
  if [ -n "${QUILL_VERSION:-}" ]; then
    base="https://github.com/$REPO/releases/download/$QUILL_VERSION"
  else
    base="https://github.com/$REPO/releases/latest/download"
  fi
  zip_url="$base/$ASSET"
  sums_url="$base/SHA256SUMS.txt"
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

say "Downloading $zip_url"
curl -fL --progress-bar -o "$work/$ASSET" "$zip_url" || die "download failed"

want="${QUILL_SHA256:-}"
if [ -z "$want" ] && [ -n "$sums_url" ]; then
  sums="$(curl -fsSL "$sums_url")" || die "couldn't fetch the release checksums"
  want="$(printf '%s\n' "$sums" | awk -v f="$ASSET" '$2 == f {print $1}')"
  [ -n "$want" ] || die "the release checksums don't list $ASSET"
fi
if [ -n "$want" ]; then
  got="$(shasum -a 256 "$work/$ASSET" | awk '{print $1}')"
  [ "$got" = "$want" ] || die "checksum mismatch (expected $want, got $got)"
  say "Checksum OK"
fi

ditto -x -k "$work/$ASSET" "$work/unpacked"
new="$work/unpacked/Quill.app"
[ -d "$new" ] || die "the archive has no Quill.app"
codesign --verify --deep --strict "$new" || die "Quill.app's code signature is invalid"
xattr -dr com.apple.quarantine "$new" 2>/dev/null || true

# Replace a running copy only after it has quit, so its files aren't swapped
# out from under it. Only the copy being replaced: Quill runs one instance, so
# when this one runs, `--quit` reaches it; any other copy is left alone.
exe="$app/Contents/MacOS/quill"
# Fixed-string match on the command line: the path may hold regex characters.
running() { ps -axo command= | awk -v e="$exe" '$0 == e || index($0, e " ") == 1 { found = 1 } END { exit !found }'; }
if running; then
  say "Quitting Quill to update it"
  "$exe" --quit </dev/null >/dev/null 2>&1 || true
  for _ in $(seq 1 20); do running || break; sleep 0.5; done
  ! running || die "Quill is still running; quit it and run this again"
fi

# Stage next to the destination, then swap with renames on one volume so an
# interrupted install never leaves a half-deleted Quill.app.
staged="$dest_dir/.Quill.app.installing"
old="$dest_dir/.Quill.app.old"
rm -rf "$staged" "$old"
ditto "$new" "$staged"
if [ -e "$app" ]; then mv "$app" "$old"; fi
mv "$staged" "$app"
rm -rf "$old"
xattr -dr com.apple.quarantine "$app" 2>/dev/null || true

version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist" 2>/dev/null || echo unknown)"
say "Installed Quill $version to $app"
if [ "${QUILL_NO_OPEN:-}" != 1 ]; then
  open "$app"
fi
exit 0
}
