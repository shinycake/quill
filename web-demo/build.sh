#!/bin/sh
# Builds the web demo into www/ (needs wasm-bindgen-cli 0.2.129+).
# RUSTC_BOOTSTRAP is required on stable because gpui-pre-web's default
# `multithreaded` feature pulls wasm_thread, which uses nightly features.
set -e
cd "$(dirname "$0")"
RUSTC_BOOTSTRAP=1 cargo build --release
wasm-bindgen --target web --no-typescript --out-dir www "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/quill_web_demo.wasm"
