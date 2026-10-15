# Group call streams, PR 1: engine stream mode

Design: `quill-tools/designs/group-call-streams.md`, build plan item 1.
Branch `call-stream-engine`. No parity id closes here; the RTMP window
(PR 2) and the live story viewer (PR 3) close `calls-rtmp-viewing` and
`stories-live-stream-playback`.

## What tdesktop does

`calls/group/calls_group_call.cpp`:

- `requestCurrentTimeStart` (RTMP only) sends
  `phone.getGroupCallStreamChannels`, answers tgcalls with the first
  channel's `last_timestamp_ms`, answers 0 for an empty list and sets
  `_emptyRtmp`. Any failure answers 0; `GROUPCALL_JOIN_MISSING` and
  `GROUPCALL_FORBIDDEN` also call `startRejoin`.
- `LoadPartTask` answers part requests. Flood errors and `TIME_TOO_BIG`
  are `NotReady`, the two membership errors rejoin, anything else is
  `ResyncNeeded`.
- There is no polling timer of its own. tgcalls asks for the time again
  after a 0.

## What ntgcalls 3.0.3 does (source read at the v3.0.3 tag)

- `response_payload.cpp`: a join answer with an `rtmp` key is RTMP mode,
  one with a `stream` key is MTProto stream mode, anything else is RTC.
- `mtproto_stream.cpp`: only RTMP calls the timestamp callback; MTProto
  streams use the server time ntgcalls already has. After a timestamp of
  0 or less it asks again 1000 ms later. RTMP uses one "unified" part
  (channel 1, quality Full, ssrc 1); MTProto streams request audio on
  channel 0 (quality None) and one video part per subscribed endpoint.
  `send_broadcast_part` throws on `DOWNLOADING`, so Quill never sends it.
