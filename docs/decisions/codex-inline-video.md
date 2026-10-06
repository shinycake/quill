# Inline video and GIF autoplay (tdesktop)

tdesktop plays downloaded videos and GIFs in the chat: muted, looped,
with no play button. A video's badge counts down the time left next to
a muted icon ("00:06 🔇"). Clicking opens the media viewer with sound.
Settings → Chat has "Autoplay GIFs" and "Autoplay videos", both on by
default.

Quill showed a static thumbnail with a play disc. Its inline paths
extracted PNG frames with ffmpeg: one clip at a time, and a 400 ms tick
for video. Now:
- `ui/inline_video.rs` keeps one native player (`NativeVideo`,
  AVFoundation) per visible downloaded clip, muted and looping, drawn
  with GPUI's `surface` straight from its pixel buffers. Players are
  keyed by message and dropped a render after their row stops rendering
  (scrolled away, chat changed, viewer open). A ~30 fps redraw tick runs
  only while a clip plays.
- Autoplay requires the native player, autoplay on for the kind, data
  saver off, no secret or spoiler, and a local clip. Your own sent videos
  play from the original file, like the viewer (`playable_clip_path`,
  now shared with it).
- Video tiles show the remaining time with a muted icon and drop the play
  disc; a click opens the viewer as before. GIF tiles keep the "GIF"
  badge.
- New `MediaPrefs::autoplay_videos` (default on) with a Settings switch.
  Turning either switch off stops the inline players.
- Real `image/gif` files keep the old frame-extraction path
  (AVFoundation doesn't play them); MP4 GIFs skip it.

Verified live: a sent video autoplays muted with the countdown, loops,
and opens the viewer on click; closing the viewer resumes the inline
clip.

Process CPU sat around 60% with or without a clip on screen. Animated
custom emoji already redraw the whole window each frame, and that
baseline is its own slice.
