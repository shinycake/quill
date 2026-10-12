# Send box options and the video editor

Parity items `parity:composer-send-options` and (with Edit Video)
`parity:menu-edit-media-items`.

## What tdesktop does

`SendFilesBox` (with `menu_send.cpp`) offers, for the files in the attach box:

- "High Quality" for a compressed photo over 1280 px
  (`canUseHighQualityPhoto`). Photos are scaled to 1280 px on the longer
  side, or 2560 px with the option on (`PhotoSideLimit`).
- A GIF (`isGifv`, an animated `.gif` or a clip without sound) is sent as an
  animation, with a caption like other media. The video editor's GIF button
  mutes a video and sends it that way when it is not in an album.
- "Make This Content Paid" / "Change Price" (`EditPriceBox`, "Paid
  Content"): only in a channel the account posts to that allows paid media
  (`canPostPaidMedia`), for 1 to 10 compressed photos and videos. Empty or
  zero clears the price, above `stars_paid_post_amount_max` (10000) is an
  error. Spoilers are switched off while a price is set.
- "Edit Cover" / "Clear Cover" on a video in a channel, Saved Messages or a
  bot chat, and "Edit Video" (`canEditVideo`) on a compressed local video.
- The video editor: trim handles, a cover dot on the timeline, a quality
  slider (360p to 1080p and Original), a GIF button, and crop, rotate and
  flip. The file is encoded again when anything but the cover changed
  (`VideoEdited`); the cover frame goes out as the start timestamp
  (`videoCoverOffset = cover - from`).

## What changed in Quill

- Core: `composer/media_options.rs` (photo limits, `MediaSendOptions` on the
  snapshot, `AttachmentExtras` on each attachment, paid media rules, price
  parsing, ffmpeg downscaling), `video_edit.rs` (edits, clamp and cover
  math, quality levels, ffmpeg command lines, `render_edited` and
  `render_frame`), `telegram/requests/send_media_options.rs`
  (`inputMessageAnimation` for a local file, `inputVideo.cover` and
  `start_timestamp`, `inputMessagePaidMedia`), and
  `connect/messages/media_options.rs` (the send path: photo scaling, cover
  and start frame, GIF, paid media). `supergroupFullInfo.has_paid_media_allowed`
  is now read.
- A lone `.gif` goes out as a GIF with its caption; next to other files it
  stays a file. "Send without compression" switches it to a file and back.
- UI: `ui/send_box_options.rs` (High quality checkbox, "Make paid" button and
  the Paid Content box, "Edit video" and "Cover" on a video card, no spoiler
  button on paid media), `ui/video_editor.rs` (a dialog with the cover-frame
  picture, trim and cover sliders, quality and GIF), `ui/send_box_state.rs`.
  The edit box menu gets "Edit Video" for a staged replacement video.
- Photos are now scaled to 1280 px (2560 px with High quality) like
  tdesktop; before, the original file went to TDLib. When ffmpeg is missing
  or fails, the original is sent.

## Differences from tdesktop

- No crop, rotate or flip in the video editor, and no playback: the picture
  is the frame at the last moved handle.
- A soundless video is not turned into a GIF on its own; use the editor's
  GIF option or a `.gif` file.
- Clearing "Edit Cover" does not open tdesktop's photo editor on the cover
  image.
- The price limit is the fixed 10000 default, not the app config value.

## How it was checked

- Unit tests: edit and trim math, quality levels, ffmpeg arguments, a real
  ffmpeg trim, scale and GIF run on a generated clip, request JSON for
  animation, cover, start timestamp and paid media, driver tests on the
  recorded requests (GIF with caption, cover only where allowed, 1280 and
  2560 px photos, paid media accepted and refused), attachment rules for
  GIFs, and GPUI tests for the tray options and the price box.
- Screenshots of the tray (High quality, price, video buttons), the Paid
  Content box and the Edit Video dialog through a temporary demo (not
  committed).
- Not verified: sending to a real channel (nothing is sent to real chats).
