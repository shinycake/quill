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
