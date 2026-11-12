//! `StreamFeeder`: answers the engine's broadcast requests for one
//! stream-mode group call with TDLib, as tdesktop's `GroupCall` does with
//! `phone.getGroupCallStreamChannels` and `upload.getFile`
//! (`calls/group/calls_group_call.cpp`: `requestCurrentTimeStart`,
//! `broadcastPartStart`, `LoadPartTask`).
//!
//! The feeder is plain logic: it owns no sender and no engine. The connect
//! side passes a [`StreamPort`] that sends the TDLib requests and the
//! engine's answers and knows the server's time, so the loop is tested
//! without a network or an engine.
//!
//! - Time request: for an RTMP stream, `getGroupCallStreams`; the first
//!   channel's `time_offset` is the answer, an empty list answers 0 and
//!   marks NoStream. Every waiting request shares one `getGroupCallStreams`.
//!   Other streams are answered at once with the server's time
//!   (tdesktop's `approximateServerTimeInMs`). tgcalls asks again on its
//!   own one second after a 0, so the feeder keeps no timer.
//! - Part request → `getGroupCallStreamSegment` at the scale for the
//!   part's duration, its channel and quality (`BroadcastChannel`); a
//!   request for "now" (timestamp 0) loads the server's current second, as
//!   `LoadPartTask` does. `data` → Success, and the first Success makes
//!   the stream Live.
//! - Errors: flood and `TIME_TOO_BIG` → NotReady; `GROUPCALL_JOIN_MISSING`
//!   / `GROUPCALL_FORBIDDEN` → NotReady plus a rejoin; `GROUPCALL_INVALID`
//!   → ResyncNeeded and the call counts as left (TDLib's
//!   `on_group_call_left`); anything else → ResyncNeeded. A missing or
//!   badly encoded `data` is ResyncNeeded as well. Every answer carries
//!   the server's time as its response time.
//! - Cancelled requests are forgotten (tdesktop's `broadcastPartCancel`
//!   and `requestCurrentTimeCancel`); TDLib has no way to cancel a request,
//!   so its late reply is dropped.
//! - At most [`MAX_PENDING_PARTS`] parts are in flight; more answer
//!   NotReady at once. [`StreamFeeder::drain`] answers every pending part
//!   ResyncNeeded so the engine never waits on a request that will not
//!   come back.
use super::*;
use crate::ids::RequestId;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use std::collections::HashMap;
use std::time::Instant;

/// Segment parts in flight at once, a guard against a runaway rather than
/// a limit a healthy stream meets: tgcalls keeps a short buffer of
/// segments pending, each with one audio or unified part plus one part
/// per video channel, and a quality update per shown video. tdesktop has
/// no cap. A part over it answers NotReady, which tgcalls asks again
/// 100 ms later. The feeder holds request metadata only: segment bytes
/// are handed to the engine as soon as TDLib returns them.
pub const MAX_PENDING_PARTS: usize = 16;

/// One channel from `groupCallStreams`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamChannel {
    pub channel_id: i32,
    pub scale: i32,
    /// When the stream currently ends; Unix milliseconds.
    pub time_offset: i64,
}

/// A `getGroupCallStreamSegment` to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentFetch {
    pub group_call_id: i32,
    pub time_offset: i64,
    pub scale: i32,
    pub channel_id: i32,
    /// `None` for audio parts: no video fields, as tdesktop sends them.
    pub quality: Option<StreamQuality>,
}

/// How a stream request failed, classified from TDLib's error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamFetchError {
    /// Code 420 / 429 or `FLOOD_WAIT_*`.
    Flood,
    /// `TIME_TOO_BIG`: the segment is not ready on the server yet.
    TimeTooBig,
    /// `GROUPCALL_JOIN_MISSING` / `GROUPCALL_FORBIDDEN`: the call must be
    /// rejoined.
    RejoinNeeded,
    /// `GROUPCALL_INVALID`: the call is gone. TDLib marks it left without
    /// a rejoin.
    CallLeft,
    Other,
}

