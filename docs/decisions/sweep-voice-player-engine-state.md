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
