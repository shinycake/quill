# Native video playback (macOS), file paste, sending without a caption

## Problem
"Video playback is choppy, and the full viewer just shows the thumbnail."
Quill had no video player:
- ffmpeg wrote the clip out as PNG files (8 fps, at most 600 frames) and
  the UI flipped through them on a 125 ms tick;
- a separate `ffplay -nodisp` process played the audio from its own clock;
- the viewer showed a still until the extraction finished.

Found during live testing:
- Files copied in Finder couldn't be pasted: GPUI's macOS clipboard reads
  only text and image data, not file URLs.
- Attachments without a caption could not be sent: both Send and Enter
  required non-empty text.
- Your own sent videos never played: TDLib's local copy of an upload is
  the original file you picked, outside the display sandbox, and TDLib
  won't download it again.

## Telegram Desktop
Clicking a video opens the media viewer, which streams and plays it with
a play/pause, seek, volume and speed transport. Only GIFs and round videos
play inline. It decodes in-process (FFmpeg), with audio and video on one
clock.

## Decision
- `ui::native_video::NativeVideo` wraps `AVPlayer` with an
  `AVPlayerItemVideoOutput` (bi-planar 4:2:0, the format GPUI's Metal
  surface path draws). AVFoundation gives hardware decode, audio, A/V sync
  and frame-accurate seeking. The viewer pulls the current
  `CVPixelBuffer` each frame and draws it with GPUI `surface`. While
  playing, the overlay requests animation frames, so video advances at the
  display rate.
- The viewer's `PlaybackClock` stays the UI's time model (labels, seek
  slider) but mirrors the player's position; pause, resume, seek, volume
  and speed drive the player. Other platforms keep the ffmpeg path until
  they get a native engine (`native_video::SUPPORTED`).
- Chat video tiles open the viewer to play (tdesktop behavior); video
  notes still play inline.
- Paste reads `public.file-url` items from `NSPasteboard`, so copied files
  attach like a drop.
- Send/Enter accept an attachment-only message.
- An outgoing message may play its original local file while it exists.

## Verified live
A 12 s clip pasted from Finder, sent to Saved Messages and played in the
viewer: smooth playback, pause holds, seeking while paused shows the
exact frame.

## Not yet
- Native inline playback for GIFs and round videos.
- PiP on the native player.
- Streaming playback before a download completes (tdesktop streams).