/// A TDLib answer to one of the feeder's requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamReply<'a> {
    Streams(&'a [StreamChannel]),
    /// The `data` field of a `data` answer, still base64.
    Data(&'a str),
    Error(StreamFetchError),
}

/// What the connect side must do after a reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyOutcome {
    /// The request id is not one of this feeder's (or was cancelled).
    NotMine,
    Handled,
    /// The call must be rejoined (`join.rs`).
    Rejoin,
    /// The call is gone: stop the native call, no rejoin.
    Left,
}

/// Sends what the feeder decides. The connect side implements it over the
/// TDLib sender and the call engine.
pub trait StreamPort {
    /// Sends `getGroupCallStreams`; `None` when the send failed.
    fn request_streams(&mut self, group_call_id: i32) -> Option<RequestId>;
    /// Sends `getGroupCallStreamSegment`; `None` when the send failed.
    fn request_segment(&mut self, fetch: &SegmentFetch) -> Option<RequestId>;
    /// The server's Unix time in ms, as well as Quill knows it.
    fn server_time_ms(&self) -> i64;
    /// `CallEngine::answer_broadcast_time`.
    fn answer_time(&mut self, chat_id: i64, request: BroadcastRequestId, timestamp_ms: i64);
    /// `CallEngine::answer_broadcast_part`.
    fn answer_part(&mut self, chat_id: i64, answer: BroadcastPartAnswer);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeederState {
    /// Created, no stream seen yet. A `getGroupCallStreams` in flight is
    /// [`StreamsSync::Asking`], not a state of its own.
    Idle,
    /// The stream list names a stream and no segment has arrived yet.
    Listed,
    Streaming,
    NoStream,
    /// The server said the call must be rejoined; the rejoin replaces
    /// this feeder.
    Failed,
    /// The server said the call is gone.
    Ended,
}

/// The `getGroupCallStreams` in flight and the time requests that wait for
/// it (tdesktop answers every waiter with one request). The waiters live in
/// the request's stage, so none outlives it: the reply, a failed send and
/// `drain` all end it.
#[derive(Debug, Default)]
enum StreamsSync {
    #[default]
    Idle,
    Asking {
        request: RequestId,
        waiters: Vec<BroadcastRequestId>,
    },
}

#[derive(Debug, Clone, Copy)]
struct PendingPart {
    request: BroadcastRequestId,
    /// The part being loaded, in server ms.
    timestamp_ms: i64,
    requested_at: Instant,
}

/// The segment loop of one stream-mode call, keyed by chat id like the
/// native call.
pub struct StreamFeeder {
    group_call_id: i32,
    chat_id: i64,
    /// An RTMP stream asks TDLib for its time; others use the server's.
    rtmp: bool,
    channels: Vec<StreamChannel>,
    pending: HashMap<RequestId, PendingPart>,
    sync: StreamsSync,
    state: FeederState,
    last_timestamp_ms: i64,
    trace: bool,
}

impl StreamFeeder {
    pub fn new(group_call_id: i32, chat_id: i64, mode: GroupCallMode) -> Self {
        Self {
            group_call_id,
            chat_id,
            rtmp: mode == GroupCallMode::Rtmp,
            channels: Vec::new(),
            pending: HashMap::new(),
            sync: StreamsSync::Idle,
            state: FeederState::Idle,
            last_timestamp_ms: 0,
            trace: trace_stream(),
        }
    }

    pub fn group_call_id(&self) -> i32 {
        self.group_call_id
    }

    pub fn chat_id(&self) -> i64 {
        self.chat_id
    }

    pub fn state(&self) -> FeederState {
        self.state
    }

    pub fn channels(&self) -> &[StreamChannel] {
        &self.channels
    }

    pub fn last_timestamp_ms(&self) -> i64 {
        self.last_timestamp_ms
    }

