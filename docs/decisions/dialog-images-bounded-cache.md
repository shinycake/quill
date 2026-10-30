# Pictures in dialogs go through the bounded image cache

Status: done. Found in the 2026-10 memory sweep.

## Problem

`QuillApp` renders inside `image_budget::CacheScope`, so its path images
load through `BoundedImageCache`, which evicts what is off screen. Kit
dialogs render in the window's root layer above `QuillApp`, outside that
scope. Their images went to GPUI's own asset cache, which keeps every image
for the life of the app, decoded in RAM and uploaded to the atlas.

The sticker set box draws each sticker from its file (512 px WebP for a
static pack), so every pack opened kept all its stickers: about 1 MB of RAM
and 1 MB of GPU memory each. On a Mac, after the box closed and 30 s of
idle, both copies of the demo's sticker image were still held: the box's
copy and the history's own copy through the bounded cache.

## Fix

The base kit dialog (`gpui-base`'s `Dialog`) lays its popup out in place
but prepaints and paints it in a `deferred` pass, after the rest of the
window, where no scope is in force. A scope around the dialog layer covers
only the layout: a first try wrapped the whole window surface in a `Root`
plugin, and the review found that each picture then decoded twice, once
into the bounded cache during layout and once into GPUI's cache to paint,
which still kept it for good.

The scope has to sit inside the deferred subtree. Vendored `gpui-component`'s
`Dialog` gains `wrap_surface`, which wraps the dialog's surface (everything
above the backdrop) before it goes to the base dialog.
`src/ui/dialog_image_cache.rs` (`cover`) wraps the surface of every dialog
the shell opens in the main window in a `CacheScope` over the main window's
cache (`QuillShell::sync_kit_dialogs_in`). `CacheScope` passes its child's
layout through, so the dialog lays out as before. The call sits in the shell,
a hotspot file, because that is the one place every kit dialog opens; the
commit that adds it carries a `Hotspot-change` trailer.

Dialog pictures use no slice, so they count as visible for one frame after
they were last painted, like anything else `QuillApp` draws outside a
slice. They are evicted once they are off screen and the cache is over
budget, or at the idle trim.

Not covered:
- A dialog in a chat window keeps GPUI's cache: the bounded cache counts the
  main window's frames only.
- `deferred` content inside a dialog (a popover or menu) lays out inside
  the dialog's scope but paints in a later deferred pass, outside it, so a
  picture there can decode twice and keep GPUI's copy (the title emoji
  panel of the group edit box shows custom emoji set icons that way).
- Alert dialogs (`open_alert_dialog`) carry text only.

## Tests

`dialog_image_cache::tests::a_dialogs_pictures_are_painted_from_the_bounded_cache`
opens the sticker set box demo and checks that each sticker picture is
loaded through the bounded cache and that this copy is the one in the
window's atlas, so the dialog painted it from there. It fails without the
wrapper.
