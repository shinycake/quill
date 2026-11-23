#!/usr/bin/env bash
# Signs the in-app update packages in <dir> for <version>: writes
# <package>.sig beside each one (one line of base64 Ed25519 signature) and
# checks it against the public key Quill verifies with
# (src/update_install/signature.rs, docs/decisions/release-secrets.md).
#
# The signed text is "quill-update-v1\n<version>\n<file name>\n<sha256>\n".
#
#   UPDATE_SIGNING_KEY=<PEM private key> scripts/release-sign-updates.sh <dir> <version> [package ...]
#
# Without package names it signs the four packages the updater downloads.
# UPDATE_PUBLIC_KEY overrides the expected public key (tests only).
set -euo pipefail
dir=${1:?usage: release-sign-updates.sh <dir> <version> [package ...]}
version=${2:?usage: release-sign-updates.sh <dir> <version> [package ...]}
shift 2
[ $# -gt 0 ] || set -- quill-macos-aarch64.zip quill-linux-x86_64-bundle.tar.gz \
  quill-linux-aarch64-bundle.tar.gz quill-windows-x86_64.zip
: "${UPDATE_SIGNING_KEY:?UPDATE_SIGNING_KEY is not set}"
expected=${UPDATE_PUBLIC_KEY:-$(sed -n 's/^const RELEASE_KEY: &str = "\(.*\)";$/\1/p' "$(dirname "$0")/../src/update_install/signature.rs")}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
umask 077
printf '%s\n' "$UPDATE_SIGNING_KEY" > "$work/key.pem"
openssl pkey -in "$work/key.pem" -pubout -out "$work/pub.pem"
actual=$(openssl pkey -pubin -in "$work/pub.pem" -outform DER | tail -c 32 | base64 | tr -d '\n')
if [ "$actual" != "$expected" ]; then
  echo "release-sign-updates: the signing key does not match the public key in signature.rs" >&2
  exit 1
fi

sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1; }
for name in "$@"; do
  file="$dir/$name"
  [ -f "$file" ] || { echo "release-sign-updates: missing $file" >&2; exit 1; }
  printf 'quill-update-v1\n%s\n%s\n%s\n' "$version" "$name" "$(sha256 "$file")" > "$work/msg"
  openssl pkeyutl -sign -inkey "$work/key.pem" -rawin -in "$work/msg" -out "$work/sig"
  openssl pkeyutl -verify -pubin -inkey "$work/pub.pem" -rawin -in "$work/msg" -sigfile "$work/sig" >/dev/null
  { base64 < "$work/sig" | tr -d '\n'; echo; } > "$file.sig"
  echo "signed $name"
done
