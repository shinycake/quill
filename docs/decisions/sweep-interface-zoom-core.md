# Interface zoom and literal px values (sweep finding 41)

## Finding
Finding 41 said 1,545 literal `px()` values and 128 raw colours in `src/ui` keep interface zoom from scaling consistently (fixed chat row height, composer height).

## What interface zoom does
Zoom is not rem scaling. `src/ui/interface_zoom.rs` wraps the platform so every window reports `scale_factor = display scale x zoom` and a logical size divided by zoom (see `codex-interface-scale-full.md`). A `px(64.)` is therefore 64 logical pixels at any zoom, and those logical pixels get bigger on screen. The kit's rem size stays at 16 px. Fixed `px()` values scale exactly like rem values and like text.

## Check
`ready-chats` captured at a fixed 1200x800 window (scratch dir, not committed):
100%, 125%, 150% light, and 150% dark. At 125% and 150% the chat list rows (height, avatar, paddings, unread badge), conversation header, bubble paddings and radii, date chip, and composer all grow by the same factor as the text. Nothing is clipped or overlapping; the proportions match 100%. The hover icons beside a bubble in the 125% and 150% frames come from the scripted pointer position.

## Decision
The premise of finding 41 does not hold, so this PR migrates no code. Rewriting about 1,850 literals to rem would change nothing visible at any zoom, would risk 1 px drift at 100%, and would touch many hotspot files. No literal-px guard was added either, since there are no migrated files to scope it to.

Real zoom limits stay where the earlier decision doc lists them: IME candidate window position on macOS and Windows, fixed-size sticker/emoji decode (softer at high zoom), and kit components that draw in physical pixels.

## Counts in `src/ui` (unchanged by this PR)
- `px(` calls: 2,816, of which 1,851 have a numeric literal argument.
- Raw colour literals (`rgb(0x`, `rgba(0x`, `hsl(`, `0xRRGGBB`): 427 matches by this grep. Colours are a theme and contrast concern, not a zoom one.

The counts differ from the sweep's (1,545 and 128) because the grep patterns differ; neither is a zoom defect measure.

## Not verified
Linux and Windows hardware; 200% zoom (covered by the earlier PR); signed-in chats with media.
