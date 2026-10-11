# Background accounts: every signed-in account stays connected

Parity item: `parity:notify-all-accounts`. Follows the audit in
`codex-notify-all-accounts.md`, which stopped because Quill ran one TDLib
client and closed it on every account switch.

## What tdesktop does

- `Main::Domain` keeps every account's session connected while the app runs.
  Accounts that are not on screen still get updates.
- `System::computeSkipState` (`window/notifications_manager.cpp`) skips a
  notification from another account only when `notifyFromAll` is off.
  `notifyFromAll` is app-wide and on by default.
- Settings > Notifications shows "Show notifications from / All accounts"
  only when there are two or more accounts (`BuildMultiAccountSection`).
  Turning it off clears the notifications other accounts already showed.
- Each other account's row in the account list shows its unread counter
  (`unreadWithMentionsBadge`, drawn grey when everything unread is muted).
  The app badge and the window title add up every account
  (`Domain::unreadBadge`).
- Only the account in an active window is online (`Updates::updateOnline`).
- A click on a notification activates its account, then opens the chat.

## Design

### One receive thread for every client (`src/telegram/hub.rs`)

`td_receive` serves every client in the process, so two receive threads
would steal each other's updates. `TdHub` owns the one `td_receive` loop and
routes each envelope by `@client_id` to that client's channel. Envelopes for
an unknown client, or with no client id, are dropped and logged. Each
`LiveConnect` registers its route before its first request (TDLib sends
nothing for a new client before that) and removes it when it closes.

The shared hub is created once and never dropped, so `libtdjson` is never
unloaded. The old code unloaded it after closing the client, and doing that
while TDLib threads ran is what crashed `--connect-smoke`. With no client
registered, the thread waits on a condition variable instead of calling
`td_receive` five times a second.

`ReceiveBridge` keeps its API. Live bridges no longer own a thread
(`spawn_live` and `ordered_receive_loop` are gone); injected test bridges
are unchanged.

### Background clients (`src/ui/background_accounts/`)

- `ensure_background_accounts` runs after every connect and switch. It starts
  a client for each registered account that is not on screen and has a local
  database: the keychain read happens on a worker, then
  `start_background_live_connect`. That function does the same session
  restore as the foreground one, but loads no call engine. An account with no
  database is listed as "Not signed in" and gets no client.
- A background client is a full `ConnectDriver` and `Session`. Notifications
  then follow each account's own mute, scope, preview, sound and reaction
  settings with no second reducer, and switching to it needs no restart. It
  never opens a chat or loads history. It loads the chat list the way the
  account on screen does, because TDLib only sends unread totals for lists
  that were loaded.
- Its own poll loop runs every 100 ms, backs off to 400 ms when idle, uses a
  4 ms ingest budget per account, and stops when no client runs. Each poll
  takes the session's queued notifications, withdrawals and custom sound
  plays (`notify_accounts::drain_background`), recomputes the account's
  unread count, and publishes the total of all background accounts for the
  tray, the Dock or taskbar badge and the window title.
- When a background client becomes ready it sets `online` to false.
- A client that needs a sign-in, was signed out, or lost its receive thread
  is closed. Its status stays in the Accounts list ("Not signed in",
  "Signed out", or the error). Signing in only happens on screen.
- Removing an account closes its client before the folder is deleted. The
  passcode-screen wipe closes all of them. On quit (GPUI drops the window's
  views) every client is asked to close at once and then waited for.

### Switching

`switch_account` calls `retire_live_to_background`: the outgoing client
closes its open chat (`deselect_chat`), goes offline and keeps running. It is
closed instead when it is in a call, because the call would go on with no
window to hang it up. `take_background_live` then hands over the incoming
account's running client and `adopt_live` puts it on screen with its chat
list, folders and counters already loaded. When no client runs for that
account, the old path (`start_connection`) starts one. Two clients never open
one database: a background start that finishes after its account became
active is dropped.

### Notifications and clicks