    pub fn pending_parts(&self) -> usize {
        self.pending.len()
    }

    /// The state the call window shows.
    pub fn stream_state(&self) -> StreamState {
        match self.state {
            FeederState::Streaming => StreamState::Live,
            FeederState::NoStream => StreamState::NoStream,
            FeederState::Ended => StreamState::Ended,
            FeederState::Idle | FeederState::Listed | FeederState::Failed => {
                StreamState::Joined
            }
        }
    }

    /// Whether `id` is one of this feeder's TDLib requests.
    pub fn owns(&self, id: RequestId) -> bool {
        matches!(&self.sync, StreamsSync::Asking { request, .. } if *request == id)
            || self.pending.contains_key(&id)
    }

    /// The engine asked for the stream's current time.
    pub fn on_time_request(&mut self, request: BroadcastRequestId, port: &mut impl StreamPort) {
        if !self.rtmp {
            let now = port.server_time_ms();
            port.answer_time(self.chat_id, request, now);
            return;
        }
        if let StreamsSync::Asking { waiters, .. } = &mut self.sync {
            waiters.push(request);
            return;
        }
        if matches!(self.state, FeederState::Failed | FeederState::Ended) {
            // Terminal: no request goes out; the engine asks again in a
            // second until the rejoin replaces this feeder or the call ends.
            self.answer_time_waiters(vec![request], 0, port);
            return;
        }
        match port.request_streams(self.group_call_id) {
            Some(id) => {
                self.sync = StreamsSync::Asking {
                    request: id,
                    waiters: vec![request],
                };
            }
            // The send failed: answer 0 so the engine asks again in a second.
            None => self.answer_time_waiters(vec![request], 0, port),
        }
    }

    /// The engine asked for a stream part.
    pub fn on_part_request(
        &mut self,
        request: BroadcastRequestId,
        part: BroadcastPartRequest,
        now: Instant,
        port: &mut impl StreamPort,
    ) {
        // `LoadPartTask`: a part for "now" is the server's current second.
        let timestamp_ms = if part.timestamp_ms != 0 {
            part.timestamp_ms
        } else {
            port.server_time_ms() / 1000 * 1000
        };
        let pending = PendingPart {
            request,
            timestamp_ms,
            requested_at: now,
        };
        // Failed (the rejoin replaces this feeder) and Ended send nothing:
        // a failed segment would only fail again, every 100 ms.
        let terminal = match self.state {
            FeederState::Failed => Some(BroadcastPartStatus::NotReady),
            FeederState::Ended => Some(BroadcastPartStatus::ResyncNeeded),
            _ => None,
        };
        if let Some(status) = terminal {
            self.answer(port, &pending, status, Vec::new(), None);
            return;
        }
        if self.pending.len() >= MAX_PENDING_PARTS {
            self.answer(
                port,
                &pending,
                BroadcastPartStatus::NotReady,
                Vec::new(),
                None,
            );
            return;
        }
        // tdesktop stops on a duration it does not know; a resync asks
        // the engine to start over instead.
        let Some(scale) = part.scale() else {
            self.answer(
                port,
                &pending,
                BroadcastPartStatus::ResyncNeeded,
                Vec::new(),
                None,
            );
            return;
        };
        let (channel_id, quality) = part.channel.segment_channel();
        let fetch = SegmentFetch {
            group_call_id: self.group_call_id,
            time_offset: timestamp_ms,
            scale,
            channel_id,
            quality,
        };
        let Some(id) = port.request_segment(&fetch) else {
            self.answer(
                port,
                &pending,
                BroadcastPartStatus::NotReady,
                Vec::new(),
                None,
            );
            return;
        };
        if self.trace {
            eprintln!(
                "VS,{},req,{},{},{},{},{}",
                self.chat_id,
                request.id,
                channel_id,
                quality.map_or("none", StreamQuality::td_type),
                scale,
                timestamp_ms,
            );
        }
        self.pending.insert(id, pending);
    }

