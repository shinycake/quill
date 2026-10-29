//! Connect driver: stream-mode group calls (channel live streams over RTMP,
//! live stories and large broadcast calls). The engine asks for the
//! stream's time and its parts on its own threads, each request with an
//! id; the requests (and their cancellations) queue here and one
//! [`StreamFeeder`] per chat answers them with TDLib on the driver thread.
use super::*;
use crate::calls::engine::{
    BroadcastPartAnswer, BroadcastPartStatus, BroadcastRequestId, CallEngine, EngineEvent,
    GroupCallMode, ReplyOutcome, SegmentFetch, StreamFeeder, StreamFetchError, StreamPort,
    StreamQualityTracker, StreamReply, StreamState,
};
use crate::calls::stream_audio::gain::clamp_volume;
use crate::calls::stream_audio::watch::PlaybackWatch;
use crate::state::Session;
use crate::telegram::envelope::{
    CallsPayload, CommonPayload, Envelope, EnvelopePayload, ErrorClass, OptionValue, TdError,
};
use crate::telegram::requests::{get_group_call_stream_segment, get_group_call_streams};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Events handled per poll, so a burst never stalls the UI thread.
const EVENTS_PER_TICK: usize = 64;

/// The driver's stream-mode state: the event queue from the engine and a
/// feeder per stream chat (the pictures go to live feeds,
/// `live_feeds.rs`).
///
/// The queue is unbounded on purpose. The engine never asks again for a
/// request nobody answered: a part or time request stays open until its
/// answer or its cancellation, so a dropped request would stall the
/// stream for good. The engine bounds the backlog itself: it keeps only a
/// few segments pending, each with one request per part, plus at most one
/// time request.
pub(crate) struct GroupCallStreams {
    events_tx: Sender<EngineEvent>,
    events_rx: Mutex<Receiver<EngineEvent>>,
    feeders: HashMap<i64, StreamFeeder>,
    quality: HashMap<i64, StreamQualityTracker>,
    sound_watch: HashMap<i32, PlaybackWatch>,
    volume: f32,
    /// The server's clock minus this machine's, in ms, from TDLib's
    /// `unix_time` option (0 until it comes).
    server_offset_ms: i64,
}

