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
`Wanted` and leaves every other state alone. Only evidence that a join went
through moves back to `Idle` (see "What ends a rejoin" below). The driver's auto-rejoin starts
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

## What ends a rejoin

The first version moved any state to `Idle` on every `updateGroupCall` with
`is_joined` set and `need_rejoin` clear. That is only right when TDLib armed
the rejoin. A stream's part fetch can come back "join missing" while TDLib
still says joined, and the feeder arms the rejoin itself. Quill then starts
`InFlight` and stops the native context, and the next routine update (a viewer
count) still says joined, so the rejoin went back to `Idle`: the banner went
away, the request id was forgotten, and a failed join was never retried. The
stream stayed dead with no banner.

A routine joined update is no evidence of a rejoin. Two things are:

- The rejoin's own join answer: the `joinVideoChat` text (or the conference
  `groupCallInfo`) for the request `InFlight` names
  (`GroupCallRejoin::attempt_joined`). An answer to any other request leaves
  the state alone.
- A joined update after a not-joined one (`GroupCallRejoin::rejoined`). TDLib
  reports `is_joined` false while it is kicked and while a join is being
  processed, so the edge means a join went through. This keeps TDLib-driven
  rejoins ending on the joined update, as before.

Neither needs a flag next to the state: the edge is read from the tracked
`is_joined` before the update is applied. One case is left open: a call that
never had a joined update before the stream armed its rejoin would read its
first joined update as the edge. A joined stream has always seen one.

The "Could not join" / "Reconnect attempts exhausted" line is cleared by a
joined update only once the rejoin is `Idle`, so a spent stream rejoin keeps
its line while TDLib still says joined.

Tests: `a_routine_joined_update_does_not_end_a_stream_rejoin` in
`src/connect/tests/group_calls/stream.rs` drives the stream case: a part
fetch says join missing, a viewer-count update arrives, the rejoin fails and
is retried, and only its join answer ends it.
`src/connect/tests/group_calls/rejoin.rs` covers the banner from drop
to rejoined update, a late `need_rejoin` update not sending a second join,
and failed rejoins returning to `Wanted` until the cap, then the manual
restart.
