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

The 10 ms to 2 s ramp came before `wake::UPDATES`. After an update it mostly added polls that
found nothing, about 6 per update. A batched redraw no longer needs a 120 ms cap, because
`PolledRedraw::next_due` says when it lands.

Things that relied on the ramp without knowing it:

- A paid reaction's countdown and commit: `poll_ticks` reported progress on every pass while one
  waited, which kept the loop at 10 ms. `next_deadline` now names the next change of the number.
- An account export reports progress with no poke, so `poll_live` still asks for the next pass
  while one runs. Same cadence as before.
- `poll_device_qr` and `drain_account_notices` read what the reducer set, and ran before the
  drain, so they needed a second pass. They run after it now.
- A user avatar a render finds missing is claimed for the next pass, and the pass that led to
  that render has ended. A claim the next pass will send pokes. A repeat, a file in flight, a
  failed file and data saver do not, or a list with a stuck download would poke every frame.
  `avatar_claim_sends` decides for the poke and the send alike, so the two cannot disagree. A
  pass that sends a download asks for no redraw, so the poke cannot feed itself.
- The chat export worker writes its progress and last stage to a slot only a pass reads, and the
  Writing stage can go a long way with no request. Both writes poke now.
- These clicks leave work for the next pass, so they poke: a queued deep link, the Translate
  toggle and any Translate setting, opening Settings for the grams probe, and a linked media
  timestamp. That timestamp counted 100 passes before giving up, which is 200 s on a quiet
  account. It has a 12 s deadline now, close to what the count came to at the 8 polls a second
  its comment assumed.
- A picked Translate language took two passes: one followed the language and sent nothing. One
  pass does both now.
- The profile photo strip's thumbs are asked for on open, on a step and by a pass. The pass asked
  before it added a page's tiles, so it asks after now. The strip's width comes from a frame,
  which the open has not drawn yet, so a new width pokes. A frame at the same width does not, and
  a pass that asks for thumbs asks for no redraw, so the poke cannot feed itself.
- A group call stream takes 64 events a pass and pokes when it takes the 64th, for what is left.

Anything else with no poke waits for the next pass, up to the ceiling, which is what it waited at
rest before. A step whose result only a later pass reads adds a ceiling each, where the ramp added
10 ms. The cases above are the ones found by reading the code, so there may be others.

## The kept timer

A GPUI timer cannot be cancelled: dropped, it still fires on a worker. Each poke used to leave
one behind, firing 2 s later. `RestTimer` keeps the pending timer for the next rest unless that
rest must end sooner. At most one extra pass per ceiling comes of it, which the time-driven
checks want anyway.

The tray loop in `main.rs` still leaves a 10 s timer per applied pass. `main.rs` is a hotspot, so
it is left as a follow-up.

## Trade-off

A long backlog drains at about 8 ms of work per 20 ms, from 49 passes a second. The old loop ran
passes back to back for as long as updates kept arriving, which was faster only because nothing
else ran. 300,000 updates arriving in 2 s took 10.6 s to apply instead of 6.7 s, and the window
held 60 frames a second through the stream.

## Why 10 ms

It is the old busy cadence. A 2 ms gap drained that flood 17% faster, because frames took most of
the time it freed: passes only rose from 49 to 64 a second. Under a steady 2,000 updates a second
the same gap tripled the passes, from 70 to 218 a second, for updates nobody can see 8 ms sooner.

## Background accounts

They get 4 ms per account in about 44 ms, 9% of the thread. At the demo's rough 50 µs per update
that is about 1,800 updates a second for each account, an estimate that no run has checked.
Notifications and badges lag behind a backlog by that much. A 10 ms gap with the same budget is
the cheap change if it ever hurts.
