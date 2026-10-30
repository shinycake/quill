# 1:1 call sound through Quill's output

## Why

2026-10-23: a 1:1 call answered in the installed macOS app (2026.1022.3487)
connected, opened the microphone and the speakers, and played nothing. The
same call worked on Telegram for iOS. The macOS log showed ntgcalls'
AudioQueue output running for the whole call, so the sound path was set
up, but nothing said whether the caller's sound ever reached it: with
ntgcalls' device output, decoded audio never passes through Quill, and
the call engine's own log was only written in debug mode.

Two-engine calls on the same Mac (`loopback_tests.rs`) played the caller's
sound in every setup tried: device output, Quill's output, Quill's real
`connect` path, with the microphone open. The fault needs a real peer to
show, so the change makes the next one explain itself.

## What changed

- A 1:1 call's sound plays through Quill's own output
  (`calls::stream_audio`, already used for live streams): ntgcalls hands
  the decoded 10 ms frames to the frames callback and Quill counts them
  and their peak level (`PlaybackHealth`).
- The driver watches the call (`connect::calls::sound_watch`) with the
  stream rule: connected for 5 s with no frames, or an output that stops,
  moves the call back to ntgcalls' device output once.
- `logs/calls.log` is always on (1 MB, then `.old`): connect parameters
  (versions, server count), transport states, the sound watch's verdict,
  a per-call summary (frames, peak, output failure), and ntgcalls'
  warnings and errors. It holds no message content. Debug mode still adds
  everything else to `quill-debug.log`.

## Reading a silent call

- `ended: 0 sound frames`: nothing arrived from the other side
  (transport, negotiation or the peer). Check the ntgcalls warnings above
  it, for example the iOS 13.1 own-SSRC conflict fixed upstream after
  v3.0.5 (pytgcalls/ntgcalls#84).
- Frames with `peak 0`: the other side sent silence.
- Frames with a real peak but nothing heard: the output device.

`QUILL_STREAM_AUDIO_DEVICE=1` keeps calls and streams on ntgcalls' device
output.

## The output's format (2026-10-30)

On tgcalls (2026.1029.3985) the caller's voice played high-pitched and
twice as fast. The library hands over 48 kHz mono, but the output queued
it in the device's channel count, so on stereo speakers every two mono
samples became one stereo frame. The queue now always holds mono at the
device's rate: the frames are converted from 48 kHz when the device does
not run at it (Bluetooth headsets in call mode often run at 16 or 24 kHz),
and each sample is spread over every output channel. This is the same
shared output on macOS, Windows and Linux.
