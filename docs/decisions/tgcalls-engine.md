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
- `libquill_calls` is loaded at runtime, as ntgcalls was, which keeps
  tgcalls' LGPL-3.0 terms met the same way: users can rebuild and replace it
  (`vendor/quill-calls/share/SOURCES.txt` names every source).
  `vendor/quill-calls/share/licenses/` carries the notices each part asks
  to ship with its binaries.
- The library finds FFmpeg and OpenSSL next to itself (`@loader_path`,
  `$ORIGIN`). On macOS the build script rewrites their absolute install
  names to `@rpath/<name>` and re-signs, and fails if any build path is left.
  On Windows OpenSSL is linked in, and the Rust loader opens
  `quill_calls.dll` with `LOAD_WITH_ALTERED_SEARCH_PATH`, so FFmpeg's DLLs
  are found beside it first.

Where the build deliberately differs from tdesktop's:

- tg_owt is linked in non-packaged mode on Linux too, where tdesktop's
  Linux build uses the packaged one; one copy of every codec is simpler to
  ship and keeps the exported surface to `qc_*`.
- PipeWire 0.3.62's headers are fetched for tg_owt's camera code (the
  version tdesktop's build image uses) because Ubuntu 22.04's are too old.
- rnnoise is the desktop-app fork tdesktop uses; OpenH264 is built with its
  makefile rather than meson (its MSVC makefile on Windows); of tdesktop's
  libvpx patches (desktop-app/patches `libvpx/`) only the three a Windows
  x64 build needs are applied, on Windows: no forced `/GL`, parallel
  compiles, and the NASM 3 fix (`native/quill-calls/patches/libvpx-win-*`).
  The others fix Windows-on-Arm and let `SDKROOT` pick the macOS SDK.
- Windows: everything is MSVC with the static C runtime (`/MT`), as in
  tdesktop's win64 build, but FFmpeg stays the shared MinGW build Quill
  already ships (tdesktop links a static MSVC FFmpeg); the build script
  makes MSVC import libraries from the DLLs' exports. OpenSSL 3 and zlib
  are static from vcpkg (`x64-windows-static`, as TDLib's Windows build
  takes them) rather than built from tdesktop's pins. rnnoise is built
  with an empty `RNNOISE_EXPORT`, so its functions are not exported from
  `quill_calls.dll`.
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
  requested video channels (JSON from `groupCallParticipantVideoInfo` plus the participant's audio ssrc for lip sync) and
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
drops them until the speaking-levels parity item. Media channel requests
stay unwired: tgcalls then accepts every ssrc as it comes, which a stream
never needs (its parts carry no ssrcs) and an RTC call plays the same.

### Live streams

A stream (an RTMP channel stream, a live story or a large MTProto
broadcast) is broadcast mode. The engine trait carries tgcalls' requests
as they are: `EngineEvent::BroadcastTimeRequested`,
`BroadcastPartRequested` (start, duration, audio or a video channel at a
quality) and `BroadcastRequestCancelled`, each with a
`BroadcastRequestId` (the instance and the library's id, so an answer
for an instance a rejoin replaced is dropped). The driver's
`StreamFeeder` answers them with TDLib as tdesktop's `GroupCall` does
with MTProto (`requestCurrentTimeStart`, `broadcastPartStart`,
`LoadPartTask`):

- Time: an RTMP stream asks `getGroupCallStreams` once for every waiting
  request and answers the first channel's `time_offset` (0 for none, and
  for any failure); other streams answer the server time at once, kept
  from TDLib's `unix_time` option.
- Part: `getGroupCallStreamSegment` at the duration's scale (1000 ms is
  0, each halving one more), with the video channel and quality only for
  a video channel other than 0. A part for "now" loads the server's
  current second. `data` is Success; flood and `TIME_TOO_BIG` NotReady;
  `GROUPCALL_JOIN_MISSING` and `GROUPCALL_FORBIDDEN` NotReady and a
  rejoin; anything else ResyncNeeded.
- A cancelled request is forgotten and TDLib's late reply dropped; a
  leave or rejoin answers every open part ResyncNeeded first.

Two answers differ from tdesktop: the response time is the server time
when the answer goes out (TDLib does not pass on the reply's message
time), and a membership error answers the part NotReady before the
rejoin, where tdesktop leaves it to the new instance.

An RTMP stream also asks for tdesktop's `RtmpEndpointId` (`"unified"`)
video, at least Full while the stream tile is Full, and every picture of
a stream goes to its chat's stream feed.

Known gaps, kept on purpose for now:

- A stream shows one picture: the RTMP feed alone, otherwise one
  speaker's screen (or, without one, the first camera). Asking for every
  endpoint would send all of them to the one stream feed.
- Server time comes from TDLib's `unix_time` option, whole seconds
  measured when the driver reads the update, so MTProto time answers can
  be up to about a second off. tdesktop gets milliseconds from the join
  reply's message id, which TDLib does not pass on. A time that runs
  early is answered `TIME_TOO_BIG` and retried, so a stream starts a
  little later but does not stall.
