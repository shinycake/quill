# Voice and music player: the engine is the source of truth

Problem: the play button, seek bar, time label and Now Playing read a wall-clock `PlaybackClock`,
resumed before the sound started. A failed start or resume still showed Playing with an advancing bar.

Now `AudioEngine` (src/ui/audio/engine.rs) owns `state()` (Idle, Playing, Paused, Ended, Failed) and
`position_secs()`. The position is the frames the mixer pulled from the decoder (`Counted`, below the
speed stretcher, so it is track time at any speed; a seek resets it, a decoder that cannot seek starts
at 0 and says so). `PlaybackUi` keeps only the track length; `is_playing()` and `active_position_secs()`
derive from the engine for the row, the player bar, Touch Bar, dock menu, media keys and Now Playing.
A failed start leaves the row active and Failed (Play retries); a failed resume or seek leaves the
previous state.

The screenshot demos have no output device and use `Simulation` (explicit, set only when a demo session
is running; `QUILL_DEMO_AUDIO_FAIL=1` shows the failed start). Real playback never falls back to it.

Verified: engine tests drive a real rodio mixer by hand (no hardware); demo captures in light and dark for
playing, paused and failed.

## Review round (PR #898)

- The failure text was a UI-side mirror (`playback.error`, set by hand on start, resume and seek), so a failed
  seek left the text up while the engine kept playing and a later good seek did not clear it. The engine now
  keeps its last error (`AudioEngine::error()`): set by a failed start, resume or seek, cleared by the next one
  that works and by `stop`. The audio row reads only that. `playback.error` stays for the video viewer.
- `AudioError` Display stays for logs. The row maps the error kind to `qll_av_*` lang keys in sentence case
  ("No audio output device.", "Couldn't open the audio file.", "This audio format can't be played.").
- `track_duration` was a second hand-kept "a track is active" flag. The field is now `track_length` (plain
  data) and `PlaybackUi::track_duration()` returns it only while `playing_voice` or `playing_audio` is set, so
  activity has one source and a stale length cannot show a phantom track.
- The player bar's speed label used the video overlay's fixed white text and vanished on the light surface.
  The bar now sets `text_primary()` on that trigger (the shared helper is in `media_viewer/`, left alone).
- Tests: in-place seek while playing, speed change mid-track, error kept and cleared (start, seek, resume),
  a stale length is never active.
