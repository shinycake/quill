# Pictures left behind after use

Status: done. Found in the 2026-10 memory sweep, after the leaks in
`sticker-frame-leak.md`, `owned-images.md` and
`idle-wallpaper-and-emoji-trim.md`. Pictures in dialogs have their own note,
`dialog-images-bounded-cache.md`.

## Images of chats left, while Quill is in the background

The idle trim (`BoundedImageCache::idle_trim`) kept up to 8 MB of images
beyond the screen, so scrolling back a little shows them at once. That 8 MB
is counted as atlas share, and a photo sits in a 4 MB texture of its own
once its neighbours leave, so on a Mac the trim left two photos of a chat
already left (939x704 each) plus their RAM copies, held until something
pushed them out.

While the main window is in the background, the idle trim now keeps only
what is on screen. Going to the background counts as activity, so the trim
runs two seconds later. While Quill is in front, the 8 MB stays. The cost:
after coming back, a picture scrolled back into view decodes again (a frame
or two of its placeholder), as it would after any chat switch.

## Kept pictures off screen, while Quill is in front

In front, the idle trim still keeps 8 MB of pictures beyond the screen. On a
Mac, after a full browse, that was two photos of a chat already left, each
alone in a 4 MB atlas texture: 8 MB of GPU memory on top of their 5 MB of
decodes, for as long as nothing pushed them out.

The idle trim now keeps the decode of each picture it keeps off screen but
takes it out of the atlas (`drop_image`). Its texture goes once its last
tile does. Scrolling back to it uploads the same decode again while the
frame paints, so it shows at once with no placeholder; only a picture the
trim evicted decodes again. What may be on screen keeps its tile.

## Small pictures in big textures

GPUI's atlas made every texture at least 1024 x 1024 (4 MB). Images get
textures of their own kind (`sweep-r3-browse-memory.md`), so the few small
pictures on screen after a browse, such as avatars and a sticker, kept 4 MB
textures that were mostly empty: about 8 MB on the Mac. A photo of 512 to
1024 px either shared one of them or got a 4 MB texture for itself.

The three vendored atlases (Metal, wgpu, DirectX) now start image textures
at 512 x 512 (1 MB); a picture larger than that still gets a texture of its
own size. Glyph textures stay at 1024 x 1024: text fills them. More small
textures mean a few more sprite batches per frame, one per texture, which
is cheap next to the frame. `image_budget`'s `atlas_cost` charges a small
picture its share of a 512 x 512 texture to match.

## Wallpaper renders nobody took

Wallpaper pictures render on a worker (`wallpaper_cache`); a finished result
waits in `Board::done` until a frame at that size takes it. A result whose
size the window had already left (a resize that paused long enough to
render, then went on before the next frame) was never taken: it stayed
until 48 of them piled up, each a full history picture in RAM (about 17 MB
for a 1200 x 900 area at 2x). A new request for a slot now drops that
slot's finished results along with its queued ones. Gallery thumbnails
share one slot and never replace each other, so theirs stay until taken.

## A turned photo after the viewer closes

Rotating or flipping a photo in the viewer makes a turned copy at full size
(`ViewerUi::rotated`, up to about 20 MB, and the same again in the atlas
once painted). Opening another item dropped it; closing the viewer did not,
so it stayed until the viewer opened again. `close_media_viewer` drops it
now, and `OwnedImage` takes it out of the atlas.

## The cache's index of decoded sizes

`BoundedImageCache::variants` lists the sizes each picture is decoded at, so
a nearby size can stand in. Eviction removed the decodes but left their
index entries, about 80 bytes per picture ever shown at display size. The
index now forgets them on eviction.

## Not covered

- Pictures drawn in `deferred` content (popovers, menus, the emoji bar)
  still load through GPUI's asset cache, which keeps them: their prepaint
  and paint run after every cache scope has ended, and a cached slice can
  replay them without a load, which the bounded cache would read as off
  screen.
- Chat windows draw outside the bounded cache too. The cache counts frames
  of the main window only, so it can't tell what another window shows.

## Tests

- `wallpaper_cache::tests::a_result_its_slot_moved_past_is_dropped`
- `media_viewer::actions::rotate_ui_tests::closing_the_viewer_lets_go_of_a_turned_photo`
- `image_budget::tests::evicted_sizes_leave_the_index`
- `image_budget::tests::idle_trim_takes_kept_pictures_out_of_the_atlas`
- `wgpu_atlas::tests::small_pictures_get_small_textures` (gpui-pre-wgpu, on
  lavapipe) and `metal_atlas::tests::test_small_pictures_get_small_textures`
  (gpui-pre-apple)

Each fails without its fix.
