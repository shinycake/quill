# Cross-platform call sounds

## Problem

`call_sounds.rs` played the macOS FaceTime sound files through `afplay`, so
on Linux and Windows an incoming call rang silently. Telegram's own call
sounds (tdesktop `call_*.mp3`) are GPL and can't ship in MIT Quill.

## Decision

- **Own tones, synthesized in code** (`src/ui/call_tones.rs`, pure maths, no
  audio or GPUI types, 48 kHz mono `f32`): incoming (soft rising three-note
  chime, 1.5 s), ringback (440 + 480 Hz double ring, 1 s), connect (rising
  two-note chime), end (falling two-note chime), busy (three 480 + 620 Hz
  beeps), mute/unmute (60 ms windowed blips at 600/900 Hz). Raised-cosine
  attack and release on every note; master level 0.5. No third-party audio.
- **Playback in-process with `rodio` 0.22** (`MIT OR Apache-2.0`; cpal is
  Apache-2.0) with `default-features = false, features = ["playback"]`, so
  no decoders are compiled. cpal drives CoreAudio, WASAPI and ALSA. One
  output stream is opened lazily on the first sound and kept; a `Player`
  per sound, cut with `stop()`. If no output device opens, sounds are
  skipped silently and the open is retried at most every 5 s.
- Loops are the same poll-driven scheme as before: the loop restarts when
  the player drains, after a gap (ringback 2.5 s, incoming 1 s).
- **One sound set everywhere.** The macOS system ringtone is no longer used,
  so macOS, Linux and Windows sound the same.
- **Preference:** the existing "Play sounds" setting (`inapp_sounds_enabled`)
  mutes only the short cues (connect, end, busy, mute/unmute). The incoming
  ringtone and the outgoing ringback always play, as in tdesktop where call
  ringing isn't tied to the chat sounds toggle. No new settings UI.
- The tones can be exported for listening with the ignored test
  `QUILL_TONES_OUT=<dir> cargo test --features ui --bin quill export_wavs -- --ignored`
  (not part of the shipped app).
- `CallSounds`, `CallSound`, `SoundMarks` and `sync_call_sounds` keep their
  API (plus `CallSounds::set_enabled`).

## Build

`rodio` is an optional dependency of the `ui` feature only, so the core crate
and Linux CI (`--no-default-features`) are unaffected. Building `--features
ui` on Linux needs ALSA headers: `sudo apt-get install libasound2-dev`
(documented in `docs/build.md`; CI doesn't build the UI on Linux).

## Verification

Unit tests in `call_tones` (lengths, no clipping, audible, first and last
sample near zero, busy beep/gap structure). The tones were dumped to WAV and
auditioned once on macOS. Linux and Windows playback couldn't be run from
this Mac: the Linux/Windows UI isn't cross-checkable (GPUI build scripts),
so those backends are verified only by rodio/cpal's own platform support.
