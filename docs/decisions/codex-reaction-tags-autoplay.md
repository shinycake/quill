# Saved Messages tags, custom-emoji reaction chips, autoplay prefetch

**Tags.** Reactions in Saved Messages are tags (`are_tags`). Telegram
Desktop draws a tag as just its emoji: no count, no reactor avatar. Quill
drew a reaction chip with your avatar. Tag chips now show only the emoji.

Custom-emoji reactions do render once the emoji's sticker resolves: the
open chat's reaction ids join the existing `getCustomEmojiStickers`
batch. Adding and removing a reaction updated the chip live. The case in
the report (a missing chip) was most likely the unresolved-emoji
placeholder, which is barely visible on a bubble-less sticker row.

**Autoplay regressions.**
- With the frame clock (#392), the inline-video tick only ran once a
  player had produced a frame, so a freshly opened player never got its
  second render. It now ticks while any inline player exists.
- A clip that's no longer local (TDLib dropped the file) never
  autoplayed. As in tdesktop, autoplay clips up to 20 MB are now fetched
  in the background (`download_file`, low priority, respecting
  `should_download`) and start once local.

Verified live in Saved Messages: tag chip, live add/remove of a tag, the
video downloading and autoplaying.