- Background notifications use the tag `account:<key>:chat:<id>`, the chat
  picture and the notification sound from that account. They have no inline
  reply field, because a reply needs the account on screen; Reply switches
  and focuses the composer.
- macOS and Windows: the response callback no longer ignores other accounts.
  `notify_accounts::plan_click` sends "Mark as read" to that account's client
  and leaves the window where it is. Open and Reply queue a switch.
- Linux: the D-Bus notifier keys notifications by (account, chat) and pushes
  clicks into the account's queue. The `notify-send` fallback does the same.
- The switch runs on the next render (`flush_background_accounts`, which has
  the window), then the chat opens once that account's session is ready and
  knows the chat (up to 30 s for a cold start).
- "All accounts" is stored in `accounts.json` (`notify_from_all`, default
  on). The setting shows in Notification defaults with two or more accounts.
  Turning it off withdraws what the other accounts are showing.

## Verification

- Unit tests (`src/telegram/hub/tests.rs`): routing by client id and order,
  unknown and missing client ids dropped, a closed bridge frees its route, a
  gone reader loses its route, no `td_receive` while no client is
  registered, a dead receive thread marks every bridge stopped.
- Driver tests with the recording `JsonSender` (`src/notify_accounts/tests.rs`):
  a background account notifies for an unmuted chat and stays quiet for a
  muted one; with "All accounts" off nothing is shown but withdrawals still
  go out and nothing is left queued; force-reply never leaks; unread counts
  come from TDLib's totals and the account's badge settings; muted
  combination; status for every authorization state including logout; click
  planning.
- Registry test: `notify_from_all` defaults on, persists, and old
  `accounts.json` files read as on.
- GPUI tests (`--features demo-capture`, `src/ui/background_accounts/tests.rs`):
  the account row badge and status, a pending open runs once its account is
  on screen, and is dropped for the wrong account or after the deadline;
  "Mark as read" on another account never switches; a click on the account
  on screen takes the usual path.
- Live TDLib, no network and no account (ignored test,
  `live_clients_get_only_their_own_envelopes`): three real clients on the
  shared hub each get only their own envelopes. Run it with
  `QUILL_TDJSON_PATH=… cargo test --no-default-features live_clients -- --ignored`.
- No real account was used. Nothing was sent.

## Measurements

From the live test against `native/build/libtdjson.dylib` (TDLib 1.8.68,
macOS arm64, debug build, clients before `setTdlibParameters`):

| TDLib clients | Process RSS | Threads |
| --- | --- | --- |
| 1 | 19.0 MB | 7 |
| 2 | 19.9 MB | 11 |
| 4 | 23.0 MB | 19 |
| 8 | 27.9 MB | 35 |

- An empty client costs about 1.1 MB and 4 TDLib threads. Quill's own receive
  threads stay at one. The old design would have added one more thread per
  client and also lost updates between clients.
- What a signed-in account costs (its TDLib database caches plus a `Session`
  with its chat list) was not measured: that needs a real account, and this
  change was built without touching one. tdesktop pays the same cost, since
  it keeps every account's session in memory.
- Exit safety: with three clients left open, the test process crashed on
  exit in 3 of 60 runs (TDLib's static destructors racing live clients). With
  the clients closed first it crashed in 0 of 60. Quill closes every client
  on quit, so this does not apply to the app.
- Idle cost: the background poll wakes at most 2.5 times a second while a
  background client runs, and not at all when none runs. The receive thread
  sleeps while no client is registered.

## Not done here

- Incoming calls on a background account are not shown. tdesktop rings for
  any account. The call reaches that account's session, but nothing on screen
  answers it.
- The window title does not show the account name with two or more signed-in
  accounts (tdesktop `hideAccountName`).
- The main menu does not show the other accounts' unread dot
  (`FiltersMenu::setupMainMenuIcon`, the main menu toggle badge).
- Proxy settings are per account in TDLib. A background account uses its own,
  and Quill's proxy auto-switch (`proxy_tick`) only runs for the account on
  screen.
- Chat windows that show a chat of the outgoing account are not closed on a
  switch. This was already the case before this change.
