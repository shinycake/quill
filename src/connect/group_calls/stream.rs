//! Connect driver: stream-mode group calls (channel live streams over RTMP
//! and large broadcast calls). ntgcalls asks for the stream's time and its
//! segment parts on its own threads; the requests queue here and one
//! [`StreamFeeder`] per chat answers them with TDLib on the driver thread.
use super::*;
use crate::calls::engine::{
    BroadcastPartAnswer, BroadcastPartStatus, CallEngine, EngineEvent, GroupCallMode, ReplyOutcome,
    SegmentFetch, StreamFeeder, StreamFetchError, StreamPort, StreamQualityTracker, StreamReply,
    StreamState, VideoFrame,
};
use crate::state::Session;
use crate::telegram::envelope::{CallsPayload, Envelope, EnvelopePayload, ErrorClass, TdError};
use crate::telegram::requests::{get_group_call_stream_segment, get_group_call_streams};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Events handled per poll, so a burst never stalls the UI thread.
const EVENTS_PER_TICK: usize = 64;

/// The driver's stream-mode state: the event queue from the engine, a
/// feeder per stream chat and the latest stream picture per chat.
///
/// The queue is unbounded on purpose. ntgcalls never asks again for a
/// request nobody answered: a part stays Downloading until its answer, and
/// the time request stays outstanding until `send_broadcast_timestamp`.
/// So a dropped request would stall the stream for good. ntgcalls bounds
/// the backlog itself: it keeps only a few segments pending, each with one
/// request per part, plus at most one time request.
pub(crate) struct GroupCallStreams {
    events_tx: Sender<EngineEvent>,
    events_rx: Mutex<Receiver<EngineEvent>>,
    feeders: HashMap<i64, StreamFeeder>,
    quality: HashMap<i64, StreamQualityTracker>,
    /// One slot per chat with a live feeder. The engine hook only fills
    /// an existing slot, so a picture that arrives between the drain and
    /// the native stop is dropped instead of bringing the slot back.
    frames: Arc<Mutex<HashMap<i64, Option<VideoFrame>>>>,
}

impl Default for GroupCallStreams {
    fn default() -> Self {
        let (events_tx, events_rx) = channel();
        Self {
            events_tx,
            events_rx: Mutex::new(events_rx),
            feeders: HashMap::new(),
            quality: HashMap::new(),
            frames: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl GroupCallStreams {
    fn next_event(&self) -> Option<EngineEvent> {
        self.events_rx.lock_or_recover().try_recv().ok()
    }
}

/// Sends what a feeder decides: TDLib requests through the driver's
/// sender, native answers through the engine.
struct DriverStreamPort<'a, S: JsonSender> {
    session: &'a mut Session,
    sender: &'a S,
    engine: Option<&'a mut (dyn CallEngine + 'static)>,
}

impl<S: JsonSender> DriverStreamPort<'_, S> {
    fn send(
        &mut self,
        group_call_id: i32,
        json: impl FnOnce(RequestId) -> String,
    ) -> Option<RequestId> {
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::GroupCallStream { group_call_id }),
            None,
        );
        if self.sender.send_json(&json(extra)).is_err() {
            self.session.requests.take(extra);
            return None;
        }
        Some(extra)
    }
}

impl<S: JsonSender> StreamPort for DriverStreamPort<'_, S> {
    fn request_streams(&mut self, group_call_id: i32) -> Option<RequestId> {
        self.send(group_call_id, |extra| {
            get_group_call_streams(extra, group_call_id)
        })
    }

    fn request_segment(&mut self, fetch: &SegmentFetch) -> Option<RequestId> {
        self.send(fetch.group_call_id, |extra| {
            get_group_call_stream_segment(extra, fetch)
        })
    }

    fn answer_timestamp(&mut self, chat_id: i64, timestamp_ms: i64) {
        if let Some(engine) = self.engine.as_deref_mut()
            && let Err(err) = engine.send_broadcast_timestamp(chat_id, timestamp_ms)
        {
            eprintln!("quill: stream timestamp answer for chat {chat_id} failed: {err}");
        }
    }

    fn answer_part(&mut self, chat_id: i64, answer: BroadcastPartAnswer) {
        if let Some(engine) = self.engine.as_deref_mut()
            && let Err(err) = engine.send_broadcast_part(chat_id, &answer)
        {
            eprintln!("quill: stream part answer for chat {chat_id} failed: {err}");
        }
    }
}

