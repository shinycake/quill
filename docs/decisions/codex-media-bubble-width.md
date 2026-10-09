# Media decides the bubble width

## What tdesktop does
`Photo::countOptimalSize` / `countCurrentSize` (history_view_photo.cpp) size a
photo from its dimensions (at most `maxMediaSize`), never below
`historyPhotoBubbleMinWidth` or the width the bottom info needs
(`minWidthForMedia`). The bubble takes the media's width; the caption is laid
out at that width and wraps, the reactions block wraps into rows within it and
the date/views/signature info goes to its own line when it does not fit.
tdesktop additionally lets a long caption widen a photo up to `msgMaxWidth`
and stretches the photo to match. The owner's rule is stricter (text never
widens the bubble beyond the media), so Quill keeps the media width.

## What changed
- `MessageChrome.media_width` (src/ui/synthetic.rs): when set, the bubble gets a
  fixed outer width of the media plus the bubble chrome
  (`bubble_outer_width`), so captions, reaction chips and the footer wrap to it.
- `media_content_width` (src/ui/message_media.rs): media width, at least 100 pt
  and at least the footer reserve for tiny media.
- `single_media_width`: display width of photo / video / GIF; albums use the
  mosaic box width. Text-only bubbles, documents and link previews are
  unchanged (tdesktop sizes those from their text).
- RTL captions are untouched: wrapping still goes through the same text
  elements, only the container width changed.

## How verified
Unit tests for the width helpers; `--screenshot-demo ready-showcase` with
QUILL_DEMO_SHOWCASE=channel and =chat in light and dark, before and after.
