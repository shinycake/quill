# Calls on tgcalls instead of ntgcalls

Idan asked on 2026-10-29 to replace ntgcalls with our own call engine and then
ship the parity items ntgcalls blocks. Quill now builds Telegram's own engine,
tgcalls, the one Telegram Desktop and Telegram's mobile apps ship, into a
library of ours with a plain C API. ntgcalls goes in the PR that switches the
engine over; there is no fallback switch (Idan, same day).

## Why tgcalls and not a Rust engine

- Call quality comes from libwebrtc's media engine (echo cancellation, the
  NetEq jitter buffer, congestion control, codecs with hardware paths). No
  Rust crate replaces it: webrtc-rs 0.20 is transport only. Any engine keeps
  libwebrtc; the choice is who writes the Telegram layer on top.
- tgcalls is that layer, written by Telegram, and already has every hook
  ntgcalls lacks: `e2eEncryptDecrypt` for conference calls (TDLib's
  `encryptGroupCallData` / `decryptGroupCallData` say they exist for
  tgcalls), the peer's battery flag, per-SSRC audio levels, signal bars and
  muted-speech detection.
- ntgcalls is a reimplementation of the protocol for server bots. It lags on
  desktop needs (conference calls, battery flag) and on interop fixes (the
  iOS own-SSRC bug, pytgcalls/ntgcalls#84, fixed upstream after v3.0.5 and
  still unreleased).

## How it is built (`scripts/build-tgcalls.sh`, `native/quill-calls/`)

Everything matches Telegram Desktop at `dev` f985723b (26 Oct 2026):

| Piece | Pin | Where tdesktop pins it |
| --- | --- | --- |
| tgcalls | 1c236c09 | `Telegram/ThirdParty/tgcalls` submodule |
| tg_owt | 07fb35bc | `Telegram/build/prepare/prepare.py` |
| Opus | v1.5.2 | prepare.py |
| libvpx | v1.14.1 | prepare.py |
| OpenH264 | v2.6.0 | prepare.py |
| mozjpeg | v4.1.5 | prepare.py |
| rnnoise | d8ea2b0 (desktop-app fork) | prepare.py |
| FFmpeg | n9.0.2 | Quill's `scripts/build-ffmpeg.sh` (same release as tdesktop) |

- tg_owt is built static with `TG_OWT_BUILD_AUDIO_BACKENDS=OFF` as tdesktop
  does: the app brings its own audio device module. Quill's is in the C API,
  so its existing cpal capture (with RNNoise) and output keep working.
