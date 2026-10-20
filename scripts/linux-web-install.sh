#!/usr/bin/env bash
# Install or update Quill on Linux from the latest GitHub release, for the
# current user and without root:
#   curl -fsSL https://raw.githubusercontent.com/shinycake/quill/main/scripts/linux-web-install.sh | bash
#
# Downloads the release tarball for this machine's architecture, checks it
# against the release's SHA256SUMS.txt, quits the copy it replaces if that
# copy is running, and runs the package's own install.sh: Quill goes to
# ~/.local/opt/quill, with ~/.local/bin/quill and a desktop entry and icons
# under ~/.local/share. Remove it with ~/.local/opt/quill/install.sh --uninstall.
#
# Environment:
#   QUILL_PREFIX     install prefix (default ~/.local)
#   QUILL_VERSION    release tag such as v2026.1017.2948 (default: latest)
#   QUILL_TAR_URL    tarball URL (curl syntax, file:// works); skips the
#                    checksum unless QUILL_SHA256 is set
#   QUILL_NO_OPEN=1  don't start Quill when done
set -euo pipefail

# Braces: piped into bash, nothing runs until the whole script has arrived.
{
REPO=shinycake/quill

say() { printf '%s\n' "$*"; }
die() { printf 'quill install: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = Linux ] || die "this installer is for Linux"
[ "$(id -u)" != 0 ] || [ -n "${QUILL_PREFIX:-}" ] \
  || die "run this as your own user, not root: Quill installs into your home folder"

case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) die "Quill has no build for $(uname -m) Linux" ;;
esac
# The release is built against glibc; musl systems (Alpine) can't load it.
getconf GNU_LIBC_VERSION >/dev/null 2>&1 \
  || die "Quill needs a glibc-based distribution (this system's C library isn't glibc)"
for tool in curl tar gzip; do
  command -v "$tool" >/dev/null || die "this installer needs $tool"
done
if command -v sha256sum >/dev/null; then
  sha256() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null; then
  sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
else
  die "this installer needs sha256sum (coreutils) to check the download"
fi

prefix="${QUILL_PREFIX:-$HOME/.local}"
mkdir -p "$prefix"
prefix="$(cd "$prefix" && pwd -P)"
dest="$prefix/opt/quill"
exe="$dest/quill"

name="quill-linux-$arch"
asset="$name-bundle.tar.gz"
if [ -n "${QUILL_TAR_URL:-}" ]; then
  tar_url="$QUILL_TAR_URL"
  sums_url=""
else
  if [ -n "${QUILL_VERSION:-}" ]; then
    base="https://github.com/$REPO/releases/download/$QUILL_VERSION"
  else
    base="https://github.com/$REPO/releases/latest/download"
  fi
  tar_url="$base/$asset"
  sums_url="$base/SHA256SUMS.txt"
fi

want="${QUILL_SHA256:-}"
if [ -z "$want" ] && [ -n "$sums_url" ]; then
  # Read before the big download, so a missing build fails fast.
  sums="$(curl -fsSL "$sums_url")" || die "couldn't fetch the release checksums"
  want="$(printf '%s\n' "$sums" | awk -v f="$asset" '$2 == f {print $1}')"
  [ -n "$want" ] || die "this release has no build for $arch Linux yet"
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

say "Downloading $tar_url"
curl -fL --progress-bar -o "$work/$asset" "$tar_url" || die "download failed"
if [ -n "$want" ]; then
  got="$(sha256 "$work/$asset")"
  [ "$got" = "$want" ] || die "checksum mismatch (expected $want, got $got)"
  say "Checksum OK"
fi

mkdir "$work/unpacked"
tar -C "$work/unpacked" -xzf "$work/$asset" || die "couldn't unpack the download"
pkg="$work/unpacked/$name"
[ -x "$pkg/quill" ] && [ -f "$pkg/install.sh" ] || die "the archive has no Quill package"

# Replace a running copy only after it has quit: it loads libraries from the
# package lazily, so swapping them under it could mix two versions. Only this
# install's copy counts; Quill runs one instance, so `--quit` reaches it.
running() {
  local link
  for link in /proc/[0-9]*/exe; do
    [ "$(readlink "$link" 2>/dev/null)" = "$exe" ] && return 0
  done
  return 1
}
if running; then
  say "Quitting Quill to update it"
  "$exe" --quit </dev/null >/dev/null 2>&1 || true
  for _ in $(seq 1 20); do running || break; sleep 0.5; done
  ! running || die "Quill is still running; quit it and run this again"
fi

bash "$pkg/install.sh" "$prefix" >/dev/null || die "the package's install.sh failed"
version="$("$exe" --version 2>/dev/null || true)"
say "Installed ${version:-Quill} to $dest"

# Libraries the package expects from the system (README.txt lists them):
# the ones it links, from ldd, and the ones it loads at runtime, from the
# loader cache. A check that can't run reports nothing rather than guessing.
missing=""
if command -v ldd >/dev/null; then
  missing="$(ldd "$exe" 2>/dev/null | awk '/not found/ {print $1}' | sort -u | tr '\n' ' ' || true)"
fi
cache="$( (ldconfig -p || /sbin/ldconfig -p) 2>/dev/null || true)"
if [ -n "$cache" ]; then
  for lib in libvulkan.so.1 libgtk-3.so.0; do
    # A here-string, not printf | grep -q: grep stops at the first match and
    # pipefail would report the writer's SIGPIPE as a missing library.
    grep -qF "$lib (" <<<"$cache" || missing="$missing$lib "
  done
fi
if [ -n "$missing" ]; then
  say "Quill needs these system libraries, which aren't installed: $missing"
  say "See $dest/README.txt for the packages that provide them."
fi
case ":$PATH:" in
  *":$prefix/bin:"*) ;;
  *) say "Open Quill from your applications menu, or add $prefix/bin to PATH to run 'quill'." ;;
esac

if [ "${QUILL_NO_OPEN:-}" != 1 ] && [ -z "$missing" ] \
  && { [ -n "${WAYLAND_DISPLAY:-}" ] || [ -n "${DISPLAY:-}" ]; }; then
  # Detached, so closing this terminal leaves Quill running.
  if command -v setsid >/dev/null; then
    setsid "$exe" </dev/null >/dev/null 2>&1 &
  else
    nohup "$exe" </dev/null >/dev/null 2>&1 &
  fi
fi
exit 0
}
