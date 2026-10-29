# Pictures Quill keeps in its own state leave the GPU when replaced

Status: done. Found in the 2026-10 memory sweep (see
`sticker-frame-leak.md` for the main leak of that sweep).

## Problem

GPUI keeps an `Arc<RenderImage>` that was painted once in the window's
sprite atlas until someone calls `drop_image`. Quill's caches retire the
images they evict through `image_budget::retire_all`, but a few screens
kept their own picture in a plain `Arc<RenderImage>` field and replaced or
dropped it without retiring it. The pixels left RAM with the last `Arc`;
the atlas copy stayed on the GPU until Quill quit.

| Where | Kept on the GPU for good |
| --- | --- |
| Media viewer, Rotate or Flip on a photo | the photo at full size, every turn |
| Photo editor, Rotate, Flip, close | the preview (up to 1600 px) and each placed sticker, every turn |
| Video editor, moving the cover, close | the still (up to 1600 px) for each cover picked, the strip frames on each strip resize |
| Story cover picker | the preview for each time picked, the strip frames on close |
| Passkey sign-in with a phone | each QR code shown |
| QR login | each QR code (the link changes while the screen waits) |

## Fix

`src/ui/owned_image.rs` adds `OwnedImage`, an `Arc<RenderImage>` that hands
itself to `image_budget::retire_all` when it is dropped, so replacing the
field or closing the screen retires the old picture, and the sweep drops it
from the atlas once no frame can still show it. It derefs to the `Arc` for
painting and is not `Clone`, so a copy for painting is a plain `Arc` and
never a second owner. The fields in the table hold an `OwnedImage` now.

It is not `Send`, so it stays on the UI thread: retired images wait in a
thread-local list that only the UI thread sweeps. The decoders that run on
workers still return a plain `Arc`, and the UI thread wraps it where it
stores it.

## Tests

`owned_image::tests` checks that a replaced picture comes back from the
sweep and nothing holds it after, that the new one stays, and that a picture
still being painted stays until that copy is gone. Both fail without the
retire on drop.