    /// The engine no longer wants `request`. Returns whether it was one
    /// of this feeder's.
    pub fn on_cancel(&mut self, request: BroadcastRequestId) -> bool {
        let mut changed = false;
        if let StreamsSync::Asking { waiters, .. } = &mut self.sync {
            let before = waiters.len();
            waiters.retain(|waiter| *waiter != request);
            changed = waiters.len() != before;
        }
        let parts = self.pending.len();
        self.pending.retain(|_, part| part.request != request);
        changed || parts != self.pending.len()
    }

    /// A TDLib answer arrived for `id`.
    pub fn on_reply(
        &mut self,
        id: RequestId,
        reply: StreamReply<'_>,
        now: Instant,
        port: &mut impl StreamPort,
    ) -> ReplyOutcome {
        if self.owns_streams_request(id)
            && let StreamsSync::Asking { waiters, .. } = std::mem::take(&mut self.sync)
        {
            return self.on_streams_reply(reply, waiters, port);
        }
        let Some(part) = self.pending.remove(&id) else {
            return ReplyOutcome::NotMine;
        };
        let elapsed = Some(now.saturating_duration_since(part.requested_at));
        let (status, data, outcome) = match reply {
            StreamReply::Data(encoded) => match STANDARD.decode(encoded) {
                Ok(bytes) => (BroadcastPartStatus::Success, bytes, ReplyOutcome::Handled),
                Err(_) => (
                    BroadcastPartStatus::ResyncNeeded,
                    Vec::new(),
                    ReplyOutcome::Handled,
                ),
            },
            StreamReply::Error(error) => {
                let (status, outcome) = part_error_status(error);
                match outcome {
                    ReplyOutcome::Rejoin => self.state = FeederState::Failed,
                    ReplyOutcome::Left => self.state = FeederState::Ended,
                    ReplyOutcome::NotMine | ReplyOutcome::Handled => {}
                }
                (status, Vec::new(), outcome)
            }
            // A stream list can't answer a segment request.
            StreamReply::Streams(_) => (
                BroadcastPartStatus::ResyncNeeded,
                Vec::new(),
                ReplyOutcome::Handled,
            ),
        };
        // Live only once a segment actually arrived, not when the stream
        // list says there is one.
        if status == BroadcastPartStatus::Success
            && !matches!(self.state, FeederState::Failed | FeederState::Ended)
        {
            self.state = FeederState::Streaming;
        }
        self.answer(port, &part, status, data, elapsed);
        outcome
    }

    /// The call is leaving or being replaced: answer every pending part
    /// ResyncNeeded before the native call stops.
    pub fn drain(&mut self, port: &mut impl StreamPort) {
        let mut pending: Vec<PendingPart> = self.pending.drain().map(|(_, part)| part).collect();
        pending.sort_by_key(|part| (part.timestamp_ms, part.request));
        for part in pending {
            self.answer(
                port,
                &part,
                BroadcastPartStatus::ResyncNeeded,
                Vec::new(),
                None,
            );
        }
        self.sync = StreamsSync::Idle;
    }

    fn owns_streams_request(&self, id: RequestId) -> bool {
        matches!(&self.sync, StreamsSync::Asking { request, .. } if *request == id)
    }