- `group_call.cpp`: `NTG_ERR_RTMP_STREAMING_UNSUPPORTED` ("Streaming is
  not supported when using RTMP") is thrown when an RTMP connect finds
  capture readers, and by `set_stream_sources(Capture)` in RTMP mode. It
  is not a build flag.
- `stream_manager.cpp`: incoming video reaches the frames callback only
  when the playback sources carry an external camera or screen writer
  (`optimize_sources` gates `enable_video_incoming` on them).
- The C wrapper copies the part bytes into a `std::vector` before it
  returns, and runs the request callbacks on its update thread.

## What changed

Engine (`src/calls/engine/`):

- `stream.rs`: `GroupCallMode {Rtc, Stream, Rtmp}`, `StreamQuality` with
  the tile thresholds (640 and wider Full, 320 and wider Medium, else
  Thumbnail), the owned `BroadcastPartRequest`, `BroadcastPartStatus`,
  `EngineEvent`, `GroupVideoTarget` with the pure routing function, and
  `StreamState {Connecting, NoStream, Live, Ended, Unsupported}`.
- `native.rs`: the timestamp and part trampolines copy the request and
  call the engine-event hook inside `ffi_boundary`. Frames for a chat in
  `stream_chats` go to the stream-frame hook (one slot per chat) instead
  of the ssrc lookup.
- `ntgcalls/mod.rs`: all six callbacks are registered once per instance
  (`register_callbacks`) and cleared together on failure and in `Drop`
  (`clear_callbacks`). Group sources follow the mode: RTMP issues no
  capture source (it would throw -103) and both stream modes add external
  camera and screen playback writers so pictures arrive.
- `ntgcalls/call_engine.rs`: after `ntg_connect` the engine reads
  `ntg_get_connection_mode` and records it; new `group_call_mode`,
  `set_stream_quality`, `stream_quality`, `send_broadcast_timestamp`,
  `send_broadcast_part`. The mock engine records all of them.
- `stream_feeder.rs`: the per-chat loop, with no IO of its own; a
  `StreamPort` sends the requests and answers.

Connect side (`src/connect/group_calls/stream.rs`): the engine hook
`try_send`s into a bounded channel (256); a full queue drops the request
and logs every hundredth drop. `request_tick` drains up to 64 events per
poll. TDLib answers to stream requests are taken in `ingest` before the
flood retry and the reducer, so segment bytes never pass through the
session. A membership error sets `reconnecting` and runs the existing
`join.rs` rejoin. Leave, end, replacement and rejoin drain the feeder
(pending parts answered `RESYNC_NEEDED`) before the native stop. A
broadcast request for a chat with no feeder is answered at once (0 or
`RESYNC_NEEDED`).

State: `ActiveGroupCall` gains `stream_mode`, `stream` and
`viewer_count` (from `participant_count`, negative read as 0). TDLib
errors gain two classes, `GroupCallRejoin` and `StreamTimeTooBig`,
classified from the exact server messages; other screens show the same
text as before for them.

`QUILL_TRACE_VIDEO=1` also prints `VS,<chat>,req,...` when a segment is
asked of TDLib and `VS,<chat>,ans,<segment>,<part>,<status>,<elapsed_ms>,<bytes>`
when ntgcalls gets its answer.

## Deviations from the design

- No 2 s retry timer for an empty stream list. ntgcalls asks again 1 s
  after a 0 answer, and tdesktop relies on the same behavior from
  tgcalls. Each ask sends a fresh `getGroupCallStreams`.
- There is no `EngineEvent::RejoinRequired`. The rejoin need is found on
  the connect side (in a TDLib answer), so the feeder returns
  `ReplyOutcome::Rejoin` and the driver sets `reconnecting`.
- The design's "existing engine event channel" did not exist; the
  engine used per-kind hooks into outboxes. The new hook follows that
  pattern with a bounded `sync_channel`.
- Video parts use the tile's quality once it has reported one;
  quality-update parts and audio parts keep what ntgcalls asked for. For
  RTMP, ntgcalls always asks for Full, so the override is what makes the
  tile size matter.
- `StreamState::Ended` exists but nothing sets it yet. A stream that ends
  ends the group call, and the tracked call is dropped; the live story
  viewer (PR 3) uses Ended for its 2 s placeholder.
- Live stories still resolve the native chat id through
  `chat.video_chat`, which a live story does not have, so they join with
  the no-device parameters and get no feeder. PR 3 owns that.

## RTMP support per platform

| Platform | Binary | Finding |
| --- | --- | --- |
| macOS arm64 | ntgcalls v3.0.3 (`vendor-ntgcalls.sh`, checksum verified) | Verified at runtime, offline: `native_rtmp_connect_reports_stream_mode` connects with `{"rtmp":true}`, reads mode `Rtmp`, issues playback-only sources and receives the timestamp request; after a 0 answer the request comes again. Nothing reaches Telegram. |
| Linux x86_64 | v3.0.3 | Not run. Same source and build config (`targets/c/build.cfg` builds all platforms the same way, and the stream code has no platform `#if`). Expected to work; unverified. |
| Windows x86_64 | v3.0.3 | Not run, same reasoning as Linux. Unverified. |

The main checkout's `vendor/ntgcalls` held an older dylib that is missing
`ntg_participants_request_free`, so `NtgcallsEngine::load` failed there;
re-running `scripts/vendor-ntgcalls.sh` fixes it.

## How it was verified

- Unit tests: `stream.rs` (quality buckets, native conversions, frame
  routing), `stream_feeder/tests.rs` (15 tests covering the brief's list),
  `envelope/auth.rs` (error classes), `types.rs` (`forget_group` clears
  the stream chat).
- Driver tests with the recording sender and the mock engine
  (`connect/tests/group_calls/stream.rs`, 12 tests): mode recorded on
  connect, timestamp round trip, empty list, segment request JSON and
  `data` answer, error mapping (420, 429, `TIME_TOO_BIG`, other),
  `GROUPCALL_JOIN_MISSING` / `GROUPCALL_FORBIDDEN` rejoin, drain on
  leave, requests without a feeder, quality reported only on bucket
  change, Unsupported as a join error, stream frame slot, viewer count.
- The native RTMP test above on macOS (`QUILL_VERIFY_NATIVE_CALLS=1`).

Not verified: a real stream end to end (needs joining a live stream,
which this PR does not do), the exact TDLib error strings for segment
errors (taken from tdesktop's server constants), MTProto stream mode at
runtime (its connect opens the microphone), and Linux and Windows.

No performance work in this PR. Memory is bounded by 8 pending parts per
stream (about 1 MB each at Full) and the 256-event queue.
