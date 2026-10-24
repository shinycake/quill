# Photo previews in notifications

## Why

The owner asked for it on 2026-10-23, with a setting. Telegram Desktop
never shows a message's photo in a notification. Its native and custom
notifications show the sender's picture, the name and "Photo". Its own
popups build the text with `generateImages = false`
(`notifications_manager_default.cpp`). So this is a Quill addition, with a
switch in Settings > Notifications.

## What it does

- "Show photo previews" sits under "Show message text" and is on by
  default. It is greyed out while the text is hidden, because the photo is
  part of the message. It is saved as `notify_photo_previews` in
  `prefs.json`.
- The reducer picks the photo when it queues the notification
  (`notify::preview_photo`). It takes the `x` size (box 800), or the
  thumbnail when there is no `x`. The original is never used.
- It never picks a secret or spoiler photo, a self-destructing message, or
  anything from a secret chat. The OS keeps notifications on screen and in
  its history.
- A burst of notifications keeps the first photo.
- When the photo is not on disk yet, the UI asks TDLib for it at the top
  priority. The notification then waits up to 3 s for the download and
  shows with or without the photo. Data saver skips the download. Three
  things drop the waiting notification:
  - a newer notification for the chat
  - the chat being read
  - the notification being dismissed

  If the passcode lock comes up during the wait, the notification shows in
  its locked form.
- Nothing is decoded on the main thread. Each platform reads TDLib's file
  itself:
  - macOS: the photo replaces the sender's picture as the attachment. The
    system shows one attachment. The copy keeps its extension, so JPEG
    works. It is copied on the notification thread.
  - Windows: an inline `<image>` under the text. The circular sender
    picture stays.
  - Linux D-Bus: the `image-path` hint. The `notify-send` fallback uses the
    photo as its icon.
  - Quill's own popups: a 48 px rounded square left of the close button.

## Checked

- On macOS, a test bundle posted `QUILL_DEBUG_NOTIFICATION` with
  `QUILL_DEBUG_NOTIFICATION_PHOTO`. The banner showed the photo, and the
  unified log shows the `.jpg` attachment accepted. With
  `QUILL_DEBUG_NOTIFICATION_CUSTOM`, Quill's popup showed it beside the text.
- Not checked: a real incoming photo on a live account, a Windows toast and
  a Linux daemon.