/// TDLib's error for a stream request, classified as tdesktop does.
pub(crate) fn stream_fetch_error(err: &TdError) -> StreamFetchError {
    match err.class {
        ErrorClass::GroupCallRejoin => StreamFetchError::RejoinNeeded,
        ErrorClass::GroupCallLeft => StreamFetchError::CallLeft,
        ErrorClass::StreamTimeTooBig => StreamFetchError::TimeTooBig,
        _ if err.is_flood() || err.code == 420 => StreamFetchError::Flood,
        _ => StreamFetchError::Other,
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Routes the engine's broadcast requests and stream pictures into the
    /// driver. The hooks run on ntgcalls threads: they only queue or store
    /// and never block. The queue never drops a request (see
    /// [`GroupCallStreams`]); a send only fails once the driver is gone.
    pub(crate) fn install_stream_hooks(&self, engine: &mut dyn CallEngine) {
        let events = self.group_call_streams.events_tx.clone();
        engine.set_engine_event_callback(Arc::new(move |event| {
            if events.send(event).is_err() {
                eprintln!("quill: stream request after the driver closed; ignored");
            }
        }));
        let frames = self.group_call_streams.frames.clone();
        engine.set_stream_frame_callback(Arc::new(move |chat_id, frame| {
            if let Some(slot) = frames.lock_or_recover().get_mut(&chat_id) {
                *slot = Some(frame);
            }
        }));
    }

    /// The newest picture of the stream-mode call in `chat_id`.
    pub fn latest_stream_frame(&self, chat_id: i64) -> Option<VideoFrame> {
        self.group_call_streams
            .frames
            .lock_or_recover()
            .get(&chat_id)
            .and_then(Clone::clone)
    }

    /// The stream tile's width in logical pixels. The quality bucket goes
    /// to the engine only when it changes. ntgcalls 3.0.3 has no API to
    /// pick a stream's quality (it asks Full for RTMP and chooses per
    /// endpoint for MTProto streams), so the engine records it for the
    /// window and for a later ntgcalls that has one; segment requests keep
    /// the quality ntgcalls asked for.
    pub fn report_stream_tile_width(&mut self, chat_id: i64, width: f32) {
        let Some(quality) = self
            .group_call_streams
            .quality
            .entry(chat_id)
            .or_default()
            .observe(width)
        else {
            return;
        };
        if let Some(engine) = self.call_engine.as_deref_mut() {
            engine.set_stream_quality(chat_id, quality);
        }
    }

    /// After the native connect: record the call's mode and, for a stream
    /// mode, start its feeder.
    pub(super) fn start_stream_feeder(&mut self, group_call_id: i32) {
        let Some(chat_id) = self.group_call_chat_id(group_call_id) else {
            return;
        };
        let mode = self
            .call_engine
            .as_deref()
            .and_then(|engine| engine.group_call_mode(chat_id))
            .unwrap_or_default();
        if let Some(call) = self
            .session
            .calls
            .active_group_call
            .as_mut()
            .filter(|call| call.id == group_call_id)
        {
            call.stream_mode = mode;
            call.stream = StreamState::Connecting;
        }
        if !mode.is_stream() {
            return;
        }
        self.group_call_streams
            .feeders
            .insert(chat_id, StreamFeeder::new(group_call_id, chat_id));
        self.group_call_streams
            .frames
            .lock_or_recover()
            .insert(chat_id, None);
    }

    /// `ntg_connect` refused the stream: a fatal join error shown as the
    /// Unsupported state. The mode comes from the join answer (the native
    /// call never got one), and the native call is stopped so no context
    /// is left connected without a feeder.
    pub(super) fn mark_stream_unsupported(&mut self, group_call_id: i32, answer: &str) {
        if let Some(engine) = self.call_engine.as_deref_mut() {
            let _ = engine.leave_group_call(group_call_id);
        }
        if let Some(call) = self
            .session
            .calls
            .active_group_call
            .as_mut()
            .filter(|call| call.id == group_call_id)
        {
            call.stream_mode = GroupCallMode::from_join_answer(answer);
            call.stream = StreamState::Unsupported;
            // Not retried on every pump: the Join button can try again.
            call.join_payload.clear();
        }
    }

    /// The call is leaving, ending or rejoining: answer the feeder's
    /// pending parts RESYNC_NEEDED before the native call stops, and drop
    /// its picture.
    pub(super) fn drain_stream_feeder(&mut self, group_call_id: i32) {
        let chats: Vec<i64> = self
            .group_call_streams
            .feeders
            .iter()
            .filter(|(_, feeder)| feeder.group_call_id() == group_call_id)
            .map(|(chat_id, _)| *chat_id)
            .collect();
        for chat_id in chats {
            let Some(mut feeder) = self.group_call_streams.feeders.remove(&chat_id) else {
                continue;
            };
            let mut port = DriverStreamPort {
                session: &mut self.session,
                sender: &self.sender,
                engine: self.call_engine.as_deref_mut(),
            };
            feeder.drain(&mut port);
            self.group_call_streams.quality.remove(&chat_id);
            self.group_call_streams
                .frames
                .lock_or_recover()
                .remove(&chat_id);
        }
    }

    /// Per poll: answer the broadcast requests ntgcalls queued. Returns
    /// whether anything was handled.
    pub(crate) fn pump_group_call_streams(&mut self, now: Instant) -> bool {
        let mut handled = false;
        for _ in 0..EVENTS_PER_TICK {
            let Some(event) = self.group_call_streams.next_event() else {
                break;
            };
            handled = true;
            self.handle_engine_event(event, now);
        }
        if handled {
            self.sync_stream_state();
        }
        handled
    }

    fn handle_engine_event(&mut self, event: EngineEvent, now: Instant) {
        let chat_id = match &event {
            EngineEvent::BroadcastTimestampRequested { chat_id }
            | EngineEvent::BroadcastPartRequested { chat_id, .. } => *chat_id,
        };
        let Self {
            session,
            sender,
            call_engine,
            group_call_streams,
            ..
        } = self;
        let mut port = DriverStreamPort {
            session,
            sender: &*sender,
            engine: call_engine.as_deref_mut(),
        };
        let Some(feeder) = group_call_streams.feeders.get_mut(&chat_id) else {
            // No feeder (the call left, or is not in stream mode): answer at
            // once so ntgcalls never waits on a request nobody will serve.
            match event {
                EngineEvent::BroadcastTimestampRequested { chat_id } => {
                    port.answer_timestamp(chat_id, 0);
                }
                EngineEvent::BroadcastPartRequested { chat_id, request } => port.answer_part(
                    chat_id,
                    BroadcastPartAnswer {
                        segment_id: request.segment_id,
                        part_id: request.part_id,
                        status: BroadcastPartStatus::ResyncNeeded,
                        quality_update: request.quality_update,
                        data: Vec::new(),
                    },
                ),
            }
            return;
        };
        match event {
            EngineEvent::BroadcastTimestampRequested { .. } => {
                feeder.on_timestamp_request(&mut port)
            }
            EngineEvent::BroadcastPartRequested { request, .. } => {
                feeder.on_part_request(request, now, &mut port);
            }
        }
    }

    /// Takes TDLib's answer to a stream request before the reducer sees
    /// it, so segment bytes never pass through the session. Returns
    /// whether the envelope was one.
    pub(crate) fn absorb_stream_answer(&mut self, envelope: &Envelope, now: Instant) -> bool {
        let Some(extra) = envelope.extra else {
            return false;
        };
        if !matches!(
            self.session.requests.purpose(extra),
            Some(RequestPurpose::Calls(CallsPurpose::GroupCallStream { .. }))
        ) {
            return false;
        }
        self.session.requests.take(extra);
        self.sender.forget_retained(extra);
        let reply = match &envelope.payload {
            EnvelopePayload::Calls(CallsPayload::GroupCallStreams { streams }) => {
                StreamReply::Streams(streams)
            }
            EnvelopePayload::Calls(CallsPayload::Data {
                base64: Some(base64),
            }) => StreamReply::Data(base64),
            // A `data` answer without bytes is not a segment.
            EnvelopePayload::Calls(CallsPayload::Data { base64: None }) => {
                StreamReply::Error(StreamFetchError::Other)
            }
            EnvelopePayload::Error(err) => StreamReply::Error(stream_fetch_error(err)),
            _ => StreamReply::Error(StreamFetchError::Other),
        };
        let Self {
            session,
            sender,
            call_engine,
            group_call_streams,
            ..
        } = self;
        let mut port = DriverStreamPort {
            session,
            sender: &*sender,
            engine: call_engine.as_deref_mut(),
        };
        let mut outcome = ReplyOutcome::NotMine;
        let mut feeder_call = None;
        for feeder in group_call_streams.feeders.values_mut() {
            outcome = feeder.on_reply(extra, reply, now, &mut port);
            if outcome != ReplyOutcome::NotMine {
                feeder_call = Some(feeder.group_call_id());
                break;
            }
        }
        self.sync_stream_state();
        if outcome == ReplyOutcome::Left
            && let Some(group_call_id) = feeder_call
        {
            // TDLib's `on_group_call_left` without a rejoin: answer what is
            // pending and stop the native call. The tracked call stays
            // until TDLib's `updateGroupCall` says otherwise.
            self.teardown_group_call_transport(group_call_id);
            if let Some(call) = self
                .session
                .calls
                .active_group_call
                .as_mut()
                .filter(|call| call.id == group_call_id)
            {
                call.transport_ready = false;
                call.join_payload.clear();
            }
        }
        if outcome == ReplyOutcome::Rejoin {
            // tdesktop's `startRejoin`, through the existing rejoin path
            // (`join.rs`, at most 3 attempts).
            if let Some(call) = self.session.calls.active_group_call.as_mut() {
                call.reconnecting = true;
            }
            let _ = self.maybe_auto_rejoin_group_call();
        }
        true
    }

    /// Mirrors the feeder's state onto the tracked call.
    fn sync_stream_state(&mut self) {
        let Some(call) = self.session.calls.active_group_call.as_mut() else {
            return;
        };
        if !call.stream_mode.is_stream() || call.stream == StreamState::Unsupported {
            return;
        }
        if let Some(feeder) = self
            .group_call_streams
            .feeders
            .values()
            .find(|feeder| feeder.group_call_id() == call.id)
        {
            call.stream = feeder.stream_state();
        }
    }
}
