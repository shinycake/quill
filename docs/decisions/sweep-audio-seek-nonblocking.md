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

## Review round: a stalled device says so

- A seek the audio thread leaves unanswered no longer looks like a healthy bar. Each posted seek carries the engine
  clock reading at post time (`Seek { target, posted }`), and it keeps it when the worker hands it to the audio
  thread. `AudioEngine::error()` derives `AudioError::DeviceStalled` on every read while the in-flight seek is
  `SEEK_STALL_AFTER` (2.5 s) old, and drops it as soon as the device answers. Scrubbing replaces the queued seek,
  not the one in flight, so a live device that answers in milliseconds never trips it. The row's error line already
  reads `audio.error()`, so the UI gets the text from the engine with no mirror: "The audio device isn't responding."
  (`qll_av_the_audio_device_isnt_responding`). The bar still shows the target; the state stays `Playing`.
- The clock is `EngineClock`; tests drive a manual one, so the bound is tested at 2499 ms and 2500 ms without sleeping.
- The offline demo simulation gets the same behavior for screenshots: `QUILL_DEMO_AUDIO_STALL=1` makes every seek hang.
- Test determinism: `pull_until_settled` pulls one frame at a time and only while the audio side owes an answer to a
  seek it was handed (a test-only count of handed vs handled seeks). The position cannot drift past the target by
  more than a few frames whatever the worker's scheduling, so the windows are now 0.2 s wide.

Follow-ups, not done here:
- A worker blocked in `Player::try_seek` on a stalled device is parked until the source is pulled or dropped, so each
  track seeked during a stall leaks one thread. A single worker per engine would not help: it would sit behind the
  blocked call and the next track's seeks would never be delivered. Fixing it needs a seek with a deadline or a way to
  drop the source from rodio's side; bounded by user actions meanwhile.
- A refused seek still restarts the track (file open and decoder setup) on the UI thread in `settle`. The old code did
  the same; moving the open to a worker needs the new `Player` handed back through the board.

Both follow-ups above are handled in `sweep-audio-stall-followups.md` (the leaked worker is released when the
device answers; the restart is built off the UI thread).
