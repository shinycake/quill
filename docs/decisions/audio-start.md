# audio-start: a track starts off the UI thread

## tdesktop
`media/audio/media_audio.cpp`: `Mixer::play` sets the track's state and position at once, with
`loading = true`, and a thread does the rest. `ShowPauseIcon` is true for Starting and Playing,
so the button says Pause from the press on. The audio and voice row
(`history_view_document.cpp`) reads no waiting state: its only radial is the file's download or
upload progress, and `Media::Player::Instance` drops `WaitingForData` for audio. The 500 ms
`kWaitingShowDelay` in `media_streaming_document.cpp` belongs to the video surfaces only.

## What changed in Quill
- `AudioEngine::start`, a resume after the end, and a seek the decoder can't do return at once.
  A `quill-audio-build` thread opens the file, seeks it, and sends back a paused `Built`.
- Every state change stays on the UI thread. `settle` lands the build, and the `AUDIO_BUILT`
  signal has the app call it as soon as the thread is done, so a landing needs no redraw to
  come first.
- `Building` holds what a press asked for while the file is read: `latest_target` is the newest
  seek and `land_paused` the newest pause. A landing built for another position builds again.
  A build holds no state of the engine's, and a dropped `Built` plays nothing.
- Dropping `Building` (a stop, a newer start) sets the build's cancelled flag. The thread checks it
  after the open and after the seek, so a build nobody waits for skips the seek scan and makes no
  player.
- `EngineState::Loading` shows as Pause at the offset and nothing else, however long the file
  takes to open, as in tdesktop.
- A failed start fails when it lands. The failure note moved to the landing for that reason.
  The build thread wakes the app from a drop guard, so a decoder that panics still brings the
  landing and its note. Each new build clears the last start's failure, so a good start after a
  failed one says nothing. A replay after the end that can't open fails like a first start, so
  the tick does not take it for the track finishing and play the next one.
- `OpusSource` keeps its packets in one block, so ending a track on the audio thread frees two
  allocations and not one per packet.

## Seeks in a playing track
A seek in a playing track is a build too: the thread opens the file at the target and `settle`
swaps the new player in, so the decoder's seek (a scan on a long MP3) never runs on the audio
thread. The old sound plays on until the swap, and a build that fails leaves it playing with the
reason in `error`. tdesktop's loader thread does the same work for a seek outside what it has
buffered (`Mixer::seek`, `Loaders::loadData`). The seek board, its worker thread and the stall
report it fed are gone. A stalled device shows as `DeviceStalled` only in the demo simulation now.

## Test
`cargo test --features demo-capture --bin quill -- ui::audio ui::playback_state`. The row test
`a_long_start_shows_pause_alone_on_the_row` holds a real build open on the engine clock, so it
runs the start the app runs, not a simulation.
