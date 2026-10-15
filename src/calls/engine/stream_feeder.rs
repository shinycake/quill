//! `StreamFeeder`: answers ntgcalls' broadcast requests for one
//! stream-mode group call with TDLib, as tdesktop's `GroupCall` does with
//! `phone.getGroupCallStreamChannels` and `upload.getFile`
//! (`calls/group/calls_group_call.cpp`, `requestCurrentTimeStart` and
//! `LoadPartTask`).
//!
//! The feeder is plain logic: it owns no sender and no engine. The connect
//! side passes a [`StreamPort`] that sends the TDLib requests and the
//! native answers, so the loop is tested without a network or ntgcalls.
//!
//! - Timestamp request → `getGroupCallStreams`; the first channel's
//!   `time_offset` is the answer, an empty list answers 0 and marks
//!   NoStream. ntgcalls asks again on its own one second after a 0
//!   (`mtproto_stream.cpp`, `send_broadcast_timestamp`), which is the
//!   polling tdesktop gets from tgcalls too, so the feeder keeps no timer.
//! - Part request → `getGroupCallStreamSegment` (scale 0, the request's
//!   channel, the tile's quality for video parts); `data` → SUCCESS.
//! - Errors: flood and `TIME_TOO_BIG` → NOT_READY; `GROUPCALL_JOIN_MISSING`
//!   / `GROUPCALL_FORBIDDEN` → NOT_READY plus a rejoin; anything else →
//!   RESYNC_NEEDED. A base64 decode failure is RESYNC_NEEDED as well.
//! - At most [`MAX_PENDING_PARTS`] parts are in flight; more answer
//!   NOT_READY at once. Dropping the feeder ([`StreamFeeder::drain`])
//!   answers every pending part RESYNC_NEEDED so ntgcalls never waits on
//!   a request that will not come back.
use super::*;
use crate::ids::RequestId;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use std::collections::HashMap;
use std::time::Instant;

/// Segment parts in flight at once. A Full segment is about 1 MB, so this
/// bounds the bytes held for one stream.
pub const MAX_PENDING_PARTS: usize = 8;

/// tdesktop only ever asks for scale 0: one-second segments.
pub const SEGMENT_SCALE: i32 = 0;

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
    /// `None` for audio parts: TDLib then picks the worst quality, which
    /// is what an audio channel ignores anyway.
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
    /// The request id is not one of this feeder's.
    NotMine,
    Handled,
    /// The call must be rejoined (`join.rs`).
    Rejoin,
}

/// Sends what the feeder decides. The connect side implements it over the
/// TDLib sender and the call engine.
pub trait StreamPort {
    /// Sends `getGroupCallStreams`; `None` when the send failed.
    fn request_streams(&mut self, group_call_id: i32) -> Option<RequestId>;
    /// Sends `getGroupCallStreamSegment`; `None` when the send failed.
    fn request_segment(&mut self, fetch: &SegmentFetch) -> Option<RequestId>;
    /// `ntg_send_broadcast_timestamp`.
    fn answer_timestamp(&mut self, chat_id: i64, timestamp_ms: i64);
    /// `ntg_send_broadcast_part`.
    fn answer_part(&mut self, chat_id: i64, answer: BroadcastPartAnswer);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeederState {
    /// Created, nothing asked yet.
    Idle,
    /// Waiting for `getGroupCallStreams`.
    Syncing,
    Streaming,
    NoStream,
    /// The server said the call must be rejoined; the rejoin replaces
    /// this feeder.
    Failed,
}

#[derive(Debug, Clone, Copy)]
struct PendingPart {
    request: BroadcastPartRequest,
    requested_at: Instant,
}

/// The segment loop of one stream-mode call, keyed by chat id like the
/// native call.
pub struct StreamFeeder {
    group_call_id: i32,
    chat_id: i64,
    channels: Vec<StreamChannel>,
    pending: HashMap<RequestId, PendingPart>,
    /// The `getGroupCallStreams` in flight and how many timestamp requests
    /// wait for it (tdesktop answers every waiter with one request).
    streams_request: Option<RequestId>,
    timestamp_waiters: u32,
    /// Last quality the stream tile asked for; `None` until it reports.
    quality: Option<StreamQuality>,
    state: FeederState,
    last_timestamp_ms: i64,
    trace: bool,
}

impl StreamFeeder {
    pub fn new(group_call_id: i32, chat_id: i64) -> Self {
        Self {
            group_call_id,
            chat_id,
            channels: Vec::new(),
            pending: HashMap::new(),
            streams_request: None,
            timestamp_waiters: 0,
            quality: None,
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
            FeederState::Idle | FeederState::Syncing | FeederState::Failed => {
                StreamState::Connecting
            }
        }
    }

