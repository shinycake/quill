# Session recovery (R2 + R7)

## What tdesktop does

When the server terminates a session (`AUTH_KEY_UNREGISTERED` and friends) tdesktop shows a notice and returns to the sign-in screen. It never leaves a dead window, and the connection strip stays quiet until the first state change after launch.

## What changed

- **Unexpected `authorizationStateClosed`.** The Closed view now reads "Connection closed / The connection to Telegram was closed." with a Retry button that calls `restart_live_connection`. Logout (`LoggingOut` then `Closed`, including a server-side session revoke, which TDLib reports as `LoggingOut` first) already restarts into the sign-in screen through the `saw_logging_out` latch and is unchanged. Only a Closed with no logout in sight reaches the Retry card.
- **Bridge liveness.** `ReceiveBridge::spawn_*` return `io::Result` instead of panicking (`ConnectBlocker::ReceiveThread`). A guard marks the bridge `stopped_unexpectedly()` when the receive loop ends without a shutdown request (closed channel, panic). `poll_live` drains the queue first, then shows the same Closed/Retry card (`connection_lost`). `LiveConnect::shutdown` skips the 5 s close wait for a dead bridge, so Retry is instant.
- **Unparsable responses.** `copy_and_parse` still drops the payload, but when `@extra` (string or number) can be read from the raw JSON it delivers a synthetic `error` (code 500, fixed message, the raw text is never copied) so the pending request resolves and the UI shows a failure instead of a spinner. Updates without `@extra` are dropped as before.
- **Initial connection state.** `ConnectionState::Initial` is the new `Session` default and renders no indicator, so the offline banner no longer flashes at startup. `is_offline()` still treats it as not yet online.

## How verified

- Unit tests: bridge unexpected-stop vs requested shutdown, unparsable response with/without `@extra` (including a canary that must not reach diagnostics), `bridge_lost` gating, Closed auth view, fresh-session indicator.
- `bash quill-tools/gate.sh`: GATE OK.
- Demo capture `connection-closed` for the Retry card (English).
- Not verified live: a real server-side session revoke or a crashing receive thread (no live account).
