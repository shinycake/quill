# RTL edge backport

## What tdesktop does

Telegram Desktop lays right-to-left text out with Qt, which measures a run by
the ink of its shaped glyphs, so an Arabic line ends flush with the right edge
of the field.

## What changed

- `fragment_extent` in `third_party/gpui-base/src/input/editor/display_map/bidi.rs`
  added the character's own advance to the rightmost glyph's x. A letter at the
  start of an Arabic or Persian word is shaped into a narrower initial form, so
  the fragment came out a few pixels too wide and right-aligned rows sat in from
  the edge. The advance is now used only when the glyph starts at or past the
  reported width (hanging whitespace). A test covers an initial form.
  The fix was made in the upstream-prep clone (branch `bidi-1-rtl-layout`) and
  is ported here without the rest of that file.
- Removed the never-read `wrap_width` field of `BidiParagraph`. Wrapping happens
  during layout, and hit-testing reads only the shaped rows, so nothing needs it.

## Verification

Gate, the new unit test, and a demo capture of `ready-rtl-composer`.
