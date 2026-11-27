# Changes made by the Quill project

This directory is gpui-base 0.11.0 from crates.io (Apache-2.0, `LICENSE-APACHE`),
used through `[patch.crates-io]` in Quill's `Cargo.toml`. The commit "Vendor gpui-base and gpui-component 0.11.0 unmodified from crates.io"
added the unmodified registry copy, so `git diff <that commit> -- third_party/gpui-base`
shows every change. (The patch was first made on 0.7.0, vendored in `eb465597`,
carried to 0.7.1 in `docs/decisions/codex-gpui-kit-0.7.1.md`, to 0.7.3 in
`docs/decisions/gpui-kit-0-7-3.md` and to 0.8.0 in
`docs/decisions/gpui-kit-0-8-0.md`, to 0.8.1 in `docs/decisions/gpui-kit-0-8-1.md`, and to 0.11.0 in
`docs/decisions/gpui-kit-0-11-0.md`.)

0.11.0 note: upstream moved the input to a new row model (`f64` scrolling,
rows placed by `local_row_y` and aligned one by one by `LineLayout::row_x`)
and added its own long-line work: rows far from the viewport left
unshaped (`InputLine::unshaped`), natural alignment (`start_align`) and
width caches (`char_widths`, monospace detection, `measurements`,
`wrap_width`). Quill's engine sits on the new row model: a right-to-left
paragraph aligns its rows to the right edge (`align_lines` in
`element.rs`), rows a long line leaves out count toward row offsets
(`LineLayout::row_count`), and the caret clamp, bidi rows, formatting
spans, `row_wrap.rs` and the `LineWindow` layout stay. Upstream's
unshaped rows, natural alignment and width caches are left out (and their
`text_wrapper/` submodules and benches removed), since `LineWindow`, the
bidi direction and `row_wrap.rs` cover the same ground. Upstream's
single-pass line walk (`LinesFrom`), `RunIndex`, `prepare_wrap_width` and
`f64` scroll are kept. `start_align`'s test became
`a_right_aligned_paragraph_aligns_each_row_by_its_own_width`.

The Quill project changed it in 2026 to add bidirectional text support to the
input engine:

- Added `src/input/bidi_paragraph.rs` and `src/input/editor/display_map/bidi.rs`.
- Modified `Cargo.toml` (adds the `unicode-bidi` and `unicode-bidi-mirroring`
  dependencies), `src/lib.rs`, `src/text_selection.rs`, `src/input/mod.rs`,
  `src/input/base/{element,movement,state}.rs` and
  `src/input/editor/display_map/{inline_line,mod,text_wrapper}.rs`.

In `src/input/base/element.rs`, upstream applies the horizontal scroll offset to
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

## Formatting spans and inline tokens in right-to-left text

Added in 2026 for the formatted composer field
(`docs/decisions/codex-composer-input.md`):

- `src/input/base/text_spans.rs` (new): formatting spans, named tags over
  UTF-8 byte ranges (`TextSpan`), kept as part of the document of an `Input`
  or `Textarea`. Edits move them (text typed inside a span joins it, text
  typed at its end continues it over letters and digits only, as Telegram
  Desktop's `InputField` does), undo and redo restore them (each change
  records a before/after snapshot), and `InputContent` carries them
  (`with_span`, `spans`). New API on `InputState` and `TextareaState`:
  `spans`, `set_spans` (one undo step), `set_typing_spans` /
  `typing_spans` (the tags the next typed text gets),
  `replace_range_with_content` (text, tokens and spans as one undo step) and
  `set_span_styler` (how a tag is drawn: a `HighlightStyle` plus an optional
  font family). Copy and cut put the spans and tokens of a single selection on
  the clipboard as JSON metadata; paste restores them when the clipboard text
  still matches.
- The wrapper measures a line that holds spans with their fonts, so bold text
  wraps where it is drawn (`text_wrapper.rs`: `styled_wrap_boundaries`,
  `set_span_fonts`); lines without spans keep the old path.
