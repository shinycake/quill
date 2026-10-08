#!/usr/bin/env bash
# Regenerate THIRD_PARTY_LICENSES.md (license texts of every Rust crate compiled
# into the quill binary) with cargo-about, from licenses/about.toml and
# licenses/about.hbs.
#
#   scripts/third-party-licenses.sh           rewrite THIRD_PARTY_LICENSES.md
#   scripts/third-party-licenses.sh --check   fail if it is out of date
#
# Offline on purpose: cargo-about reads the license files packaged in each
# crate (after `cargo fetch`), so the output depends only on Cargo.lock and
# not on the network. Requires cargo-about 0.9 (cargo install cargo-about
# --locked --features cli).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
command -v cargo-about >/dev/null || { echo "error: cargo-about is not installed" >&2; exit 2; }

cargo fetch --locked >/dev/null
OUT="$(mktemp)"
trap 'rm -f "$OUT"' EXIT
cargo about generate --locked --offline -c licenses/about.toml -o "$OUT" licenses/about.hbs

if [[ "${1:-}" == --check ]]; then
  if ! cmp -s "$OUT" THIRD_PARTY_LICENSES.md; then
    diff -u THIRD_PARTY_LICENSES.md "$OUT" | head -40 >&2 || true
    echo "error: THIRD_PARTY_LICENSES.md is stale; run scripts/third-party-licenses.sh" >&2
    exit 1
  fi
  echo "THIRD_PARTY_LICENSES.md is up to date"
else
  cp "$OUT" THIRD_PARTY_LICENSES.md
  echo "Wrote THIRD_PARTY_LICENSES.md"
fi
