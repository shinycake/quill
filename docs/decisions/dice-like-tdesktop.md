# Dice drawn like Telegram Desktop

Owner report (2026-10-21): a basketball dice sat in a full outgoing bubble with
a "Rolled 4" line, and a slot machine overflowed its bubble to the right and
bottom, covering its caption.

## Causes

- The no-bubble list in `session_history_row` had stickers and round videos
  but not dice, so dice got a bubble and the `result_line` caption under the
  picture. Telegram Desktop draws neither (`history_view_dice.cpp`).
- The slot machine box was 128 px, but each of its five layers went through
  `sticker_attachment`, which sizes by the sticker's own dimensions (512 →
  224 px) and adds a top margin. The layers spilled out of the box.
- The five clips of a slot machine (about 60 MB of frames at 224 px) did not
  fit the 48 MB sticker playback cache next to anything else. The cache
  refuses a clip when everything held was on screen in the last 500 ms, so
  some layers never decoded and the machine showed with reels or body
  missing.
- `initial_state` was dropped, so a dice whose value was still unknown
  showed the emoji and "Rolled 0".

## Decisions

- `DiceContent` keeps both states as `DiceStickers` (`Regular` or
  `SlotMachine` with five layers) and `animation()` picks what draws:
  `final_state` once TDLib sends it (plays once, rests on the value), else
  `initial_state` looping. tdesktop waits for the start animation's end
  before switching; Quill switches when the final stickers arrive.
- Every dice draws in one 112 px square (`maxAnimatedEmojiSize`,
  `bubble_layout::DICE_SIZE`) with `overflow_hidden`, each layer filling it.
  A layer with no animation and no still draws nothing; with no layer at all
  the glyph stands in. Nothing a layer draws can leave the square.
- Dice clips get their own playback cache (`PlaybackSize::Dice`, 224 px for
  2x screens, 96 MB, 16 clips), so a slot machine never competes with
  stickers for room.
- The result line is now only the square's accessible label.

## Verify

`--screenshot-demo ready-dice`: a basketball (value 4), a slot machine
(grapes, lemon, bar) and a die still rolling. It uses `dice-*.tgs` from
`docs/screenshots/fixtures` when present (not committed: Telegram's
animations, copied from a TDLib cache) and the demo sticker otherwise. Point
`QUILL_RLOTTIE_PATH` at an rlottie built with
`native/patches/rlottie-animated-shape-budget.patch`, or the reels and the
net drop out.
