# Inline reply and sender picture in OS notifications, on all three platforms

Finishes `parity:notify-inline-actions` and `parity:notify-avatar`, which
`codex-notify-inline-actions.md` and `codex-notify-avatar.md` left blocked on
the GPUI notification API.

## What tdesktop does

`Window::Notifications::Manager::notificationReplied` sends the typed text
through the normal send path: a reply to the notified message in groups and
channels (`!history->peer->isUser()`), a plain message in private chats; the
send reads the history (`ApiWrap::sendAction` → `readInbox`). Per platform:

- macOS (`platform/mac/notifications_manager_mac_un.mm`): a category with a
  `UNTextInputNotificationAction` ("reply") and a plain "markAsRead" action;
  the delegate reads `UNTextInputNotificationResponse.userText`; the userpic
  is a `UNNotificationAttachment` from a temporary PNG.
- Windows (`platform/win/notifications_manager_win.cpp`): toast XML with
  `<input id="fastReply" type="text">`, a "Send" action with `hint-inputId`
  and `activationType="background"`, a "Mark as read" action; the text is read
  from `UserInput()` on activation; the userpic is `<image
  placement="appLogoOverride" hint-crop="circle" src="file:///...">`.
- Linux (`platform/linux/notifications_manager_linux.cpp`): D-Bus
  `org.freedesktop.Notifications`; `inline-reply` goes in the action list when
  `GetCapabilities` lists it, the text arrives in the `NotificationReplied`
  signal, buttons in `ActionInvoked`; the userpic rides as image data.
- Replies and "Mark as read" never bring the window forward; opening the
  chat does. While the passcode lock is up, the reply button is not offered.

## The API problem and the chosen shape

`SystemNotification`, `SystemNotificationAction` and
`SystemNotificationResponse` are defined in the `gpui` crate (`gpui-pre`
0.3.8), which Quill does not vendor; only the platform crates
`gpui-pre-{macos,windows,linux}` are. Growing the structs would mean vendoring
the whole core crate and re-applying that on every GPUI update. Instead the
two vendored backends gain a small public side channel, `notification_extras`
(one std-only file, identical in both crates, checked and unit-tested by
`scripts/test-notification-extras.sh`, run in CI next to the frame-idle
tests):

- `set_notification_extras(tag, NotificationExtras { image, text_input })`
  is called right before `App::show_system_notification` with the same tag;
  the backend takes the entry when it builds the platform notification.
  `text_input` names the action id that becomes a text field, with
  placeholder and send-button label.
- The backend parks the typed text under the tag (`store_reply_text`) before
  it sends the `SystemNotificationResponse`, whose `action_id` stays the
  reply action's id; the application's handler takes it with
  `take_reply_text(tag)`.

Quill names the vendored crates directly (`gpui_macos` / `gpui_windows`,
optional, in the `ui` feature) for that module; the `[patch.crates-io]`
entries already point them at `third_party/`. `QUILL-CHANGES.md` in each
crate lists the patch.

Linux does not need the GPUI patch: Quill's Linux notifications were already
Quill's own code (`notify-send`), so they now speak D-Bus from Quill with
`zbus` (already a Linux dependency for MPRIS and the tray).

## What changed

### Vendored GPUI (macOS, Windows)

- `third_party/gpui-pre-macos/src/system_notifications.rs`: categories are
  keyed by actions plus text input; the named action is a
  `UNTextInputNotificationAction`; the image is copied to a temporary file
  (the center moves attached files into its own store) and attached; the
  delegate downcasts to `UNTextInputNotificationResponse` and parks
  `userText`.
