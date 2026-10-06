# Send box options and spoiler media (tdesktop)

Telegram Desktop's send box (`SendFilesBox`) lets you choose, before
sending:
- **"Send without compression":** photos and videos go as files. It's a
  switch for the whole send, since an album can't mix files and media.
- **"Hide with spoiler":** per photo or video, as `has_spoiler`.

Quill only sent media compressed, never with a spoiler. Now:
- the attachment tray has "Send without compression" whenever a
  photo/video file is attached (`ComposerAttachment::set_send_as_files`
  flips every attachment to a document and back; unit-tested);
- each photo/video tile has a spoiler toggle in its actions slot. The
  single and album send paths set `has_spoiler` on the content;
- an image going as a file still previews as the picture in the tray.

**Spoiler media in the history.** Quill showed spoiler photos as a flat
"Photo (spoiler) — not downloaded" box, and spoiler videos/GIFs as a
placeholder. tdesktop shows a soft preview under drifting dust and
reveals on click. Now `spoiler_cover` draws the inline minithumbnail
(already blur-like at its size) under a shade and deterministic white
dust. A click reveals the media for the session (`spoiler_revealed`),
after which it renders as normal media; a revealed photo starts its
download at once. The dust is static: animating it would redraw the
window every frame (see the frame-clock notes).

The photo editor (crop, rotate, draw, stickers) is the next slice.

Verified live in Saved Messages: sent a spoiler photo and an
uncompressed file; covered and revealed spoiler.
