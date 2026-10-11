# Viewer Rotate on the native video player

Follow-up to `codex-viewer-video-extras.md`, which left Rotate off the macOS
AVPlayer path.

## What tdesktop does

`playbackControlsRotate` turns the video 90 degrees per press, swaps the
content size and keeps the turn per document for the session.

## What changed

- `ui/native_video_rotate.rs`: quarter-turn math (size, pixel position,
  strided plane transpose) plus two users. AVPlayer frames are
  `420YpCbCr8BiPlanarFullRange` GPU buffers drawn by GPUI's `surface`, which
  cannot be transformed, so `turn_surface` copies the luma plane and the
  interleaved chroma plane transposed into a new IOSurface-backed,
  Metal-compatible buffer of the turned size. `turn_render_image` does the
  same for the FFmpeg backend's images (Linux, Windows).
- `NativeVideo::set_rotation` stores the turns; each backend turns a frame
  once per new frame and reuses it while paused. The viewer sets it from its
  orientation on every frame, so a quality switch keeps the turn.
- Rotate is offered for any playable viewer video with the native player or
  decoded frames. The viewer already swaps the layout size for odd turns.
- Turns are remembered per video for the run (`ViewerExtra.rotations`) and
  restored when the video is shown again.

## Differences from tdesktop

The turn is kept for the run, not saved to disk. The per-frame copy runs on
the CPU; clips above 1080p are the costly case.

## Verification

Unit tests for the size and position math, strided planes, four turns being
the identity and the remembered-turns map. A macOS test builds a real
bi-planar CVPixelBuffer, turns it and reads both planes back. Not verified:
a full AVPlayer clip on screen (the screenshot demo uses decoded frames).
