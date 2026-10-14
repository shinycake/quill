#!/bin/bash
# SessionStart hook for Claude Code cloud sessions: installs what the Linux CI
# job (.github/workflows/ci.yml, linux-fmt-clippy-test) needs so fmt, clippy and
# both test suites (core and headless UI) run in the session. Idempotent.
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-$(dirname "$0")/../..}"

# Keep this list in sync with the "apt dependencies (UI build)" step in ci.yml.
APT_PACKAGES=(
  build-essential cmake pkg-config nasm
  zlib1g-dev libssl-dev libgtk-3-dev libxkbcommon-dev libxkbcommon-x11-dev
  libwayland-dev libxcb1-dev libx11-xcb-dev libxcb-xkb-dev libfontconfig-dev
  libfreetype-dev libasound2-dev libvulkan-dev libdbus-1-dev libclang-dev
)

missing=()
for pkg in "${APT_PACKAGES[@]}"; do
  if ! dpkg-query -W -f='${Status}' "$pkg" 2>/dev/null | grep -q "install ok installed"; then
    missing+=("$pkg")
  fi
done

if [ "${#missing[@]}" -gt 0 ]; then
  SUDO=""
  if [ "$(id -u)" -ne 0 ]; then SUDO="sudo"; fi
  echo "Installing apt packages: ${missing[*]}" >&2
  $SUDO apt-get update -qq >&2
  DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq --no-install-recommends "${missing[@]}" >&2
fi

# The toolchain (with rustfmt and clippy) pinned in rust-toolchain.toml.
rustup toolchain install >&2 2>/dev/null || rustup show active-toolchain >&2

# Download every crate in Cargo.lock so builds do not stall on the network.
cargo fetch --locked >&2
