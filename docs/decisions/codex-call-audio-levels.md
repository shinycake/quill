# Call audio: your microphone level, speaking detection, the peer's microphone

## What tdesktop does

- tgcalls measures the microphone (`group/GroupInstanceCustomImpl.cpp`, the capture sink): the loudest sample of every 4,400-sample window on the 16-bit scale, divided by 4,000, is the level. With RNNoise off the window is "voice" when that level reaches 1.0; with it on, RNNoise's voice probability decides.
- `GroupCall::audioLevelsUpdated` (`calls_group_call.cpp`) treats a level above `kSpeakLevelThreshold` 0.2 as sound, ignores your own levels while muted, and sends the speaking action every `kUpdateSendActionEach` 500 ms while voice continues. `Data::GroupCall::applyLastSpoke` keeps a participant "sounding" or "speaking" for `kSoundStatusKeptFor` 1,500 ms after the last such window.
- The members row (`calls_group_members_row.cpp`) animates its blobs to a new level over `kLevelDuration` 215 ms; the panel feeds your own level to the mute button (`calls_group_panel.cpp`).
- Settings > Calls (`settings/sections/settings_calls.cpp`) shows a `LevelMeter` fed by `Webrtc::AudioInputTester` every `kMicTestUpdateInterval` 100 ms, animated over `kMicTestAnimationDuration` 200 ms. The meter (`groupCallLevelMeter` in `calls.style`) is 44 rounded lines, 3 px wide, 5 px apart, 18 px tall.
- A 1:1 call shows "{user}'s microphone is off" (`lng_call_microphone_off`, `callRemoteAudioMute`, 12 px) while the peer's audio state is muted (`Call::updateRemoteMediaState`).

## What changed

- `src/calls/audio_level.rs` (pure, tested): tgcalls' level scale (`level_from_peak`, `PeakWindow`), the speaking tracker with tdesktop's thresholds and timings (`SpeakingTracker`: sound above 0.2, voice from 1.0, 1,500 ms kept, a send on every change and every 500 ms while speaking, muted levels ignored), the level animation (`LevelAnimation`, 215 ms for the halo, 200 ms for the meter), the meter's lit lines and width, and the halo reach.
- `src/calls/level_tap.rs` (`ui` feature): the default input device through cpal, the same path as voice notes (`voice_input::open_default`, mono 48 kHz), measured on its own thread; the latest level is an atomic the driver and the UI read. `LevelSource` is the trait behind it so tests and demos feed fixed levels.
- `src/connect/call_audio.rs`: `pump_call_audio` runs from the driver pump. While you are joined and allowed to speak (`tap_wanted`: joined, not muted, not muted by an admin) the tap is open; each completed window goes through the tracker, and a change in speaking state (or the 500 ms refresh) sends `setGroupCallParticipantIsSpeaking` with your `audio_source_id`. TDLib marks your participant (`is_speaking` comes back through `updateGroupCallParticipant`) and sends the speaking action to the other members. Muting or leaving closes the tap and withdraws the mark. A failed open is retried after 5 s; an audio source of 0 sends nothing. Failures of the request are ignored (no banner).
- The voice chat window (`group_call_panel.rs`): while you are live a ring around the mute button grows with your level (`halo_reach`: 6 px at rest, 20 px at full level, animated over 215 ms); your row says "speaking" from the tap as well as from TDLib's flag. Frames are requested at 30 fps only while the animation runs or the level is above zero.
- Settings > Calls (`src/ui/call_audio_ui.rs`): a "Test microphone" button opens the tap and shows tdesktop's 44-line meter, sampled every 100 ms and animated over 200 ms, at 30 fps while it runs. The meter shows the peak as a fraction of full scale, as tdesktop's tester does. A watchdog closes the microphone once the meter has not rendered for a second (another tab, another settings page), so no device stays open behind an unseen section.
- The 1:1 call: ntgcalls reports the peer's MediaState `is_muted` as its remote Microphone source going idle (`p2p_call.cpp`); the engine now routes that device to a `set_remote_audio_state_callback` hook, the driver pump stores it on the call (`remote_audio_muted`), and the call window shows "{name}'s microphone is off" under the status.
- Request builder `set_group_call_participant_is_speaking`, purpose `CallsPurpose::SetGroupCallParticipantIsSpeaking`.