    /// The quality the tile asked for; used from the next part request on.
    /// ntgcalls asks for `quality_update` parts itself when it wants to
    /// switch, so nothing is re-requested here.
    pub fn set_quality(&mut self, quality: StreamQuality) {
        self.quality = Some(quality);
    }

    pub fn quality(&self) -> Option<StreamQuality> {
        self.quality
    }

    /// Whether `id` is one of this feeder's requests.
    pub fn owns(&self, id: RequestId) -> bool {
        self.streams_request == Some(id) || self.pending.contains_key(&id)
    }

    /// ntgcalls asked for the stream's current time.
    pub fn on_timestamp_request(&mut self, port: &mut impl StreamPort) {
        self.timestamp_waiters = self.timestamp_waiters.saturating_add(1);
        if self.streams_request.is_some() {
            return;
        }
        if self.state == FeederState::Idle {
            self.state = FeederState::Syncing;
        }
        match port.request_streams(self.group_call_id) {
            Some(id) => self.streams_request = Some(id),
            // The send failed: answer 0 so ntgcalls asks again in a second.
            None => self.answer_timestamp_waiters(0, port),
        }
    }

    /// ntgcalls asked for a segment part.
    pub fn on_part_request(
        &mut self,
        request: BroadcastPartRequest,
        now: Instant,
        port: &mut impl StreamPort,
    ) {
        if self.pending.len() >= MAX_PENDING_PARTS {
            self.answer(
                port,
                &request,
                BroadcastPartStatus::NotReady,
                Vec::new(),
                None,
            );
            return;
        }
        let quality = match request.quality {
            None => None,
            // A quality update asks for one quality on purpose.
            Some(asked) if request.quality_update => Some(asked),
            Some(asked) => Some(self.quality.unwrap_or(asked)),
        };
        let fetch = SegmentFetch {
            group_call_id: self.group_call_id,
            time_offset: request.timestamp,
            scale: SEGMENT_SCALE,
            channel_id: request.channel_id,
            quality,
        };
        let Some(id) = port.request_segment(&fetch) else {
            self.answer(
                port,
                &request,
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
                request.segment_id,
                request.part_id,
                request.channel_id,
                quality.map_or("none", StreamQuality::td_type),
                request.timestamp,
            );
        }
        self.pending.insert(
            id,
            PendingPart {
                request,
                requested_at: now,
            },
        );
    }

