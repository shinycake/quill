# Notifications, mute and auto-delete menus, idle presence (batch B3)

## What tdesktop does
- `Window::Notifications::Manager` (default/mac/win/linux backends) shows
  "Reply" (inline text on macOS `UNTextInputNotificationAction`, Windows toast
  input, libnotify actions) and "Mark as read". `clearFromHistory` /
  `clearFromItem` withdraw shown notifications when the chat is read.
- `menu/menu_mute.cpp`: Mute for 1 hour / 8 hours / 2 days / Custom (a
  days+hours `ChooseTimeWidget`), Disable/Enable sound, Mute forever, Unmute.
- `menu/menu_ttl.cpp`: header "Auto-Delete" item with a picker (1-6 days,
  1-3 weeks, 1-6 months, 1 year) and Disable; the default timer lives in
  Privacy settings.
- `core/application.cpp` `lastNonIdleTime` + `offlineIdleTimeout` (30 s) drive
  online presence; the passcode auto-lock uses the same idle clock.

## What changed
- Notification buttons: `quill::notify::NotificationAction` (Open / Reply /
  Mark as read). macOS and Windows pass them as GPUI `SystemNotification`
  actions; Linux `notify-send` gets `--action=reply=...` / `--action=read=...`
  and the picked name is parsed from stdout. No buttons while the app is locked.
- Mark as read runs `mark_chat_as_read` without raising the window. Reply
  raises the window, opens the chat and focuses the composer.
  **Fallback:** GPUI notifications have no inline text field on any platform,
  so Reply does not accept typed text in the notification itself.
- Clearing: `Session::clear_chat_notifications` runs on `updateChatReadInbox`
  with 0 unread, on an emptied `updateNotificationGroup`, and remembers chats
  from `updateActiveNotifications`. The UI dismisses the toast by tag
  (macOS, Windows). **Linux fallback:** `notify-send --wait` processes cannot
  be recalled; the daemon expires them. Queued but unshown toasts are dropped
  on every platform.
- Reaction notifications: `updateMessageUnreadReactions` now carries the newest
  unread reaction; a grown counter runs `decide_reaction_notify` against
  `reactionNotificationSettings` (None / Contacts / All, preview, mute, open
  chat, sound 0).
- Mute panel: Custom days/hours stepper (`mute_menu::CustomMute`, 1 hour floor,
  above 366 days is forever), Unmute, Disable/Enable sound.
- Auto-delete: the header menu offers "Auto-Delete" in private chats, basic
  groups and supergroups/channels with change-info rights
  (`auto_delete::can_edit_regular_ttl`; basic-group rights are left to TDLib's
  answer). Presets plus a Custom stepper over tdesktop's list.
- Default timer: `getDefaultMessageAutoDeleteTime` /
  `setDefaultMessageAutoDeleteTime` in the Account dialog (Settings > Privacy).
- Idle presence and passcode auto-lock were already in place
  (`ui/presence.rs`, `quill::presence`, `passcode::os_idle_ms`): offline after
  30 s without input, back online on input, auto-lock from OS idle time. No
  change needed.

## Not done
- Inline typed Reply (see fallback above), Linux toast recall.
- "Desktop notification options" (position/count/volume) and the other
  `notify-*` items are untouched.

## Verified
Unit tests for the action ids/parsing, reaction decision, clear/reaction/
default-timer reducers, request shapes, driver guards and the mute/auto-delete
logic; the gate passes. Demo captures `ready-mute-custom` and
`ready-auto-delete` checked by eye. Not verified against a live account or the
real OS notification centers.
