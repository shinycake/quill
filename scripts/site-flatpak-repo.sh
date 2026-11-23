#!/usr/bin/env bash
# Build Quill's own Flatpak repository into the site:
#
#   bash scripts/site-flatpak-repo.sh site/flatpak
#
# Imports the latest release's Flatpak bundles (quill-linux-<arch>.flatpak,
# attached by flatpak.yml) into a fresh OSTree repo signed with
# FLATPAK_GPG_KEY (an armored private key without a passphrase), and writes
# quill.flatpakref and quill.flatpakrepo beside it. The repo holds only the
# latest release; the commit inside each bundle is the same on every rebuild,
# so redeploying the site never looks like an update to installed copies.
#
# Without the key or the bundles it builds nothing and exits 0, so the site
# still deploys. Needs GH_TOKEN, flatpak and gpg.
set -euo pipefail

OUT="${1:?usage: site-flatpak-repo.sh OUT_DIR}"
REPO_SLUG="${GITHUB_REPOSITORY:-shinycake/quill}"
APP_ID=io.github.shinycake.Quill
SITE_URL="https://shinycake.github.io/quill/flatpak"

if [[ -z "${FLATPAK_GPG_KEY:-}" ]]; then
  echo "::notice::FLATPAK_GPG_KEY is not set; the site has no Flatpak repo"
  exit 0
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The release the page shows (site-release-data.py runs first): the newest
# verified one, whose bundles must match their signatures.
DATA_DIR="$(dirname "$OUT")/data"
if [[ -f "$DATA_DIR/release-tag.txt" ]]; then
  tag="$(cat "$DATA_DIR/release-tag.txt")"
  verify=true
else
  tag="$(gh api "repos/$REPO_SLUG/releases/latest" -q .tag_name)"
  verify=false
fi
gh release download "$tag" -R "$REPO_SLUG" -p 'quill-linux-*.flatpak' -p 'quill-linux-*.flatpak.sig' -D "$work/bundles" || true
shopt -s nullglob
bundles=("$work"/bundles/*.flatpak)
if [[ "$verify" == true ]]; then
  for bundle in "${bundles[@]}"; do
    python3 -I "$(dirname "$0")/release_verify.py" file "${tag#v}" "$bundle" \
      || { echo "::error::$(basename "$bundle") in $tag does not match its signature"; exit 1; }
  done
fi
if (( ${#bundles[@]} == 0 )); then
  echo "::notice::$tag has no Flatpak bundles; the site has no Flatpak repo"
  exit 0
fi

export GNUPGHOME="$work/gnupg"
mkdir -m 700 "$GNUPGHOME"
gpg --batch --quiet --import <<<"$FLATPAK_GPG_KEY"
key="$(gpg --batch --with-colons --list-secret-keys | awk -F: '$1 == "fpr" { print $10; exit }')"
[[ -n "$key" ]] || { echo "error: FLATPAK_GPG_KEY holds no secret key" >&2; exit 2; }
gpg_args=(--gpg-sign="$key" --gpg-homedir="$GNUPGHOME")

repo="$OUT/repo"
rm -rf "$repo"
mkdir -p "$OUT"
ostree init --mode=archive-z2 --repo="$repo"
for bundle in "${bundles[@]}"; do
  flatpak build-import-bundle "${gpg_args[@]}" "$repo" "$bundle"
done
flatpak build-update-repo "${gpg_args[@]}" --title="Quill" --default-branch=master \
  --prune "$repo"

pubkey="$(gpg --batch --export "$key" | base64 -w0)"
gpg --batch --export "$key" > "$OUT/quill.gpg"

cat > "$OUT/quill.flatpakref" <<EOF
[Flatpak Ref]
Name=$APP_ID
Branch=master
Title=Quill
Url=$SITE_URL/repo/
SuggestRemoteName=quill
RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
IsRuntime=false
GPGKey=$pubkey
EOF

cat > "$OUT/quill.flatpakrepo" <<EOF
[Flatpak Repo]
Title=Quill
Url=$SITE_URL/repo/
Homepage=https://shinycake.github.io/quill/
Comment=Quill, an open-source Telegram client
Icon=https://shinycake.github.io/quill/img/icon-256.png
GPGKey=$pubkey
EOF

# Read it back the way a client does: signed summary, signed commits.
check="$work/check"
FLATPAK_USER_DIR="$check" flatpak remote-add --user --gpg-import="$OUT/quill.gpg" check "file://$(realpath "$repo")"
FLATPAK_USER_DIR="$check" flatpak remote-ls --user check --columns=application,arch,version | tee "$work/ls.txt"
grep -q "$APP_ID" "$work/ls.txt" || { echo "error: $APP_ID is not in the repo" >&2; exit 2; }

echo "Flatpak repo for $tag: $(ostree refs --repo="$repo" | tr '\n' ' ')"
