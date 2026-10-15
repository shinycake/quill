# Watching streams: channel live streams (RTMP) and live stories

Parity items `calls-rtmp-viewing` and `stories-live-stream-playback`.
Build-ready design, Fable architect pass 2026-10-15. Supersedes the
2026-10-15 triage note. Format follows `designs/video-player.md`.

Scope: Quill can *watch* a channel live stream (RTMP video chat) in the
group call window, and *watch* a live story (stream, comments, paid
reactions, top donors) in the story viewer. Starting a stream (RTMP
URL/key box) is a small admin add-on at the end. Broadcasting camera or
screen into a stream is already covered by the normal video chat code.

## Summary of the approach

1. **Stream mode lives in the call engine, not the UI.** ntgcalls enters
   `NTG_CONNECTION_MODE_STREAM` or `_RTMP` on its own when the join answer
   from TDLib carries `{"stream": true}` / `{"rtmp": true}` (same as
   tgcalls). Quill only has to (a) register the two broadcast callbacks on
   the instance, (b) answer them with TDLib calls, and (c) route the
   frames that come back. No new native code, no new FFI surface beyond
   the functions already in `crates/ntgcalls-sys/src/ffi.rs` (header
   `vendor/ntgcalls/include/ntgcalls.h:496-509, 595-608`).
2. **One `StreamFeeder` per joined stream call** owns the segment loop:
   timestamp request → `getGroupCallStreams`; part request →
   `getGroupCallStreamSegment`; TDLib answer → `ntg_send_broadcast_part`.
   It runs on the connect thread (TDLib reducer side), never on the UI
   thread, and is keyed by chat id exactly like the frame routing.
