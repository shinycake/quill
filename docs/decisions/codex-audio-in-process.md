# In-process audio (no ffplay / afplay / osascript)

Voice notes, audio files, the media viewer soundtrack and notification sounds
all play through the one `rodio` output Quill already opened for call tones
(CoreAudio, WASAPI, ALSA/PulseAudio). No external player binary is needed on
any platform; ffmpeg is still used for recording and frame extraction, not
playback.

Code: `src/ui/audio/` (`mod.rs` engine/selection, `opus.rs`, `tempo.rs`),
wired from `audio_playback.rs`, `media_viewer.rs`, `notifications.rs`,
`notification_settings.rs`, `call_sounds.rs`.

## Formats and decoders

| Format | Where it appears | Decoder |
| --- | --- | --- |
| Ogg/Opus (`.oga`, `.ogg`, `.opus`) | every Telegram voice note | `ogg` (demux) + `opus-decoder` (pure Rust, RFC 8251 conformant, MIT/Apache) |
| MP3 | audio messages, saved notification sounds | symphonia (via rodio `mp3`) |
| AAC / M4A / MP4 audio | audio messages, video soundtrack | symphonia (`mp4` = isomp4 + aac) |
| FLAC | audio messages | symphonia (`flac`) |
| Ogg Vorbis | audio messages | symphonia (`vorbis`) |
| WAV / PCM | audio messages | symphonia (`wav`) |

Anything else fails with "this audio format can't be played" on the row (no
silent stall). There is deliberately no ffmpeg decode fallback.

The decoder is chosen by file content (`choose_codec`: Ogg capture pattern
plus `OpusHead`), not by name or MIME type, because TDLib stores voice notes
under several extensions and an `.ogg` may be Vorbis or Opus. Extension, then
MIME type, only give symphonia a probe hint (`format_hint`).

## Why a pure-Rust Opus decoder

symphonia has no Opus decoder. Options considered: `opus`/`audiopus`
(libopus bindings; builds C with cmake, adds a toolchain requirement on all
three platforms and in CI), ffmpeg as a decode fallback (the exact external
dependency being removed, usually missing on Windows/macOS), and
`opus-decoder` 0.1 (pure Rust, `forbid(unsafe_code)`, passes all 12 RFC 8251
vectors, no new system dependency). The last wins. Risk: young crate (0.1.x);
mitigated by the fixture test and by a bad packet becoming silence rather
than ending playback. Swapping to libopus later only touches `opus.rs`.

`OpusSource` reads the compressed packets up front (about 4 KB/s), so the
duration is exact (final-page granule minus pre-skip) and seeking is a packet
index lookup plus the 80 ms pre-roll Opus needs. Mono and stereo (mapping
family 0) are supported; surround voice notes do not exist.

## Playback model

- `PlaybackClock` (`quill::playback`) still owns the shown position, so the
  seek bar, waveform progress and remembered positions are unchanged.
  `AudioEngine` owns the sound: start at offset, pause, resume, seek,
  volume, speed.
