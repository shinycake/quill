## codex/rtl-emoji (2026-10-08)

A regular Unicode emoji inside right-to-left text (a Hebrew channel caption
ending in U+261D U+FE0F) was drawn as a tiny monochrome text glyph in the
wrong spot, where Telegram Desktop draws the colour emoji inline. Read
`codex-rtl-composer.md`, `codex-rtl-bubbles.md` and `codex-rtl-polish.md`
first; this fixes the shared bidi module they describe.

### Root cause (hypothesis 1)

`Paragraph::visual_runs` (vendored `gpui-base`,
`input/editor/display_map/bidi.rs`) cut runs by embedding level and then
`split_rtl_run` turned every edge neutral of a right-to-left run into a run
of **one `char`** each (so a platform shaper cannot misplace a lone space or
bracket). An emoji is a neutral (bidi class ON) followed by a variation
selector (NSM), a skin tone (ON), ZWJ parts (BN) and so on, so a trailing
`☝️` became two runs, `☝` and `U+FE0F`, shaped separately. The shaper gets the
text-presentation base alone, never sees the selector, and picks the
monochrome text glyph; the selector shapes to nothing. Skin tones, ZWJ
families and keycaps broke the same way, and the same char-wise cuts existed
in row wrapping (over-wide word broken at characters) and one-line elision
(`layout_one_line` kept a prefix by `char_indices`).

Not the cause: hypothesis 2 (fallback). Runs are shaped with the same
`shape_line` and font as the non-bidi path, and CoreText (and cosmic-text,
DirectWrite) choose the colour emoji font per shaped string, so a whole
cluster gets it. Hypothesis 3 (UBA N1/N2): `unicode-bidi` already resolves a
neutral emoji between a Hebrew letter and the end of an RTL paragraph to the
RTL level, which puts it at the left end, left of the last word, as in
Telegram; a unit test pins that.

### Fix (separate commit, vendored crate)

- `visual_runs` builds level runs on **grapheme cluster** boundaries
  (`unicode-segmentation`, already a dependency): a cluster takes the level
  of its first character, runs are reordered with `reorder_visual` over the
  run levels (UAX #9 L2).
- `split_rtl_run` cuts edge neutrals per grapheme, not per char.
- `BidiParagraph::wrap` breaks an over-wide word between graphemes and
  `layout_one_line` elides at grapheme boundaries.

The composer (`input/base/element.rs`) uses the same `visual_runs`, so it is
fixed by the same change. Chat-list previews and bubbles go through
`BidiParagraph` and the same change.

### Platforms

Shaping is per platform text system (CoreText, DirectWrite, cosmic-text), all
of which do colour-emoji fallback for a whole cluster; the fix only makes sure
they receive one. Verified visually on macOS only; Linux and Windows not run.

### Tests

- `vendored_input_bidi` (unit, in `cargo test`): no run cuts a cluster for
  U+261D U+FE0F, thumbs up with skin tone, ZWJ family, flag, keycap, at the
  start, middle and end of Hebrew and mixed Hebrew/Latin text; a trailing
  emoji is one RTL run at the left end; flag/keycap inside Hebrew stay whole.
- `cargo test --features demo-capture --bin quill composer_rtl` (UI, 4 new):
  the text system records every shaped string; bubbles, one-line previews at
  many widths (elision) and the composer never shape a piece that starts
  inside or ends inside a cluster, and the emoji is shaped whole; a trailing
  emoji sits left of the last Hebrew word. Without the fix 4 of them fail.
- Demo: `--screenshot-demo ready-rtl-polish` now has the report's caption
  (incoming bubble, chat preview) and a message/preview with all five emoji.
