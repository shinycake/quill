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

## Second round (owner, 2026-11-22)

The owner's rule for Hebrew and right-to-left text: do what reads correctly,
not what Telegram Desktop does. So the two pending decisions are settled
that way: arrow keys stay visual, and a Hebrew chat-list preview stays
right-aligned in reading order (tdesktop forces it left to right). This round
closes the rest of the register:

- D7: "Forwarded from", the name and "via @bot" are separate boxes (the name
  is a button); the row takes the direction of the whole sentence with the
  name isolated, so a Hebrew label puts them right to left. "via {bot}" is
  `lng_inline_bot_via`, no longer hard-coded English.
- D8: a rich list reads in the direction of its first item with text: a
  Hebrew list has its bullets and numbers on the right, written ".1" as a
  right-to-left reader sees "1.", and its items rest against them.
- D9: a caption with right-to-left text under a wider photo spans the bubble,
  so an English paragraph starts at the left and a Hebrew one at the right.
- D10: the bot stream reveal sweeps each row from its own reading edge (it
  used the first row's direction and the widest row's width for all rows).
- D11 and D2 leftovers: names are isolated in the window title, the group
  call chat (whose lines now align by the message's own direction) and the
  archive row, whose "and N more chats" is `lng_archived_last`; the reply
  header lays its name out by direction.
- D12: search highlights ignore niqqud and harakat on both sides ("שלום"
  finds "שָׁלוֹם") and are found in the filtered text that is drawn.
- C10: Backspace after a Hebrew or Arabic point removes only that point.
- C13: Hebrew letters under a left-to-right override (LRO) keep typing order
  in the composer: each is shaped on its own, so the platform cannot reverse
  them again.
- C14 stays as is: a quote or code block's lines are separate paragraphs, and
  each takes its own direction, as UAX #9 says.
- D5 is covered by the isolated sender prefix (round one).
