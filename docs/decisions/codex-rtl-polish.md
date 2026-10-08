## codex/rtl-polish (2026-10-08)

Follow-up to the RTL composer (#440), bubbles (#442) and inline rich text (#448)
decisions. Read those first.

### 1. Time footer on RTL messages: already what tdesktop does

Read from `lib_ui/ui/text/text.cpp`: `String::recountNaturalSize` sets
`_endsWithQuoteOrOtherDirection` when the *last paragraph's* direction differs
from the UI direction (`lastLineDirection != style::LayoutDirection()`, and a
paragraph with no strong character is `LayoutDirectionAuto`, which never
differs) or the text ends in a quote/`pre` block. `String::updateSkipBlock`
(the block `Message::refreshInfoSkipBlock` adds for the time) then inserts a
`Newline` before the skip block, so the time gets its own line. This is
unconditional: a one-word Hebrew message in an LTR UI has its time under the
text although it would fit, exactly as Quill already did (see `codex-rtl-paragraphs.md`). The renderer also right-aligns the
last line, so the time (bottom-right) sits under the line's start edge.

So no layout change; one mismatch fixed: Quill's `last_line_is_rtl` looked for
the last line *with a letter*, so `שלום\n123` was treated as RTL-ending, but
tdesktop's last paragraph (`123`) has no direction and shares the line with
the time. It now judges the last line of the trimmed text only. Quotes and
`pre` blocks already put the time on its own line (`rich_text_reserving`).
LTR-last-line messages, including `Meeting at six שלום`, keep the time inline.
Tests: `text::tests::rtl_detection_follows_the_first_strong_character`.

### 2. Single-line text

tdesktop draws previews with a one-line `Ui::Text::String`
(`TextDialogOptions`, `elisionLines = 1`, default `al_left`). In
`text_renderer.cpp` (`Qt::AlignLeft` with an RTL paragraph direction adds
`_wLeft` to x) "left" means the paragraph's *start edge*: a Hebrew preview is
right-aligned inside its box, and elision (`prepareElidedLine`) removes the
end of the reading order, the *left* end of a Hebrew line. The same holds for
dialog names (`rowName.draw`), reply and pinned strips.

Done: `BidiParagraph::layout_one_line` (vendored gpui-base) lays the line out
with the engine's per-run shaping, bisects the longest kept prefix that fits
with the ellipsis, keeps the paragraph direction, and the line is painted
right-aligned for an RTL paragraph (left-aligned for a Latin-first line that
holds a Hebrew word, as in tdesktop). Quill's `ui::bidi_line` is the element;
text with no RTL character stays GPUI's `StyledText`. Applied to: chat-list
previews (plain and with entities; bold/italic/code/spoiler as highlights),
chat-list titles (elision only), search results (chat title, preview, in-chat
hit), the archive row, reply strips in bubbles, the pinned bar and its list.
The composer reply bar already goes through the chat-list preview line.

Sender prefix: tdesktop builds "From: text" as one `Ui::Text::String`, so the
paragraph direction is that of the first strong character: `Shahar: חחחח…` is a
left-to-right line (accent-coloured prefix as the first run, Hebrew following),
`בר: …` is right-to-left. Quill lays prefix, media icon glyph and preview out as
one bidi line when the preview has RTL text (`chat_list_preview_line_layered`
`prefix`). With a mini-thumbnail the prefix and thumb stay separate boxes and
the text is right-aligned in the rest, as in tdesktop (sender, thumbs, text).
Draft prefixes ("Draft:") are still separate. Search hit rows carry no sender
prefix in Quill.

Deviations: a chat-list *title* stays at the start edge next to its badges
(tdesktop right-aligns the name in its box and then places badges from the
left, which overlaps); previews that contain a rendered custom-emoji picture
keep GPUI's flex layout (pushed to the end edge when RTL, elided visually).
Native OS notification text is rendered by the OS, not Quill.
The pinned bar is now one line (it showed all lines before).

Cross-platform: `layout_one_line` only uses `shape_line` per single-direction
run, like the composer, so it does not depend on the platform's base direction
(DirectWrite) or on glyph order; elision is by characters in typing order on
every text system. Not run on Linux/Windows.

### 3. Word movement

tdesktop's `InputField` is a `QTextEdit`; Option/Ctrl+Arrow is Qt's visual
cursor movement (no custom code in `input_field.cpp`). Quill's old step flipped
by *paragraph* direction only. Now in a paragraph that holds RTL content
`step_word_horizontally` walks the same caret stops as the arrows: it skips
spaces, then passes one word (alphanumerics) or one run of punctuation, going
the direction on screen, so inside a Hebrew word in a Latin paragraph Left
moves forward in typing order and the step crosses run boundaries (tests:
`word_movement_*`). Paragraphs without RTL content keep the logical word
step. At the visual edge the step flows into the neighbouring paragraph only
from the logical end the key points at (same rule as the arrows). Delete-word
(Option+Backspace) stays logical. Keystrokes: macOS Option+Left/Right
(+Shift), Linux/Windows Ctrl+Left/Right (+Shift). Existing behaviour kept:
"move to the end of the word" in both directions (tdesktop's Qt moves to the
next word *start* on Windows/Linux Ctrl+Right).

### 4. Caret affinity at direction boundaries

An offset between a Latin and a Hebrew run has two places on screen (after the
Latin character, and before the Hebrew one at the run's far edge). The caret
state already had a "hangs on the previous character" flag (soft-wrap line-end
affinity); it now also means this. `BidiLine` gained `x_for_index_trailing`,
`has_two_stops`, `closest_stop_for_x` and `visual_step_stop`: Left/Right visit
both places of the boundary offset (walking right through `abשג`: offsets
0, 1, 2 (after b), 4, 2 (before ש)), a click picks the side that was clicked,
Up/Down keep the side through the column anchor, any edit resets the flag.
Places shared by two offsets (end of a run, start of its neighbour) resolve to
the run the caret travels in. New public API: `TextareaState::caret_bounds`,
`caret_hangs_on_previous_character`. LTR-only text is untouched.

### 5. Other

- Placeholder stays at the UI side (tdesktop: placeholder keeps the UI
  direction); unchanged.
- Link panel / single-line inputs: still platform-shaped (no bidi path for
  single-line inputs), unchanged.
- Known: `visible_range`-based IME bounds use the leading position of a
  boundary offset.

### Tests

Core: `vendored_input_bidi` (35: two places, walking, clicks, char between).
UI (`cargo test --features demo-capture --bin quill composer_rtl`, 27): both
sides of a boundary by keyboard and by click, typing resets affinity,
word movement in RTL and Latin paragraphs, shift-word selection, logical word
movement for plain text, `layout_one_line` elision (RTL at the left end, Latin-first
left-aligned, plain Latin left to GPUI).

### Screenshots

`docs/decisions/codex-rtl-polish/{before,after}-{chat,search}-{light,dark}.png`:
`cargo build --features demo-capture`, then
`QUILL_DEMO_THEME=light|dark QUILL_DEMO_RTL_VIEW=chat|search QUILL_DEMO_CAPTURE=out.png QUILL_DEMO_WINDOW_SIZE=1200x900 target/debug/quill --screenshot-demo ready-rtl-polish <dir>`.
"Before" is the same demo on the previous rendering.
