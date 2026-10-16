# Memory bounds: mirrored stickers, minithumbs, background histories

Audit findings F7, F8 and F10 (perf sweep), each re-checked against main first. All three held.

## What tdesktop does

tdesktop keeps a bounded run of messages around the viewport and evicts images by use (`Data::Session` image
and `Ui::Animations` caches). Nothing grows with the length of a session.

## What changed

- F7 `ui/sticker_mirror.rs`: the flipped-sticker cache was bounded by count (16) only and held the source frames
  strongly, so frames the playback cache had evicted stayed alive. It now keys sources by `Weak`, drops sets whose
  source is gone, and counts bytes against the sticker playback budget (`PlaybackSize::Sticker.budget_bytes()`,
  48 MB) as well as the count. The newest set always stays.
- F8 `ui/mini_image.rs` (new): minithumbnails were `ImageSource::Image`, which GPUI's asset cache never evicts. The five
  call sites (message photo preview, picture-file preview, thumbed document, service-row photo, composer reply
  bar thumb) now use `image_budget::sized_image(SizedSource::Mini(..))`, the existing public API, so they leave with
  the rest of the bounded cache. `image_budget.rs` is untouched.
- F10: `upsert_message` now trims the history of a chat that is not shown (not the open chat, not a window chat) with the
  existing window cap, dropping the oldest messages and clearing `loaded_complete`. `media.files` is left alone on purpose: a file a message brought can also be shown by a non-message surface (sticker panel, emoji, avatars, backgrounds, story thumbs), and dropping its entry would blank that surface until TDLib resent it. The entries are small metadata.

## Verified

- Unit tests: `state::tests::history_window_cap_live` (cap on live updates, open chat untouched), `ui::sticker_mirror` (byte budget eviction, dead sources dropped, source not pinned),
  `ui::mini_image` (not an `ImageSource::Image`).
- Not measured on a running app: the memory numbers (~94 MB worst case for F7) are the audit's arithmetic.
