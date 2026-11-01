# Closing TDLib clients off the UI thread

Quality sweep finding S-03: the UI thread blocked for up to 5 s per account
while it waited for TDLib's `close` to reach `authorizationStateClosed`
(`CLIENT_CLOSE_TIMEOUT`, `LiveConnect::shutdown` / `wait_closed`).

## Where it blocked

All of these ran on the UI thread and called `LiveConnect::shutdown(5 s)`:

- the background poll, when an account signed out or needed a sign-in
- `close_all` (lock-screen log out, data wipe) and `BackgroundAccounts`' Drop:
  one 5 s wait per client, one after another
- `stop_background_account` (removing an account)
- account switch: `retire_live_to_background` (an account in a call, a dead
  client, or one that needs a sign-in) and `take_background_live` (a dead
  background client)
- the lock-screen log out, for the client on screen
- `LiveConnect`'s Drop (the client on screen at quit)

## What TDLib and Telegram X do

TDLib's `close` flushes and closes the databases, then sends
`authorizationStateClosed`. After that the `td_json_client` client is
destroyed by TDLib on its own. Quill never destroys a client id (there is no
call for that). It only stops listening: the route is removed from the shared
hub, and the hub keeps `libtdjson` loaded for the life of the process. So a
deadline can't interrupt TDLib in the middle of a close. TDLib finishes the
close on its own threads, but nothing may open the same database until it
does.

tdesktop doesn't use TDLib, so it has nothing to copy here. Telegram X
(`Tdlib.java`) sends `Close` and returns right away. Work that has to come
after the close is queued with `awaitClose` and runs when
`AuthorizationStateClosed` arrives. Quill now follows the same pattern.

## What changed

- New `quill::connect::closing` (`ClosingSet`, `Closable`):
  - `push` sends `setOption online=false` (only when the client is Ready,
    as before) and `close`, then returns.
  - `poll` drains each closing client once without waiting, for at most
    4 ms. A client leaves the set only when TDLib confirms Closed. There
    is no deadline outside quit. `CLOSE_GRACE` (the old 5 s) only logs a
    slow close once.
  - A client whose receive loop died can't deliver Closed any more. It
    leaves the set as `Lost`, and its account goes on the set's
    "unconfirmed" list for the rest of the process: `may_be_open` stays
    true, so its database is never reopened or deleted in that run. The
    hub's `stopped` flag is shared by every client, so before this change
    a dead receive loop made every closing client look Closed.
  - `close_all_within(deadline)` is for quit. All clients share one
    deadline, with a 5 ms sleep between rounds. Clients still open at the
    deadline are finished and also marked unconfirmed.
  - A client in a call ends it first. tdesktop's `Instance::isQuitPrevent`
    hangs up unless the call is still ringing in, and holds quit up to
    1500 ms. Here `begin_close` sends `discardCall` or `leaveGroupCall`,
    and `close` follows once the terminal `updateCall` or the `ok` arrives,
    or after `HANG_UP_HOLD` (500 ms), which leaves `close` the rest of
    `QUIT_CLOSE_DEADLINE`. `discardCall` answers before the server has
    heard it, so the hold waits for the state, not the answer. tdesktop's
    group call has no leave on quit in practice: it destroys its sessions
    first. Quill sends one anyway, so the voice chat does not list you
    until the server drops you. An account switch closes a client in a
    call the same way. tdesktop keeps that call going on the old session;
    Quill's call UI reads the client on screen, so it hangs up instead.
- `LiveConnect` implements `Closable` through `begin_close_bridged` and
  `drain_bridged`. Both are generic over the sender, so the tests run the
  same code on a routed `ReceiveBridge`.
