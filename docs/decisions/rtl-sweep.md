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
- DirectWrite (on top of #1490): U+2028 and U+2029 go in as spaces, so a line
  keeps one base direction instead of splitting; tabs in a right-to-left run are
  mirrored like glyphs; index lookup steps back instead of restarting.
- The input engine: an arrow key that arrives before the first layout (say,
  right after a draft is restored) moves the caret by the paragraph's own
  direction. It used to move the wrong way in a Hebrew paragraph.
- gpui-pre-macos declares the `UNNotificationAttachment` feature it uses; it
  only compiled before because Quill turned that feature on.

## Still open

The full register (backends B1–B18, composer C1–C14, display D1–D12) and the
two follow-up PRs (composer and inputs; display surfaces) are tracked in the
sweep's findings register. Owner decisions pending: arrow keys stay visual
(tdesktop moves logically); chat-list previews follow tdesktop's left alignment.
