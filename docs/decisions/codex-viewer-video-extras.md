# Viewer video extras: share at time, quality, rotate

Parity item `parity:viewer-video-extras`.

## What tdesktop does

`media/view/media_view_overlay_widget.cpp`:

- **Copy Frame**: the context menu says "Copy Frame" instead of "Copy" when
  a video is streaming (`lng_mediaview_copy_frame`).
- **Share at time**: on a forwardable video message the menu shows "Share at
  M:SS" (`canShareAtTime`, live-updated every 120 ms). It pauses the video and
  opens the share box titled with that time; the box's copy-link button calls
  `CopyPostLink` with the video timestamp, and is offered only for public
  channel posts (`hasDirectLink && isBroadcast && hasUsername`).
- **Quality**: `playbackControlsQualities` lists the original plus the
  alternative renditions (`resolveQualities`), one entry per height; the
  choice is saved as the `videoQuality` setting (auto or a height) and applied
  to every video opened afterwards.
- **Rotate**: `playbackControlsRotate` turns the picture 90 degrees per press
  and remembers it per document.

## What Quill already had

Copy Frame (`copy_viewer_frame`) and photo rotate/flip. Both stay; Copy Frame
now copies the rotated frame.

## What changed

- `messageVideo.alternative_videos` is parsed (`VideoContent.alternatives`,
  new `telegram/envelope/video_alternatives.rs`).
- `media_viewer/quality.rs`: renditions list (short side, one per height,
  original wins, h264 preferred), pick by saved preference, per-viewer apply.
  The preference lives in `MediaPrefs.video_quality` (0 = Auto).
- A quality popover button in the player panel when a video has two or more
  renditions; choosing one restarts the clip at the same position. The saved
  choice applies when stepping to another video.
- "Share at M:SS" context-menu item (Chat source, videos, not protected). It
  pauses, closes the viewer and opens the forward picker; the picker's "Copy
  link" then sends `getMessageLink` with `media_timestamp`
  (`telegram/requests/messages/link_at.rs`, `connect/message_link_at.rs`) and
  the usual clipboard hand-off.
- "Rotate" for playing videos whose frames Quill decodes itself
  (`ui/media_viewer/video_extras.rs`); one oriented frame is cached.

## Differences from tdesktop

- The macOS system player draws into its own surface, so Rotate is hidden
  there (it works on the decoded-frame path used elsewhere and for GIFs).
- Quill has no username data per channel, so the picker's "Copy link" is
  offered by the existing share box rules; a private channel gives TDLib's
  private link, and the existing note says so.
- The share box title does not carry the time, and rotation is per viewing,
  not remembered per file.

## Verification

Unit tests: quality list/pick/apply, alternative parsing, timestamp request
JSON, rotation state and frame turn. Visual: temporary fixture with an
alternative rendition in the `ready-video-pip` demo showed the quality
button ("180p") and a rotated frame; the fixture edit was not committed.
Not verified live: the real `getMessageLink` answer and a real alternative
download.
