## codex/rtl-bubbles (2026-10-08)

Long right-to-left (Hebrew, Arabic) messages in bubbles wrapped in the wrong
row order: the first row held the END of the sentence and the first words
were on the last row. Stacked on `codex/rtl-composer` (PR #440), whose bidi
module it reuses. Read that decision doc first
(`codex-rtl-composer.md`).

### Cause

`gpui`'s `TextLayout` shapes the paragraph as one line and
`compute_wrap_boundaries` (`gpui-pre-0.3.7 text_system/line_layout.rs`) walks
the glyphs in the order the platform returned them, as a left-to-right
sequence. For an RTL run that is visual order, so the rows are cut out of
the visual line, not out of the sentence. Telegram Desktop wraps first, in
typing order (`Ui::Text`, `lib_ui/ui/text/text.cpp`), and then orders each
row with the paragraph's own direction.

### Design

- `gpui_base::input::bidi_paragraph::BidiParagraph` (vendored `gpui-base`,
  next to the input engine's bidi module it shares code with):
  1. text is split at `\n`; each hard line gets its own base direction
     (first strong character, same rule as the composer and Telegram);
  2. each hard line is wrapped greedily in **typing order** at the available
     width: words are measured by shaping them with their real runs (bold,
     code font), so formatting is accounted for; trailing break spaces hang
     (no room, no effect on alignment); a word wider than the row breaks at
     characters;
  3. every row is shaped run by run in visual order (UAX #9 L1/L2 with the
     paragraph's level, edge neutrals as runs of their own), the exact path
     the composer uses, so it does not depend on the platform's base direction;
  4. rows are painted right-aligned when their hard line is RTL, left-aligned
     otherwise (an LTR paragraph with a Hebrew word still gets correct
     rows). The element is as wide as the wrap width when a row is
     right-aligned, so text reaches the bubble's right edge.
- Only text with right-to-left characters takes this path
  (`quill::text::has_rtl_text`, same test as the engine's `needs_bidi`);
  everything else keeps GPUI's `StyledText` untouched.
- `SelectableRichText` (`src/ui/selectable_text.rs`) chooses the path in
  `request_layout` (`request_measured_layout`, re-laid out at the final width
  in prepaint when the measured width differs). Highlights (bold, italic,
  links, spoilers' transparent text, strike/underline), font-family overrides
  (code) and the footer's em-space reservation are turned into `TextRun`s the
  way `StyledText` does at layout time.
- Selection: `gpui-base` text selection reads its glyph geometry through a
  small trait, `RunGeometry` (implemented for `TextLayout`, so every other
  user is unchanged). The bidi paragraph supplies its own: caret positions,
  hit testing and row extents from the row geometry, and `selected_range`,
  which makes a drag select the **logical** range between its two endpoints
  (the reading-order band walk assumes left-to-right rows). Double and triple
  click go through the same positions.
- Links, spoilers (their specks are drawn over `range_rects`), the selection
  highlight (one rectangle per row and per visual piece) and the pointing-hand
  cursor use `index_for_position` / `range_rects`.

### Not changed

- Chat-list previews: a single truncated line shaped by the platform, left
  aligned in an LTR UI as in Telegram Desktop; no wrapping to get wrong.
- Paragraphs with resolved custom-emoji images (per-run flex layout) and other
  `StyledText` users (search results, archive row).

### Limits

- Not run on Linux/Windows; the row shaping is the composer's, which does not
  rely on the platform's base direction.
- Word measurement adds per-word widths (no kerning across spaces); a row can
  differ from the platform's own by a fraction of a space.
- No hyphenation or CJK break opportunities: rows break after spaces and,
  for over-wide words, between characters.

### Tests

`cargo test --features demo-capture --bin quill composer_rtl` (17 tests, the
last four are bubbles): plain text is left to GPUI, a long Hebrew message
wraps in typing order with every row at the right edge, hit testing and
rectangles follow the glyphs, logical drag selection, mixed runs ordered
visually and identical for both platform base directions. `has_rtl_text` is
tested in `src/text.rs`.

Screenshots in `docs/decisions/codex-rtl-bubbles/`:
`before-bubbles.png` (end of sentence on the first row), `after-bubbles.png`
(long Hebrew, mixed, and a Hebrew message with a link, bold and code).
Reproduce with `ready-rtl-composer` (see `codex-rtl-composer.md`).
