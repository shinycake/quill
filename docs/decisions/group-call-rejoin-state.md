# Group-call rejoin as one state

Date: 2026-11-07

## What was wrong

A tracked group call kept its own rejoin in two loose fields next to TDLib's
`need_rejoin`: a `reconnecting` flag and a `rejoin_attempts` counter. The flag
did two jobs. It told the driver a rejoin should be sent, and it showed the
"Connection lost" banner (and hid your own row behind "Connecting").

The driver cleared the flag as soon as it started a rejoin, so the banner went
away and your row came back while the offer was still being built and
`joinVideoChat` was still in flight. Worse, every `updateGroupCall` that still
carried `need_rejoin` set the flag again. Once the offer had gone out, nothing
guarded against that, so an update older than the join answer started a second
rejoin: it tore down the native context in the middle of the handshake and
used up an attempt.

## What changed

`ActiveGroupCall::rejoin` is now one `GroupCallRejoin` value:

- `Idle`: joined, or never dropped.
- `Wanted { attempts }`: a rejoin is due, and `attempts` were already spent.
- `InFlight { request, attempt }`: attempt number `attempt` is being made. Its
  offer is being built or its join request (`request`) is waiting for TDLib.
- `GaveUp`: every attempt failed, and only the manual Rejoin is left.

`need_rejoin` (or a stream feeder asking for a rejoin) moves `Idle` to
`Wanted` and leaves every other state alone. A joined update without
`need_rejoin` moves any state back to `Idle`. The driver's auto-rejoin starts
only from `Wanted` with attempts left, and records the request id as it
starts. A failure counts only when it matches that id: a failed offer or a
TDLib error on the request moves to `Wanted { attempts: attempt }`, or to
`GaveUp` at the cap. A send that never reached TDLib is not counted. The
manual Rejoin resets the attempts. While a request is pending it returns that
request instead of starting another one. It may restart a rejoin whose request
is gone (answered, or its offer was dropped), because nothing else would get
the call out of that state.

The UI reads derived methods. `reconnecting()` (anything but `Idle`) drives
the banner and the connecting self row, so they stay up for the whole rejoin,
the spent state included, because the banner is the manual Rejoin.
`rejoining()` (`Wanted` with attempts left, or `InFlight`) keeps its meaning
for the stream PiP. `join_failed()` and the conference "dead" check read
`GaveUp`. The JoinVideoChat error arm used to tell a rejoin from a first join
by `rejoin_attempts > 0`. It now matches the failing request against the one
in flight.

The transport resets on rejoin (`transport_ready`, `join_payload`, the
screen-share fields) stay where they were. They belong to the native context
being replaced, not to the rejoin's progress, so folding them in would not
remove a writer.

Tests: `src/connect/tests/group_calls/rejoin.rs` covers the banner from drop
to rejoined update, a late `need_rejoin` update not sending a second join,
and failed rejoins returning to `Wanted` until the cap, then the manual
restart.
