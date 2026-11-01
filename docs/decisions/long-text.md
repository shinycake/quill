# long-text: one very long composer line

## tdesktop
The compose field is a Qt `QTextEdit` with `QTextOption::WrapAtWordBoundaryOrAnywhere`. Qt lays a
paragraph out once and keeps it; `QTextDocumentLayout` lays out from the edited block on and stops
early when it can. A long paste costs one layout, and a keystroke costs far less than the paragraph.

Quill wraps greedily too: it breaks at the last UAX 14 opportunity that fits and at a grapheme
when a word is wider than the row, so nothing below changes where a row may break.

## What was wrong
A line is one `LineItem` in the kit's `TextWrapper`, and any edit on it wrapped the whole line
again with `measured_wrap_boundaries`, which shaped from every row start. On one line of 100 KB a
keystroke took 3.5 G instructions and 260 to 290 ms on the main thread, at the end of the line
and at its start. 90% of that was `TextWrapper::update`. The element then laid out every row of
the line, so a repaint cost in proportion to the line too.

## What changed
The wrap moved to `display_map/row_wrap.rs`, which also keeps the text metrics.

- A row depends only on where it starts and on the text after that. So when a single line changes,
  rows ending well before the edit are kept (`Resume::kept_rows`), wrapping starts from there, and
  the rows after the edit are reused as soon as a new row starts where an old one did, shifted by
  the edit (`Resume::settled`).
- A line of at least `LONG_LINE` bytes (16 KB) measures a row by summing cached character widths
  (`LineWrapper::width_for_char`) instead of shaping. That is 20 ns against 250 ns a character, and
  more to the point it is the same on any row, so a wrap of a long line never reaches CoreText,
  DirectWrite or cosmic-text. The regime is a function of the line's length alone, and crossing the
  mark wraps the whole line again, so a line's rows do not depend on its edit history.
- A long line lays out only the rows near the viewport. `LineLayout` says which rows it left out
  (`LineWindow`, `OmittedRows`), and every position query starts from them. Code editors and masked
  fields keep the full layout, because nothing there reads a window. The window is chosen before
  the viewport is final, so `prepaint` lays out a second time in the same frame if a scroll to the
  caret moved it elsewhere. There is no blank frame, and the caret stands at its shaped position.
- Bidi runs on the laid-out part. The base direction comes from the start of the line and is
  looked for in the first `BASE_DIRECTION_REACH` characters (`starts_rtl`), so a part far down an
  Arabic line stays right to left.
- Arrows on a caret scrolled away from its row step logically, and a click on a left-out row lands
  on the nearest laid-out one, which is what the viewport shows.

The sum of isolated widths ignores kerning and shaping, so a long-line row can end at a different
column than a shaped row would. Text stays inside the field because the layout of the row wraps at
the same boundaries; in the worst case an Arabic row, whose joined forms differ from the isolated
ones, ends a little short or a little over.

## Why not
- Chunks of a fixed size, each wrapped on its own: a row would break at a chunk edge. Resuming at
  a shared row start gives the same saving with rows that match a fresh wrap exactly, and the
  property test in `row_wrap.rs` holds it to that.
- Off the main thread: with the cost gone there is nothing to move. Wrapping on another thread would
  also make the layout depend on when the result arrived, against the rule that UI is a function
  of state.
- Predicting the reveal to choose the window: a second layout pass always covers the final
  viewport, and the prediction would have to copy the scroll logic.

## Kit tests
`cargo test -p gpui-base` is refused outside the workspace and CI does not run the kit's tests. The
ones that need a real text system are in `src/ui/composer_long_line_tests.rs`, which CI runs. To run
the kit's own, add `third_party/gpui-base` to `members` in a scratch copy, then
`cargo test -p gpui-base -p quill --lib --features gpui-base/test-support,quill/demo-capture -- input::`.
