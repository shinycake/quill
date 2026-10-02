# Third-party notices

This file tracks licenses of dependencies Quill links or vendors. It is not a completed legal audit.

| Component | License | Notes |
|---|---|---|
| Quill | MIT | `LICENSE` |
| GPUI Kit, GPUI (`gpui-pre*`), gpui-base, gpui-component, gpui-kit-assets | Apache-2.0 | crates.io 0.6.1 / gpui-pre 0.3.x family |
| Lucide icons (via gpui-kit-assets) | ISC (Lucide) | bundled only when the `assets` feature is on |
| TDLib | Boost Software License 1.0 | schema vendored in `schema/td_api.tl`; native library optional |
| ntgcalls (media engine) | GNU Lesser General Public License v3.0 (LGPL-3.0) | **Not linked into the Quill binary.** `pytgcalls/ntgcalls` v3.0.0 (released 2026-09-25) prebuilt shared libraries `libntgcalls.so` (Linux x86_64) and `libntgcalls.dylib` (macOS ARM64), procured unmodified by `scripts/vendor-ntgcalls.sh` (SHA256-pinned, git-ignored, never committed) and loaded at runtime via `dlopen` from the `ntgcalls-sys` crate. The macOS sidecar can also be copied unmodified into the app bundle’s Frameworks directory; Windows assets remain unsupported by the procurement script. Source: https://github.com/pytgcalls/ntgcalls — the complete corresponding LGPL source for the unmodified library is available from upstream at that URL (release tag v3.0.0). |
| Rust crates in `Cargo.lock` | various OSI | inspect `cargo license` before a public release |

No source was copied from Paper Plane, Coop, or Mezon (GPL). ZapFast was not forked.
