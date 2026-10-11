# Background accounts, part 2: calls, title, menu, proxies, chat windows

Follows `codex-background-accounts.md`, which kept every signed-in account
connected and left five gaps. Each one here was checked against
tdesktop first.

## Incoming calls on a background account

tdesktop's `Calls::Instance` takes calls from every account and handles one
at a time. A call that comes in while another is in progress is declined
as busy.

Quill now does the same (`src/ui/background_accounts/calls.rs`):

- After each background poll, `notify_accounts::ringing_incoming_call`
  finds an incoming call that is still pending.
  `plan_background_call` declines it as busy (`discardCall` on that
  account) when the account on screen is in a call or video chat, or when
  another account is already ringing. Otherwise it rings.
- Ringing loads that client's call engine on demand
  (`LiveConnect::attach_call_engine`). The new
  `ConnectDriver::start_engine_for_tracked_call` starts the already-tracked
  call in the engine, since `pump_call_engine` only does that when a call
  first appears.
- The app shows an "Incoming voice call" or "Incoming video call" prompt
  ("Alice is calling Work.") with Answer and Decline. The ringtone loops
  through `sync_call_sounds`. The OS call notification goes out once, with
  that account's tag, when no Quill window is in front.
- Answer switches to the account. Its client comes forward with the call
  still pending, then `accept_incoming_call` runs and the call window opens
  as it does for the account on screen. Decline discards the call on the
  background account and nothing switches. Esc hides the prompt and stops
  the in-app ringtone; the caller keeps ringing until the call ends.
- The notification's Accept, Decline and Open buttons work for the
  background account. The macOS and Windows callback accepts the ringing
  account's tag, and the picks are taken at render before the on-screen
  account's.
- The ring stops when the call stops pending: answered elsewhere, missed,
  or hung up. The toast is withdrawn then.

Difference from tdesktop: its call panel runs against the other account
without switching. Quill's call window draws the account on screen, so
Answer switches first.

## Account name in the window title

tdesktop `MainWindow::updateTitle` adds the signed-in user's name
(`authedName`) while `accountsAuthedCount() > 1`:
"Chat @ Name (3)", or "Name (3)" with no chat open. `window_title` already
took an account name. The main window and the chat windows now pass it
(`title_account_name`). The account on screen must be signed in, and at
least one background client must be running (a client still restoring its
session counts, one that needs a sign-in does not). tdesktop's
"hide account name" title option has no Quill equivalent yet.

## Main menu unread for other accounts

tdesktop marks the main menu button with a dot while another account has
unread messages (`dialogsMenuToggleUnread`, grey when all of it is muted),
and the accounts toggle in the menu shows their counter. Quill's menu
button now carries a kit `Badge` dot with the same colors. Its "Accounts"
item becomes a row with the counter pill used on the Accounts list rows.

## Proxy auto-switch per account

TDLib stores proxies in each account's database, so each background
client now runs `proxy_tick`: the first `getProxies`, then the auto-switch
(tdesktop `ProxyRotationManager`) with that account's own
`proxy_prefs.json`. tdesktop keeps one proxy list for the whole app.
Syncing the on-screen account's list into the others would rewrite other
accounts' TDLib settings, so it is left for a separate decision.

## Chat windows on a switch

tdesktop keeps a separate chat window on its own account when the main
window switches. Quill's chat windows draw the session on screen, so
leaving them open would show the incoming account's version of the chat,
or nothing. `close_chat_windows_for_switch` saves the outgoing account's
windows under that account (`chat_windows.json` is already per account),
flushes their drafts, closes their chats in TDLib and closes the windows.
The restore state is re-armed, so the incoming account's saved windows come
back once it is ready. Switching back reopens the closed ones.

## Verification

- Driver tests (`src/connect/tests/calls/late_engine.rs`): an engine
  installed while a call rings starts that call and can answer it;
  declining stops the ring; starting with no engine does nothing.
- `notify_accounts` tests: the title rule, one call at a time, and only an
  incoming pending call rings (outgoing and ended calls do not).
- GPUI tests (`--features demo-capture`): the call prompt opens for a
  ringing background account and closes on Decline; a dismissed prompt
  neither shows nor rings; the main menu counts the other accounts; a
  switch closes the chat windows and their chats while the main window
  keeps its chat.
- `gate.sh`, `check-file-size.sh`, `check-hotspots.sh origin/main` and
  `check-app-name.sh` pass.
- No real account was used, and no call was placed or answered.

`cargo test --features demo-capture --bin quill` has three failures that
are also on `main` (checked on `origin/main` d1d4a5e4): two `chat_window`
tests panic in `history_message_list.rs:489`, and
`chat_theme::tests::high_contrast_roots_palette` fails in the full run but
passes alone.

## Not done

- One proxy list for all accounts, as in tdesktop.
- Group call invites and video chats on background accounts.
- A setting to hide the account name in the title.