- Inline tokens in a paragraph with right-to-left text take the bidi path:
  each visual run is cut at its tokens and a token is an object of its
  measured width in the run's visual order (`element.rs`:
  `layout_bidi_token_row`; `inline_line.rs`: a bidi row's shaped lines are
  optional). Tokens are placed at the left edge of their own box
  (`LineLayout::object_position`), which in a right-to-left run is not where
  the caret of their start offset is. Token rows paint backgrounds now.
- Modified: `src/input/mod.rs`, `src/input/base/{change,element,inline_tokens,state,undo_manager}.rs`,
  `src/input/editor/display_map/{display_map,inline_line,mod,text_wrapper,wrap_map}.rs`.
  Inputs without spans or tokens take the unchanged code paths.

## Tooltips hide when the pointer leaves

`src/tooltip.rs`: `TooltipOverlay::request_hide` clears the tooltip at once
instead of after the grace period (`TooltipDefaults::grace_period`, 300 ms by
default since 0.7.3), which left it on screen after the pointer moved off (or
a scroll carried the trigger away). The grace period still keeps the
"recently shown" state, so a neighbouring trigger shows its tooltip without
the show delay. The doc comments of `TooltipDefaults::with_grace_period` and
`grace_period` say so.

## Slider labels

`src/slider.rs`: `SliderState::label` stores an accessible name, and `Slider`
sets it as the root's `aria_label`. The kit `Slider` and composed sliders
both render through the base `Slider` from the same state, so a label set
where the state is built names either one. Quill uses it for the video
player's "Seek" and "Volume" sliders (`docs/decisions/video-player-controls.md`).
0.8.0 adds `Slider::accessibility_label` on the element (#3494). That label
wins when both are set; the state's label is the fallback, so a slider
drawn in several places keeps its name without repeating it at each one.

## Text style accessor

`InputState::text_style` (through `DisplayMap`, `WrapMap` and `TextWrapper`)
returns the font and size the input lays its text out with. Quill's inline bot
hint is drawn over the field and uses it, so the hint follows the field's own
style (`docs/decisions/composer-gaps-2.md`).

## Scroll bounce reports its displacement

`src/scroll_bounce.rs`, `src/overscroll.rs`, `src/lib.rs`:
`ScrollBounce::displacement(Rc<Cell<ScrollBounceDisplacement>>)` writes the
edge's offset and whether a finger still holds it, once per prepaint. The
wrapper keeps the stretch private, so Quill's chat list could not tell a pull
(a drag past the top, which expands the stories strip) from a fling that
throws the edge. Nothing else changes: the offset the list is drawn at and
the physics are untouched. Background:
`docs/decisions/elastic-chat-list.md`.

`ScrollBounceMotion::with_max_throw` caps how far a fling throws the edge
while `max_stretch` still bounds a finger's pull. Quill's chat list lets a
finger pull the top 96 px (to open the stories strip) but a fling throws it
no farther than a normal bounce.

## Element wrappers forward the accessibility tag

Added in 2026 for Agent access's UI driver. GPUI's `Element::a11y_tag`
(a Quill addition to `gpui-pre`, see its `QUILL-CHANGES.md`) is forwarded
by the wrappers that forward `a11y_role`: `test_support.rs`,
`focus_trap.rs` and `a11y_disabled.rs`. Without it a tagged `div` that
passes through `.test_support()` or a focus trap lost its tag.

## Cocoa Ctrl text keys on macOS

`src/input/base/state.rs`: the input binds the rest of macOS's standard
Ctrl text keys next to the existing Ctrl+A / Ctrl+E: Ctrl+F / B / N / P move
the caret, Ctrl+D deletes forward, Ctrl+H deletes back, Ctrl+K deletes to the
end of the line. GPUI hands a key to AppKit's text system only when no
binding matches, and then drops the command, so a field had none of these
unless the keymap names them. Background:
`docs/decisions/native-macos-shortcuts.md`.

## The caret blinks in an active window only

`src/input/base/state.rs`: a field focused while its window was inactive
(the window in the background, or never made key) started the caret's
blink. Deactivating a window stops it (the blur handler), but nothing
stopped one started after that. An inactive window never draws the caret
(`show_cursor`), yet each blink notified the view, so the window drew two
invisible frames a second for as long as it stayed in the background.
That kept the display link from going idle, so the idle release of its
drawables never ran (one window-sized drawable, about 20 MB, held for
nothing). `focus` and the focus handler now start the blink only in an
active window; activating the window starts it as before.

The input tests' window (`InputView::build_with`) is activated, as a real
window that takes a click or a key is, and the two tests that timed the
blink after a focus wait for the focus handler's restart.

The blur handler returns early while the code editor's completion or
code-action menu is open, so the menu keeps the field looking focused
(`show_cursor`). That left the blink running in two cases: the window
deactivated with the menu open (the background window repainted twice a
second until it was activated again), and the menu closed on a field that
had lost its focus under it (the field repainted twice a second for good,
with no caret drawn). The input's paint (`src/input/base/element.rs`,
`InputBaseState::sync_blink`) now keeps the blink running exactly while the
caret can show in the window painting the field (`caret_can_show`: focused
there or under its menu, and the window active): it stops a blink that shows
nothing and starts one no listener started. The paint, not a listener,
because a field can be drawn in another window than the one it was made in
(Quill's call window draws a message field the main window made), and the
focus and activation listeners watch the window it was made in: stopping
from them left such a field's caret hidden when its window came back. Only
the window that started a blink (`blink_window`) stops it, so a field
painted in two windows can't have them take turns. Background:
`docs/decisions/idle-inactive-redraw.md`.

## The scrollbar keeps the app's clock

`src/scrollbar.rs`: the bar shows on a scroll and hides `DEFAULT_IDLE`
later. It read the wall clock (`Instant::now()`) while its idle timer waited
on the executor's timer. Under the test clock the timer's two seconds pass
in a few real milliseconds, the wall clock said the idle was not over, and
the timer was armed again, a redraw each time, until two seconds of real
time had passed: a test counting a window's frames counted those, more on a
faster machine. Every time the bar reads now comes from
`cx.background_executor().now()`, which is the wall clock outside tests.

## A laid-out paragraph keeps no vector slack

`src/input/bidi_paragraph.rs`: Quill caches laid-out paragraphs
(`docs/decisions/paragraph-cache-widths.md`), so `BidiParagraph` no longer
keeps space it does not use. `shape_row` sizes a row's fragment and shaped
line vectors for its runs instead of letting them double (a slot is 136
bytes), and `layout_unchecked` drops the slack of its row vector (208 bytes
a row). Each run's `ShapedLine` keeps its text, which `ShapedLine::cursor`
and `split_at` read. What is drawn and measured is unchanged.

`BidiParagraph::heap_size` (and `InputLine::heap_size` under it) counts
about how many bytes a laid-out paragraph holds on the heap, from its rows,
runs, shaped lines, glyphs, clusters and text, so Quill's cache keeps
laid-out text to a budget of bytes rather than a count of paragraphs.

## A long line costs in proportion to the edit

A keystroke on a 100 KB line wrapped the whole line through the shaper, then
shaped and painted every row, every frame. `docs/decisions/long-text.md` has
the numbers.

- Added `src/input/editor/display_map/row_wrap.rs`. A row is a pure function of
  where it starts and the text after it, so an edit wraps again from just
  before itself and stops at the first row start the old wrap shares
  (`Resume`). Each row works in a window of text seeded by summed character
  widths and shapes only to settle the last few characters. A line of
  `LONG_LINE` bytes or more, or any line of a text of `LONG_TEXT` bytes or
  more, trusts the sum and shapes nothing. `TextWrapper::update` wraps every
  line again when an edit takes the text across `LONG_TEXT`.
- `src/input/editor/display_map/text_wrapper.rs`: `TextWrapper::update` goes
  through `wrap_rows` for every line, so tokens, spans and tabs share one path.
  The `LineFragment` token path, the plain path and the `WrappingIndent::None`
  second pass are gone. `LineLayout` can hold only some rows of a line
  (`OmittedRows`); offsets and positions stay relative to the whole line.
- `src/input/base/element.rs`: `layout_lines` and `layout_token_lines` shape a
  long line's rows near the viewport (`LineItem::window`) in an input with no
  gutter. When the caret scrolls the viewport past those rows, prepaint lays out
  the rows it landed on in the same frame.
- `src/input/editor/display_map/bidi.rs`: `starts_rtl` and
  `Paragraph::analyze_part` give the laid-out part of a long line the base
  direction of the whole line.
- `src/input/base/movement.rs`, `src/input/base/state.rs`: an arrow key on a
  caret outside the laid-out rows steps logically, and a pointer over those rows
  resolves on the nearest laid-out row.

## Reading the value costs one copy per edit

A keystroke on a 1 MB rich draft read `value()` about ten times, and each read
copied the rope (sweep item X02).

- `src/input/base/state.rs`: `value()` keeps its string in `value_memo` until
  the next edit takes it. Every write to `text` clears it, and a debug assert in
  `value()` compares the two.

0.9.0 note (`docs/decisions/gpui-kit-0-9-0.md`): `text_wrapper.rs` stays on
Quill's `row_wrap.rs`. Upstream's width-range cache for live resize
(`text_wrapper/measurements.rs`, `wrap_width.rs`) solves the same problem on the
algorithm Quill replaced, so those files are removed. `BlinkCursor::is_blinking`
reads upstream's `last_activity` where it read `epoch`.

## Tests that run (RTL sweep)

Changed by the Quill project in 2026 (`docs/decisions/rtl-sweep.md`): the crate
is a member of Quill's workspace so `cargo test -p gpui-base --features
test-support --lib` runs, in CI on every OS. `Cargo.toml` drops the
dev-dependencies only the benches used (`criterion`, `gpui-fast`,
`gpui-fast-platform`, `gpui-pre-reqwest-client`) and the two `[[bench]]`
targets, whose files are removed. `row_wrap.rs` tests pass `wrap_rows` its
`long_text` argument. `src/input/base/movement.rs`: before a line is laid out,
the arrow keys step by the paragraph's own direction (read from its text), so
Right in a Hebrew paragraph moves toward its start as it does once drawn;
`test_rtl_arrow_keys_move_visually` checks it with no layout. The Quill-modified
files are formatted with `cargo fmt`.

Second RTL round (Quill project, 2026-11-22): `input/base/state.rs` Backspace
after a Hebrew or Arabic point removes only that point
(`text_boundary::is_detachable_point`); `display_map/bidi.rs` shapes Hebrew or
Arabic letters at an even level (a left-to-right override) one grapheme at a
time, so the platform cannot reorder them; `input/bidi_paragraph.rs` adds
`BidiParagraph::row_shapes` (each row's width and direction).

## Rounded content masks

Changed by the Quill project in 2026
(`docs/decisions/rounded-content-masks.md`): gpui-pre's `ContentMask` gained
a rounded part, so the masks built here are made with `ContentMask::rect`
instead of a struct literal. Nothing else changes.

## Mouse wheel notches glide

Added in 2026 with gpui-pre's `WheelGlider` (`docs/decisions/smooth-wheel-scroll.md`).

- `src/scrollbar.rs`: `ScrollbarHandle::wheel_glider` (default `None`) and
  `set_glide_offset` (default `set_offset`), implemented for `ScrollHandle`,
  `UniformListScrollHandle` and `ListState`; `src/virtual_list.rs` for
  `VirtualListScrollHandle`.
- `src/scroll_bounce.rs`: the wheel-owning listener hands a gliding notch
  (`gpui::wheel_notch_glides`) to the handle's glider instead of moving by
  the whole notch. Each frame step clamps to the range, writes with
  `set_glide_offset`, notifies and runs `on_scroll`, so the history and the
  chat list see every step as they saw the one jump. The overscroll is still
  reset at once, and pixel input still bounces and stops a glide through
  `set_offset`. Tests that assumed the jump step the glide's frames:
  `ordinary_mouse_wheel_keeps_the_shared_viewport_undisplaced`,
  `wheel_scrolling_interrupts_the_shared_return`, and the `wheel` helper of
  the `scrollable_mask` tests.
