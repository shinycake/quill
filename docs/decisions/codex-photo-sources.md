# Profile photo sources: camera, emoji avatar, video avatar

## What tdesktop does

`UserpicButton::choosePhotoLocally` (`ui/controls/userpic_button.cpp`) offers,
for your own photo: choose a photo or video, Camera (only when a camera exists
and no call is running), paste from the clipboard, and "Use an emoji".

- Camera: a box with the live webcam preview, then the photo editor (crop),
  mirrored like a selfie.
- Emoji builder (`info/userpic/info_userpic_emoji_builder_widget.cpp`): a
  preview, seven 4-color gradient presets (`PaletteGradients`, each list rotated
  by 180 degrees) plus a custom gradient editor, and an emoji/sticker selector.
  The result goes out as a video-size emoji or sticker markup.
- Video: `PrepareProfileVideo` opens the video editor with an exact 800 x 800
  crop, 1 to 9.6 s trim, 30 fps, audio removed, and a cover (main frame) pick.

## What changed in Quill

Edit profile > Photo gets three buttons, each opening a panel in place of the
dialog body:

- **Camera...**: takes one snapshot with ffmpeg (`media_tools::capture_input`
  without the microphone, 10 warm-up frames skipped, mirrored, center square),
  shows it, and sends it as `inputChatPhotoStatic`.
- **Use an emoji...**: the seven desktop gradient presets and the custom emoji
  from `getDefaultProfilePhotoCustomEmojiStickers`; sends
  `inputChatPhotoSticker` with `chatPhotoStickerTypeCustomEmoji` and a
  `backgroundFillFreeformGradient` of the preset's four colors.
- **Video...**: file picker, ffprobe check (at least 1 s, shape within 10:1),
  a scrubber that renders the frame at the chosen point (ffmpeg, on release),
  then ffmpeg encodes the first 9.6 s to 800 x 800, 30 fps, no audio, H.264
  with a bitrate that fits TDLib's 2 MB limit, and sends
  `inputChatPhotoAnimation` with `main_frame_timestamp` from the scrubber.

Code: `src/profile_photo_sources.rs` (requests, presets, limits, ffmpeg
arguments, probe parsing), `src/connect/profile_photo_sources.rs` (driver),
`src/ui/profile_photo_sources.rs` (panels). New request purpose
`GetProfilePhotoEmojiIcons` with its state fields in `UsersState`.

## Differences from tdesktop

- The camera takes a snapshot instead of showing a live preview, and has no
  crop step (center square).
- The video clip is the first 9.6 s with a center crop; there is no trim or
  crop editor. The scrubber chooses the main frame only.
- The builder lists custom emoji only (no stickers tab) and has no custom
  gradient editor. The request builder already supports regular stickers.
- The preview approximates the four-color gradient with two stops.
- Needs ffmpeg and ffprobe, like other media features.

## Verification

Unit tests cover each `setProfilePhoto` input type's JSON, the fill choice,
the preset colors, the plan limits, the frame timestamp clamping, and the
ffmpeg argument lists. gate.sh passes (core 3313, ui 282). Not verified: the
panels on screen, a real camera snapshot, and an upload to a live account (the
real profile photo was not changed).