- `TG_OWT_SPECIAL_TARGET` puts tg_owt in tdesktop's non-packaged mode, which
  takes include paths only; every codec library is linked once, statically,
  into `libquill_calls`. On Linux pipewire is loaded with `dlopen`
  (`TG_OWT_DLOPEN_PIPEWIRE`, as tdesktop's Linux build).
- The tgcalls source list and its compile definitions (`WEBRTC_APP_TDESKTOP`,
  `RTC_ENABLE_H265`, `RTC_ENABLE_VP9`, `TGCALLS_USE_STD_OPTIONAL`) copy
  tdesktop's `Telegram/cmake/lib_tgcalls.cmake`: platform/darwin on macOS minus
  the iOS camera and Metal views, platform/tdesktop elsewhere, no Android, no
  reference implementation.
- The library registers the same three 1:1 implementations tdesktop does
  (`InstanceImpl`, `InstanceV2Impl`, `InstanceV2ReferenceImpl`), so Quill
  offers the same protocol versions.
- Only `qc_*` symbols are exported (`--exclude-libs,ALL` on Linux, an
  exported-symbol list on macOS), so tg_owt's copies of libyuv, abseil,
  libsrtp and the codecs can't clash with anything else in the process.
- FFmpeg and OpenSSL 3 are linked dynamically against the copies Quill
  already bundles. FFmpeg is new in the macOS bundle; it is needed by
  tg_owt's H.264 decoder and tgcalls' stream parts.
- `libquill_calls` is loaded at runtime like ntgcalls today, which keeps
  tgcalls' LGPL-3.0 terms met the same way: users can rebuild and replace it
  (`vendor/quill-calls/share/SOURCES.txt` names every source).
  `vendor/quill-calls/share/licenses/` carries the notices each part asks
  to ship with its binaries.
- The library finds FFmpeg and OpenSSL next to itself (`@loader_path`,
  `$ORIGIN`). On macOS the build script rewrites their absolute install
  names to `@rpath/<name>` and re-signs, and fails if any build path is left.

Where the build deliberately differs from tdesktop's:

- tg_owt is linked in non-packaged mode on Linux too, where tdesktop's
  Linux build uses the packaged one; one copy of every codec is simpler to
  ship and keeps the exported surface to `qc_*`.
- PipeWire 0.3.62's headers are fetched for tg_owt's camera code (the
  version tdesktop's build image uses) because Ubuntu 22.04's are too old.
- rnnoise is the desktop-app fork tdesktop uses; OpenH264 is built with its
  makefile rather than meson; tdesktop's libvpx patches (desktop-app/patches
  `libvpx/`) are not applied: they fix its MSVC and Windows-on-Arm builds
  and let `SDKROOT` pick the macOS SDK, none of which this build needs.
- tgcalls carries Quill's patches from `native/quill-calls/patches/`,
  applied after the checkout. `tgcalls-v2-reference-release-on-media-thread`
  fixes a deadlock in `InstanceV2ReferenceImpl` (10.0.0, 11.0.0) when a call
  is destroyed right after it is created: the stats task on the worker thread
  could hold the internal object's last reference, and destroying its
  PeerConnection there waits on the media thread while `~PeerConnection` on
  the media thread waits on the worker. The task now drops its reference on
  the media thread. Upstream tgcalls has the same code on every branch
  (October 2026). Before the patch, about 1 in 75 runs of the contract test
  hung, and `qc_p2p_destroy` would have hung Quill's thread.

## The C API (`native/quill-calls/include/quill_calls.h`)

- 1:1 calls take TDLib's `callStateReady` as JSON and build the
  `tgcalls::Descriptor` the way Telegram Desktop builds it from `phoneCall`
  (`calls_call.cpp`): the same config flags, a reflector becomes both a UDP
  relay endpoint and a TURN server ranked by id, WebRTC servers become STUN
  and TURN entries.
- Sound: tg_owt has no audio backends, so every call gets tgcalls'
  `FakeAudioDeviceModule` on an absolute 10 ms clock. Quill pushes 48 kHz mono
  microphone frames in and receives the call's sound out, so its own capture
  (with RNNoise) and output stay as they are.
- Video: a camera tgcalls opens itself, as Telegram Desktop does, or an
  external source Quill pushes I420 pictures into (its screen capture). The
  external source is a wrapper around tgcalls' platform layer
  (`src/platform.cpp`): tgcalls' own `CreatePlatformInterface` is renamed at
  compile time and wrapped, and device ids `quill:external:<n>` get a capturer
  fed from Quill.
- No callback runs after `qc_p2p_destroy` returns, even though tgcalls
  finishes tearing a call down on its own threads. For the same reason the
  Rust loader (`crates/quill-calls-sys`) never unloads the library.
- `crates/quill-calls-sys/tests/contract.rs` checks the bindings against
  the real library in the quill-calls workflow: symbols, versions, a refused
  config, and a call made and torn down for every protocol version. No test
  waits on the network or a timer (Idan, 29 Oct 2026: tests that rely on
  timeouts are flaky by nature); whether calls connect is tested by hand.
- `qc_p2p_destroy` waits for tgcalls' stop and then for the tasks it posts
  to its threads. Returning earlier left them running into process exit,
  where they used OpenSSL and WebRTC statics already torn down (the contract
  test crashed about every other run).

## Group calls (`qc_group_*`)

- `qc_group_create` makes a `tgcalls::GroupInstanceCustomImpl` with the
  descriptor Telegram Desktop builds in `GroupCall::tryCreateController`
  (`calls_group_call.cpp`): Generic video content, VP8 first when the app
  config asks (`confcall_prioritize_vp8`), and the same audio-level filter
  that drops repeated levels of a muted microphone. `"presentation": true`
  makes the screen-sharing instance of `tryCreateScreencast` instead:
  Screencast content, no levels, no sound out, no media channel requests.
  Sound goes through the same AudioBridge as 1:1 calls.
- `qc_group_join_payload` blocks until tgcalls' `emitJoinPayload` delivers,
  so Quill gets joinGroupCall's payload (and its audio ssrc) as a return
  value. The join response, connection mode (none, RTC, broadcast with
  tdesktop's `keepBroadcastIfWasEnabled = true` and the RTMP
  `isUnifiedBroadcast` flag), mute, per-ssrc volume, removed ssrcs,
  requested video channels (JSON from `groupCallParticipantVideoInfo`) and
  outgoing video (`qc_video_capture`) map one to one onto tgcalls.
- Incoming video is per endpoint: `qc_group_add_incoming_video` hands
  tgcalls a sink that calls `incoming_video` with the endpoint id and an
  I420 picture; the library owns the sinks (tgcalls keeps weak pointers),
  so removing one stops it.
- tgcalls' requests (ssrc to participant, the stream's time, audio and video
  stream parts) become callbacks with a request id; Quill answers each from
  any thread with `qc_group_answer_*`. tgcalls' `cancel()` becomes
  `request_cancelled`, and an answer to an id that is gone does nothing.
  The time and part requests are wired for a presentation too, which
  tdesktop leaves empty, so broadcast mode can never call an empty function.
