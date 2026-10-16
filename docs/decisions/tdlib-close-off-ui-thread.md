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
    4 ms. A client is finished and dropped when it reaches Closed or when
    its deadline passes (`CLOSE_GRACE`, the old 5 s, which no longer
    blocks anything).
  - `close_all_within(deadline)` is for quit. All clients share one
    deadline, with a 5 ms sleep between rounds.
- `LiveConnect` implements `Closable`. A dead receive loop counts as closed
  at once, as before.
- UI (`src/ui/background_accounts/closing.rs`):
  - The closing set lives in `BackgroundAccounts` and counts as work for
    the background poll, which drains it every tick.
  - `after_closing(ready, then)` runs follow-up work once the set satisfies
    `ready`. The check and the work happen in the same update. It drives
    the drain itself, so it also works while the background poll is
    stopped (after `stop_background_accounts`).
  - `closed(key)` lets `start_connection` wait, without blocking, until the
    account's previous client has closed, so two clients never open one
    database.
  - Accounts closed only because of a call carry `restart`. The background
    client starts again once the old one has closed. `ensure_background_accounts`
    skips any account that is still closing.
  - Quit: an `on_app_quit` observer moves the client on screen and every
    background client into the set and closes them together within
    `QUIT_CLOSE_DEADLINE` (1.5 s in total, not per client).
    `BackgroundAccounts`' Drop and `ClosingSet`'s Drop use the same shared
    deadline as a last resort. `LiveConnect`'s Drop now waits at most 1.5 s.
    By the time it runs, the quit observer has normally finished the client
    already, so it returns at once.
- Account removal (`remove_account_key`) deletes the folder and keychain
  item after the close, through `after_closing`. If the account was
  switched to in the meantime, nothing is deleted.
- Lock-screen log out wipes local data and starts the fresh connection
  after every client has closed. A wipe cancels any pending restarts.
- `connect-smoke` and `passkey-probe` keep the blocking 5 s
  `LiveConnect::shutdown`. They are command-line tools with no window.

## Measurements

Same machine (M-series Mac), debug build, `RecordingSender` driver with a
client that never answers `close`:

| Path | Before | After |
| --- | --- | --- |
| One client closed on the UI thread (poll tick, switch, removal) | 5.003 s, 5.000 s (two runs of the old `wait_closed`, 5 s timeout) | 34 µs for `ClosingSet::poll` over two clients |
| Lock-screen log out / wipe with N background clients | up to N × 5 s plus 5 s for the client on screen | no wait; the wipe runs once all are Closed (at most 5 s later, window responsive) |
| Quit with N clients | up to 5 s for the client on screen plus N × 5 s | 1.504 s for two clients (shared 1.5 s deadline), sooner when they answer |

## Tests

- `src/connect/tests/closing.rs` (driver tests, recording sender):
  - `push` sends offline, then `close`, and returns at once
  - a client that never answers costs one poll and is dropped at its
    deadline
  - a client that answers Closed leaves the set (Closing alone doesn't)
  - a Closed client is sent nothing
  - quit with four silent clients takes one shared deadline, not four
  - quit returns as soon as every client is Closed
  - dropping the set stays within the quit deadline
- `src/ui/background_accounts/closing_tests.rs` (GPUI):
  - a silent client doesn't hold up `poll_background`, and a later Closed
    finishes it
  - a client past its deadline is dropped without a wait
  - `after_closing` work runs only once its account is closed, and isn't
    held up by another account
  - `stop_background_accounts` doesn't wait and cancels restarts
  - `close_all_for_quit` uses one deadline for three clients
  - the real `on_app_quit` observer (`cx.quit()`) closes every client
    within the quit deadline
- The existing `wait_closed` test and the background-account tests pass
  unchanged.

## Not verified

- No run against a real TDLib (owner rule: no real account). The live path
  is the same `LiveConnect` ingest and `bridge.next_timeout`, now called with
  a zero timeout.
- Not tried on Linux or Windows hardware. The code has nothing
  platform-specific.