impl Default for GroupCallStreams {
    fn default() -> Self {
        let (events_tx, events_rx) = channel();
        Self {
            events_tx,
            events_rx: Mutex::new(events_rx),
            feeders: HashMap::new(),
            quality: HashMap::new(),
            sound_watch: HashMap::new(),
            volume: 1.0,
            server_offset_ms: 0,
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
    server_offset_ms: i64,
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

    fn server_time_ms(&self) -> i64 {
        local_time_ms().saturating_add(self.server_offset_ms)
    }

    fn answer_time(&mut self, chat_id: i64, request: BroadcastRequestId, timestamp_ms: i64) {
        if let Some(engine) = self.engine.as_deref_mut()
            && let Err(err) = engine.answer_broadcast_time(chat_id, request, timestamp_ms)
        {
            eprintln!("quill: stream time answer for chat {chat_id} failed: {err}");
        }
    }

    fn answer_part(&mut self, chat_id: i64, answer: BroadcastPartAnswer) {
        if let Some(engine) = self.engine.as_deref_mut()
            && let Err(err) = engine.answer_broadcast_part(chat_id, &answer)
        {
            eprintln!("quill: stream part answer for chat {chat_id} failed: {err}");
        }
    }
}

/// This machine's Unix time in ms.
fn local_time_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

/// The server's clock minus this machine's, from TDLib's `unix_time`
/// option (whole seconds of the server's time when TDLib sent it), taken
/// at the middle of that second.
pub(crate) fn server_offset_ms(unix_time: i64, local_ms: i64) -> i64 {
    unix_time
        .saturating_mul(1000)
        .saturating_add(500)
        .saturating_sub(local_ms)
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
    /// driver. The hooks run on engine threads: they only queue or store
    /// and never block. The queue never drops a request (see
    /// [`GroupCallStreams`]); a send only fails once the driver is gone.
    pub(crate) fn install_stream_hooks(&self, engine: &mut dyn CallEngine) {
        let events = self.group_call_streams.events_tx.clone();
        engine.set_engine_event_callback(Arc::new(move |event| {
            // Everyone's speaking levels are a parity item still to come
            // (docs/decisions/tgcalls-engine.md); until then they are not
            // queued, so a level batch never wakes the driver.
            if matches!(event, EngineEvent::GroupAudioLevels { .. }) {
                return;
            }
            if events.send(event).is_err() {
                eprintln!("quill: stream request after the driver closed; ignored");
            }
            crate::wake::UPDATES.poke();
        }));
        engine.set_stream_volume(self.group_call_streams.volume);
    }

    /// The volume of every stream Quill plays (the media viewer's volume,
    /// as tdesktop's stream starts at `videoVolume`), now and for later
    /// joins.
    pub fn set_stream_volume(&mut self, volume: f32) {
        let volume = clamp_volume(volume);
        if volume == self.group_call_streams.volume {
            return;
        }
        self.group_call_streams.volume = volume;
        if let Some(engine) = self.call_engine.as_deref_mut() {
            engine.set_stream_volume(volume);
        }
    }

    pub fn stream_volume(&self) -> f32 {
        self.group_call_streams.volume
    }

    /// Whether Quill plays group call `group_call_id`'s stream itself, so
    /// its volume can change (device playback has mute only).
    pub fn stream_has_volume(&self, group_call_id: i32) -> bool {
        self.call_engine
            .as_deref()
            .is_some_and(|engine| engine.stream_sound_health(group_call_id).is_some())
    }

    fn watch_stream_sound(&mut self, now: Instant) {
        let Some(engine) = self.call_engine.as_deref_mut() else {
            return;
        };
        let calls: Vec<i32> = self
            .group_call_streams
            .feeders
            .values()
            .map(StreamFeeder::group_call_id)
            .collect();
        for group_call_id in calls {
            let Some(health) = engine.stream_sound_health(group_call_id) else {
                continue;
            };
            let state = self
                .session
                .calls
                .group_call(group_call_id)
                .map(|call| call.stream)
                .unwrap_or_default();
            let watch = self
                .group_call_streams
                .sound_watch
                .entry(group_call_id)
                .or_default();
            let Some(reason) = watch.observe(now, state, health) else {
                continue;
            };
            let line = format!(
                "stream sound: falling back to device playback for group call {group_call_id} ({reason:?})"
            );
            eprintln!("quill: warning: {line}");
            crate::advanced::debug_mode::debug_log(&line);
            if let Err(err) = engine.reopen_stream_sound(group_call_id) {
                eprintln!(
                    "quill: stream sound fallback for group call {group_call_id} failed: {err}"
                );
            }
        }
    }

    /// An RTMP join answer carries the stream's server URL and key when you
    /// can manage it (`rtmp_stream_url`, `rtmp_stream_key`). tdesktop keeps
    /// them as the call's RTMP info (`GroupCall` join handling), which
    /// picks the admin text when nobody is broadcasting. Other answers
    /// leave the call alone.
    pub(super) fn note_stream_rtmp_info(&mut self, group_call_id: i32, answer: &str) {
        let Some((url, key)) = rtmp_info_from_join_answer(answer) else {
            return;
        };
        if let Some(call) = self.session.calls.group_call_mut(group_call_id) {
            call.rtmp_url = Some(url).filter(|url| !url.is_empty());
            call.rtmp_stream_key = Some(key).filter(|key| !key.is_empty());
        }
    }

    /// The stream tile's width in logical pixels. The quality bucket goes
    /// to the engine only when it changes; tgcalls uses it for the RTMP
    /// stream's video, as tdesktop does, and segment requests keep the
    /// quality the engine asked for.
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
        if let Some(call) = self.session.calls.group_call_mut(group_call_id) {
            call.stream_mode = mode;
            call.stream = StreamState::Connecting;
        }
        if !mode.is_stream() {
            return;
        }
        self.group_call_streams
            .feeders
            .insert(chat_id, StreamFeeder::new(group_call_id, chat_id, mode));
        self.open_stream_feed(chat_id);
    }

    /// The call is leaving, ending or rejoining: answer the feeder's
    /// pending parts ResyncNeeded before the native call stops, and drop
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
                server_offset_ms: self.group_call_streams.server_offset_ms,
            };
            feeder.drain(&mut port);
            self.group_call_streams.quality.remove(&chat_id);
            self.group_call_streams.sound_watch.remove(&group_call_id);
            self.close_stream_feed(chat_id);
        }
    }

    /// Per poll: answer the broadcast requests the engine queued. Returns
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
        self.watch_stream_sound(now);
        handled
    }

    fn handle_engine_event(&mut self, event: EngineEvent, now: Instant) {
        let chat_id = match &event {
            EngineEvent::BroadcastTimeRequested { chat_id, .. }
            | EngineEvent::BroadcastPartRequested { chat_id, .. }
            | EngineEvent::BroadcastRequestCancelled { chat_id, .. } => *chat_id,
            EngineEvent::GroupAudioLevels { .. } => return,
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
            server_offset_ms: group_call_streams.server_offset_ms,
        };
        let Some(feeder) = group_call_streams.feeders.get_mut(&chat_id) else {
            // No feeder (the call left, or is not in stream mode): answer at
            // once so the engine never waits on a request nobody will serve.
            match event {
                EngineEvent::BroadcastTimeRequested { chat_id, request } => {
                    port.answer_time(chat_id, request, 0);
                }
                EngineEvent::BroadcastPartRequested {
                    chat_id,
                    request,
                    part,
                } => {
                    let response_time = port.server_time_ms() as f64 / 1000.0;
                    port.answer_part(
                        chat_id,
                        BroadcastPartAnswer {
                            request,
                            status: BroadcastPartStatus::ResyncNeeded,
                            timestamp_ms: part.timestamp_ms,
                            response_time,
                            data: Vec::new(),
                        },
                    );
                }
                EngineEvent::BroadcastRequestCancelled { .. }
                | EngineEvent::GroupAudioLevels { .. } => {}
            }
            return;
        };
        match event {
            EngineEvent::BroadcastTimeRequested { request, .. } => {
                feeder.on_time_request(request, &mut port);
            }
            EngineEvent::BroadcastPartRequested { request, part, .. } => {
                feeder.on_part_request(request, part, now, &mut port);
            }
            EngineEvent::BroadcastRequestCancelled { request, .. } => {
                feeder.on_cancel(request);
            }
            EngineEvent::GroupAudioLevels { .. } => {}
        }
    }

    /// Keeps the server's clock from TDLib's `unix_time` option: tdesktop
    /// answers a stream's time and its parts with the server's time.
    pub(crate) fn note_server_time(&mut self, envelope: &Envelope) {
        if let EnvelopePayload::Common(CommonPayload::UpdateOption {
            name,
            value: OptionValue::Integer(unix_time),
        }) = &envelope.payload
            && name == "unix_time"
            && *unix_time > 0
        {
            self.group_call_streams.server_offset_ms =
                server_offset_ms(*unix_time, local_time_ms());
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
            server_offset_ms: group_call_streams.server_offset_ms,
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
            if let Some(call) = self.session.calls.group_call_mut(group_call_id) {
                call.transport_ready = false;
                call.join_payload.clear();
            }
        }
        if outcome == ReplyOutcome::Rejoin
            && let Some(group_call_id) = feeder_call
        {
            // tdesktop's `startRejoin`, through the existing rejoin path
            // (`join.rs`, at most 3 attempts).
            if let Some(call) = self.session.calls.group_call_mut(group_call_id) {
                call.reconnecting = true;
            }
            let _ = self.maybe_auto_rejoin_group_call();
        }
        true
    }

    fn sync_stream_state(&mut self) {
        for group_call_id in self.session.calls.group_call_ids().into_iter().flatten() {
            let Some(call) = self.session.calls.group_call_mut(group_call_id) else {
                continue;
            };
            if !call.stream_mode.is_stream() {
                continue;
            }
            let Some(feeder) = self
                .group_call_streams
                .feeders
                .values()
                .find(|feeder| feeder.group_call_id() == group_call_id)
            else {
                continue;
            };
            call.stream = feeder.stream_state();
            let (chat_id, state) = (feeder.chat_id(), call.stream);
            self.sync_stream_feed(chat_id, state);
        }
    }
}

/// `(url, key)` of an RTMP join answer (tdesktop `ParseJoinResponse`:
/// `rtmp_stream_url` and `rtmp_stream_key`, empty when absent); `None`
/// for an answer that is not an RTMP stream.
pub(crate) fn rtmp_info_from_join_answer(answer: &str) -> Option<(String, String)> {
    let serde_json::Value::Object(fields) = serde_json::from_str(answer).ok()? else {
        return None;
    };
    if GroupCallMode::from_join_answer(answer) != GroupCallMode::Rtmp {
        return None;
    }
    let text = |name: &str| {
        fields
            .get(name)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Some((text("rtmp_stream_url"), text("rtmp_stream_key")))
}
