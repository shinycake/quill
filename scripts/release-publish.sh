#!/usr/bin/env bash
# Publishes a draft release after checking that every file on it is exactly
# what the release workflow built (docs/decisions/release-secrets.md).
#
#   scripts/release-publish.sh v2026.1122.6914 [--check-only]
#
# Anyone with write access can replace a draft's files, and published
# releases are immutable, so a swap made before publishing would stay. The
# check: the draft's files must be exactly the workflow's `quill-release-<tag>`
# artifact plus the two Flatpak bundles and their signatures, from a
# release.yml run on main, each with the same SHA-256, each uploaded by
# github-actions; then every file's signature must verify. Run it as the owner:
# publishing creates the tag, which only admins may create.
set -euo pipefail
TAG=${1:?usage: release-publish.sh <tag> [--check-only]}
MODE=${2:-}
REPO=${REPO:-shinycake/quill}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
die() { echo "release-publish: $*" >&2; exit 1; }
sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1; }

[ "$(gh release view "$TAG" -R "$REPO" --json isDraft --jq .isDraft)" = true ] || die "$TAG is not a draft"

# The newest artifact of that name made by release.yml on main.
run=$(gh api "repos/$REPO/actions/artifacts?name=quill-release-$TAG&per_page=100" \
  --jq '[.artifacts[] | select(.expired | not) | select(.workflow_run.head_branch == "main")] | sort_by(.created_at) | last | .workflow_run.id // empty')
[ -n "$run" ] || die "no quill-release-$TAG artifact from a run on main"
path=$(gh api "repos/$REPO/actions/runs/$run" --jq '.path')
[ "$path" = .github/workflows/release.yml ] || die "run $run is $path, not release.yml"

gh run download "$run" -R "$REPO" -n "quill-release-$TAG" -D "$work/release"
for name in quill-flatpak-x86_64 quill-flatpak-aarch64 quill-flatpak-signatures; do
  gh run download "$run" -R "$REPO" -n "$name" -D "$work/release/release-out"
done

expected="$work/expected.txt"
for f in "$work"/release/release-out/*; do
  [ -f "$f" ] && printf '%s sha256:%s github-actions[bot]\n' "$(basename "$f")" "$(sha256 "$f")"
done | sort > "$expected"
id=$(gh api "repos/$REPO/releases" --paginate --jq ".[] | select(.tag_name == \"$TAG\") | .id" | head -n 1)
gh api "repos/$REPO/releases/$id/assets" --paginate \
  --jq '.[] | "\(.name) \(.digest) \(.uploader.login)"' | sort > "$work/actual.txt"

if ! diff -u "$expected" "$work/actual.txt"; then
  die "the draft's files differ from what run $run built (lines above: - built, + on the draft). Do not publish."
fi
echo "release-publish: $(wc -l < "$expected" | tr -d ' ') files match run $run"
# Drafts' files are not public, so check the signatures on the local copies.
for f in "$work"/release/release-out/*; do
  case "$f" in *.sig) continue ;; esac
  python3 -I "$(dirname "$0")/release_verify.py" file "${TAG#v}" "$f" || die "$(basename "$f") does not match its signature"
done

[ "$MODE" = --check-only ] && exit 0
gh release edit "$TAG" -R "$REPO" --draft=false
gh release view "$TAG" -R "$REPO" --json isDraft,isImmutable,url --jq '"published: draft=\(.isDraft) immutable=\(.isImmutable) \(.url)"' 2>/dev/null \
  || gh release view "$TAG" -R "$REPO" --json isDraft,url --jq '"published: draft=\(.isDraft) \(.url)"'
