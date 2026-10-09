# Story albums / story page follows the theme

## What tdesktop does
Every panel and box in tdesktop is painted from the active palette (`boxBg`,
`windowFg`, `windowSubTextFg`, `windowBgOver`); nothing is hard-coded to a
dark island. Only the media viewer and story player stay fixed dark.

## What changed
`src/ui/story_albums.rs` (the story page overlay: albums, chat-page stories,
archive) used about 40 hard-coded dark GitHub-style colors, so the panel was a
dark island in the light theme. They now use theme tokens:

| Was | Now |
| --- | --- |
| `0x161b22` panel background | `theme.popover` |
| `0x21262d` row background | `theme.secondary` |
| `0xffffff` text and ghost button text | `theme.foreground` |
| `0x9aa0a6` hint and status text | `theme.muted_foreground` |

Kept fixed: the backdrop scrim `rgba(0x000000e6)` behind the modal. It is a
scrim, not content, and reads correctly over either theme.

## Other literals checked, no change
- `src/ui/appearance.rs:271`: white text on bubbles only when the colored
  bubble style is on (text sits on the accent bubble fill); plain mode already
  uses `theme.foreground`.
- `src/ui/appearance.rs:484`: fully transparent border on the unselected
  swatch; not a color, only reserves the border width.
- `src/ui/synthetic.rs:146`: the pre-login placeholder look, intentionally
  fixed (white on colored bubble).

## Verification
Demo capture, `ready-story-albums`, 1200x1000, both `QUILL_DEMO_THEME=light`
and `dark`, before (origin/main) and after:
`story-albums-{before,after}-{light,dark}.png` in the session scratchpad
(`.../scratchpad/cap/`). Before in light: dark panel with white text; after: light
panel with dark text and secondary rows. Dark theme looks equivalent to before.
`gate.sh` passes.
