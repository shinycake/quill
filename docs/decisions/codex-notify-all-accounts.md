# Show notifications from all accounts: not implemented (blocked)

Parity item: `parity:notify-all-accounts`. No code change and no parity
fragment. This note records why, so the next attempt starts from the facts.

## What tdesktop does

- `settings_notifications.cpp` (`BuildMultiAccountSection`): when
  `Core::App().domain().accounts().size() >= 2`, a "Show from" subsection with
  one toggle, "Show notifications from all accounts" (`notifyFromAll`,
  default on, persisted in the settings file). Turning it off clears
  notifications already shown for every non-active account.
- `notifications_manager.cpp` (`System::computeSkipState`): a notification is
  skipped when `notifyFromAll` is off and the item's session belongs to an
  account other than `Domain::active()`. Otherwise the normal per-chat mute and
  silent rules apply.
- This works because tdesktop keeps every account's session connected and
  receiving updates for as long as the app runs. The toggle only filters.

## What Quill does

- Exactly one TDLib client runs: `QuillApp::live: Option<LiveConnect>`, with one
  `ConnectDriver`, one `Session` and one poll loop (`spawn_poll_loop` in
  `src/ui/notifications/mod.rs`, tied to `connection.generation`).
- `switch_account` (`src/ui/accounts.rs`) calls `LiveConnect::shutdown`, then
  `set_active_account`, then `start_connection`. Inactive accounts have no
  client, so they receive no updates and can raise no notification.
- Notification state (`session_notifications.rs`, tray counters, badge, window
  title) reads the single active `Session`.

## Why it stops here

A toggle that filters nothing would be a lie, and making it real means running
a second (or Nth) live client per background account:

- Another `LiveTdJson` + `ReceiveBridge` thread + `ConnectDriver` + `Session`
  per account, restored from its own keychain-held database key, with its own
  auth-state handling (an account that needs re-login in the background needs
  a "sign in again" path that does not steal the foreground UI).
- A second poll path that feeds only notification-relevant updates
  (`updateNewMessage`, mute scopes, chat notification settings, read state)
  into a notification-only reducer, plus per-account unread counters for the
  tray and badge.
- Routing a click: switch accounts (shutdown/start of the foreground client)
  and then open the chat, which needs the switch flow to hand over or reuse the
  already-running background client instead of closing it.
- Lifecycle, memory and battery policy for background clients (cross-platform:
  Linux and Windows too).

That is an architecture change to the connect layer and `QuillApp`'s single-
`live` assumption (`src/ui/connect_ui.rs`, `src/ui/notifications/`,
`src/ui/accounts.rs`, `src/connect/live.rs`), not a feature module with small
hooks. It was out of scope for a focused parity change.

## Suggested plan when it is picked up

1. `src/connect/background.rs`: a `BackgroundAccount` owning a `LiveConnect`
   in notification-only mode, started for every non-active registered account.
2. `src/notify_accounts.rs`: pure decision logic (`should_notify(setting_on,
   account_active, muted, silent)`), the persisted `notify_from_all` pref
   (default on), and a `NotificationRoute { account, chat }`.
3. Settings row shown only with 2+ accounts, with the tdesktop clear-on-off
   behaviour.
4. Click routing: `switch_account` then open the chat once the account is live.

## Verification

Read-only research of tdesktop and an audit of the Quill code paths above.
Nothing was built or run for this change.
