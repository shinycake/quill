# Poll loop pacing

Supersedes the cadence in `codex-adaptive-poll.md`, the "busy cadence" note in
`codex-ingest-budget.md` and the `next_poll_delay` rules in `sweep-idle-app-timers.md`. The 8 ms
budget itself stays.

## What the loops do now

Both poll loops (`spawn_poll_loop`, `spawn_background_poll`) run through one function,
`wake_wait::run_poll_loop`, and one pure rule, `PollPace::next`. A pass reports a `PassEnd`:
whether work is known to be left, and when timed work is due.

- Work left, or a poke landed during the pass: wait `gap` on a plain timer. Pokes cannot end it.
- Otherwise rest until a poke, the due time or the ceiling (2 s), whichever comes first.
- A poke that ends a rest inside the gap waits out the rest of the gap.

`gap` is 10 ms on the main loop and 40 ms for background accounts.

## Why a plain timer

A Rust future holds the main thread until it returns `Pending`. `sleep_or_wake` returns `Ready`
on its first poll when the epoch moved, and the receive thread pokes after every envelope, so
under a stream of updates the loop ran pass after pass inside one poll. All three platform
dispatchers run a foreground task until it returns, so no frame or input ran until TDLib went
quiet. The budget only split the work into passes that had no gap between them. This broke when
the poke wait replaced the fixed tick (2026-10-16).

## Why no ramp

The 10 ms to 2 s ramp came before `wake::UPDATES`. Every async source now pokes, so after an
update it only added polls that found nothing, about 8 per update. A batched redraw no longer
needs a 120 ms cap, because `PolledRedraw::next_due` says when it lands.

Things that relied on the ramp without knowing it:

- A paid reaction's countdown and commit: `poll_ticks` reported progress on every pass while one
  waited, which kept the loop at 10 ms. `next_deadline` now names the next change of the number.
- An account export reports progress with no poke, so `poll_live` still asks for the next pass
  while one runs. Same cadence as before.
- `poll_device_qr` and `drain_account_notices` read what the reducer set, and ran before the
  drain, so they needed a second pass. They run after it now.

Anything else with no poke waits at most the ceiling, as it did at rest before.

## The kept timer

A GPUI timer cannot be cancelled: dropped, it still fires on a worker. Each poke used to leave
one behind, firing 2 s later. `RestTimer` keeps the pending timer for the next rest unless that
rest must end sooner. At most one extra pass per ceiling comes of it, which the time-driven
checks want anyway.

The tray loop in `main.rs` still leaves a 10 s timer per applied pass. `main.rs` is a hotspot, so
it is left as a follow-up.

## Trade-off

A long backlog drains at about 8 ms of work per 18 ms. The old loop ran passes back to back for
as long as updates kept arriving, which was faster only because nothing else ran. 300,000 updates
arriving in 2 s took 10.6 s to apply instead of 6.7 s, and the window stayed at 60 frames a second
for all of it.

## Why 10 ms

It is the old busy cadence. A 2 ms gap drained that flood 17% faster, because frames took most of
the time it freed: passes only rose from 49 to 64 a second. Under a steady 2,000 updates a second
the same gap tripled the passes, from 70 to 218 a second, for updates nobody can see 8 ms sooner.