- UI (`src/ui/background_accounts/closing.rs`):
  - The closing set lives in `BackgroundAccounts` and counts as work for
    the background poll. `close_later` starts the poll when it isn't
    running. `stop_background_accounts` starts a poll for the new
    generation, so a closing client is drained even when nothing waits on
    it.
  - `after_closing(wait, then)` runs follow-up work once `wait` returns
    `Go`, or tells it `Never` (a close that can't be confirmed). The check
    and the work happen in the same update.
  - `closed(key)` lets `start_connection` wait, without blocking, until the
    account's previous client has closed and its folder isn't being
    deleted. If the close can't be confirmed it never opens and the status
    line says the connection was closed. A client still on screen is closed
    first, and when it belongs to the same account the new one waits for
    that close too.
  - Accounts closed only because of a call carry `restart` and start again
    in the background once the close is confirmed.
  - No client is dropped on the UI thread any more: clients that
    `ensure_background_accounts` forgets, the previous client in
    `start_connection`, and the logged-out client in
    `restart_live_connection` all go through the closing set. `QuillApp`'s
    Drop closes the client on screen and every background client under the
    one quit deadline (before, a drop without the quit observer could cost
    1.5 s for the client on screen plus 1.5 s for the background ones).
  - Quit: an `on_app_quit` observer moves the client on screen and every
    background client into the set and closes them together within
    `QUIT_CLOSE_DEADLINE` (1.5 s in total, not per client).
- Deleting account folders (`src/ui/background_accounts/deleting.rs`,
  `quill::settings::accounts::deletion`):
  - The folders to delete are written to `accounts.json` first
    (`pending_deletes`). They are deleted on a worker once the account's
    client has confirmed Closed and no preparation for it is running.
    `prepare_connect` creates the folders and can create a keychain item,
    so a preparation still running counts as "may be open". Only then is
    the record cleared.
  - If the app quits (or crashes) first, the next start deletes the
    recorded folders before any client opens. The first `start_connection`
    of a run sweeps them on its worker, and `prepare_startup` refuses to
    open an account whose folder is still recorded. We considered running
    the deletions in the quit hook after `close_all_within`, as the review
    suggested, and rejected it: deleting a TDLib folder with cached media
    has no time bound, so it would block the UI thread past the shared quit
    deadline. Deleting a folder whose client the deadline cut off would
    also break the database rule.
  - Removing an account takes it out of `accounts.json` at once, so it
    can't be switched to and `ensure_background_accounts` never restarts it.
    Its start in flight ends: the start callback returns unless the key is
    still in `starting`, and removal takes it out. Its call restart is
    cancelled. A second click on Remove does nothing. If the folder can't
    be deleted, the error shows in the account list, the record stays and
    the next start tries again. `add_account` doesn't hand out a key whose
    old folder is still recorded.
  - Lock-screen log out deletes the passcode file at once (so the next
    start doesn't lock), records every account's folder, and sets
    `wiping`. While `wiping` is set, `start_connection`, `switch_account`
    and `ensure_background_accounts` do nothing, and the connection
    generation is bumped so a connection already being prepared can't open.
    Once every client has closed and the folders are gone, the wipe starts
    the fresh connection itself. If a close can't be confirmed in this run,
    nothing connects and the next start finishes the log out.
- `connect-smoke` and `passkey-probe` keep the blocking 5 s
  `LiveConnect::shutdown`. They are command-line tools with no window.

## Measurements

Same machine (M-series Mac), debug build, `RecordingSender` driver with a
client that never answers `close`:

| Path | Before | After |
| --- | --- | --- |
| One client closed on the UI thread (poll tick, switch, removal) | 5.003 s, 5.000 s (two runs of the old `wait_closed`, 5 s timeout) | 34 µs for `ClosingSet::poll` over two clients |
| Lock-screen log out / wipe with N background clients | up to N × 5 s plus 5 s for the client on screen | no wait. The folders are deleted on a worker once every client has confirmed Closed. The window stays responsive |
| Quit with N clients | up to 5 s for the client on screen plus N × 5 s | 1.504 s for two clients (shared 1.5 s deadline), sooner when they answer |
| `QuillApp` dropped without the quit observer | 1.5 s (client on screen) + 1.5 s (background) | one shared 1.5 s deadline |

The review follow-up changes when work runs, not how long anything blocks,
so these numbers still hold. Folder deletion moved from the UI thread to a
worker.

## Tests

- `src/connect/tests/closing.rs` (driver tests). The clients run
  `LiveConnect`'s own close code on the recording sender, fed by a real
  routed `ReceiveBridge` of a test `TdHub`:
  - `push` sends offline, then `close`, and returns at once
  - a client that never answers costs one poll and is still waited for
    long after `CLOSE_GRACE`, until Closed
  - a client that answers Closed leaves the set (Closing alone doesn't),
    and its hub route is removed
  - a receive loop that dies is never taken for Closed: the account stays
    `may_be_open`
  - a Closed queued before the loop died still counts
  - per-account and global restart cancelling
  - quit with four silent clients takes one shared deadline and leaves them
    unconfirmed
  - quit returns as soon as every client is Closed
  - a Closed client, or one whose bridge was shut down, is sent nothing
  - dropping the set stays within the quit deadline
- `src/settings/accounts/deletion.rs` (unit): removal leaves the list at
  once and keeps the folder recorded, double removal, no key reuse while
  recorded, the log-out record survives a reload, the last account can't be
  removed.
- `src/ui/background_accounts/closing_tests.rs` (GPUI):
  - a silent client doesn't hold up `poll_background`, and a later Closed
    finishes it
  - a slow close is waited for, never dropped at a deadline
  - a dead receive loop keeps the account closed for the run
  - `after_closing` work runs only once its account is closed
  - switching to an account waits for its old client to close, and never
    opens it when the close can't be confirmed
  - `stop_background_accounts` doesn't wait, cancels restarts, and its new
    poll drains the client with nobody waiting
  - `close_all_for_quit` and the real `on_app_quit` observer use one
    deadline for three clients
- `src/ui/background_accounts/deleting_tests.rs` (GPUI, temporary app
  root, in-memory keychain):
  - a removed account's folder and keychain item go only after its client
    confirmed Closed. It leaves the list at once, a second click does
    nothing, and its call restart is cancelled
  - removal while a start is in flight: nothing is deleted until the
    preparation is done, and the ended start adds no client
  - an account closed for a call restarts once closed. A removed one
    doesn't, and neither restarts while still closing
  - lock-screen log out: the passcode goes at once, nothing connects while
    wiping, folders go only after both clients closed, then a fresh
    connection starts
  - quit before the clients closed: nothing is deleted under them, both
    the removal and the log out stay recorded, and the next start's sweep
    deletes them
  - the startup sweep skips accounts this run still has open
- Timing asserts allow three times the shared deadline and 500 ms for
  "doesn't wait", so a loaded CI machine doesn't flake.

## Not verified

- No run against a real TDLib (owner rule: no real account). The live path
  is the same `LiveConnect` ingest and `bridge.next_timeout`, called with a
  zero timeout, and the tests run that code on a routed bridge.
- After a receive-loop death (16 panics in a row), an account whose close
  was cut off can't be opened again until the app restarts. This is
  deliberate, since TDLib may still be closing its database, but the
  message is only the generic "connection was closed".
- Not tried on Linux or Windows hardware. The code has nothing
  platform-specific.

## Review follow-ups (re-review at 8028b955)

- Sweep race: `startup_sweep` now reads `pending_deletes` once, on the UI
  thread, drops the accounts this run still has open, and hands exactly
  that list to the worker. A removal made after the sweep began is never
  in it. Test: `the_startup_sweep_deletes_only_what_was_recorded_when_it_began`.
- Log out: if recording the folders fails, the passcode is kept, nothing
  starts, and the lock screen note says "Could not log out: ...". Test:
  `a_log_out_that_cannot_be_recorded_keeps_the_passcode_and_says_why`
  (Unix only, read-only root).
- A wipe whose close can't be confirmed now shows "Restart Quill to finish
  logging out. Nothing connects until then." instead of the generic
  closed-connection note. Test:
  `an_unconfirmed_log_out_tells_the_user_to_restart_quill`.
  Both strings are in `lang/en/rest.strings`; `scripts/check-lang.py` passes.
