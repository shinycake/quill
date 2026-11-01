# audio-start: a track starts off the UI thread

## tdesktop
`media/audio/media_audio.cpp`: `Mixer::play` sets the track's state and position at once, with
`loading = true`, and a thread does the rest. `ShowPauseIcon` is true for Starting and Playing,
so the button says Pause from the press on. A local track shows no spinner.
`media/streaming/media_streaming_document.cpp` shows the waiting ring only after
`kWaitingShowDelay`, 500 ms.

## What changed in Quill
- `AudioEngine::start`, a resume after the end, and a seek the decoder can't do return at once.
  A `quill-audio-build` thread opens the file, seeks it, and sends back a paused `Built`.
- Every state change stays on the UI thread. `settle` lands the build, and the `AUDIO_BUILT`
  signal has the app call it as soon as the thread is done, so a landing needs no redraw to
  come first.
- `Building` holds what a press asked for while the file is read: `latest_target` is the newest
  seek and `land_paused` the newest pause. A landing built for another position builds again.
  Nothing is cancelled, since a build holds no state of the engine's, and a dropped `Built` plays
  nothing.
- `EngineState::Loading` shows as Pause at the offset, as in tdesktop. The ring shows only from
  500 ms on, by the engine clock, so a short start shows nothing at all.
- A failed start fails when it lands. The failure note moved to the landing for that reason.
- `OpusSource` keeps its packets in one block, so ending a track on the audio thread frees two
  allocations and not one per packet.

## Left alone
A seek within a playing track still runs in the source on the audio thread. A coarse MP3 seek
would be quicker there, but lands in the wrong place on a VBR file.

## Test
`cargo test --features demo-capture --bin quill -- ui::audio ui::playback_state`.
`QUILL_DEMO_AUDIO_SLOW=1` with `ready-audio` or `ready-voice-playing` shows the long wait.