- Requests queued by an instance a rejoin replaced still reach TDLib
  once; their answers are dropped by instance id.

## The switch (step 6)

The app loads `TgcallsEngine` (`connect/live.rs`) and preloads its library
off the UI thread (`preload_tgcalls`, `ui/early_connect.rs`). There is no
other engine and no switch back; tests drive the mock engine.

Without an engine (the library did not load) no call is joined: a 1:1
call, a voice chat, a live stream and a live story all say "Calls are
unavailable" (`ConnectSendError::CallsUnavailable` from
`join_video_chat`, `rejoin_group_call` and `join_live_story`, and the UI's
`refuse_without_call_engine` before it asks anything). Before the switch
a group join went out signaling-only, and you sat in a call you could
neither hear nor see.

Deleted with ntgcalls: `crates/ntgcalls-sys`, `src/calls/engine/ntgcalls/`,
the ntgcalls trampolines in `engine/native.rs`, `scripts/vendor-ntgcalls.sh`,
its CI and packaging steps, its license files and its deps-watch source.
Shared code shaped by ntgcalls went too:

- `CallbackShared` keeps only the driver's hooks; the per-chat routing
  tables (and `NtgPartRequests`) were ntgcalls' callback demultiplexing.
- `GroupCallMode::from_native`, `StreamQuality::from_native`,
  `BroadcastPartStatus::to_native` and `group_video_target` mapped
  ntgcalls' enums.
- `StreamState::Unsupported` and `EngineError::is_stream_unsupported`:
  only ntgcalls refused a stream (`NTG_ERR_RTMP_STREAMING_UNSUPPORTED`).
- The story-stream key yield for 1:1 calls: ntgcalls keyed a group call
  by chat id and a 1:1 call by user id, so a live story and a call with
  its poster could not coexist. tgcalls instances are independent; both
  keep running. The yield stays for a group call in the story's own chat
  (`yield_story_stream_to_chat`, `refuse_story_stream_in_member_chat`):
  a stream's picture (`LiveKey::Stream`), mode and quality are still
  keyed by chat id, so a channel's live story and the same channel's
  stream joined as a member would share them. The call you join wins and
  the story shows failed, with Retry.
- `QUILL_CALL_CAPTURE=native` and `QUILL_STREAM_AUDIO_DEVICE=1` left the
  microphone or the sound to ntgcalls' devices. The library has none, so
  Quill always captures and plays. The sound self-healing step now
  reopens Quill's output (`reopen_stream_sound`, `reopen_call_sound`)
  instead of moving to a device output that no longer exists.

### Packaging

`quill-calls.yml` is a reusable workflow: ci.yml (pushes) and release.yml
run it before the Linux and macOS package jobs, which download its
`quill-calls-<target>` artifact from the same run. The tg_owt and codec
cache stays in that workflow.

- Linux: `lib/libquill_calls.so` beside `libtdjson.so` (RUNPATH
  `$ORIGIN`), linking the package's FFmpeg and OpenSSL;
  `check-bundle-elf.sh` and `check-bundle-dlopen.py` check it.
- macOS: `Contents/Frameworks/libquill_calls.dylib`, id
  `@rpath/libquill_calls.dylib`, with the FFmpeg dylibs and OpenSSL it
  links as `@rpath/<name>` copied beside it (the app's first FFmpeg
  files). The package job restores or builds the same FFmpeg as
  quill-calls.yml and loads the library from the bundle.
- Both ship `licenses/quill-calls/` (the library's `SOURCES.txt` and
  every part's notice). `release-lgpl-sources.sh` archives tgcalls at its
  pinned commit in place of ntgcalls and its FFmpeg and GLib.
- `quill-calls-sys` finds the library in `lib/` beside the executable,
  in `Frameworks/`, or beside the executable on Windows.

TODO(tgcalls-windows): Windows builds `quill_calls.dll` in another
branch. Until it joins packaging, `windows-package.ps1` ships the zip
without it (it copies `vendor/quill-calls/lib/quill_calls.dll` when one is
there), the bundle checks treat it as optional, and Windows calls show as
unavailable, as they do whenever the library is missing. When the DLL
arrives: build it in quill-calls.yml, download it in package-windows.yml,
and make it required in `windows-package.ps1`, `check-bundle-pe.ps1` and
`check-bundle-load.ps1`.

## Build order

1. Build pipeline and the version API (this doc's first PR).
2. The C API and `crates/quill-calls-sys`.
3. 1:1 calls.
4. Group calls.
5. Live streams.
6. Switch to tgcalls and delete ntgcalls.

Then the parity items: conference calls, battery-low line, speaking levels for
everyone in group calls, signal bars, the muted-speech hint.

Windows builds with MSVC like tdesktop (`scripts/build-tgcalls.sh` from
MSYS2 bash with the MSVC environment inherited) in the quill-calls workflow.
No package ships `libquill_calls` yet on any platform; the packages add it,
beside `quill.exe` on Windows, before step 6.
