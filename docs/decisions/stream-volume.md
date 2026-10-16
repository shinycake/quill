# Stream volume

`parity:calls-rtmp-volume`: a volume slider for a channel's live stream,
in the stream window and in a live story. It replaces the "No stream
volume slider" decision in `video-player-live-ui.md`.

## What ntgcalls does

ntgcalls v3.0.3 has no playback volume or gain call. It can hand the
received audio to the app instead of the speaker. Read from the v3.0.3
source:

- A playback description with `NTG_MEDIA_SOURCE_EXTERNAL` makes
  `StreamManager::handle_playback_config` add an external writer, and
  `setup_audio_playback_callbacks` sends each received chunk to
  `ntg_on_frames_callback`, tagged playback / microphone.
- The format is 16-bit native-endian interleaved PCM, 10 ms per chunk
  and per source. The rate and channel count are the ones in the
  description (`AudioReceiver::resample_frame` converts between mono and
  stereo only). An RTMP stream has one source, ssrc 1.
- The callback runs synchronously on ntgcalls' own thread.
- Setting a device description again after an external one rebuilds the
  writers, so going back to device playback works without leaving.

## Design

- Only stream joins (a channel stream and a live story) play through
  Quill. 1:1 calls and video chats keep ntgcalls' device playback, which
  WebRTC's echo cancellation uses.
- `src/calls/stream_audio/`: the frames callback writes into a
  lock-free single-producer, single-consumer ring of 200 ms. On overrun
  it drops the oldest 20 ms. The cpal output callback waits for 60 ms
  before it starts to play, plays silence on underrun, and applies the
  gain. The callback thread never blocks or allocates.
- The output opens on the call's speaker setting (matched by name, the
  system default otherwise), at 48 kHz when the device takes it. It
  reopens when the speaker changes. The ring is mono or stereo at the
  device rate, and ntgcalls resamples to it.
- The gain is an atomic `f32` clamped to 0 to 1, the slider's 0 to
  100 % (tdesktop's range for a stream; the stage slider's 200 % is for
  a participant). Mute sets the gain to 0 and
  keeps the description, so the slider value survives mute and frames
  keep arriving.
- The volume is the media viewer's volume (`playback.volume`), as
  tdesktop's slider reads and writes `videoVolume`. Changing it in
  either place moves the other.

## Self-healing

A stream that plays through Quill falls back to ntgcalls' device
playback, once per join, when either of these happens:

- the stream has been live for 5 s and no playback chunk has arrived
  (the clock starts when the stream state turns Live, because nothing
  plays while it connects or waits for a broadcaster);
- the output device stops with an error.

The driver checks on every poll. It prints
`quill: warning: stream sound: falling back to device playback for group call <id> (<reason>)`
to the terminal and the debug log, and the slider goes away (device
playback has no volume, so mute is the only control again).
`QUILL_STREAM_AUDIO_DEVICE=1` skips Quill's output and always uses
device playback.

## Controls

- Stream window: the slider sits in the bar beside mute. Up and Down
  move it by 5 %.
- Live story: the slider is in a pill that drops down from the header's
  sound toggle while the pointer is on the toggle or the pill, and
  stays open during a drag. This is tdesktop's
  `Header::createVolumeToggle` layout: 75 px slider, 20 px under it,
  `mediaviewSaveMsgBg`. A slider inline in the 360 px header cut the
  channel name to its first letters.
- The slider shows only while Quill plays the stream. On device
  playback, after a fallback, or with the environment variable set, it
  is absent rather than disabled.

## Differences from tdesktop and the design

- Up and Down: the design asks for them "like the file player", but
  neither tdesktop nor Quill's file viewer has volume keys. They work
  in the stream window only, because the story viewer has no key
  routing.
- The pill closes as soon as the pointer leaves. tdesktop waits 200 ms
  after the pointer leaves the pill.
- The pill opens on hover only, and the kit slider takes no keyboard
  focus. In a live story a keyboard user reaches mute but not the
  volume; in the stream window Up and Down stand in for the slider.
- tdesktop's group call settings box also has an RTMP volume row
  (`calls_group_settings.cpp`, `MenuVolumeItem`). Quill's settings box
  does not; the bar slider is the one control.

## What only a real stream can check

Nothing here ran against a real stream. The tests cover the ring, gain,
channel spreading, the callback-to-ring path with fake frame batches,
the 5 s rule with injected instants, the driver's fallback and the
slider writing the volume. The captures show the controls in the
screenshot demos, which have no engine and no sound.

To verify, Idan runs a release build signed as usual:

1. Open a public channel that is streaming and join its live stream
   from the chat header. Sound plays within a few seconds.
2. Drag the slider in the stream window's bar to about 20 %, then to
   100 %. The sound gets quieter, then returns.
3. Click mute, then unmute. The sound stops and comes back at the
   slider's level, and the slider does not move.
4. Press Down four times and Up four times with the stream window
   focused. The slider and the sound follow in 5 % steps.
5. Open a channel's live story, hover the sound toggle, and drag the
   pill's slider. The sound follows, and the stream window's slider
   (if open) shows the same value.
6. Check that the terminal and, with debug mode on (`--debug`),
   `logs/quill-debug.log` have no
   `stream sound: falling back` line.
7. Quit, start with `QUILL_STREAM_AUDIO_DEVICE=1`, join again. Sound
   plays, and there is no slider, only mute.
8. Optional: unplug or switch the output device while the stream plays.
   Sound continues on device playback, and the log has one
   `falling back` line with `OutputFailed`.

Also unchecked: clock drift between the stream and the output over a
long stream (the ring drops or pads, so it would show as rare clicks),
cpal device names on Linux and Windows matching ntgcalls' speaker
names, and devices that do not take 48 kHz.
