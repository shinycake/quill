# Changes made by the Quill project

This directory is gpui-base 0.7.0 from crates.io (Apache-2.0, `LICENSE-APACHE`),
used through `[patch.crates-io]` in Quill's `Cargo.toml`. Commit `eb465597`
added the unmodified registry copy, so `git diff eb465597 -- third_party/gpui-base`
shows every change.

The Quill project changed it in 2026 to add bidirectional text support to the
input engine:

- Added `src/input/bidi_paragraph.rs` and `src/input/editor/display_map/bidi.rs`.
- Modified `Cargo.toml` (adds the `unicode-bidi` and `unicode-bidi-mirroring`
  dependencies), `src/lib.rs`, `src/text_selection.rs`, `src/input/mod.rs`,
  `src/input/base/{element,movement,state}.rs` and
  `src/input/editor/display_map/{inline_line,mod,text_wrapper}.rs`.

Each changed file starts with a comment saying so. The changes are licensed
under Apache-2.0, like the rest of the crate. Background:
`docs/decisions/codex-rtl-composer.md`.
