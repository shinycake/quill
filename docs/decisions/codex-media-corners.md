# Media corners follow the bubble

## What Telegram Desktop does
Media fills the bubble edge to edge (no inset). Its corners take the bubble's
`BubbleRounding`: the large radius (16 px) on edges free of other content, the
small radius where a caption, header, sender name or neighbouring album tile
touches the picture.

## What was wrong in Quill
Photos, videos and GIFs sat on a 4 px inset with a fixed `rounded_md`, so their
corners were tighter than the bubble's (`radius_2xl`). Album tiles had no
rounding at all (GPUI clips children to rectangles only).

## What changed
- `MediaCorners` (message_media.rs) carries per-corner radii. `in_bubble`
  derives the large radius from the theme (`radius_2xl` minus the 1 px bubble
  border); free edges get it, others the small 6 px radius. `tile` handles
  album tiles by mosaic position.
- Photo/GIF/video images, placeholders, spoiler covers and minithumbnails round
  themselves with per-corner radii.
- Native video surfaces cannot be clipped, so `inline_video::corner_mask`
  overlays a cached corner-cutout image in the bubble color (unit tested).
- The media-led bubble inset (`p_1`) is now `p_0`; captions get `py_1` so text
  keeps its spacing. Albums use the same corners per tile.
- Sponsored/game media keep the small radius.

## Verification
`cargo check --features ui`, gate OK. demo-capture screenshots of
`ready-media`, `ready-albums`, `ready-video` show the picture corners matching
the bubble's, square at caption edges, and outer-only large corners on albums.
Not verified: live inline video mask (needs a real playing clip), reply/forward
header bubbles and inline-keyboard cases, spoiler dust specks at corners (still
painted rectangularly).
