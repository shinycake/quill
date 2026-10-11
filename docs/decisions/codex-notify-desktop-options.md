# Desktop notification options: volume only (position, count, display do not apply)

## What tdesktop does
`settings/sections/settings_notifications.cpp` shows, under "Desktop
notifications": a screen-corner picker (`NotificationsCount` in
`settings_notifications_common.h` draws the corners), a count slider (1-5,
`kMaxNotificationsCount`), a "Display" monitor choice, and "Use native
notifications". The corner, count and display options only position tdesktop's
own `Notifications::DefaultManager` widgets; they are hidden when native
notifications are used. A separate "Volume" slider
(`Core::Settings::notificationsVolume`, 0-100, default 100) scales the
notification sound in `media_audio_track.cpp` (linear, `volume / 100`), and
plays the sound on release as a preview.

## What Quill does
Quill never draws its own notification widgets. Messages and calls go to the
OS: GPUI `SystemNotification` on macOS and Windows, `notify-send` on Linux
(`src/ui/notifications/`, `quill::notify`). The OS owns placement, stacking
and the monitor, so a corner, count or display picker would be dead controls.
They are intentionally not built, and no parity fragment is written for
`parity:notify-desktop-options` (only one of its four parts is real).

Quill does play its own alert sound (`audio::NotificationSounds`, rodio on the
shared output) for messages, reactions and incoming calls. That is where the
volume applies.

## What changed
- `src/notify_prefs.rs`: app-wide `NotifyPrefs { volume }` stored in
  `notify_prefs.json` beside `file_prefs.json` (shared by accounts, default
  100, clamped to 0-100, linear gain).
- `NotificationSounds::play` sets the rodio player volume from the pref, so
  the default tone and downloaded sounds both follow it.
- `src/ui/notification_settings/volume.rs`: "Sound volume" slider row in the
  notification defaults dialog; saves live while dragging and plays the tone
  at the new level on release.
- Works the same on macOS, Linux and Windows (rodio mixer, no OS calls).

## Verified
Unit tests: prefs round trip, missing-field default, gain scaling and
clamping, slider rounding. `gate.sh`, hotspot and file-size checks.
Not verified: audible loudness and the slider in a running window.
