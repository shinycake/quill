#!/usr/bin/env bash
# Fail if any Mach-O inside an app bundle depends on something that is not part
# of the OS or the bundle itself (e.g. /opt/homebrew/..., /usr/local/...).
#
# Allowed references: /usr/lib/*, /System/Library/*, and @rpath/@loader_path/
# @executable_path names that resolve to a file inside the bundle. Install ids
# and LC_RPATH entries are held to the same rule. With MACOSX_DEPLOYMENT_TARGET
# set, as in CI, every file must also load on that macOS: a library built for a
# newer one refuses to load there.
#
# Usage: scripts/check-bundle-macho.sh [path/to/Quill.app]
set -euo pipefail

APP="${1:-dist/Quill.app}"
[[ -d "$APP/Contents" ]] || { echo "error: $APP is not an app bundle" >&2; exit 2; }
APP="$(cd "$APP" && pwd)"
FW="$APP/Contents/Frameworks"
EXE_DIR="$APP/Contents/MacOS"

bad=0
checked=0
fail() { echo "BAD  $1: $2" >&2; bad=$((bad + 1)); }

is_system() { [[ "$1" == /usr/lib/* || "$1" == /System/Library/* ]]; }

# Accept reference $3 of file $1 (in dir $2) if it is system or resolves inside the bundle.
check_ref() {
  local file="$1" dir="$2" ref="$3" what="$4" cand
  if is_system "$ref"; then return 0; fi
  case "$ref" in
    @rpath/*)
      cand="${ref#@rpath/}"
      if [[ -e "$FW/$cand" || -e "$dir/$cand" || -e "$EXE_DIR/$cand" ]]; then return 0; fi ;;
    @loader_path/*)     if [[ -e "$dir/${ref#@loader_path/}" ]]; then return 0; fi ;;
    @executable_path/*) if [[ -e "$EXE_DIR/${ref#@executable_path/}" ]]; then return 0; fi ;;
  esac
  fail "${file#"$APP"/}" "$what $ref"
}

while IFS= read -r -d '' f; do
  if ! file -b "$f" | grep -q 'Mach-O'; then continue; fi
  checked=$((checked + 1))
  dir="$(dirname "$f")"
  id="$(otool -D "$f" 2>/dev/null | sed -n '2p')"
  if [[ -n "$id" ]] && ! is_system "$id" && [[ "$id" != @rpath/* ]]; then
    fail "${f#"$APP"/}" "install id $id (must be @rpath/...)"
  fi
  while IFS= read -r dep; do
    # Dylibs list their own id first; it was validated above.
    if [[ -n "$id" && "$dep" == "$id" ]]; then continue; fi
    check_ref "$f" "$dir" "$dep" "links"
  done < <(otool -L "$f" | tail -n +2 | sed -E 's/^[[:space:]]+//; s/ \(compatibility version.*$//')
  while IFS= read -r rp; do
    case "$rp" in
      @loader_path*|@executable_path*) ;;
      *) if ! is_system "$rp"; then fail "${f#"$APP"/}" "LC_RPATH $rp"; fi ;;
    esac
  done < <(otool -l "$f" | awk '/cmd LC_RPATH/{r=1} r&&/ path /{print $2; r=0}')
  if [[ -n "${MACOSX_DEPLOYMENT_TARGET:-}" ]]; then
    minos="$(otool -l "$f" | awk '/cmd LC_BUILD_VERSION/{r=1} r&&/ minos /{print $2; exit}')"
    newest="$(printf '%s\n%s\n' "$minos" "$MACOSX_DEPLOYMENT_TARGET" | sort -V | tail -n 1)"
    if [[ -n "$minos" && "$newest" == "$minos" && "$minos" != "$MACOSX_DEPLOYMENT_TARGET" ]]; then
      fail "${f#"$APP"/}" "needs macOS $minos, newer than $MACOSX_DEPLOYMENT_TARGET"
    fi
  fi
done < <(find "$APP" -type f ! -type l -print0)

if (( bad )); then
  echo "FAIL: $bad bad reference(s) in $checked Mach-O file(s) of $APP" >&2
  exit 1
fi
echo "OK: $checked Mach-O file(s) in $APP depend only on the OS and the bundle"
