# Stickers, avatars and fixed-size images display correctly

## Problems (seen in a live session)
- **Stickers never rendered.** TDLib downloads stickers, thumbnails, profile
  photos and wallpapers into its *database* directory (`tdlib/stickers`,
  `tdlib/thumbnails`, …), not `files_directory`. The display sandbox only
  allowed `files/`, so every downloaded sticker was rejected and showed
  "not downloaded". Chat avatars only worked because they skip the sandbox.
- **Images overflowed their boxes.** For every `img` GPUI copies the image's
  own aspect ratio into the layout style. When a fixed `w`/`h` box has a
  different ratio, layout derives a minimum height from that ratio and makes
  the image taller than its box. A 240×140 link-preview thumbnail grew past
  its card and covered the title and timestamp. Photo bubbles and the viewer
  could overflow the same way.
- The sticker placeholder text overflowed its 128pt box.

## Decision
- `ConnectDriver::tdlib_media_roots` lists `files/` plus TDLib's four
  database media folders, and the display sandbox uses it. The database
  itself (`db.sqlite`, `td.binlog`) stays outside.
- Every fixed-size `img` pins its aspect ratio to its box
  (`.aspect_ratio(w / h)` or `.aspect_square()`), so `ObjectFit` decides how
  the picture fills the box.
- The sticker placeholder is a 128pt square with the sticker's emoji, the
  same size as the sticker, so the row doesn't jump when the image lands.

## Not yet
- Video (WEBM) stickers show their still thumbnail; they don't animate.
- Account export still copies media from `files/` only.
