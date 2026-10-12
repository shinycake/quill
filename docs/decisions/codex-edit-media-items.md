# Edit Image and Edit Cover on your own media

Parity item `parity:menu-edit-media-items`.

## What tdesktop does

The three items are not in the message context menu. They live in the media
preview menu of the edit box (`EditCaptionBox::showMenu`, and
`MediaEditManager` for the inline composer edit), next to "Replace
attachment":

- "Edit Image" (`lng_context_draw`) opens the photo editor. It shows for a
  photo file, including the photo already on the message, unless it would be
  sent as a document.
- "Edit Video" (`lng_context_edit_video`) opens the video editor. It shows
  only for a prepared local video file (`PreparedFile::canEditVideo`), never
  for the video already on the message.
- "Edit Cover" (`lng_context_edit_cover`) asks for an image
  (`VideoCoverUploader::choose`, images filter) and uses it as the video
  cover. `CanEditVideoCover` requires a video with known dimensions in a
  channel, Saved Messages or a bot chat. "Clear Cover" appears once a cover
  is set.

All results are applied when the edit is saved, as one edit with new media
(`Api::editMedia`), keeping the caption.

## What changed in Quill

- The edit banner gets an "Edit media" menu before "Replace"
  (`src/ui/edit_media_tools.rs`). Entries follow the conditions above:
  `ComposerEdit::edit_media_tools` in `src/composer/edit_media_tools.rs`.
- Edit Image opens Quill's photo editor on the staged replacement, or on the
  downloaded original of the message photo (it starts the download and asks
  you to try again when the file is missing). "Done" stages the edited PNG as
  the replacement, so saving the edit sends `editMessageMedia` with the new
  caption. The photo's spoiler is kept. `photo_editor.rs` only gained a flag
  and a constructor; the PNG writer moved into the new module.
- Edit Cover picks an image, stages it, and the edit sends
  `editMessageMedia` with `inputMessageVideo.cover`. Without a replacement
  video the existing video is resent by `inputFileId` with its duration,
  size, start timestamp and spoiler (`src/telegram/requests/edit_media_cover.rs`).
  With a replacement video the cover rides on that video. The driver repeats
  the peer rule and the image check, so a stale UI state cannot send a
  rejected request. "Clear Cover" only drops a staged cover; Quill does not
  parse the cover of an existing video, so it cannot offer tdesktop's
  server-side removal.
- The gate is local (outgoing, not scheduled, supported media, plus the
  cover peer rule). `messageProperties.can_edit_media` (TDLib 1.8.68) is not
  read: the existing "Replace" button already works without it, and TDLib
  rejects an edit it does not allow.

## Not done: Edit Video

tdesktop's video editor trims, picks the cover frame and sets quality, and
only works on a local file being sent. Quill has no video editor, and
inventing one (trim with ffmpeg re-encode, a timeline UI) is a feature of its
own. Edit Video is therefore not offered, and the parity item stays open for
it. Covers from a video frame are not offered for the same reason; Edit Cover
takes an image file as tdesktop's chooser does.

## How it was verified

- Unit tests: tool conditions (photo, photo as file, replacement photo, video
  with and without dimensions, animation, document, scheduled, cover peers,
  cover file check), request JSON for the by-id cover edit and the
  replacement-video cover, and a driver test that sends the edit through
  `edit_snapshot` (refused in an ordinary private chat, allowed in Saved
  Messages, refused for a non-image cover).
- A temporary demo capture (not committed) showed the "Edit media" button in
  the edit banner of a video in Saved Messages.
- Not verified against live Telegram: that the server accepts
  `inputFileId` plus a new cover for a video edit, and the photo editor
  round trip on a real downloaded photo. No real messages were edited.