Levels on both sides keep tgcalls' scale, so tdesktop's 0.2 and 1.0 thresholds mean the same thing here. The queue suggested RMS over 20 ms; tgcalls uses the peak over 4,400 samples, and the thresholds were tuned for that, so Quill measures the same way.

## Noise suppression: evaluated, not added

- ntgcalls builds the outgoing audio channel with `echo_cancellation`, `noise_suppression`, `auto_gain_control` and `highpass_filter` all `false` (`wrtc/src/interfaces/media/channels/outgoing_audio_channel.cpp`, v3.0.2), and the C API (`ntgcalls.h`) has no switch for any of them. So calls currently run with no noise suppression at all, and no flag can turn WebRTC's on.
- `nnnoiseless` 0.5.2 (BSD-3-Clause, allowed by `deny.toml`) is a Rust port of RNNoise: 48 kHz, 480-sample frames, which is what tgcalls uses when its noise suppression is on. It can only run on audio Quill handles. The microphone goes straight from ntgcalls' device capture to the encoder, so the only way to put a denoiser in the path is an external microphone source (`ntg_set_stream_sources` with `NTG_MEDIA_SOURCE_EXTERNAL` and `ntg_send_external_frame`), fed from the level tap's cpal stream at 10 ms. That changes how every call captures audio and needs testing on all three platforms, so it is a slice of its own. The tap built here is the capture half of it.
- A "Noise suppression" switch that changed nothing would be worse than none, so Settings has no such switch. `parity:calls-noise-suppression` and `parity:calls-audio-fx` stay open.

## Platforms

- The tap opens the microphone a second time while ntgcalls has it. CoreAudio, WASAPI in shared mode and PulseAudio / PipeWire share capture devices between clients. A raw ALSA `hw:` device may refuse the second open; the tap then reports the error, the call carries on without a level, and the open is retried every 5 s.
- cpal's default input is the system default device; a microphone chosen in the call window is not followed (the tap would need the same device by name across cpal and ntgcalls' device JSON).
- Permission errors keep `voice_input`'s wording with the Settings path for macOS and Windows.

## Cost

- Audio stays off the UI thread: the cpal callback hands chunks to the tap's thread, which keeps a running peak and writes one atomic per window (every 92 ms).
- The driver pump reads one atomic per pass; the UI reads one tuple per frame.
- Frames: 30 fps only while the mute button's ring is moving or the microphone test is on screen; nothing animates otherwise. Battery mode is unchanged.

## Not done

- Other participants' levels: tgcalls reports them per ssrc; ntgcalls has no equivalent (`get_audio_level_and_speech` is internal to its RTP sender). Their "speaking" stays TDLib's flag.
- A voice detector: tgcalls uses RNNoise's probability when noise suppression is on; Quill uses the 1.0 level rule tgcalls applies without it.
- Your own level in a 1:1 call window: tdesktop shows none there either.
- The peer's low-battery line (`parity:calls-peer-indicators` also wants it; ntgcalls does not surface it).

## Verified

- Unit tests: level scale and windows, the speaking timings and send cadence, muted levels, the animation, meter lines; `tap_wanted`; the driver sending and withdrawing the mark with a fake level source, an unknown audio source sending nothing, and the peer microphone state reaching the tracked call only; the request shape; the microphone-off line; member statuses.
- Screenshot demos (temp directory, not committed): Settings > Calls with the meter at a fixed level (`QUILL_DEMO_MIC_TEST`), the voice chat window with your level on the mute button and "speaking" on your row (`QUILL_DEMO_SELF_LEVEL`), the video call window with the peer's microphone off (`QUILL_DEMO_CALL_REMOTE_MUTED`).
- Not verified: a real call (never started on a real account), and the second device open on Linux and Windows.
