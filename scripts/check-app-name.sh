#!/usr/bin/env bash
# Refer to the app as Quill; use Telegram only for the service.
#
# Fails when a non-comment line in src/ or crates/ (.rs) uses wording that
# calls this app "Telegram" / "Telegram Desktop". Legitimate uses (naming the
# official app, matching another app's name, the service doing something) go
# in scripts/app-name-allowlist.txt as `path|text`: a hit is ignored when its
# file path ends with `path` and the line contains `text`.
#
# Usage: scripts/check-app-name.sh [-a allowlist] [dir ...]   (default: src crates)
# Run from the repository root (CI and the PR gate do).
set -u
allow=scripts/app-name-allowlist.txt
if [ "${1:-}" = "-a" ]; then
  allow=$2
  shift 2
fi
[ $# -gt 0 ] || set -- src crates

PHRASES=(
  "Telegram Desktop"
  "quit Telegram"
  "restart Telegram"
  "close Telegram"
  "exit Telegram"
  "Telegram is running"
  "Telegram is minimized"
  "Telegram will"
  "Telegram needs access"
  "Telegram couldn't open"
  "Run Telegram in the background"
  "Telegram's settings"
)

fail=0
for phrase in "${PHRASES[@]}"; do
  while IFS= read -r hit; do
    [ -n "$hit" ] || continue
    file=${hit%%:*}
    rest=${hit#*:}
    lineno=${rest%%:*}
    text=${rest#*:}
    # Skip comment-only lines (//, ///, //!, and block-comment continuations).
    trimmed=${text#"${text%%[![:space:]]*}"}
    case $trimmed in
      //* | \** | /\**) continue ;;
    esac
    if [ -f "$allow" ]; then
      allowed=0
      while IFS='|' read -r apath atext; do
        case $apath in
          '' | \#*) continue ;;
        esac
        case $file in
          *"$apath") ;;
          *) continue ;;
        esac
        case $text in
          *"$atext"*)
            allowed=1
            break
            ;;
        esac
      done <"$allow"
      [ $allowed = 1 ] && continue
    fi
    echo "$file:$lineno: \"$phrase\" -> $trimmed"
    fail=1
  done < <(grep -rnF --include='*.rs' -- "$phrase" "$@" 2>/dev/null)
done

if [ $fail = 1 ]; then
  echo
  echo "Refer to the app as Quill; use Telegram only for the service."
  echo "If a hit really means Telegram (the service, another app), add 'path|text' to $allow."
  exit 1
fi
echo "check-app-name: OK"
