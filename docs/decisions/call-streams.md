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
- Quality: `updateRequestedVideoChannels` hands tgcalls a min and max
  quality per endpoint from the tile size. The RTMP endpoint always gets
  max Full, and tgcalls asks Full for RTMP's unified channel.
- A listener of an MTProto stream who is allowed to speak rejoins
  (`Rejoin after unforcemute in stream mode`) to get an RTC connection.

## What TDLib does

`GroupCallManager::finish_get_group_call_streams` and
`finish_get_group_call_stream_segment` call `on_group_call_left` for
`GROUPCALL_JOIN_MISSING`, `GROUPCALL_FORBIDDEN` and `GROUPCALL_INVALID`,
with `need_rejoin` only for `GROUPCALL_JOIN_MISSING`. A segment request
with `channel_id` 0 is an audio request; `video_quality` maps
Thumbnail/Medium/Full to 0/1/2.

## What ntgcalls 3.0.3 does (source read at the v3.0.3 tag)

- `response_payload.cpp`: a join answer with an `rtmp` key is RTMP mode,
  one with a `stream` key is MTProto stream mode, anything else is RTC.
- `mtproto_stream.cpp`: only RTMP calls the timestamp callback; MTProto
  streams use the server time ntgcalls already has. After a timestamp of
  0 or less it asks again 1000 ms later. RTMP uses one "unified" part
  (channel 1, quality Full, ssrc 1); MTProto streams request audio on
  channel 0 (quality None) and one video part per subscribed endpoint,
  camera at Medium and screen at Full (`add_incoming_video`). There is no
  API to change those qualities.
- It keeps at most three segments pending (a 2000 ms buffer of 1000 ms
  segments, `mtproto_stream.hpp`). A part it asked for stays
  `Downloading` until it gets an answer, and the time request stays
  outstanding until `send_broadcast_timestamp`. It never asks again for
  a request nobody answered.
- `send_broadcast_part` throws on `DOWNLOADING`, so Quill never sends it.
  The C wrapper turns a null data pointer into "no data". A SUCCESS
  without data then makes ntgcalls ask for a normal part forever, and
  for a quality update part it reads the missing data on the media
  thread, which crashes the process.
- `group_call.cpp`: a Stream connection may later reconnect as RTC (a new
  `ntg_connect` with an RTC answer); RTC cannot go back to a stream.
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
sends into an unbounded channel and never drops a request, because
ntgcalls never asks again for one that went unanswered. ntgcalls bounds
the backlog itself (three pending segments, each with a part per
channel, plus one time request). `request_tick` drains up to 64 events
per poll. TDLib answers to stream requests are taken in `ingest` before the
flood retry and the reducer, so segment bytes never pass through the
session. A membership error sets `reconnecting` and runs the existing
`join.rs` rejoin. `GROUPCALL_INVALID` counts as having left, as TDLib
does: the pending parts are answered, the native call stops, no rejoin
is tried, and the stream shows Ended. Leave, end, replacement and rejoin
drain the feeder (pending parts answered `RESYNC_NEEDED`) before the
native stop. The rejoin now stops the old native call too, before it
asks for the fresh offer. Before, ntgcalls still held a connection for
the chat, so the new offer failed and the old stream kept asking for
parts. A broadcast request for a chat with no feeder is answered at once
(0 or `RESYNC_NEEDED`). Stream pictures are kept only for chats with a
live feeder, so a frame that lands between the drain and the native stop
is dropped.

The answer to ntgcalls: SUCCESS always passes a non-null data pointer,
even for 0 bytes; NOT_READY and RESYNC_NEEDED pass none. A `data` answer
from TDLib without a `data` string is RESYNC_NEEDED, never SUCCESS. The
stream turns Live on its first successful segment, not when the stream
list says there is one.

