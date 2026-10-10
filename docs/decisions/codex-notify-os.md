# Notifications and the OS (cluster notify-os)

## What tdesktop does

- Alerts: when a notification fires and the window is inactive, it bounces
  the Dock icon, flashes the taskbar or asks the window manager for
  attention (`flashBounceNotify`, `lng_settings_alert_*`). Its own sound and
  the flash are skipped while the OS is in Do Not Disturb: macOS reads the
  `doNotDisturb` preference of `com.apple.notificationcenterui`, Windows asks
  `SHQueryUserNotificationState` and Focus Assist, Linux reads the
  `org.freedesktop.Notifications` `Inhibited` property.
- Events: "Contact joined Telegram" and "Pinned messages" decide whether
  those service messages notify.
- Folder counters: each folder tab shows its unread chats; "Include muted
  chats in folder counters" removes muted chats from that number, and a tab
  whose counted chats are all muted gets a muted-colored badge.

## What changed

- `src/notify_focus.rs`: the Do Not Disturb query per OS (cached, refreshed
  on a worker thread so the UI never waits on a process) and `plan_alert`,
  which drops both sound and flash during DND. macOS reads only the
  notification-center `doNotDisturb` preference, like tdesktop; the Focus
  assertions file sits in a privacy-protected folder and is left alone.
  Pure parsers are unit tested.
- Flash or bounce: `Session::pending_attention` is set whenever a message
  passes the notify rules, even with "Desktop notifications" off (tdesktop's
  alert does not depend on that switch). The UI calls
  `Window::request_attention`, which GPUI implements on macOS (Dock bounce),
  Windows (`FlashWindowEx`) and X11 (urgency hint). GPUI does nothing on
  Wayland.
  The switch is `BadgePrefs::flash_bounce`, labeled per OS as tdesktop does.
- Events: `decide_notify` takes `pinned_allowed` (the chat's or scope's
  `disable_pinned_message_notifications`, which was stored but never applied
  to toasts) and `contact_joined_allowed` (TDLib option
  `disable_contact_registered_notifications`, set from a new switch).
  Pinned messages stay per chat type in the existing scope sections because
  TDLib keeps the choice there; tdesktop has one global switch.
- Folder counters: the reducer keeps `updateUnreadChatCount` per folder;
  `UnreadPair::folder_badge` applies the tdesktop rule; tabs and the left
  rail draw the badge. `BadgePrefs::include_muted_folders` is the switch.
- Settings UI: new rows in Notification defaults for the flash/bounce
  switch, the Events section and the folder-counter switch.

## Not done, and why

- Inline reply text inside the notification: GPUI's `SystemNotification`
  has action buttons but no text field on any platform. Reply keeps opening
  the chat with the composer focused. Mark as read works from the button.
- Windows toast buttons: the audit in `codex-cross-platform-gaps.md` is out
  of date. The vendored `gpui-pre-windows` already writes `<actions>` into
  the toast, and `action_buttons` passes Reply and Mark as read. Not
  verified on a real Windows session.
- Position, count and display options: Quill uses the OS notification
  center, which owns placement and stacking. Volume would need a mixer
  control and was left out.
- Sender avatar: `SystemNotification` has no image field, so macOS and
  Windows cannot show one through GPUI.
- All accounts: Quill runs one TDLib client at a time (switching accounts
  shuts the client down), so other accounts cannot notify.

## Verification

- `gate.sh`: GATE OK (fmt, clippy, tests).
- Unit tests: DND parsers and alert plan, event gating in `decide_notify`,
  reducer tests for the option, scope pinned flag, attention flag and
  folder counts, `BadgePrefs` defaults for old files.
- Demo captures `ready-notify-os` and `ready-folder-badges` were rendered
  and read: the Work tab shows a gray 5 (only muted unread chats), All and
  News show blue badges, and the new settings rows appear.
- Not verified live: the Windows and Linux DND queries, the Dock bounce
  itself, and Wayland (no GPUI support).
