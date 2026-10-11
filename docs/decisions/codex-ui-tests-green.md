# UI tests green and in CI; one proxy list for the app

## The three failing UI tests

`cargo test --features demo-capture --bin quill` failed three tests on
`main`. No gate or CI job ran that command, so nothing caught it.

### Two chat window tests (`history_message_list.rs:489`)

The panic was "range end index 2 out of range for slice of length 1". It
came from 1ba0a18a ("Chat themes recolour the chat; … rotation on send"):
the check that turns a freeform wallpaper when a message is sent looked at
the appended rows with `self.history.rows[prev..count]`. At that point
`self.history.rows` still holds the old, shorter list; the new rows are the
local `rows`, stored a few lines later. Any appended row with the history
following the tail panicked; the app runs the same path when a
message arrived or was sent in an open chat. The check now reads the new
list.

### `chat_theme::tests::high_contrast_roots_palette` (order dependent)

The palette mode (dark or light) and the high-contrast flag were
process-wide atomics. Each GPUI test builds a `QuillApp`, and applying its
appearance sets both: the System theme is light on the test platform. The
unit test asserts the dark tokens after turning high contrast off, so it
failed whenever a GPUI test had run before it, and could race one running
beside it. The app has a single UI thread and keeps the atomics. Under
`cfg(test)` the state is thread-local, so each test, and each GPUI test app
on its thread, has its own palette. `chat_palette` already keeps its chat
recolouring per thread the same way.

Result: 414 passed, 0 failed, three runs in a row. The full run takes about
6 s of test time on an M1 Pro, after the build.

## UI tests in CI

The Linux job (`linux-fmt-clippy-test`, the required check) now installs
the packages the UI binary links and runs:

```
cargo test --features demo-capture --locked --bin quill
```

GPUI's test platform needs no display: `TestPlatform` uses a no-op text
system and no window server, so there is no Xvfb. The same command is the
one to add to `quill-tools/gate.sh`.

## One proxy list for the app

tdesktop keeps proxy settings app-wide (`Core::Settings::proxy()`: the
list, the selected proxy and "try IPv6"), and every account connects
through them. TDLib keeps them per client, in each account's database.
#714 had each background account run its own auto-switch on its own list;
this replaces that.

- `src/proxy_sync.rs`: `plan_proxy_sync(target, current)` lists the
  changes that make a background account's list match the account on
  screen. Proxies match by their full connection data (kind, address,
  login, secret), because TDLib ids differ between accounts. The steps
  remove extra proxies, add missing ones, enable the app's selected proxy,
  or go direct.
- `ConnectDriver::sync_proxies_to` fetches the account's list, then sends
  one change at a time; each change refetches the list, and the next poll
  plans again. It also mirrors "prefer IPv6".
- The background poll calls it with the on-screen account's list once that
  list is loaded and no change of it is in flight. Only the account on
  screen runs the auto-switch. The proxy it picks reaches the others like
  any other selection.
- The auto-switch settings (`proxy_prefs.json`) move to the app root. An
  account folder's old file is still read until the app file exists. A
  client that comes on screen re-reads them.
- Nothing new is written to disk with proxy data: the proxies, with their
  logins and secrets, stay in TDLib's encrypted databases.

The first sync after updating makes every other account's list match the
account on screen. Proxies that only another account had are removed from
it, as in tdesktop, where only one list exists.

## Verification

- `gate.sh`: GATE OK (core 3666, ui 312). The full UI suite passes (414),
  and `check-file-size.sh`, `check-hotspots.sh origin/main` and
  `check-app-name.sh` pass.
- `proxy_sync` tests: matching lists need nothing whatever the ids, missing
  proxies are added and extra ones removed, a changed login counts as a
  different proxy, the selection follows the app, and removing an enabled
  extra needs no separate disable.
- Driver test (`connect/tests/proxy.rs`): a background account fetches its
  list, removes the extra proxy, adds the missing one enabled and mirrors
  IPv6, one change per call, then sends nothing more.
- Settings test: proxy prefs are app-wide, and an old account file is read.
- No real account or proxy was used.
