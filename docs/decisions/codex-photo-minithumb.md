# Photo minithumbnail previews

Undownloaded photos rendered as a flat gray box captioned "Photo 800×600 — not downloaded". `photo.minithumbnail` (a tiny inline JPEG TDLib sends with every photo) is now parsed into `PhotoContent`. The placeholder draws it cover-fitted to the photo's final frame; upscaling a ~40px JPEG reads as a soft blur, as in the official apps. The status ("Downloading…" / "Click to load") sits on a translucent pill over it. Secret and spoiler photos never show the preview; they keep their own placeholder labels. The image's GPUI id is a hash of its bytes, so re-renders reuse the decoded texture.

The media demo's pending photo now carries a 40×30 minithumbnail.
