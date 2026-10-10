#!/usr/bin/env bash
# Keep Rust files small (docs/contributing/structure.md).
#
# Fails when a .rs file under src/ or crates/ has more than LIMIT lines
# (default 1000). Not counted: test-only files (under a tests/ directory,
# or named tests.rs, *_tests.rs or *_tests_*.rs) and the #[cfg(test)] mod
# blocks at the end of a file.
#
# Files that were already over the limit are listed in the baseline
# (scripts/file-size-baseline.txt, "<lines> <path>"). A baselined file may
# shrink but not grow, and must leave the baseline once it is under the
# limit. --update lowers baseline entries to the current counts and drops
# the ones now under the limit; it never adds or raises an entry.
#
# Usage: scripts/check-file-size.sh [-l limit] [-b baseline] [--update] [dir ...]
# Run from the repository root (CI does). Works with bash 3.2 (macOS).
set -u
limit=1000
baseline=scripts/file-size-baseline.txt
update=0
while [ $# -gt 0 ]; do
  case $1 in
    -l) limit=$2; shift 2 ;;
    -b) baseline=$2; shift 2 ;;
    --update) update=1; shift ;;
    *) break ;;
  esac
done
[ $# -gt 0 ] || set -- src crates

# Counted lines of one file: everything before a trailing run of
# #[cfg(test)] modules, without the blank lines in front of it.
count_lines() {
  awk '
    { line[NR] = $0 }
    function trailing(i,   j, l, test) {
      j = i; test = 0
      while (j <= NR) {
        l = line[j]
        if (l ~ /^[ \t]*$/ || l ~ /^\/\//) { j++; continue }
        if (l ~ /^#\[/) { if (l ~ /cfg\((all\()?test/) test = 1; j++; continue }
        if (!test) return 0
        if (l ~ /^(pub(\([a-z]+\))? )?mod [A-Za-z0-9_]+;$/) { test = 0; j++; continue }
        if (l ~ /^(pub(\([a-z]+\))? )?mod [A-Za-z0-9_]+ \{$/) {
          j++
          while (j <= NR && line[j] != "}") j++
          if (j > NR) return 0
          test = 0; j++; continue
        }
        return 0
      }
      return 1
    }
    END {
      cut = NR
      for (i = 1; i <= NR; i++)
        if (line[i] ~ /^#\[cfg\((all\()?test/ && trailing(i)) { cut = i - 1; break }
      while (cut > 0 && line[cut] ~ /^[ \t]*$/) cut--
      print cut
    }' "$1"
}

counts=$(mktemp)
trap 'rm -f "$counts"' EXIT
for root in "$@"; do
  root=${root%/}
  find "$root" -name '*.rs' -type f -not -path '*/target/*' | sort | while IFS= read -r f; do
    # Test-only files, judged by the path below the scanned directory.
    case "/${f#"$root"/}" in */tests/*|*/tests.rs|*_tests.rs|*_tests_*.rs) continue ;; esac
    echo "$(count_lines "$f") ${f#./}"
  done
done >"$counts"

base_in=$baseline
[ -f "$baseline" ] || base_in=/dev/null

# Compare. Output lines: "FAIL ...", "NOTE ...", or "KEEP <n> <path>" for
# the --update rewrite.
report=$(awk -v limit="$limit" -v update="$update" -v bfile="$baseline" '
  FILENAME == ARGV[1] {
    if ($0 ~ /^[ \t]*(#|$)/) next
    base[$2] = $1; next
  }
  {
    n = $1; f = $2; seen[f] = 1
    if (f in base) {
      if (n + 0 > base[f] + 0)
        print "FAIL GREW " f ": " n " lines, baseline " base[f] ". Baselined files may only shrink; move code out instead."
      else if (n + 0 <= limit + 0) {
        if (update) next
        print "FAIL UNDER " f ": " n " lines, now under " limit ". Remove it from " bfile "."
      } else {
        if (n + 0 < base[f] + 0 && !update)
          print "NOTE " f " shrank to " n " (baseline " base[f] "); --update lowers the entry."
        print "KEEP " n " " f
      }
    } else if (n + 0 > limit + 0)
      print "FAIL OVER " f ": " n " lines (limit " limit "). Split it before it crosses the limit."
  }
  END {
    for (f in base) if (!(f in seen) && !update)
      print "FAIL STALE " f ": listed in " bfile " but not found. Remove the entry."
  }' "$base_in" "$counts")

fail=0
if printf '%s\n' "$report" | grep -q '^FAIL '; then fail=1; fi
printf '%s\n' "$report" | sed -n 's/^FAIL //p; s/^NOTE /note: /p'

if [ "$update" = 1 ] && [ "$fail" = 0 ]; then
  {
    [ -f "$baseline" ] && grep '^#' "$baseline"
    printf '%s\n' "$report" | sed -n 's/^KEEP //p' | sort -k2
  } >"$baseline.tmp"
  mv "$baseline.tmp" "$baseline"
  echo "updated $baseline"
fi

if [ "$fail" = 0 ]; then
  echo "file sizes OK (limit $limit)"
else
  echo "See docs/contributing/structure.md."
fi
exit $fail
