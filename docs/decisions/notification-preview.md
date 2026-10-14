# Notification previews

## What was wrong

Every message notification read "New message". `hide_notification_previews`
defaulted to true on the session and in `prefs.json`, nothing loaded it, and
no setting could turn it off. The reducer also hid the text whenever the
chat's or scope's server `show_preview` was off.

## What tdesktop does

- `Core::Settings::NotifyView` (`core_settings.h`), default `ShowPreview`.
  Settings > Notifications has two checkboxes, "Name" and "Text"
  (`lng_notification_show_name` / `_show_text`); unchecking the name
  unchecks the text, checking the text checks the name
  (`settings_notifications.cpp`, `BuildNotifyViewSection`).
- `Manager::getNotificationOptions` (`notifications_manager.cpp`): the text
  is hidden when the view is past `ShowPreview` (body
  `lng_notification_preview`, "You have a new message"), and the name and
  photo when it is past `ShowName` (title is the app name). The same happens
  while the passcode lock is up. Reply buttons go with the text.
- The server `show_preview` is not consulted for message notifications
  (it governs mobile pushes); only reaction notifications read the reaction
  settings' own `show_preview`.
- A group message carries `notificationHeader()`, the sender's name. Linux
  shows it as "Sender: text" (`lng_dialogs_text_with_from`); Quill's GPUI
  notifications have no subtitle field, so all platforms use that form.
- Reaction notifications are dropped when the name is hidden.

## What changed

- `notify::NotifyView` with the three states, `prefs.json` key
  `notify_view` (default `show_preview`). The old key is ignored, so every
  account starts showing previews.
- "Show sender name" / "Show message text" switches under "Desktop
  notifications" in the notification settings dialog, hidden while desktop
  notifications are off, as in tdesktop.
- `decide_notify` follows the view and prefixes the group sender. The
  "nothing" view shows the toast like the locked one: no picture, no
  buttons.

## Verified

Unit tests for the decision and the switch rules, reducer tests for the
default and group sender, a prefs migration test, and a GPUI test for the
switches. Not yet seen on a live macOS toast.
