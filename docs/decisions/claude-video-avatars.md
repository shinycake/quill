# Video profile photos (cover and viewer)

Parity: `parity:profile-video-avatar` (scout rows a10 and e7).

## What tdesktop does

- Profile cover (`Info::Profile::TopBar::updateVideoUserpic`,
  info_profile_top_bar.cpp): when the shown photo has video sizes and they
  have not failed, a `Ui::VideoUserpicPlayer` streams the **large** video
  (`PhotoData::createStreamingLoader` uses `PhotoSize::Large`), muted,
  looping, from `videoStartPosition()` (the main frame). The still is drawn
  until the player is ready. Frames are not marked shown while
  `isGifPausedAtLeastFor(GifPauseReason::Layer)`: a layer (box, media
  viewer) is over the section or the window is inactive. Saved Messages
  never plays one. A failed stream sets `setVideoPlaybackFailed` and the
  still stays.
- Media viewer (`OverlayWidget::displayPhoto`): a photo whose video can be
  played is streamed with `Mode::Video`, `loop = true`, no sound and no
  playback controls, from the main frame. "Save" writes the large video
  (`lng_save_video`); Copy still copies the photo.
- Emoji/sticker markup (`videoSizeEmojiMarkup`, `videoSizeStickerMarkup`)
  is ignored for display: only real video sizes play.

## What Quill does

- Envelope: `chatPhoto.animation`, falling back to `small_animation`, is
  parsed into `ChatPhotoAnimation { file_id, start_ms }`
  (`src/telegram/envelope/chat_photo_animation.rs`) on every parsed chat
  photo: `ParsedProfilePhoto` (user photo lists, personal/public/main photo
  of `userFullInfo`, group and channel full info) and
  `messageChatChangePhoto`. The clip file joins the photo's cached files.
- State: `ProfilePhoto.animation`; `Session::cover_animation(owner)` is the
  shown photo's clip (personal photo first, as TDLib documents; none for
  Saved Messages); `Session::info_cover_animation()` is the open panel's,
  `None` once it closes. `info_cover_file_ids_to_download` downloads the
  clip with the open chat's thumbnails while the panel is open.
- Cover (`src/ui/video_avatar.rs`): at the start of every main-window render
  the app opens, keeps, pauses or drops one `VideoPlayer` to match: open
  when the panel is on screen (right column not hidden, or the profile
  modal) and the clip is local; paused while the window is inactive or the
  viewer, a story or a kit dialog is over it; dropped (decoder freed) the
  moment the panel closes, the layout hides it or the clip changes. The
  picture is drawn by a view of its own (`CoverVideo`), circle-cut with the
  existing `video_frame` corner mask, over the still avatar. Decoding goes
  through the existing player (`Purpose::Inline`, FFmpeg decodes at the
  cover's pixel size; AVPlayer on macOS); no new decoder.
- Viewer: a profile item with a clip keeps kind `Photo` (header, zoom,
  rotate, copy of the still, menu) and gets `play_file_id`;
  `MediaViewerItem::{is_video_photo, plays_clip, loops}` route it through the
  existing GIF-loop path (`Purpose::ViewerLoop`, muted, no transport),
  starting at the main frame. Save writes the clip. The thumb strip draws
  the still sizes as before.

## Deviations

- Autoplay: the cover keeps its still while "Autoplay GIFs" is off or data
  saver is on (tdesktop's cover ignores these). Asked for by the parity
  brief to keep idle CPU and data down; the viewer always plays.
- Save of a video photo whose clip is not downloaded yet says to download
  it first instead of waiting for it (tdesktop `SavePhotoVideo::QuickSave`);
  the viewer starts downloading the clip as soon as it opens, so this only
  shows in the first moment.
- A sticker-based photo with no video sizes stays a still, as in tdesktop.
- `profilePhoto.has_animation` / `chatPhotoInfo.has_animation` are not read:
  tdesktop decides from the full photo's video sizes too.
- Service rows of a chat-photo change in history do not play the clip
  (tdesktop's `HistoryView::Photo` does for userpic changes); out of this
  item's scope.

## Verified

- Tests: envelope parsing (large/small/sticker/none, main frame), state
  (clip follows the open panel, autoplay/data saver, personal photo, Saved
  Messages, history photos keep clips), viewer items, and a GPUI test on a
  scripted backend (cover plays muted from the main frame, pauses in the
  background and under the viewer, viewer loops with no seek bar, closing
  the panel drops the decoder; autoplay off drops it).
- Captures (`ready-info-video-avatar`, `ready-info-video-avatar-user`):
  cover and viewer in light and dark show the clip's main frame (zoomed
  toward the sun) instead of the still; a real AVPlayer run shows the
  circle mask on a playing surface.
