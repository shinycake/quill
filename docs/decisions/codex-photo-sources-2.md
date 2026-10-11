# Profile photo sources, second pass

Closes the differences listed in `codex-photo-sources.md`.

## What tdesktop does

- Emoji builder: the selector has an emoji tab and a stickers tab. A custom
  gradient editor adds a color to the seven presets (one to four colors, +/-,
  a color picker). The background is drawn by `Images::GenerateGradient` at 45
  degrees: a linear gradient for one or two colors, and for three or four a
  blend around moving points (`GenerateSmallComplexGradient`, 64 x 64, scaled).
- Video: `PrepareProfileVideo` opens the video editor with a square crop,
  a trim range of 1 to 9.6 s and a cover frame.
- Camera: `CameraBox` shows the live webcam, mirrored, then the photo editor.

## What changed

- Emoji builder: a Stickers tab (recent and favorite animated stickers, sent as
  `chatPhotoStickerTypeRegularOrMask`; static stickers have no markup on
  desktop either). A custom gradient editor (eighth swatch): 1 to 4 colors,
  + and -, hex field, hue, saturation and lightness sliders. The preview and
  swatches draw the real background, ported from lib_ui
  (`profile_photo_gradient.rs`, rotation 45 = phase 1), instead of a two-stop
  approximation.
- Video: a range slider trims the clip to 1 to 9.6 s (the dragged handle
  decides which end gives way), zoom and left/right and top/bottom sliders set
  the square crop, the cover slider runs over the trimmed part. The preview is
  the cover frame as cut. ffmpeg gets `-ss/-t` and `crop=s:s:x:y`.
  Phone clips with a rotation tag are probed with their turned size.
- Camera: ffmpeg streams the camera (mirrored, center square, 640 x 640 RGB at
  15 fps) to a reader thread. The dialog pulls the newest frame as it renders
  and asks the frame clock for one redraw per camera frame (no looping
  animation). Take photo saves the frame on screen as JPEG and releases the
  camera; Retake restarts it.

Code: `src/profile_photo_gradient.rs`, `src/profile_photo_video.rs`,
`src/profile_photo_camera.rs`, `src/ui/profile_photo_sources/` (mod, emoji,
gradient_editor, video, camera). The driver gains
`set_profile_photo_sticker(PhotoSticker, colors)`.

## Differences that remain

- The crop is set with sliders, not by dragging a rectangle over the frame, and
  the camera photo has no crop step after the capture (the preview is the
  square that is kept).
- The gradient editor picks colors with sliders and a hex field, not
  tdesktop's color field.

## Verification

Unit tests: freeform and linear background drawing, hex and HSL maths, trim
clamping for both handles, crop rectangles, the encode, preview and camera
stream command lines, the probe with rotation, and the frame reader. gate.sh:
core 3322, ui 282. A demo capture (temporary, not committed) showed the emoji
panel with the preset row and the custom editor with a four-color preview.

Not verified: a real camera stream (needs the camera permission), the video
panel with a real clip, the stickers tab with account data, and any upload.
The real profile photo was not changed.