    /// A TDLib answer arrived for `id`.
    pub fn on_reply(
        &mut self,
        id: RequestId,
        reply: StreamReply<'_>,
        now: Instant,
        port: &mut impl StreamPort,
    ) -> ReplyOutcome {
        if self.streams_request == Some(id) {
            self.streams_request = None;
            return self.on_streams_reply(reply, port);
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
                if outcome == ReplyOutcome::Rejoin {
                    self.state = FeederState::Failed;
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
        if status == BroadcastPartStatus::Success && self.state != FeederState::Failed {
            self.state = FeederState::Streaming;
        }
        self.answer(port, &part.request, status, data, elapsed);
        outcome
    }

    /// The call is leaving or being replaced: answer every pending part
    /// RESYNC_NEEDED before the native call stops.
    pub fn drain(&mut self, port: &mut impl StreamPort) {
        let mut pending: Vec<PendingPart> = self.pending.drain().map(|(_, part)| part).collect();
        pending.sort_by_key(|part| (part.request.segment_id, part.request.part_id));
        for part in pending {
            self.answer(
                port,
                &part.request,
                BroadcastPartStatus::ResyncNeeded,
                Vec::new(),
                None,
            );
        }
        self.streams_request = None;
        self.timestamp_waiters = 0;
    }

    fn on_streams_reply(
        &mut self,
        reply: StreamReply<'_>,
        port: &mut impl StreamPort,
    ) -> ReplyOutcome {
        match reply {
            StreamReply::Streams(channels) => {
                self.channels = channels.to_vec();
                match channels.first() {
                    Some(first) => {
                        self.last_timestamp_ms = first.time_offset;
                        if self.state != FeederState::Streaming {
                            self.state = FeederState::Streaming;
                        }
                        self.answer_timestamp_waiters(first.time_offset, port);
                    }
                    None => {
                        self.state = FeederState::NoStream;
                        self.answer_timestamp_waiters(0, port);
                    }
                }
                ReplyOutcome::Handled
            }
            StreamReply::Error(error) => {
                // tdesktop answers 0 on any failure and rejoins on the two
                // membership errors.
                self.answer_timestamp_waiters(0, port);
                if error == StreamFetchError::RejoinNeeded {
                    self.state = FeederState::Failed;
                    ReplyOutcome::Rejoin
                } else {
                    ReplyOutcome::Handled
                }
            }
            StreamReply::Data(_) => {
                self.answer_timestamp_waiters(0, port);
                ReplyOutcome::Handled
            }
        }
    }

    fn answer_timestamp_waiters(&mut self, timestamp_ms: i64, port: &mut impl StreamPort) {
        let waiters = std::mem::take(&mut self.timestamp_waiters).max(1);
        for _ in 0..waiters {
            port.answer_timestamp(self.chat_id, timestamp_ms);
        }
    }

    fn answer(
        &self,
        port: &mut impl StreamPort,
        request: &BroadcastPartRequest,
        status: BroadcastPartStatus,
        data: Vec<u8>,
        elapsed: Option<std::time::Duration>,
    ) {
        if self.trace {
            eprintln!(
                "VS,{},ans,{},{},{:?},{},{}",
                self.chat_id,
                request.segment_id,
                request.part_id,
                status,
                elapsed.map_or(-1, |elapsed| i64::try_from(elapsed.as_millis())
                    .unwrap_or(i64::MAX)),
                data.len(),
            );
        }
        port.answer_part(
            self.chat_id,
            BroadcastPartAnswer {
                segment_id: request.segment_id,
                part_id: request.part_id,
                status,
                quality_update: request.quality_update,
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
        StreamFetchError::Other => (BroadcastPartStatus::ResyncNeeded, ReplyOutcome::Handled),
    }
}

/// Reports a stream tile's size as a quality bucket, once per change.
#[derive(Debug, Clone, Copy, Default)]
pub struct StreamQualityTracker {
    last: Option<StreamQuality>,
}

impl StreamQualityTracker {
    /// The bucket for `width`, or `None` when it is the one already
    /// reported.
    pub fn observe(&mut self, width: f32) -> Option<StreamQuality> {
        let quality = StreamQuality::for_tile_width(width);
        if self.last == Some(quality) {
            return None;
        }
        self.last = Some(quality);
        Some(quality)
    }
}

/// `QUILL_TRACE_VIDEO=1` also traces stream segments on stderr:
/// `VS,<chat>,req,<segment>,<part>,<channel>,<quality>,<timestamp_ms>` when
/// a segment is asked of TDLib and
/// `VS,<chat>,ans,<segment>,<part>,<status>,<elapsed_ms>,<bytes>` when
/// ntgcalls gets its answer (`elapsed_ms` is -1 for answers that never
/// went to TDLib).
fn trace_stream() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("QUILL_TRACE_VIDEO").is_some())
}

#[cfg(test)]
mod tests;
