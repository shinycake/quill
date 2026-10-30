# The audio device never opens on the UI thread

## What was wrong

Opening the output device takes 10 to 75 ms on macOS (about 75 ms measured
on a MacBook), up to five frames. Since #1245 the device closes 0.75 to 1 s
after the last sound, so opening is no longer a once-per-launch cost. Before
this change the UI thread opened it (`src/ui/audio/output.rs`, `open()`
with `Open::Now`, and `outlet_when` for the first device) for:

- the first sound of the app's life, whatever it was, notifications
  included: the app mixer was made with the first device's layout, so no
  player could connect before that device was open;
- every voice note or audio file started or resumed after that silence
  (`AudioEngine::start` through `outlet()`, `resume` through `play()`);
- every call cue (connect, end, busy, mute) and the first ring of a call;
- a video soundtrack's start, play and seek on the FFmpeg backend (Linux,
  Windows): its seek (`StreamSound::flush`, `Player::try_seek`) also waited
  for the audio thread's next callback, and on a paused video opened the
  device just to reset the speed stretcher;
- any play or seek while a notification's device was opening off the UI
  thread waited on a condvar for the rest of that open.

A stale device found by a play was also dropped (stopping its stream) on
the calling thread, under the output's lock.

## What changed

- The app mixer has a fixed layout, 48 kHz stereo (`MIX_RATE`,
  `MIX_CHANNELS`), made with the output and kept for the app's life. Every
  device plays it through `Feed`, which rodio converts to the device's
  layout, as it already did for every device after the first one. Telegram
  voice notes and the app's tones are 48 kHz, and tdesktop mixes at 48 kHz
  too (`Media::Player::kDefaultFrequency`).
- Every open runs on `quill-audio-open`; nothing on the UI thread waits for
  it. `outlet`, `play` and a sound queued on a playing player start an open
  and return. `play` is false only when no device opened in the last 5 s.
- A player that plays while no device is up or opening is `lost`: its
  device failed to open. The engine's `settle` (the 250 ms tick and every
  command) pauses such a track and shows "no audio output device"; a call
  ring that loops counts it as finished and tries again after its gap.
- Only `OutputPlayer::try_seek` waits for an opening device, and only the
  engine's seek worker calls it, so a seek still gets an answer or fails.
- A video soundtrack's seek sets a flag that the stretcher reads at its
  next pull (`Flushed`), so it neither waits for the audio thread nor opens
  the device.
- A stale device is dropped on the opener thread.

No "starting" engine state: while the device opens the row shows the
track playing at its start, as it did during the device's first buffer.
A voice note now opens the device while its file is decoded, not before,
so its sound starts no later than before, and usually earlier.

## Costs weighed

- No launch-time open. Opening the device right after launch, then letting
  the 500 ms close apply, would keep the first device's layout but wake the
  audio hardware and load CoreAudio in every session, including ones that
  never play a sound, and would not help any later reopen.
- Same device lifetime as before, so no new CPU or battery cost: the device
  opens for the same sounds and closes after the same silence.
- A device that doesn't run at 48 kHz gets the mix converted with rodio's
  linear resampler. On such a device a file at the device's own rate (a
  44.1 kHz MP3 on a 44.1 kHz output) is now converted twice, which softens
  its treble slightly; before, only the first device of the app's life was
  spared that. CPU cost of the extra pass is negligible.

## How it was checked

Tests use a stand-in device (`Gate` in `output.rs`): each open waits on the
opening thread for the test's answer, and opening on the test thread, or
waiting there for an open, fails the test. With the old synchronous open
put back, the new tests fail with "opened the device on the UI thread".

- `the_first_sound_plays_once_the_device_opens_off_the_ui_thread`
- `a_player_whose_device_failed_to_open_is_lost`
- `a_seek_waits_for_the_opening_device_and_fails_without_one`
- `a_track_starts_while_the_device_opens_off_the_ui_thread`
- `a_device_that_fails_to_open_pauses_the_track`
- `a_video_seek_never_waits_for_the_device`

The UI thread's part of a first sound (outlet, queue, play) took about
50 µs in a test on Linux: a thread spawn and a mixer connection.

On a Mac, with debug logging on: the log line "audio: output: opened the
output device in N ms on T" names the thread. Before this change the first
sound after launch logs `on main`; after it, every open logs
`on quill-audio-open`. Time the UI thread around the first notification
sound and the first voice-note play after launch, and around a voice-note
play after 2 s of silence: expected about 75 ms before, under 1 ms after,
with the sound starting as promptly as before.
