#!/usr/bin/env bash
# Fail if any ELF file inside a Linux package directory needs a library that is
# neither bundled nor a standard system library, or carries an RPATH/RUNPATH
# that does not resolve inside the package. Linux analogue of
# scripts/check-bundle-macho.sh.
#
# Policy: OpenSSL (libssl/libcrypto) is bundled and therefore deliberately NOT in
# the system allowlist below; anything else the allowlist does not name must be
# present in the package. See docs/decisions/codex-linux-package.md.
#
# Usage: scripts/check-bundle-elf.sh [path/to/quill-linux-x86_64]
set -euo pipefail

PKG="${1:-dist/linux/quill-linux-x86_64}"
[[ -d "$PKG" ]] || { echo "error: $PKG is not a directory" >&2; exit 2; }
command -v readelf >/dev/null || { echo "error: readelf (binutils) is required" >&2; exit 2; }
PKG="$(cd "$PKG" && pwd)"

# Standard system libraries a desktop Linux install provides (glibc, the C++
# runtime, zlib, and the GPU/windowing/font/audio/GTK stack GPUI and tray-icon
# link). WebKitGTK 4.1 and libsoup 3 are what quill-webview (mini apps) links;
# they are a system package, not bundled (docs/decisions/codex-miniapp-webview.md).
# Matched against the NEEDED soname.
SYSTEM_RE='^(linux-vdso\.so\.1|ld-linux[^/]*\.so[.0-9]*|lib(c|m|dl|pthread|rt|util|resolv|anl|nsl|crypt)\.so\.[0-9]+|libstdc\+\+\.so\.6|libgcc_s\.so\.1|libz\.so\.1|libatomic\.so\.1|libX11\.so\.6|libX11-xcb\.so\.1|libX[a-z0-9]+\.so\.[0-9]+|libxcb[a-z0-9-]*\.so\.[0-9]+|libxkbcommon(-x11)?\.so\.0|libwayland-[a-z-]+\.so\.0|libvulkan\.so\.1|libEGL\.so\.1|libGL(ESv2|X)?\.so\.[0-9]+|libfontconfig\.so\.1|libfreetype\.so\.6|libasound\.so\.2|libdbus-1\.so\.3|libgtk-3\.so\.0|libgdk-3\.so\.0|libgdk_pixbuf-2\.0\.so\.0|libglib-2\.0\.so\.0|libgobject-2\.0\.so\.0|libgio-2\.0\.so\.0|libgmodule-2\.0\.so\.0|libcairo(-gobject)?\.so\.2|libpango(cairo)?-1\.0\.so\.0|libatk-1\.0\.so\.0|libatk-bridge-2\.0\.so\.0|libharfbuzz\.so\.0|libayatana-appindicator3\.so\.1|libappindicator3\.so\.1|libwebkit2gtk-4\.1\.so\.0|libjavascriptcoregtk-4\.1\.so\.0|libsoup-3\.0\.so\.0)$'

bad=0
checked=0
fail() { echo "BAD  $1: $2" >&2; bad=$((bad + 1)); }

is_elf() { [[ "$(head -c4 "$1" 2>/dev/null | od -An -tx1 | tr -d ' \n')" == 7f454c46 ]]; }

while IFS= read -r -d '' f; do
  is_elf "$f" || continue
  checked=$((checked + 1))
  rel="${f#"$PKG"/}"
  dir="$(dirname "$f")"
  dyn="$(readelf -d "$f" 2>/dev/null || true)"

  # Directories this file's loader will search inside the package.
  search=("$dir")
  while IFS= read -r rp; do
    [[ -n "$rp" ]] || continue
    IFS=: read -ra parts <<<"$rp"
    for part in "${parts[@]}"; do
      sub="$(sed -E 's#^\$(\{ORIGIN\}|ORIGIN)/?##' <<<"$part")"
      if [[ "$part" == "$sub" ]]; then
        fail "$rel" "RUNPATH/RPATH entry '$part' is not \$ORIGIN-relative"
        continue
      fi
      resolved="$dir/$sub"
      if [[ -d "$resolved" ]] && [[ "$(cd "$resolved" && pwd)/" == "$PKG"/* ]]; then
        search+=("$resolved")
      else
        fail "$rel" "RUNPATH entry '$part' does not resolve to a directory inside the package"
      fi
    done
  done < <(sed -n -E 's/.*\((RUNPATH|RPATH)\).*\[(.*)\]$/\2/p' <<<"$dyn")

  while IFS= read -r need; do
    [[ -n "$need" ]] || continue
    found=0
    for d in "${search[@]}"; do
      if [[ -e "$d/$need" ]]; then found=1; break; fi
    done
    if (( found )); then continue; fi
    if [[ "$need" =~ $SYSTEM_RE ]]; then continue; fi
    fail "$rel" "needs $need, which is neither bundled nor an allowed system library"
  done < <(sed -n -E 's/.*\(NEEDED\).*\[(.*)\]$/\1/p' <<<"$dyn")
done < <(find "$PKG" -type f ! -type l -print0)

# Where the host supports it, also let the real loader confirm that nothing is
# unresolved with LD_LIBRARY_PATH cleared.
if [[ "$(uname -s)" == Linux ]] && command -v ldd >/dev/null; then
  while IFS= read -r -d '' f; do
    is_elf "$f" || continue
    out="$(env -u LD_LIBRARY_PATH ldd "$f" 2>&1 || true)"
    while IFS= read -r line; do
      [[ -n "$line" ]] && fail "${f#"$PKG"/}" "ldd: ${line#"${line%%[![:space:]]*}"}"
    done < <(grep 'not found' <<<"$out" || true)
  done < <(find "$PKG" -type f ! -type l -print0)
fi

if (( checked == 0 )); then
  echo "FAIL: no ELF files found in $PKG" >&2
  exit 1
fi
if (( bad )); then
  echo "FAIL: $bad bad reference(s) in $checked ELF file(s) of $PKG" >&2
  exit 1
fi
echo "OK: $checked ELF file(s) in $PKG depend only on the system and the package"
