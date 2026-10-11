# Inline Reply and Mark as read on notifications: what is real

## What tdesktop does
`Window::Notifications::Manager` (`window/notifications_manager.cpp`) routes
`notificationReplied` (send the text as a reply to the notified message) and
`notificationActivated`/mark-as-read. Per platform:
- macOS (`platform/mac/notifications_manager_mac_un.mm`): a notification
  category with a text-input "Reply" action (`withReply`) and a "Mark as read"
  action.
- Windows (`platform/win/notifications_manager_win.cpp`): toast XML with a
  `fastReply` `<input type="text">`, a send button and a "Mark as read"
  button; the typed text is read from `UserInput()` on activation.
- Linux (`platform/linux/notifications_manager_linux.cpp`): `inline-reply`
  action (daemons that advertise it) and a `notification-mark-as-read` action.

## What Quill has
- "Mark as read" works on all three platforms already:
  `NotificationAction::MarkRead` (`src/notify/mod.rs`) is offered as a button
  by GPUI `SystemNotification` (macOS, Windows) and `notify-send --action`
  (Linux), and `run_notification_action` calls `mark_chat_as_read` (viewMessages
  through the chat's last message, as tdesktop does) without moving the window.
- "Reply" opens the chat and focuses the composer. That is the fallback for
  inline input.

## What is missing for a real inline reply
The vendored GPUI notification API (`gpui-pre` 0.3.8 and the
`third_party/gpui-pre-{macos,windows,linux}` copies) has:
- `SystemNotificationAction { id, label }` only: no text-input action kind or
  placeholder.
- `SystemNotificationResponse { tag, action_id }` only: no field carrying typed
  text.
Needed patches, per platform:
- macOS: use `UNTextInputNotificationAction` for the reply action and read
  `UNTextInputNotificationResponse.userText` in the delegate in
  `third_party/gpui-pre-macos/src/system_notifications.rs`.
- Windows: add the `<input>` element to the toast XML and read `UserInput()`
  in `third_party/gpui-pre-windows/src/system_notifications.rs`.
- Linux: Quill uses `notify-send`, which cannot show an input field or return
  text. It would need the `inline-reply` action over D-Bus (the
  `notify-rust` path in `third_party/gpui-pre-linux`), only where the daemon
  lists it in capabilities.
Then Quill would add the text to the response, check the sender is unlocked,
and send it as a reply to the notified message through the existing send path.

## What changed
Nothing in code: the buildable part already exists, and the rest needs the
GPUI patches above on three platforms, two of which cannot be run here. No
parity fragment is written for `parity:notify-inline-actions`.

## Verified
Read the code paths above and the tdesktop sources. Existing tests in
`src/notify/tests.rs` cover action ids, button lists and the locked-app case.
