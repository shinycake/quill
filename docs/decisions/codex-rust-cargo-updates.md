# Rust 1.99.0 and Cargo updates

Closes #555. Part of #552 (five of the eight majors are left for their own PRs).

## What tdesktop does

Nothing relevant. It builds against a fixed Qt and a vendored dependency set, so there is no direct counterpart. This is Quill's own dependency policy (`docs/dependency-updates.md`).

## Toolchain: 1.98.1 to 1.99.0

The version is pinned in `rust-toolchain.toml`, `docs/build.md` and the six CI/packaging workflows (`ci.yml` twice, `macos-smoke-fallback.yml`, `package-{linux,macos,windows}.yml`). All now say 1.99.0. `README.md` and `DECISIONS.md` still mention 1.98.1 and are left to the merge pipeline. The MSRV (1.92) is unchanged.

New clippy lints, fixed in code with no `allow`s. The gate runs clippy without the `ui` feature, so I also ran `cargo clippy --features ui --all-targets -- -D warnings`, and that is where all of these were:

- `needless_update`: dropped four `..Default::default()` tails in `src/ui/demo.rs` where every field is already set.
- `clone_on_copy`: `IconName` in `src/ui/folder_tabs.rs`.
- `single_match`: `src/ui/chat_look_ui.rs`, now `if let`.
- `collapsible_if`: `src/ui/translate_ui.rs`.
- `useless_format`: `src/ui/premium_demo.rs`.
- `items_after_test_module`: moved the test module to the end in `src/ui/wallpaper.rs` and `src/ui/saved_sublists.rs`. The wallpaper diff is large but it is only a move.

## Compatible updates

`cargo update`, 66 entries. `gpui-base`, `gpui-pre-*`, `gpui-kit` and `gpui-component` stay at their locked versions (verified by diffing the `gpui*` entries in `Cargo.lock` before and after). `gpui-component-macros` 0.7.1 is also held back by the `=` pin on `gpui-kit`.

## Major updates

Done (three, all compile for `ui` and core, tests pass):

- `libloading` 0.8 to 0.9, in the root crate and `crates/ntgcalls-sys`. We only use `Library::new` and `get`. `clang-sys` (build-time, via gpui-pre) still pulls 0.8; that is a second copy in the lockfile and does no harm.
- `resvg` 0.46 to 0.48, used for the wallpaper pattern rasteriser. resvg 0.48 made gzip (SVGZ, the TGV wallpaper format) an optional feature, so Cargo.toml now enables `svgz`; without it `gzipped_svg_is_the_tgv_format` fails. The wallpaper demo capture is byte-identical.
- `tray-icon` 0.21 to 0.26 (macOS and Windows only; Linux uses ksni). The template-icon methods are deprecated in 0.26 and macOS only, so `src/tray.rs` uses `with_icon_templated` / `set_icon_templated` on macOS and plain `with_icon` / `set_icon` elsewhere. Tested compile-wise on macOS only; the Windows branch is unverified.

Left for their own PRs:

- `core-video` 0.5 to 0.6: gpui-pre (vendored, pinned) depends on 0.5 and hands CoreVideo types across the boundary. Bumping only ours would give two incompatible type sets. It moves with the next gpui-kit bump.
- `cpal` 0.17 to 0.18: `rodio` 0.22 depends on cpal 0.17, so we would run two cpal versions in one process, one for playback and one for the microphone. Wait for a rodio release on 0.18.
- `pbkdf2` 0.12 to 0.13 and `chacha20poly1305` 0.10 to 0.11: both are RustCrypto trait-generation changes (hybrid-array, `sha2`/`hmac` 0.11) that touch the passcode and database-key encryption in `src/passcode.rs` (`Key::from_slice` and friends go away). This needs a deliberate change with compatibility tests on existing on-disk data, not a drive-by bump.
- `x11rb` 0.13 to 0.14: Linux-only (global push-to-talk via RECORD). Cannot compile or run it on this macOS machine, and no Linux target is installed. Needs a Linux check.

## Verification

- `cargo deny check licenses`: ok. `THIRD_PARTY_LICENSES.md` regenerated with `scripts/third-party-licenses.sh`.
- Gate: GATE OK (core and ui tests).
- Demo captures (1200x800) of `ready-chat-list`, `ready-appearance-wallpapers` and `ready-folders-sidebar` from main (1.98.1) and from this branch are byte-identical. `ready-bubble-headers` differs only in the wall-clock times shown in the bubbles. I looked at the captures.
- Windows and Linux builds were not run locally. CI covers them.