    fn on_streams_reply(
        &mut self,
        reply: StreamReply<'_>,
        waiters: Vec<BroadcastRequestId>,
        port: &mut impl StreamPort,
    ) -> ReplyOutcome {
        match reply {
            StreamReply::Streams(channels) => {
                self.channels = channels.to_vec();
                match channels.first() {
                    Some(first) => {
                        self.last_timestamp_ms = first.time_offset;
                        // A stream exists; it is Live from its first
                        // segment on.
                        if matches!(self.state, FeederState::Idle | FeederState::NoStream) {
                            self.state = FeederState::Listed;
                        }
                        self.answer_time_waiters(waiters, first.time_offset, port);
                    }
                    None => {
                        // Failed and Ended are terminal: a late empty list
                        // must not revive them.
                        if !matches!(self.state, FeederState::Failed | FeederState::Ended) {
                            self.state = FeederState::NoStream;
                        }
                        self.answer_time_waiters(waiters, 0, port);
                    }
                }
                ReplyOutcome::Handled
            }
            StreamReply::Error(error) => {
                // tdesktop answers 0 on any failure and rejoins on the two
                // membership errors; TDLib counts GROUPCALL_INVALID as left.
                self.answer_time_waiters(waiters, 0, port);
                match error {
                    StreamFetchError::RejoinNeeded => {
                        self.state = FeederState::Failed;
                        ReplyOutcome::Rejoin
                    }
                    StreamFetchError::CallLeft => {
                        self.state = FeederState::Ended;
                        ReplyOutcome::Left
                    }
                    StreamFetchError::Flood
                    | StreamFetchError::TimeTooBig
                    | StreamFetchError::Other => ReplyOutcome::Handled,
                }
            }
            StreamReply::Data(_) => {
                self.answer_time_waiters(waiters, 0, port);
                ReplyOutcome::Handled
            }
        }
    }

    fn answer_time_waiters(
        &self,
        waiters: Vec<BroadcastRequestId>,
        timestamp_ms: i64,
        port: &mut impl StreamPort,
    ) {
        for request in waiters {
            port.answer_time(self.chat_id, request, timestamp_ms);
        }
    }

    fn answer(
        &self,
        port: &mut impl StreamPort,
        part: &PendingPart,
        status: BroadcastPartStatus,
        data: Vec<u8>,
        elapsed: Option<std::time::Duration>,
    ) {
        if self.trace {
            eprintln!(
                "VS,{},ans,{},{:?},{},{}",
                self.chat_id,
                part.request.id,
                status,
                elapsed.map_or(-1, |elapsed| i64::try_from(elapsed.as_millis())
                    .unwrap_or(i64::MAX)),
                data.len(),
            );
        }
        // tdesktop answers with the reply's server time
        // (`TimestampFromMsgId`); TDLib does not pass that on, so it is
        // the server's time as Quill knows it now.
        let response_time = port.server_time_ms() as f64 / 1000.0;
        port.answer_part(
            self.chat_id,
            BroadcastPartAnswer {
                request: part.request,
                status,
                timestamp_ms: part.timestamp_ms,
                response_time,
                data,
            },
        );
    }
}

/// tdesktop's `LoadPartTask` failure mapping.
fn part_error_status(error: StreamFetchError) -> (BroadcastPartStatus, ReplyOutcome) {
    match error {
        StreamFetchError::Flood | StreamFetchError::TimeTooBig => {
            (BroadcastPartStatus::NotReady, ReplyOutcome::Handled)
        }
        StreamFetchError::RejoinNeeded => (BroadcastPartStatus::NotReady, ReplyOutcome::Rejoin),
        StreamFetchError::CallLeft => (BroadcastPartStatus::ResyncNeeded, ReplyOutcome::Left),
        StreamFetchError::Other => (BroadcastPartStatus::ResyncNeeded, ReplyOutcome::Handled),
    }
}

/// `QUILL_TRACE_VIDEO=1` also traces stream segments on stderr:
/// `VS,<chat>,req,<request>,<channel>,<quality>,<scale>,<timestamp_ms>`
/// when a segment is asked of TDLib and
/// `VS,<chat>,ans,<request>,<status>,<elapsed_ms>,<bytes>` when the
/// engine gets its answer (`elapsed_ms` is -1 for answers that never went
/// to TDLib).
fn trace_stream() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("QUILL_TRACE_VIDEO").is_some())
}

#[cfg(test)]
mod tests;
