# History media decodes at the size it is drawn

Follow-up to `codex-perf-round3` ("History photos still decode at file
size"). Photos, posters, album tiles, link-preview images and sticker
stills in the history now go through `image_budget::sized_media`: decoded on
the background executor so the picture's shorter side is the size it is drawn
at times the window scale factor (never enlarged), in the bounded cache, and
evictable like avatars. The media viewer, photo editor and story viewer are
untouched: they still load the file at full size (`img(path)` through the
cache's full-size loader) and let it go through the normal LRU / idle trim
once closed.

## What changed

- `sized_media(path, frame, dims, fit)` (`ui/image_budget.rs`): the wanted
  edge is `scale * min(image side)` for a `Cover` frame (photos, posters,
  tiles, link previews, audio covers) or a `Contain` one (stickers), from the
  picture's own size when TDLib gave it (`shorter_side_in_frame`), else the
  bound over all aspects. GIF files keep GPUI's loader (it plays them).
  Used by `photo_attachment`, GIF and video posters, album tiles
  (`history.rs`), `sticker_attachment`, audio covers, `preview_thumb`.
  Spoiler / secret photos never load the file; blurred previews already
  come from the inline minithumbnail.
- Size bucketing (`bucket_edge`): the edge is rounded up to 32 px (to 256),
  64 px (to 1024) or 128 px steps, so frames a few pixels apart, or a window
  that moves between two displays of similar scale, ask for the same decode.
- Hysteresis (`pick_variant`): when the same picture is already decoded at
  up to 1.5x the wanted edge, that decode is reused (a shrink doesn't decode
  again); when a new size has to decode, the closest loaded size stands in,
  so a resize or a move from a 1x to a 2x display doesn't blank the picture.
  Bubble frames are fixed in points (`media_frame`: up to 360x400), so in
  practice re-decoding happens when the scale factor changes.
- Memory accounting (`atlas_cost`): an entry now counts its pixels, or, for an
  image that fits an atlas texture, its share of one. GPUI's atlas textures
  are 1024^2 and freed only when empty: a 768x576 image takes a whole 4 MB
  texture for 1.7 MB of pixels. The first version counted pixels only and
  the atlas grew to 121 MB (28 decoded images, 47 MB) before eviction; with
  the share counted the same budget holds 12 such images and the atlas stays
  at 55 MB. Avatars (~92 px) and chat-list thumbnails pack tightly, so their
  cost is unchanged.
- `decode_sized` shrinks a photo in three channels and widens only the small
  result: no 20 MB RGBA copy of a 2560 px photo per decode in flight (peak
  footprint while scrolling dropped from 351 MB to 292 MB in the 2560 run).

## Fixture

`QUILL_DEMO_STRESS_PHOTOS=<dir>`: one photo message per `photo-<i>.jpg` (any
size) appended to the open demo chat, `<dir>` added to the demo media roots.
The 200 x 2560x1920 set was made with a throwaway generator (gradients plus
2 px stripes and fine rings as a sharpness probe).

## Measurements

Release build, M1 Pro, 1200x900 window, `QUILL_DEMO_STRESS=50,0`,
`QUILL_DEMO_STRESS_PHOTOS=<200 photos>`, `QUILL_ASSUME_ACTIVE=1`,
autoscroll top to bottom through all 200 photos (~25 s,
`QUILL_DEMO_AUTOSCROLL=800,450,40,100000,1600`), then idle ~45 s; `vmmap
--summary`: physical footprint, atlas = `IOAccelerator (graphics)`. The
history ends with 3 photos on screen, which stay decoded.

| Photos | State | Before: footprint / atlas | After |
| --- | --- | --- | --- |
| 200 x 2560x1920 | Peak while scrolling | 340 / 88 MB | 292 / 55-59 MB |
| 200 x 2560x1920 | End of scroll, 3 photos on screen, idle 45 s | 217 / 69 MB | 135 / 22 MB |
| 200 x 2560x1920 | Idle trim log | 3 images, 56.2 MB (the visible ones, full size) | 12 -> 3 images, 48.0 -> 12.0 MB |
| 200 x 1280x960 (TDLib's largest) | Peak while scrolling | 230 / 66 MB | 224 / 59 MB |
| 200 x 1280x960 | End of scroll, idle 45 s | 137 / 25 MB | 120 / 24 MB |

At TDLib's real size the saving is small because of the atlas: a 1280x960
image already gets a texture of its own (no waste), and the 768x576 decode
takes a whole 1024^2 texture too. The wins are the 2560 px and larger files
(Telegram sends up to 2560 for some "document as photo" and forwarded
pictures), the 4x smaller heap per visible photo, and no transient 20 MB
buffers. Packing more than one photo per atlas texture would need smaller
decodes (<= 512 px, soft on Retina) or GPUI-side atlas compaction.

Visual: demo captures (2x, 2560 px photo in a 360x270 bubble) before/after,
crop 700x500: mean absolute difference 2.2 / 255, max 24, no edge softening
(the rings stay crisp). The 2 px checker (half the crop) is moire-aliased in
the old path (GPU bilinear from 2560 to 720) and a clean gray in the new one,
so the new picture is, if anything, cleaner. The capture tool renders at 2x
only; 1x uses the same math (`display_edge` of the logical size times the
window scale factor), covered by the `frame_edge_follows_the_fit` and
`edges_round_up_to_steps` tests.

## Risks

- A photo decodes again when it comes back after eviction (as avatars do)
  and shows the minithumbnail / placeholder until it lands.
- Zoom and the viewer use the file at full size and are separate cache
  entries; opening the viewer on a photo decodes it a second time.
- `atlas_cost` changes the byte budget meaning slightly for every image
  (pixels or atlas share, whichever is larger).
