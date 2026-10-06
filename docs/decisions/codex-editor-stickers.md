# Stickers and emoji in the photo editor (tdesktop)

tdesktop's photo editor lets you lay stickers over the picture, then
move and resize them. You asked for emoji as well.

The editor (#405) gains a **Stickers** mode:
- **Emoji row:** 24 common emoji. Each is drawn by the system's color
  emoji font: AppKit draws the string into an offscreen `NSImage`, read
  back as PNG (`editor_art::rasterize_emoji`). GPUI's glyph rasterizer is
  crate-private.
- **Sticker row:** your recent and favorite stickers (one row).
  `ConnectDriver::fetch_editor_stickers` loads them without opening the
  panel, and missing thumbnails download in the background. A sticker
  goes on as a still picture:
  - WebP through the image crate (its `webp` feature is now on);
  - Lottie as the first rlottie frame, un-premultiplied;
  - video as the first ffmpeg frame.
  Transparent margins are trimmed so handles hug the picture.
- **Placing and editing:** a picture is placed at a third of the crop's
  width in its middle and selected. Drag to move, drag the bottom-right
  handle to resize, and Delete removes it. Rotate and flip turn placed
  pictures with the photo and keep their on-screen size (`Placed`,
  unit-tested).
- **Done:** composites the pictures over the drawing at full resolution
  before cropping.

Verified live in Saved Messages: placed, moved and resized an emoji and
sent the result; placed a sticker from the recent row.
