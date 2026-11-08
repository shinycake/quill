# 1:1 call transport as one value

The 1:1 call's transport used to live in four places. `ActiveCall` had `transport: Option<TransportState>` and `transport_error: Option<String>`. `ConnectDriver` had `call_connect_params` and `reconnect_attempts`. `pump_call_engine` worked out each change by comparing a before-snapshot of the call id, and it reset the attempts in five places. Nothing stopped an error from sitting next to `Connecting`: an engine `start_call` error was written without touching `transport`.

## Changed
- `ActiveCall.transport` is now a `CallTransport` (src/state/call_types.rs). Its variants are `NotStarted { start_error }`, `Connecting`, `Connected`, `Reconnecting`, `Closed` (each holding a `CallLink`: the retained `ConnectParams` and the reconnect count) and `Failed { reason, link }`. `state()` gives the old `TransportState` to the UI and the engine API, and `error()` gives the error line. Neither can disagree with the phase.
- The driver fields are gone. A new call starts as `NotStarted` because its `ActiveCall` is new, so the resets on "a new call id" and "the call ended" go away. `end_active_call` copies `reconnect_attempts` into the summary itself.
- Engine reports go through `apply_transport_report`. A `Failed` report retries the retained parameters up to `MAX_CALL_RECONNECT_ATTEMPTS` (3) while the engine is available. Any other report moves to that phase, and `Connected` starts the count over.
- The post-ingest pumps (`run_post_ingest_pumps`, src/connect/core/pumps.rs) all run and return the first error at the end. Before, a 1:1 signaling send error skipped the group-call pump (and lost its "before" ids for that update), call media, call audio, `setTdlibParameters` and the membership probe.

## Behaviour kept
Quill connects once per Ready call. An empty encryption key gives `Failed` with that reason. Up to 3 reconnects run on `Failed`. The summary records the reconnects since the last `Connected`. The mute, camera, screen-share and device paths still only store the intent until a connect was attempted (`started()`).

## Small differences
- An engine report for a transport Quill never connected (no `CallLink`) is now dropped. Before, it was copied in. The engine only reports after `connect`, so this does not happen in the app.
- A start error that arrives after a connect no longer sits next to a live phase. A `Failed` transport takes it as its reason. A live transport ignores it.

## Verified
New tests in src/connect/tests/calls/transport.rs cover a start error that never shows as connecting, a signaling send failure that still sends `setTdlibParameters`, and the summary's reconnect count. The existing call tests pass, and so does the gate.