3. **One `StreamTile` element** draws stream video for both surfaces.
   The group call window shows it full-bleed in a wide layout; the story
   viewer shows it inside the story frame. It is a thin consumer of the
   existing `VideoFrame` element path from the video player core (PR
   #804): pictures arrive from `frames_trampoline` as the group tiles do.
4. **Quality follows the tile size** (thumbnail / medium / full), set by
   answering part requests with the quality the tile asked for, mirroring
   tdesktop's `videoQuality` choice in `calls_group_call.cpp`.
5. **Live stories reuse the stream tile plus the group call chat.**
   Comments are `groupCallMessage`s, already parsed into
   `ActiveGroupCall.messages` (Phase C2h). Paid reactions, top donors and
   the streamer card are new state slices fed by the TDLib updates listed
   below.

## Building blocks that exist

- TDLib 1.8.68 (`schema/td_api.tl`): `getGroupCallStreams group_call_id
  = GroupCallStreams` → `groupCallStream { channel_id:int32 scale:int32
  time_offset:int53 }` (`time_offset` = the stream end as Unix time in
  ms, i.e. the "current time" of the broadcast); `getGroupCallStreamSegment
  group_call_id time_offset:int53 scale:int32 channel_id:int32
  video_quality:GroupCallVideoQuality = Data` with
  `groupCallVideoQualityThumbnail | Medium | Full`. Segment length is
  `1000 / 2^scale` ms; tdesktop only ever asks for scale 0, Quill does the
  same. Live stories: `storyContentLive group_call_id is_rtmp_stream`,
  `joinLiveStory`, `getLiveStoryStreamer`, `getLiveStoryTopDonors`,
  `updateLiveStoryTopDonors`, `liveStoryDonors { total_star_count
  top_donors:vector<paidReactor> }`, `addPendingLiveStoryReaction`,
  `getLiveStoryAvailableMessageSenders`, `setLiveStoryMessageSender`,
  `sendGroupCallMessage`, `updateNewGroupCallMessage`,
  `updateGroupCallMessagesDeleted`, `deleteGroupCallMessages`,
  `updateGroupCallMessageLevels`. `groupCall` carries `is_live_story`,
  `is_rtmp_stream`, `participant_count`, `can_send_messages`,
  `are_messages_allowed`, `paid_message_star_count`.
- ntgcalls 3: `ntg_on_request_broadcast_timestamp_callback(instance,
  cb(instance, chat_id, user_data))`,
  `ntg_on_request_broadcast_part_callback(instance, cb(instance, chat_id,
  ntg_segment_part_request { segment_id:i64 part_id:i32 limit:i32
  timestamp:i64 quality_update:bool channel_id:i32 quality }, user_data))`,
  answered with `ntg_send_broadcast_timestamp(handle, chat_id, timestamp)`
  and `ntg_send_broadcast_part(handle, chat_id, segment_id, part_id,
  status, quality_update, data, len)` where status is
  `NOT_READY | RESYNC_NEEDED | DOWNLOADING | SUCCESS`.
  `ntg_get_connection_mode(handle, chat_id, out)` tells STREAM/RTMP.
  `NTG_ERR_RTMP_STREAMING_UNSUPPORTED (-103)` is the error ntgcalls
  returns when the build lacks RTMP support; treat it as a fatal join
  error with the "not supported on this build" copy, never a crash.
- Quill engine: `CallEngine` trait (`src/calls/engine/api.rs`),
  `ntgcalls/call_engine.rs::connect_group_call` (`ntg_connect`
  `:335-365`), `native.rs::frames_trampoline` (`:125-180`, group frames
  keyed by chat id, ssrc → user via `group_video_ssrc_to_user`, unknown
  ssrc dropped), `src/connect/group_calls/join.rs` (`join_video_chat`
  `:75`, `group_join_params` resolves chat id through `chat.video_chat`
  `:143-170`, `maybe_join_live_story` + `pending_live_story_join`),
  `src/state/call_types.rs::ActiveGroupCall` (`is_live_story:122`,
  `is_rtmp_stream:182`, `messages:203`), `src/ui/group_call_panel.rs`
  (420×640 window, min 380×520, `:681-685`), `src/ui/group_call_tiles.rs`,
  `src/ui/story_viewer/story_viewer_overlay.rs` (`:129-155` Join button
  for non-RTMP live stories, "RTMP playback not supported" placeholder),
  `src/ui/story_viewer/mod.rs::join_live_story_from_viewer`,
  `src/connect/stories/mod.rs::join_live_story`.

## What tdesktop does (reference, Telegram/SourceFiles)

- Segment loop: `calls/group/calls_group_call.cpp` `LoadPartTask`
  (`:138, 426-476`) answers tgcalls' part requests with `upload.getFile`
  of 128 KiB chunks on the stream DC; flood wait and `TIME_TOO_BIG` →
  `NotReady`, any other error → `ResyncNeeded`,
  `GROUPCALL_JOIN_MISSING` / `GROUPCALL_FORBIDDEN` → rejoin the call.
  `requestCurrentTime` asks `phone.getGroupCallStreamChannels` and
  answers with the first channel's `last_timestamp_ms`; an empty channel
  list sets `_emptyRtmp` and shows `lng_group_call_no_stream` ("{group} is
  currently not broadcasting live stream data to Telegram.", admins get
  `lng_group_call_no_stream_admin` with the RTMP box hint).
- RTMP viewing: one video endpoint of `Screen` type
  (`RtmpEndpointId`), quality `Full` when the tile is large, else
  `Thumbnail`…`Full` by size; volume through a single source; listener
  rows hidden; no mute actions; one fake member row for the stream.
- RTMP panel (`calls_group_panel.cpp`, `calls/calls.style`): forced
  `PanelMode::Wide`; window 720×580, min 240×160 (normal call window
  380×520); mute button click toggles fullscreen; Space is push-to-talk
  (no-op for viewers); Esc leaves fullscreen or maximised; controls hide
  on a timer in fullscreen; camera and screen buttons are hidden;
  subtitle `lng_group_call_rtmp_viewers` ("{count} viewers"); "pin on
  top" toast. Title is the chat title, not "Video Chat".
- Live stories (`media/stories/media_stories_controller.cpp`,
  `media_stories_header.cpp`, `media_stories_reply.cpp`,
  `media/view/media_view_video_stream.cpp`): `_videoStream =
  story->call()`; header shows a red LIVE badge (`lng_video_stream_live`)
  and "{count} watching"; the reply row becomes a comment composer with a
  send-as picker, no likes, no attachments, no voice; emoji-only panel;
  paid comments "Comment for {count} Stars"; disabled state
  `lng_video_stream_comments_disabled`; paid reaction toast with star
  burst; top-donor avatars with places; the `VideoStream` widget
  (`displayVideoStream`, `paintVideoStream`, `setVolume`,
  `toggleCommentsOn`) paints the call viewport inside the story frame and
  closes the viewer on `groupCallFinished` / `groupCallFailed`.

## What the user sees

**Channel live stream.** Open a channel whose video chat is an RTMP
stream (header "Live stream" bar, same as today's video chat bar). Join
opens the call window in the wide layout: 720×580 at first open, min
240×160, title = channel title, subtitle "{count} viewers". The stream
fills the window above the bottom bar. While waiting for the first
picture: the channel's photo blurred behind a kit `Spinner` and the
label "Connecting…". If the channel is not broadcasting: the blurred
photo with "{channel} is currently not broadcasting live stream data to
Telegram." (admins: "…Start a stream from OBS or similar with the RTMP
URL and key." plus a "Stream key" button opening the RTMP box). Bottom
bar: volume (same `VolumeControl` from the video player design), a
fullscreen toggle, leave. No mic, camera or screen buttons. Click on the
picture toggles the bar; double-click toggles fullscreen; Esc leaves
fullscreen; the bar hides after 1100 ms of no mouse movement in
fullscreen, same timings as the player design. Window opens pinned on
top like today's call window, with the same toast.

**Live story.** In the story viewer a live story shows the stream in
place of the story picture. Header: avatar, name, red LIVE pill
(`text_xs` semibold, `danger` background, white text, radius_full, 20 px
tall) and "{count} watching" in `text_muted`. Reply row: the comment
composer ("Comment", send-as avatar on the left when there is more than
one sender, emoji button, send). If comments cost stars the placeholder
is "Comment for {n} Stars" and sending opens the usual Stars confirm; if
disabled, a muted "Comments are disabled" row. Comments float up from
the bottom-left over the picture, newest at the bottom, each a
`radius_lg` dark pill with avatar, name and text, fading out after 8 s or
scrolling as a list when the user hovers them. Paid reactions: the
existing star reaction button sends `addPendingLiveStoryReaction`; stars
burst from the button (the particle technique in
gpui-effects-techniques); the top-donors row (three avatars with 1/2/3
place badges and the total star count) sits under the header. Volume is
the viewer's volume. Pause (space, hold) does not pause a live stream; it
only hides the chrome. Closing the viewer leaves the call. Stream ended
→ the "Live stream ended" placeholder for 2 s, then the viewer advances
as it does after a normal story.

**Not supported on this build** (`NTG_ERR_RTMP_STREAMING_UNSUPPORTED`):
the same placeholder with "Live stream playback is not available in this
build." The Join button stays usable so the user can retry after an
update. Existing keys `Q_MEDIA_LIVE_STORY`, `Q_MEDIA_JOIN_LIVE`,
`Q_MEDIA_RTMP_PLAYBACK_IS_NOT_SUPPORTED` stay; new keys below.

## Engine: stream mode (`src/calls/engine/`)

### Join

No changes to the join request: `joinVideoChat` / `joinLiveStory` already
return the server's JSON answer which ntgcalls parses in `ntg_connect`
(`call_engine.rs:359`). After `ntg_connect` returns, call
`ntg_get_connection_mode` and record it in the engine's per-call entry
(`GroupCallMode::{Rtc, Stream, Rtmp}`). Expose it through a new
`CallEngine::group_call_mode(chat_id) -> Option<GroupCallMode>` so the
reducer can flip `ActiveGroupCall.stream_mode`. Register the two
broadcast callbacks once per instance at instance creation (next to the
frames callback registration in `native.rs`), not per call: ntgcalls
callbacks are instance-wide and carry `chat_id`.

### `StreamFeeder` (`src/calls/engine/stream_feeder.rs`, new)

Owned by the connect side, one per chat id in stream mode, created when
the reducer sees `stream_mode != Rtc` and dropped on leave.

```
pub struct StreamFeeder {
    group_call_id: i32,
    chat_id: i64,
    channels: Vec<StreamChannel>,     // from getGroupCallStreams
    pending: HashMap<RequestId, PendingPart>,
    quality: StreamQuality,           // last requested by the tile
    state: FeederState,               // Idle | Syncing | Streaming | NoStream | Failed
    last_timestamp_ms: i64,
}
struct PendingPart { segment_id: i64, part_id: i32, quality_update: bool, requested_at: Instant }
```

Callback plumbing (never block the ntgcalls thread): the two trampolines
in `native.rs` push `EngineEvent::BroadcastTimestampRequested { chat_id }`
and `EngineEvent::BroadcastPartRequested { chat_id, request }` on the
existing engine event channel (the one the frames and network-state
callbacks already use). The connect loop handles them:

- **Timestamp requested** → send `getGroupCallStreams`. On
  `groupCallStreams`: if empty, `state = NoStream`, answer
  `ntg_send_broadcast_timestamp(chat_id, 0)` and schedule a retry in 2 s
  (tdesktop polls while `_emptyRtmp`); the UI shows the "not
  broadcasting" state. If not empty, keep the channel list, take the
  first channel's `time_offset`, answer with it and set `Streaming`.
- **Part requested** (`request.timestamp` is the segment's start in ms,
  `segment_id` the segment index ntgcalls derived from it) → send
  `getGroupCallStreamSegment(group_call_id, time_offset =
  request.timestamp, scale = 0, channel_id = request.channel_id,
  video_quality = quality(request.quality))` and remember the pending
  part by request id. On `data` → `ntg_send_broadcast_part(…, SUCCESS,
  quality_update, bytes)`. On error: code 420 or message containing
  `FLOOD_WAIT` or `TIME_TOO_BIG` → `NOT_READY`; message
  `GROUPCALL_JOIN_MISSING` or `GROUPCALL_FORBIDDEN` → answer `NOT_READY`
  and raise `EngineEvent::RejoinRequired { chat_id }` (the existing
  rejoin path in join.rs); anything else → `RESYNC_NEEDED`.
  `ntg_send_broadcast_part` with `data = null, len = 0` is valid for the
  non-success statuses.
- **Quality**: `StreamQuality` maps 1:1 to `GroupCallVideoQuality`. The
  tile reports its size through `CallEngine::set_stream_quality(chat_id,
  quality)`; the feeder stores it and uses it on the next part request
  (ntgcalls asks for `quality_update` parts itself when the requested
  quality changes, so Quill only reports). Thresholds as tdesktop: tile
  width ≥ 640 → Full, ≥ 320 → Medium, else Thumbnail.
- Segment bytes stay on the connect thread; the TDLib `data` answer is
  base64 in JSON, decode with the existing helper and hand the `Vec<u8>`
  straight to the FFI call. Budget: Full segments are ~1 MB/s; keep at
  most 8 pending parts, answer `NOT_READY` for requests beyond that.
- Leave/drop answers every pending part with `RESYNC_NEEDED` before the
  instance stops the call, so ntgcalls never waits on a dead request.

### Frame routing (`native.rs::frames_trampoline`)

Stream-mode calls have no participant ssrc map. Add a `stream_chats:
HashSet<i64>` beside `group_video_ssrc_to_user`; when a group frame's
chat id is in it, route the frame to `GroupVideoTarget::Stream { chat_id }`
instead of looking up the ssrc, and drop nothing. The existing
`GroupFrameSink` gains one slot per chat for the stream picture; the
`StreamTile` subscribes to that slot. Audio already plays through the
normal device path; the volume control calls the existing
`set_group_volume` with the single source.

### Never-crash

Callbacks arrive on ntgcalls threads: trampolines copy the request
struct into an owned `BroadcastPartRequest`, free nothing (ntgcalls owns
the struct for the callback's duration) and send; `try_send` failures
are logged and dropped. All TDLib answers are matched by request id;
a `data` for an unknown id is ignored. No indexing into `channels`:
`first()`. Quality and status enums are converted with exhaustive
matches that default to `Thumbnail` / `RESYNC_NEEDED`. FFI results are
checked and logged, never unwrapped. Base64 decode failure →
`RESYNC_NEEDED`.

## State (`src/state/call_types.rs`, `src/state/stories/`)

`ActiveGroupCall` gains:

```
pub stream_mode: GroupCallMode,          // Rtc | Stream | Rtmp (from the engine)
pub stream: StreamState,                 // Connecting | NoStream | Live | Ended | Unsupported
pub viewer_count: u32,                   // participant_count while streaming
```

Live story state (new `LiveStoryState` on the story item, keyed by
`group_call_id`):

```
pub streamer: Option<MessageSender>,     // getLiveStoryStreamer
pub donors: LiveStoryDonors,             // total_star_count, top_donors (≤3 shown)
pub pending_reaction_stars: u32,         // local optimistic count until the update
pub message_senders: Vec<MessageSender>, // getLiveStoryAvailableMessageSenders
pub send_as: Option<MessageSender>,
pub comments: GroupCallMessageFeed,      // view over ActiveGroupCall.messages + levels
```

Reducer handling: `updateLiveStoryTopDonors` → `donors`;
`updateGroupCallMessageLevels` → per-message level for the floating
pills; `updateGroupCallMessagesDeleted` → remove from `messages`;
`updateGroupCall` with `is_live_story` and `participant_count` → viewer
count. `joinLiveStory` answer → same `pending_live_story_join` path as
today, then `getLiveStoryStreamer`, `getLiveStoryTopDonors`,
`getLiveStoryAvailableMessageSenders` fired together.

## UI

### `StreamTile` (`src/ui/calls/stream_tile.rs`, new)

A `RenderOnce` element: `StreamTile::new(chat_id).fit(Contain|Cover)
.placeholder(photo, label)`. It pulls the latest stream picture from the
engine's stream slot, hands it to the `VideoFrame` element from
`src/ui/video_player/element.rs` with `VideoFrameAsk { interval = 1/30
s, lead = 0, plays_sound = true, keep_alive = true }` so the frame clock
keeps presenting while the stream runs (same request path the group tiles
use, no separate timer). On every layout it compares its bounds with the
last reported quality bucket and calls `set_stream_quality` only on a
bucket change. Placeholder states: Connecting (blurred photo + `Spinner`
+ label), NoStream (blurred photo + copy), Ended, Unsupported.

### Group call window, wide layout (`group_call_panel.rs`, `group_call_tiles.rs`)

When `stream_mode != Rtc`: window size 720×580, min 240×160, title =
chat title, subtitle "{count} viewers"; the tile area is one `StreamTile`
with `Contain` fit on black; the members list and the camera/screen/mic
buttons are not built; bottom bar = `VolumeControl` (left), fullscreen
toggle and leave (right), 56 px tall, `black 0.6` background with the
player design's hide/show timings in fullscreen only. Keys: Space no-op,
Esc leaves fullscreen, `F` and double-click toggle fullscreen. The
normal (non-stream) layout is untouched; the branch is one `if` at the
top of the panel's render so the two layouts stay in separate functions
(structure guardrails: new file `group_call_stream_layout.rs`, panel.rs
only dispatches).

### Story viewer (`src/ui/story_viewer/`)

New `story_viewer_live.rs` with three pieces the overlay composes:
`live_header_badges` (LIVE pill + watching count + donors row),
`live_comments` (floating pills list), `live_reply_row` (composer). The
overlay's existing live branch (`story_viewer_overlay.rs:129-155`)
becomes: live story with a joined call → `StreamTile` (`Cover` fit inside
the story frame, rounded by the frame's clip) plus the three pieces;
not joined → the current Join button; `Unsupported` → the placeholder
copy. The media viewer files are owned by the video player work; this
build touches only `story_viewer/*` and leaves `media_viewer/*` alone.
Comment send → `send_group_call_message` (exists in
`src/connect/group_calls/admin.rs:344`) with `send_as`; reaction button →
`addPendingLiveStoryReaction` with the local optimistic count, one
request per 500 ms batching clicks like tdesktop's pending reactions.

### Admin: RTMP box (`src/ui/dialogs/rtmp_stream.rs`, new, last PR)

Kit dialog with two copyable read-only fields (Server URL, Stream key with
a reveal toggle), "Revoke" (→ `replaceVideoChatRtmpUrl` /
`replaceLiveStoryRtmpUrl`) and "Start stream" for a channel without a
video chat (`createVideoChat` with `is_rtmp_stream = true`). Reached from
the channel's "…" menu ("Stream with…") and from the NoStream admin
placeholder.

### Strings (new keys, `locales/`)

`Q_CALLS_STREAM_VIEWERS` "{count} viewers", `Q_CALLS_STREAM_CONNECTING`
"Connecting…", `Q_CALLS_STREAM_NO_STREAM` "{chat} is currently not
broadcasting live stream data to Telegram.",
`Q_CALLS_STREAM_NO_STREAM_ADMIN` "Start a stream from OBS or similar
with the RTMP URL and key.", `Q_CALLS_STREAM_KEY` "Stream key",
`Q_CALLS_STREAM_UNSUPPORTED` "Live stream playback is not available in
this build.", `Q_STORY_LIVE` "LIVE", `Q_STORY_LIVE_WATCHING`
"{count} watching", `Q_STORY_LIVE_COMMENT` "Comment",
`Q_STORY_LIVE_COMMENT_PAID` "Comment for {count} Stars",
`Q_STORY_LIVE_COMMENTS_DISABLED` "Comments are disabled",
`Q_STORY_LIVE_ENDED` "Live stream ended", `Q_CALLS_RTMP_SERVER_URL`
"Server URL", `Q_CALLS_RTMP_REVOKE` "Revoke", `Q_CALLS_RTMP_START`
"Start stream". English only in this build; the language-pack threads
pick them up.

## Platform notes

Nothing here is platform specific: segments are fetched by TDLib, decoded
by ntgcalls, presented through the existing frame path. The ntgcalls
binary must be built with RTMP support on all three platforms; verify
with `ntg_get_connection_mode` in the first PR and record the answer per
platform in the decision doc. If a platform's vendored binary lacks it,
the Unsupported state is what ships there and the gap stays listed.

## Memory and performance

- Segments are 1 s each; Full quality ≈ 1 MB per segment. Only pending
  requests hold bytes; nothing is cached by Quill.
- The stream tile presents at the stream's frame rate through the frame
  clock, so an idle stream window costs no more than a PiP window.
- Blurred placeholders are rendered once per photo and cached in the
  existing image cache; no per-frame blur.
- Quality buckets stop over-fetching: a 240×160 window asks for
  Thumbnail.

## Tests

- `stream_feeder` unit tests with a fake TDLib sender: timestamp
  request → `getGroupCallStreams` → answer value; empty list → NoStream
  and retry scheduled; part request → segment request parameters;
  `data` → SUCCESS with bytes; errors 420 / TIME_TOO_BIG → NOT_READY;
  other → RESYNC_NEEDED; GROUPCALL_JOIN_MISSING → rejoin event; more than
  8 pending → NOT_READY; drop → all pending answered RESYNC_NEEDED.
- Quality bucket thresholds and "only report on bucket change".
- Reducer tests: `updateLiveStoryTopDonors`, message levels, deleted
  messages, viewer count from `updateGroupCall`.
- Demo fixtures (`demo-capture`): an RTMP stream window in Connecting,
  Live (still picture through the still backend) and NoStream states; a
  live story with LIVE badge, three donors, four comments and a paid
  placeholder. Screenshots in the PR.
- Manual: watch a public channel's live stream (e.g. a large news
  channel during a broadcast) at three window sizes, confirm quality
  changes in the log, and a live story from a channel that runs them.

## Build plan (Opus 5.5, medium; one PR at a time, in order)

1. **Engine stream mode**: callbacks + events in `native.rs`,
   `GroupCallMode`, `StreamFeeder` with tests, frame routing to the
   stream slot, `set_stream_quality`, `ActiveGroupCall.stream_mode`/
   `stream`/`viewer_count`, `QUILL_TRACE_VIDEO` lines for segment
   timing. Verified by logging a real stream's frames (no UI yet beyond
   the existing tiles showing the picture). Decision doc
   `docs/decisions/call-streams.md`.
2. **RTMP window**: `StreamTile`, wide layout, bottom bar, fullscreen
   keys, placeholders, strings, demo fixtures, screenshots. Closes
   `calls-rtmp-viewing`.
3. **Live story viewer**: live state + reducer, header badges, comments,
   reply row with send-as and paid comments, paid reactions with star
   burst, donors, ended/unsupported states, fixtures. Closes
   `stories-live-stream-playback`.
4. **Admin RTMP box** (small): dialog, menu entry, revoke.

Each PR: worktree on the SSD target, rules.txt, gate.sh, independent
reviewer, queue.sh, cleanup. Test in Saved Messages is not possible here;
watch public streams only, never start one on the owner's channels.

## Deviations and open points

- Scale 0 only (1 s segments), as tdesktop. ntgcalls computes
  `segment_id` and `timestamp` itself; Quill never derives them.
- tdesktop's 128 KiB chunked download is replaced by TDLib's whole-segment
  `getGroupCallStreamSegment`; the `limit` and `part_id` fields of the
  request are honoured by answering the single part ntgcalls asks for.
- Listeners are not shown for RTMP streams (as tdesktop); the "{count}
  viewers" subtitle is the only member information.
- Push-to-talk and raise hand are absent in stream mode.
- Live story recording and starting a live story from Quill stay out of
  scope (parity items of their own).
- If ntgcalls on a platform reports `NTG_ERR_RTMP_STREAMING_UNSUPPORTED`,
  the build ships the Unsupported state and the decision doc says which
  platform; re-vendoring ntgcalls with RTMP is a dependency task.

## Corrections after PR 1–2 (2026-10-15, checked against tdesktop)

- PR 1 (#814) deviations are in `docs/decisions/call-streams.md` on main: unbounded event queue, no quality override (ntgcalls 3.0.3 has no per-endpoint quality API), stream slot created by `start_stream_feeder`, Live on first segment, no mic capture in stream modes.
- StreamTile does not use the video_player `VideoFrame` element (it only takes a file-backed `VideoPlayer`); it draws like the group tiles (RenderImage cached by seq, `frame_clock::request_video_frame`).
- RTMP window follows tdesktop, not the "What the user sees" text above: controls hide after 5 s in fullscreen/maximized, hover show/hide otherwise, no click toggle; the big round button and title double-click toggle fullscreen, no F key; bar = settings/"…" menu + fullscreen + leave, no volume control; NoStream is a label-only rounded rect (admin text replaces the normal text, no Stream key button); Connecting = blurred photo, no spinner/label; panel closes when the call ends; plural viewers subtitle; RTMP pin toast texts. Unsupported is a Quill-only defensive label.
