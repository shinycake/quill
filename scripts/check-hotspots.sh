#!/usr/bin/env bash
# Keep shared files out of feature PRs (docs/contributing/structure.md).
#
# scripts/hotspots.txt lists the files most PRs used to edit and the change
# a normal PR may still make to each one:
#
#   <path> | <rule> | <what a normal PR may change>
#
# Rules: `none` (no change), `mod-lines` (only add or remove `mod x;`, `use`/`pub use` lines /
# `pub mod x;` lines), `max:N` (at most N added plus removed lines),
# `re:<regex>` (every added or removed line matches the extended regex).
#
# The diff runs from the merge base with BASE (default origin/main) to HEAD.
# A PR that needs a bigger change says so with the `hotspot-ok` label (read
# from $GITHUB_EVENT_PATH in CI, or from --labels) or a `Hotspot-change:`
# trailer on one of its commits; the check then only reports.
#
# It also warns, without failing, when a changed .rs file gains a `match`
# with more than 30 arms or a `pub enum` with more than 40 variants (new, or
# grown past the threshold).
#
# Usage: scripts/check-hotspots.sh [-f hotspots.txt] [--labels a,b] [BASE]
# Run from the repository root. Works with bash 3.2 (macOS).
set -u
list=scripts/hotspots.txt
labels=""
while [ $# -gt 0 ]; do
  case $1 in
    -f) list=$2; shift 2 ;;
    --labels) labels=$2; shift 2 ;;
    *) break ;;
  esac
done
base=${1:-origin/main}
MATCH_ARMS=30
ENUM_VARIANTS=40

mb=$(git merge-base "$base" HEAD) || { echo "check-hotspots: no merge base with $base"; exit 2; }

if [ -z "$labels" ] && [ -n "${GITHUB_EVENT_PATH:-}" ] && [ -f "$GITHUB_EVENT_PATH" ]; then
  labels=$(python3 -c 'import json,sys; e=json.load(open(sys.argv[1])); print(",".join(l["name"] for l in (e.get("pull_request") or {}).get("labels", [])))' "$GITHUB_EVENT_PATH" 2>/dev/null || true)
fi
override=""
case ",$labels," in *,hotspot-ok,*) override="label hotspot-ok" ;; esac
trailer=$(git log --format='%(trailers:key=Hotspot-change,valueonly)' "$mb..HEAD" | sed '/^[[:space:]]*$/d' | head -1)
[ -n "$trailer" ] && override="Hotspot-change: $trailer"

fail=0
while IFS= read -r entry; do
  case $entry in ''|'#'*) continue ;; esac
  path=$(printf '%s' "$entry" | awk -F'|' '{gsub(/^[ \t]+|[ \t]+$/, "", $1); print $1}')
  rule=$(printf '%s' "$entry" | awk -F'|' '{gsub(/^[ \t]+|[ \t]+$/, "", $2); print $2}')
  what=$(printf '%s' "$entry" | awk -F'|' '{s=$3; for (i=4;i<=NF;i++) s=s "|" $i; gsub(/^[ \t]+|[ \t]+$/, "", s); print s}')
  changed=$(git diff -U0 --no-color "$mb" HEAD -- "$path" | grep -E '^[+-]' | grep -vE '^(\+\+\+|---) ')
  [ -n "$changed" ] || continue
  n=$(printf '%s\n' "$changed" | wc -l | tr -d ' ')
  bad=""
  case $rule in
    none) bad=$changed ;;
    mod-lines)
      bad=$(printf '%s\n' "$changed" | grep -vE '^[+-][[:space:]]*(((pub(\([a-z]+\))? )?mod [A-Za-z0-9_]+;)|((pub(\([a-z]+\))? )?use [^;]*;))?[[:space:]]*$') ;;
    max:*)
      [ "$n" -gt "${rule#max:}" ] && bad="$n changed lines (allowed ${rule#max:})" ;;
    re:*)
      bad=$(printf '%s\n' "$changed" | grep -vE "^[+-](${rule#re:})$") ;;
    *) echo "check-hotspots: unknown rule '$rule' for $path"; exit 2 ;;
  esac
  if [ -n "$bad" ]; then
    echo "HOTSPOT $path: a normal PR may only: $what"
    printf '%s\n' "$bad" | head -5 | sed 's/^/    /'
    fail=1
  fi
done <"$list"

# Big matches and enums: "<kind> <count> <line> <key>" per block over the limit.
blocks() {
  awk -v arms="$MATCH_ARMS" -v variants="$ENUM_VARIANTS" '
    function ind(s) { match(s, /^ */); return RLENGTH }
    {
      l = $0; i = ind(l)
      for (k = top; k >= 1; k--) {
        if (i == bi[k] && substr(l, i + 1, 1) == "}") {
          lim = (bk[k] == "match") ? arms : variants
          if (bc[k] > lim) {
            key = bh[k]; gsub(/[ \t]+/, " ", key); seen[key]++
            print bk[k], bc[k], bl[k], key "#" seen[key]
          }
          top = k - 1
          break
        }
      }
      t = substr(l, i + 1)
      if (t ~ /^\/\//) next
      for (k = 1; k <= top; k++) {
        if (i != bi[k] + 4) continue
        if (bk[k] == "match" && t ~ /=>/ && t !~ /^\|/) bc[k]++
        if (bk[k] == "enum" && t ~ /^[A-Z][A-Za-z0-9_]*([ ,({]|$)/) bc[k]++
      }
      if (t ~ /(^|[^A-Za-z0-9_])match .*\{$/) {
        top++; bk[top] = "match"; bi[top] = i; bc[top] = 0; bl[top] = NR; bh[top] = t
      } else if (t ~ /^pub(\([a-z:]+\))? enum [A-Za-z0-9_]+.*\{$/) {
        top++; bk[top] = "enum"; bi[top] = i; bc[top] = 0; bl[top] = NR
        name = t; sub(/^pub(\([a-z:]+\))? enum /, "", name); sub(/[^A-Za-z0-9_].*$/, "", name)
        bh[top] = "enum " name
      }
    }'
}

for f in $(git diff --name-only --diff-filter=AM "$mb" HEAD -- '*.rs'); do
  [ -f "$f" ] || continue
  head_blocks=$(blocks <"$f")
  [ -n "$head_blocks" ] || continue
  base_blocks=$(git show "$mb:$f" 2>/dev/null | blocks)
  printf '%s\n' "$head_blocks" | while read -r kind count line key; do
    old=$(printf '%s\n' "$base_blocks" | awk -v k="$key" '{ c = $1; n = $2; $1 = $2 = $3 = ""; sub(/^ +/, ""); if ($0 == k) print n }')
    if [ -z "$old" ] || [ "$count" -gt "$old" ]; then
      if [ "$kind" = match ]; then
        msg="match with $count arms (over $MATCH_ARMS): give each feature its own handler instead of growing a central match"
      else
        msg="pub enum with $count variants (over $ENUM_VARIANTS): split it by domain"
      fi
      echo "warning: $f:$line: $msg"
      [ -n "${GITHUB_ACTIONS:-}" ] && echo "::warning file=$f,line=$line::$msg"
    fi
  done
done

if [ "$fail" = 1 ] && [ -n "$override" ]; then
  echo "Hotspot changes allowed by $override."
  fail=0
elif [ "$fail" = 1 ]; then
  echo "Move the change into the feature's own module (docs/contributing/structure.md),"
  echo "or, if the shared file really has to change, add the hotspot-ok label or a"
  echo "'Hotspot-change: <reason>' commit trailer."
else
  echo "hotspots OK"
fi
exit $fail
