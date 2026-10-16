# Audio seek never waits on the audio thread

Problem: `AudioEngine::seek` called `rodio::Player::try_seek`, which blocks until the audio thread applies the
seek. That ran on the UI thread, so a stalled or rerouting output device froze the window (in a test, a seek
with nothing pulling from the mixer never returned). `start`, `resume`, `pause`, `set_volume`, `set_speed` and
`stop` only set atomics or take short locks in rodio 0.23, so `try_seek` was the one call that waited.

Design: the engine posts a seek instead of performing it.
- `SeekBoard` (shared with a per-player worker thread) holds the newest queued target, the target in flight,
  and the last outcome. `seek()` writes the target and returns; a worker thread (spawned only when none is
  running for that player) hands the newest target to `try_seek` and loops until nothing newer is queued.
  Scrubbing never piles up threads and a stalled device costs one parked thread, not the UI.
- `position_secs()` returns the pending target while a seek is unconfirmed (`seek_pending()`), then the
  frames the mixer pulled (the audio thread resets that count when it applies the seek). The bar therefore
  sits at the target at once and is never a UI-side mirror.
- `settle()` (run on the 250 ms tick and before every engine command) consumes the outcome. Confirmed: clears
  the error. Refused (decoder cannot seek in place): restarts the track at the target as before, on the UI
  thread (file open and decode setup, no wait on the audio thread); a failure leaves the old player playing
  and sets `error()`. Until settled a refused seek still reads as pending at its target, so the bar does not
  jump back and forth.
- A new `Player` (start, restart) gets a fresh board, so an old worker can never report into a new track.
  `stop` drops the board; a worker whose player was stopped gets its channel closed and exits.

Verified: engine tests with a hand-driven rodio mixer (no hardware): the seek returns while nothing pulls and
the position is the newest target, then the confirmed position once the device pulls; a refused seek restarts
at the target; a refused seek whose restart fails keeps Playing, the previous position and `AudioError::Open`.
The "never pulls" test needs no clock; before the change it hangs. Demo captures of `ready-audio` after a
scrub click in light and dark.
