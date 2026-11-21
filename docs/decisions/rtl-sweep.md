# Right-to-left sweep: one text contract on every platform

## Why a Windows bug could draw every Hebrew word backwards

PR #1490 found that DirectWrite handed GPUI right-to-left glyphs in typing
order, so Hebrew and Arabic read backwards on Windows. Nothing failed, because:

- every right-to-left test ran on a fake text engine: GPUI's stand-in
  (`NoopTextSystem`) laid every line out left to right in typing order, and the
  composer tests (`src/ui/composer_rtl.rs`) use their own fake;
- `third_party/gpui-base`, which holds the input engine's bidi code, was not a
  workspace member, so `cargo test -p gpui-base` refused to run. Its 1,783
  tests had never run in CI: two no longer compiled and one failed (a real bug,
  below);
- each OS's CI job ran a hand-picked filter of platform tests, none of which
  shaped text.

## The contract

`PlatformTextSystem::layout_line` must, on every backend:

- take the line's base direction from its first strong character, looking past
  isolates (UAX #9 P2/P3);
- return glyphs left to right on screen, each with the byte index of the
  character it starts;
- place characters where UAX #9 puts them (L1/L2), trailing whitespace included;
- keep every glyph inside `[0, width]`.

`crates/text-conformance` (`quill-text-conformance`) writes the contract once,
with mixed Hebrew/Arabic/Latin lines, digits, isolates, emoji, niqqud, tabs and
line separators, and checks it against the real engine of the OS it runs on
(`tests/backends.rs`: CoreText, DirectWrite, cosmic-text) and against
`NoopTextSystem`. It also checks the line-level consumers that broke on
right-to-left glyphs: caret positions and hit tests (`LineLayout::x_for_index`,
`closest_index_for_x`) and decoration colors and underlines
(`DecorationPlan`).

CI runs the suite and gpui-base's whole test suite on Linux
(`linux-fmt-clippy-test`), Windows (`windows-check`, which now also runs
`direct_write::tests`) and macOS (`macos-text`, on every PR).

## What changed

- gpui-base is a workspace member. Its benchmark-only dev-dependencies and the
  two benches are gone; two tests were updated to the current `wrap_rows`.
- `NoopTextSystem` lays text out in UAX #9 visual order, so every GPUI test sees
  right-to-left text the way the real engines draw it.
- GPUI: a line whose glyphs are not in byte order paints each glyph in the color
  of the decoration run that holds it, with underline, strikethrough and
  background spans over those glyphs (`text_system/bidi_paint.rs`). A link in
  the middle of a Hebrew sentence used to take the color of the sentence's end.
  Its hit tests work from character clusters with a direction, so the caret of
  a right-to-left character is its right edge.
- CoreText: the trailing whitespace of a right-to-left line moves from x < 0 to
  the line's left end. UTF-16 index lookup steps back instead of restarting.
- cosmic-text: glyphs are handed over left to right (cosmic lists a
  right-to-left line from its right end).
- DirectWrite (on top of #1490): the runs and their levels come from UAX #9
  (unicode-bidi) and DirectWrite shapes each single-direction run, because its
  own bidi pass did not isolate FSI…PDI (a colon after an isolated Hebrew name
  was drawn left of it). U+2028 and U+2029 go in as zero-width spaces, so a line
  keeps one base direction instead of splitting; tabs in a right-to-left run are
  mirrored like glyphs; index lookup steps back instead of restarting.
- The input engine: an arrow key that arrives before the first layout (say,
  right after a draft is restored) moves the caret by the paragraph's own
  direction. It used to move the wrong way in a Hebrew paragraph.
- gpui-pre-macos declares the `UNNotificationAttachment` feature it uses; it
  only compiled before because Quill turned that feature on.

- The composer: a double-click selects the whole Hebrew or Arabic word (UAX #29
  word boundaries replace the old character classes). The caret stops only on
  grapheme boundaries, never inside an emoji or between a letter and its
  points, and a custom emoji is one stop. Arrow keys cross the rows of a
  wrapped paragraph by entering the facing edge of the next row, and a word
  step keeps צה"ל and acronyms whole. A one-line field (search, rename) lays a
  Hebrew value out right to left. A rich message's direction comes from its
  first block with text, and an elided placeholder drops whole graphemes.
- GPUI wrapping: a line whose glyphs are not in byte order breaks into rows in
  typing order. Each row is shaped on its own in the paragraph's direction
  (with an RLM or LRM in front when the row's own first letter would pick the
  other one), and painting and hit tests go row by row. Before, a wrapped
  Hebrew label broke the visual line, so its rows read bottom to top. The text
  contract now checks wrapping on every backend.
- The window layout stays left to right on every system (as in tdesktop, where
  only an RTL language pack mirrors it). A Hebrew or Arabic macOS locale used
  to mirror Quill's kit layout.
- A chat-list preview with pictures in it (custom emoji, emoji sprites) is a
  row of separate pieces, which drew in typing order, so a Hebrew preview read
  backwards. A right-to-left line now places its pieces in UAX #9 visual order
  (`chatlist_style::visual_pieces`), split where the direction changes, and its
  logically last text is the one that shrinks to the ellipsis.
- Names dropped into a sentence (service lines, the typing line, the sender
  prefix a left-to-right chat-list preview draws on its own, pinned excerpts)
  go inside a
  first-strong isolate when they hold right-to-left letters, as tdesktop's
  `WrapRtl` does, so a Hebrew name no longer pulls the colon, the "and" or
  the quote marks beside it to its side. A right-to-left preview and a
  notification body keep tdesktop's single "from: text" string, whose first
  letter picks the line's direction.

## Still open

The full register (backends B1–B18, composer C1–C14, display D1–D12) is
tracked in the sweep's findings register. Not done here: C10, C13 and C14 in
the composer, and D5–D12 on display surfaces. Owner decisions
pending: arrow keys stay visual (tdesktop moves logically); chat-list
previews follow tdesktop's left alignment.