- `third_party/gpui-pre-windows/src/system_notifications.rs`: the toast XML
  gets the circle-cropped `appLogoOverride`, an `<input>` named after the
  reply action and a background send button with `hint-inputId`; the
  `Activated` handler reads `UserInput()` (needs the `Foundation_Collections`
  feature of `windows`, added to the crate's `Cargo.toml`) when the send
  button fired. Other buttons keep foreground activation, as before.

### Quill core (`src/notify/`)

- `OsNotification` and `QueuedNotification` carry `message_id` (a burst keeps
  the newest); reaction notifications carry none.
- `plan_inline_reply(text, locked, private_chat, message_id)`: trimmed text,
  nothing while locked or blank, `reply_to` only outside private and secret
  chats. `INLINE_REPLY_PLACEHOLDER` / `INLINE_REPLY_SEND` are the field's
  copy.
- `notify::dbus` (new, pure): the bus names, `Capabilities::from_list`,
  `notify_actions` (`default`/Open, `inline-reply` or `reply`/Reply,
  `read`/Mark as read; empty without the `actions` capability or while
  locked), `notify_hints` (`desktop-entry=quill`, `category=im.received`),
  `app_icon` (the avatar path, as `notify-send --icon` passed it), and
  `interpret_signal` for `ActionInvoked`, `NotificationReplied`,
  `NotificationClosed`.

### Quill UI (`src/ui/notifications/`)

- `native_extras.rs`: `register(tag, image, inline_reply)` and
  `take_reply_text(tag, chat)`, per platform, no-ops elsewhere.
- `linux_dbus.rs` (Linux only): one session-bus connection per process, made
  on the first notification's worker thread; the signal match rule is added
  before the first `Notify`; a listener thread turns signals into the same
  `(chat, action)` clicks the other platforms queue and keeps the typed text
  per chat. `Notify` uses `replaces_id` so a newer message replaces the
  chat's toast, `CloseNotification` withdraws it when the chat is read
  elsewhere (not possible with `notify-send --wait`). No session bus:
  `notify-send` as before.
- `inline_reply.rs`: `register_notification_responses` (moved out of
  `app_demo.rs`), `drain_background_clicks` (runs inline replies and "Mark as
  read" without a window, from the response callback and from every poll for
  the Linux listener thread; only chat opens bring the app forward now),
  `send_inline_reply` (a `ComposerSnapshot` with the reply through
  `send_text_snapshot`, then `mark_chat_as_read` and the toast withdrawn;
  a refused send opens the chat instead), `mark_read_from_notification`.
- `spawn_call_notification.rs`: the circular avatar
  (`avatar_icon::circular_icon_for`, cached under
  `.../notify-avatars/`) is computed on every platform, never while locked;
  `reply_targets` remembers the notified message per chat; the extras are
  registered before `show_system_notification`.
- `debug_notification.rs`: with `QUILL_DEBUG_NOTIFICATION` set, the first
  frame posts one notification for a chat that does not exist. Set to an
  image path, that file is the sender picture. Unset, it does nothing, and
  it is never set in normal use.
- `NotifyUi::reply_targets`.

`src/ui/app_demo.rs` (hotspot, `max:2`) lost the 55-line response callback in
favor of one call; the commit carries `Hotspot-change:`.

## Verified

- Unit tests: `notify::tests` (message ids through coalescing and the two
  display forms, `plan_inline_reply` rules), `notify::dbus::tests` (capability
  gating, action list with and without `inline-reply`, hints, signal
  interpretation), `linux_dbus::tests` (id bookkeeping when a toast is
  replaced), `scripts/test-notification-extras.sh` (the side channel's
  register / take / reply-text semantics, and that both copies are
  identical). Existing tests for action ids, locked buttons and the
  `notify-send` fallback still pass.
- macOS: `gate.sh` printed `GATE OK core=3442 0 ui=288 0` (fmt, core and
  UI clippy `-D warnings`, core and UI tests) after the rebase onto
  `origin/main`. That compiled the patched `gpui-pre-macos` (text-input
  action, image attachment, reply text parked by tag) and Quill's
  registrar. `scripts/test-notification-extras.sh` passed.
  No live banner was observed: this machine has no Screen Recording
  permission, and a notification was not posted for inspection.
- Windows: the patched `gpui-pre-windows` compiles for
  `x86_64-pc-windows-msvc` (`cargo check --target x86_64-pc-windows-msvc`
  on a scratch crate depending on the vendored path, with the project
  toolchain; `package-windows.yml` is `workflow_call`-only and cannot be
  dispatched on a branch). Not run on a Windows machine: the field, the
  `UserInput` read and the `appLogoOverride` picture still need a live
  check there.
- Linux: `linux_dbus.rs` type-checks against `zbus` 5.19 (a scratch crate on
  the macOS host includes the file by path; zbus builds on macOS); the pure
  protocol module is covered by the core tests CI runs. The Linux CI job
  builds the core crate only, so the `ui` build on Linux lands with the
  `linux-package` job after merge. Not run against a daemon.

## What remains

- A live check of the macOS banner (the text field, the typed text, and the
  sender attachment). Not observed: no Screen Recording permission, and no
  notification was posted for inspection.
- A live check of the inline reply on Windows and on KDE Plasma (the only
  common daemon with `inline-reply`); GNOME shows the buttons and falls back
  to opening the chat on "Reply".
- The avatar on Linux is still the `app_icon` argument (what `notify-send
  --icon` did), not the `image-data` hint tdesktop uses, so daemons that
  draw both would show the app icon twice if that changed; left as is.
- Call notifications on Linux still use `notify-send`.
