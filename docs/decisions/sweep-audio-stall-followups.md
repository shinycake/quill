# Audio: a stalled track lets go of its worker, and the seek restart leaves the UI thread

Follow-ups left by `sweep-audio-seek-nonblocking` (PR 905).

## What tdesktop does

tdesktop plays through its own audio thread (`Media::Audio`) and never blocks the UI on it. It has no equivalent of
rodio's blocking `Player::try_seek`, so this is Quill-specific hardening and nothing in tdesktop changes the design.

## Worker lifetime belongs to the track

- `Track` (player + `SeekBoard` + probe) is the one owner. `Drop for Track` cancels the board and stops the player,
  so starting another track, `stop()` and dropping the engine all end the old one the same way. The engine holds
  `Option<Track>`; there is no separate player/board/probe to get out of step.
- `Player::try_seek` cannot be interrupted from outside rodio. A worker blocked on a stalled device stays parked
  until the output pulls again. The change is what happens then: the stopped player answers the pending seek on
  its next 5 ms access, the worker sees `cancelled` and returns without taking the board lock, so it never writes
  an outcome into anything. A worker that is not blocked checks `cancelled` before taking its next seek too.
- `WorkerGuard` counts live workers per engine (`live_workers()` in tests), so "no thread left behind" is a tested
  fact, not a belief.
- Still true: a device that never answers keeps one OS thread parked per seeked track. Releasing it would need a
  seek with a deadline in rodio.

## The restart after a refused seek is built off the UI thread

- `settle()` no longer calls `begin`. A refused seek starts a `Restart { target, slot }`: a thread
  (`quill-audio-restart`) opens and decodes the file and builds a held (paused) `Player`, then drops the result in
  the slot. The engine thread only fetches the mixer (the `SharedOutput` is not `Send`) and later swaps the
  finished player in on the next `settle()` (250 ms tick or any command). The engine has no GPUI handle, so it uses
  a plain thread like the seek worker rather than the executor.
- UI state stays a function of engine state: while a `Restart` exists `position_secs()` and `seek_pending()` read
  its target; the old sound keeps playing until the swap. A newer seek only retargets the restart (one thread at a
  time; a result built for an older target is discarded and rebuilt). A seek while paused, `start`, `stop` and any
  new track drop the restart. A pause during the build survives: the new player starts held and is played only if
  the old one was not paused. A build failure sets `error()` and leaves the old sound, as before.
- Still on the UI thread: restarting an ended track (`seek_to` with an empty player) because `seek` returns that
  error synchronously.

## Verified

Engine tests with the hand-driven mixer: a stalled track replaced by a new one (error clears, both workers parked,
the device wakes, count goes 2 to 0, the old board has no outcome, the new seek lands); stopping cancels the board;
a refused seek with a pause during the rebuild ends paused at the target; a seek during a rebuild retargets it.
No UI change, so no screenshots: the voice row reads `audio.error()` and `position_secs()` exactly as before.

## Follow-up: the flaky engine test was a position drift in the test, not a lost seek

`a_replaced_stalled_track_lets_its_worker_go_without_touching_state` failed now and then with `got 3.384` (it expects
2.95 to 3.3 seconds). Earlier follow-ups (#934, #936) chased the neighbouring `a_seek_while_a_restart_builds_retargets_it`;
on current main that test did not fail in about 50,000 in-process repeats at a load average of 120, and the sequence it
drives is deterministic (the seek is answered first, the rebuild is awaited without a tick, nothing pulls after the
rebuilt track lands).

What failed is a harness accounting bug, not an engine bug. The test lets the device wake with `pull_until`, which pulls
one frame on every turn until both seek workers have exited. The new track's seek is answered within one 5 ms access,
and from then on every turn keeps playing the sound while the old worker is still waiting to be scheduled. The sound
runs on at device speed, so the position ends up as far past the seek target as that wait is long. On a quiet machine
the wait is microseconds; under load it is hundreds of milliseconds.

The engine is fine: a seek is posted, answered by the audio thread and recorded on the board on every ordering that was
tried, and the stopped track's worker leaves without writing. Production never pulls "until a thread is scheduled"; the
device pulls at its own rate and the position is read from the frames it took.

Fix, test side only. `pull_while_awaited` pulls the device only while a seek the audio side still owes an answer for is
outstanding, on the current track or on the replaced one, the same gate `pull_until_settled` already uses. A new
test-only `SeekBoard::answer_delay_ms` makes the worker slow to notice an answer (600 ms in this test), so the ordering
that used to happen only under load happens every run. Every wait in the engine tests is bounded by a 30 s deadline.

Evidence, running the `audio::engine` tests in a loop with 4 copies at a time: with the old `pull_until` and the 600 ms
worker, 96 of 100 runs failed (`got 3.7xx`); with `pull_while_awaited`, 0 of 100. With 32 copies at a time plus 16
busy-loop processes, 0 of 1,280. The natural failure rate on main was two failures in roughly 9,000 module runs at that load.