- End of track: the engine reports when its source ran out, which now
  decides when a track finishes (a file's real length can differ from
  TDLib's rounded duration). Without an engine track (screenshot demo) the
  clock decides, as before. The existing 250 ms tick polls it; nothing
  uses repeating `with_animation`.
- Seek while playing seeks in place (falls back to a restart); seek while
  paused only moves the clock and the sound catches up on resume.
- Speed (0.5x to 2x, the existing cycle) keeps the pitch like ffplay's
  `atempo` did: a WSOLA time stretcher (`tempo.rs`) reads the speed from a
  shared atomic each 20 ms block, so changing speed no longer restarts
  playback. At 1x it is bit-transparent apart from a 20 ms delay.
- Volume and mute apply live to the player.
- Continue-to-next-voice-note was not implemented with ffplay either, so it
  is not part of this change.
- The viewer soundtrack (non-native video path) uses a second engine on the
  same output; it restarts the sound on speed, volume or seek changes as it
  restarted ffplay before.

## Notification sounds

`play_notification_sound` and the settings preview map the driver's
`SoundResolution` through `notification_sound` and play on the shared output
over any running voice note. The default tone is synthesized
(`call_tones::notification`) like the call tones, so Windows is no longer
silent and Linux/macOS no longer shell out. A custom file that can't be
decoded falls back to the default tone. `SoundCommand`, `ffplay`, `afplay`
and `osascript` paths and their tests were removed from `quill::notify`.

## The output closes when nothing plays (2026-10-30)

The shared output used to open the default device for the first sound and
keep it open for the life of the app. On macOS an open output keeps the
audio hardware running: coreaudiod held a "prevent idle system sleep"
assertion for the built-in speaker for hours after a single notification
sound, and CoreAudio kept calling Quill's mixer to play silence. tdesktop
detaches its device after 500 ms of silence (`kDetachDeviceTimeout` in
`media_audio_track.cpp`).

- `src/ui/audio/output.rs`: every player connects to one app mixer, made
  with the first device's channels and rate and kept for the app's life. A
  device plays it through a feed that takes 256 frames at a time (a later
  device with another layout converts it). A player made while no device
  is up waits outside the mixer and connects when one is; its sound is
  dropped if the player goes first. The players never see the device come
  and go, so a paused voice note or video carries on with the next device.
- A sound counts as playing while its player pulls it: `Outlet::append`
  wraps each queued sound in `Heard`, which marks the output on every
  sample. A paused player, or one with nothing queued, plays silence
  without pulling it. A watch thread looks every 250 ms and closes the
  device after 500 ms with nothing heard, then ends. Measuring the samples
  instead would close the device during silence inside a track, such as
  the quiet start of many songs, and leave the track stalled.
- Opening: players are `OutputPlayer`s, and whatever needs the audio
  thread has the device opened first: `outlet`, `play`, `try_seek`, and a
  sound queued on a playing player (after its file was decoded, however
  long that took). With the device closed, a play would stall and a seek
  would wait for an answer for ever (`Player::try_seek` blocks until the
  audio thread takes it). The video decoder keeps time from what the
  output pulls, so a soundtrack resumed without a device would also freeze
  the picture. With no device lately, `play` says so (the engine pauses
  again and shows no output; a video soundtrack waits for the next device)
  and `try_seek` fails at once.
- Silence counts only while the device asks for sound. A device that stalls
  (a reroute, a Bluetooth reconnect) stops asking, and closing it then could
  leave a seek parked for an answer that never comes, on the UI thread for a
  video. The watch keeps a device that didn't ask since its last look.
- A video soundtrack (`StreamSound`) never ends: it plays silence after the
  clip. The ffmpeg backend pauses it whenever the decoder isn't playing, so
  an ended clip left on screen lets the device close.
- Opening a device takes 10 to 75 ms on macOS, up to five frames on the
  UI thread, so it always opens on a thread of its own
  (`quill-audio-open`) and nothing on the UI thread waits for it
  (`docs/decisions/audio-open-off-ui-thread.md`). A sound is queued on the
  app mixer at once and plays once the device is up; a notification is
  dropped if it can't start within 2 s (a device that failed to open would
  otherwise play it whenever one opens next). `play` returns while the
  device opens: the engine shows the track playing, and its next `settle`
  (the 250 ms tick, or the next command) pauses a track whose device then
  failed to open (`OutputPlayer::lost`) and shows no output. A call ring
  that loops counts a lost ring as finished and tries again after its gap.
  Only `try_seek` waits for a device that is opening, and only the
  engine's seek worker calls it; a video soundtrack's seek just tells the
  stretcher to start over at its next pull. A closed device is dropped off
  the UI thread, since stopping a stream also takes a while.
- A call sound that loops takes a `Hold` while it loops, so the device stays
  open through the gaps between rings (1 s and 2.5 s) and is not reopened
  for every ring.
- A stale device (Windows `StreamInvalidated`, a device that went away)
  moves the app mixer to the new default device at the watch's next look,
  so a track that is playing carries on there. Before, the track stayed on
  the dead stream until the next sound.

## Verified

- Unit tests (`cargo test --features ui --bin quill audio`): codec selection
  by content, extension/MIME hints, Opus TOC durations, Opus fixture decode
  length and level, Opus seek position, garbage rejection, WAV decode through
  `open_source` regardless of extension, tempo transparency at 1x, tempo
  duration and pitch at 0.5x/1.5x/2x, live speed change, tempo seek, speed
  clamping, notification sound selection.
- Opus fixture `tests/fixtures/tone-440hz.opus.ogg` (1.5 KB, 0.6 s 440 Hz sine,
  generated with `ffmpeg -f lavfi -i sine=frequency=440:duration=0.6 -ac 1
  -c:a libopus -b:a 16k`; no third-party content, CC0).
- Not verified here: actual speaker output and real Telegram voice notes
  (live test list is in the PR).
