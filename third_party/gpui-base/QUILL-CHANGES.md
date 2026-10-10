# Changes made by the Quill project

This directory is gpui-base 0.7.1 from crates.io (Apache-2.0, `LICENSE-APACHE`),
used through `[patch.crates-io]` in Quill's `Cargo.toml`. Commit `73f33094`
added the unmodified registry copy, so `git diff 73f33094 -- third_party/gpui-base`
shows every change. (The patch was first made on 0.7.0, vendored in `eb465597`,
and carried to 0.7.1 in `docs/decisions/codex-gpui-kit-0.7.1.md`.)

The Quill project changed it in 2026 to add bidirectional text support to the
input engine:

- Added `src/input/bidi_paragraph.rs` and `src/input/editor/display_map/bidi.rs`.
- Modified `Cargo.toml` (adds the `unicode-bidi` and `unicode-bidi-mirroring`
  dependencies), `src/lib.rs`, `src/text_selection.rs`, `src/input/mod.rs`,
  `src/input/base/{element,movement,state}.rs` and
  `src/input/editor/display_map/{inline_line,mod,text_wrapper}.rs`.

In `src/input/base/element.rs`, 0.7.1 applies the horizontal scroll offset to
every caret after layout and clamps carets of right-aligned text to the right
edge; the patch adds a per-caret `rtl` flag (`CursorRenderInfo::rtl`) so a
caret on a right-to-left paragraph gets the same clamp.

Each changed file starts with a comment saying so. The changes are licensed
under Apache-2.0, like the rest of the crate. Background:
`docs/decisions/codex-rtl-composer.md`.

`fragment_extent` (in `display_map/bidi.rs`) uses the advance of the rightmost
character only for a glyph that starts at or past the reported width. A letter
at the start of an Arabic or Persian word is drawn in a narrower contextual
form, so its own advance overstated the fragment and right-aligned rows sat a
few pixels in from the edge. `BidiParagraph` no longer stores the unused
`wrap_width`.
