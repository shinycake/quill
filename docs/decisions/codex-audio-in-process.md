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