Mode and capture: `stream_chats` (the routing set for stream pictures)
gains the chat on a stream-mode connect, loses it on an RTC connect, and
loses it when a fresh context is created, so an RTC call after a stream
session routes its video by ssrc again. Neither stream mode issues a
capture source, so a listener's microphone is never opened. RTMP would
throw for one anyway, and an MTProto stream sends nothing. Only
`ntg_connect` returning `NTG_ERR_RTMP_STREAMING_UNSUPPORTED` means
Unsupported; the mode then comes from the join answer (ntgcalls has
none to report) and the native call is stopped.

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
- The tile size does not change segment quality. Every part asks TDLib
  for exactly the quality ntgcalls named: Full for RTMP (as tdesktop
  does) and ntgcalls' own per-endpoint choice for MTProto streams.
  tdesktop picks those per endpoint from the tile size, but ntgcalls
  3.0.3 has no API for that. `set_stream_quality` records the bucket for
  the window and for a later ntgcalls that has the API.
- Allowed-to-speak in an MTProto stream (tdesktop's rejoin to RTC) is
  not handled yet; it needs the mode switch above and belongs with the
  call window work.
- `StreamState::Ended` is set only by `GROUPCALL_INVALID`. A stream that
  ends normally ends the group call, and the tracked call is dropped; the
  live story viewer (PR 3) uses Ended for its 2 s placeholder.
- Live stories still resolve the native chat id through
  `chat.video_chat`, which a live story does not have, so they join with
  the no-device parameters and get no feeder. PR 3 owns that.

## RTMP support per platform

| Platform | Binary | Finding |
| --- | --- | --- |
| macOS arm64 | ntgcalls v3.0.3 (`vendor-ntgcalls.sh`, checksum verified) | Verified at runtime, offline: `native_rtmp_connect_reports_stream_mode` connects with `{"rtmp":true}`, reads mode `Rtmp`, issues playback-only sources and receives the timestamp request; after a 0 answer the request comes again. The MTProto stream test passes the same way. Nothing reaches Telegram. |
| Linux x86_64 | v3.0.3 | Not run. Same source and build config (`targets/c/build.cfg` builds all platforms the same way, and the stream code has no platform `#if`). Expected to work; unverified. |
| Windows x86_64 | v3.0.3 | Not run, same reasoning as Linux. Unverified. |

The main checkout's `vendor/ntgcalls` held an older dylib that is missing
`ntg_participants_request_free`, so `NtgcallsEngine::load` failed there;
re-running `scripts/vendor-ntgcalls.sh` fixes it.

## How it was verified

- Unit tests: `stream.rs` (quality buckets, native conversions, frame
  routing, the mode of a join answer, the data pointer for each status),
  `stream_feeder/tests.rs` (18 tests covering the brief's list plus empty
  data, Live on the first segment and `GROUPCALL_INVALID`),
  `envelope/auth.rs` (error classes), `types.rs` (`forget_group` clears
  the stream chat).
- Driver tests with the recording sender and the mock engine
  (`connect/tests/group_calls/stream.rs`, 19 tests): mode recorded on
  connect, timestamp round trip, empty list, segment request JSON and
  `data` answer, a `data` answer without bytes, error mapping (420, 429,
  `TIME_TOO_BIG`, other), `GROUPCALL_JOIN_MISSING` / `GROUPCALL_FORBIDDEN`
  rejoin, `GROUPCALL_INVALID` as left, drain on leave, drain and native
  stop on rejoin, 600 queued requests all answered, requests without a
  feeder, quality reported only on bucket change, Unsupported as a join
  error (and only from `ntg_connect`), the stream frame slot and a late
  frame after leaving, viewer count.
- Two native tests on macOS (`QUILL_VERIFY_NATIVE_CALLS=1`), both
  offline: the RTMP test above, and
  `native_stream_connect_needs_no_capture_and_asks_for_parts`, which
  connects with `{"stream":true}` and no capture source, reads mode
  Stream, gets an audio part request (channel 0, no quality) with no
  timestamp request, answers NOT_READY and gets the next part request.

Not verified: a real stream end to end (needs joining a live stream,
which this PR does not do), the RTC to stream switch in one call, and
Linux and Windows. The error strings now match TDLib's own checks in
`GroupCallManager.cpp`, but no server error was seen.

The routing fix for an RTC call after a stream session is in the
native engine and has no offline test (the mock does not route frames).

No performance work in this PR. The feeder allows 16 pending parts per
stream and holds request metadata only: segment bytes go to ntgcalls as
soon as TDLib returns them. That covers three pending segments with four
video endpoints and a quality update; past it a part answers NOT_READY
and ntgcalls asks again 100 ms later. The event queue holds only what
ntgcalls asked for and has not been answered.
