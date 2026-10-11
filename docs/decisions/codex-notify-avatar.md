# Sender avatar in OS notifications

## What tdesktop does

`Window::Notifications::Manager` renders the chat userpic (circular, with a
letters placeholder when there is no photo), caches it as a file, and hands it
to the platform: libnotify `image-path`/`image-data` on Linux, the toast
`appLogoOverride` (circle crop) on Windows, a notification attachment on macOS.

## What changed

- Linux: `notify-send` now gets `--icon=<png>` before the `--` separator. The
  PNG is a 128 px circular crop of the chat's already-downloaded small photo
  (`Session::chat_photo_path`), cached under the platform cache directory
  (`.../notify-avatars/<hash>.png`, key = path + length + mtime). No photo, an
  unreadable photo or a locked app means no `--icon`, so the daemon keeps the
  Quill app icon. No download is triggered; the chat list already requests
  small photos.
- `src/notify/avatar.rs`: pure cache naming and circular alpha mask, with unit
  tests. `src/ui/notifications/avatar_icon.rs`: decode, square crop, resize,
  mask and atomic PNG write with the existing `image` crate (no new
  dependency).

## What is not possible yet (macOS, Windows)

GPUI's `SystemNotification` (gpui-fast 0.1.3 and the vendored gpui-pre-*)
has only `tag`, `title`, `body` and `actions`. There is no image, icon or
attachment field, so Quill cannot pass an avatar to
`UNUserNotificationCenter` (`UNNotificationAttachment`) or to the WinRT toast
(`appLogoOverride`). Needed upstream: an optional image path field on
`SystemNotification` that the macOS backend turns into an attachment and the
Windows backend into `appLogoOverride` with `hint-crop="circle"`. Because of
that, the `parity:notify-avatar` tick is not added.

## Verification

Unit tests for cache naming, mask, rendering a rectangular photo to a round
icon, and the Linux argument order. `gate.sh` passes. Not run against a real
notification daemon.