- `qc_group_destroy` is synchronous like `qc_p2p_destroy`: it closes the
  callback guard, drops open requests, waits for `stop`, destroys the
  instance (which itself waits on the media thread) and drains tgcalls'
  threads.

Left out on purpose: conference calls (`e2eEncryptDecrypt`, `isConference`;
a later parity item), tgcalls' own noise suppression (Quill runs RNNoise
before pushing audio), tgcalls' log file (logs go to `qc_set_log`), audio
device selection (no devices in the library), `getStats` and the
video-only media channel descriptions tdesktop never sends. Whether a group
call connects is tested by hand; the contract test checks the payload, every
setter and teardown for both flavours.

### The engine side (`src/calls/engine/tgcalls/group.rs`)

`TgcallsEngine` drives one `qc_group` per group call and a second one for
its presentation, as tdesktop does. The join payload goes to
`joinVideoChat` unchanged (it is the JSON TDLib forwards), and its `ssrc`
becomes `audio_source_id`. A join answer with `"stream": true` sets
broadcast mode (unified for `"rtmp": true`); any other answer sets RTC
mode and then the answer, in tdesktop's order. Incoming video is routed by
endpoint, so each `GroupVideoSource` says whether it is a screen. Requested
qualities follow tdesktop's grid defaults and budget, since Quill does not
report tile sizes. Speaker levels reach the engine event hook; the driver
drops them until the speaking-levels parity item. Nobody answers media
channel, time or part requests yet: a stream connects but gets no parts
until the live-streams step.

## Build order

1. Build pipeline and the version API (this doc's first PR).
2. The C API and `crates/quill-calls-sys`.
3. 1:1 calls.
4. Group calls.
5. Live streams.
6. Switch to tgcalls and delete ntgcalls.

Then the parity items: conference calls, battery-low line, speaking levels for
everyone in group calls, signal bars, the muted-speech hint.

Windows builds with MSVC like tdesktop and joins the pipeline before step 6.
