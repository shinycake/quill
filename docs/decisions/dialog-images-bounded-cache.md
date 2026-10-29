# Pictures in dialogs go through the bounded image cache

Status: done. Found in the 2026-10 memory sweep.

## Problem

`QuillApp` renders inside `image_budget::CacheScope`, so its path images
load through `BoundedImageCache`, which evicts what is off screen. Dialogs,
sheets and notifications render in kit `Root` overlays above `QuillApp`,
outside that scope. Their images went to GPUI's own asset cache, which
keeps every image for the life of the app, decoded in RAM and uploaded to
the atlas.

The sticker set box draws each sticker from its file (512 px WebP for a
static pack), so every pack opened kept all its stickers: about 1 MB of RAM
and 1 MB of GPU memory each. On a Mac, after the box closed and 30 s of
idle, both copies of the demo's sticker image were still held: the box's
copy and the history's own copy through the bounded cache.

## Fix

`src/ui/window_image_cache.rs` registers a `Root` plugin whose `decorate`
wraps the main window's whole surface, overlays included, in a
`CacheScope` over the main window's cache. `QuillApp` marks its window and
cache each frame (`cover`). Other windows (chat windows) keep GPUI's cache
as before.

Overlay pictures use no slice, so they count as visible for one frame after
they were last painted, like anything else `QuillApp` draws outside a
slice. They are evicted once they are off screen and the cache is over
budget, or at the idle trim.

Still outside the scope: `deferred` content (popovers and menus), whose
prepaint and paint run after the scope has ended.

## Tests

`window_image_cache::tests::a_dialogs_pictures_load_through_the_bounded_cache`
opens the sticker set box demo and checks that every sticker picture loads
through the bounded cache. It fails without the plugin.
