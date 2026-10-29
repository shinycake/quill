use crate::calls::engine::{
    BroadcastChannel, BroadcastPartAnswer, BroadcastPartRequest, BroadcastPartStatus,
    BroadcastRequestId, FeederState, GroupCallMode, MAX_PENDING_PARTS, ReplyOutcome, SegmentFetch,
    StreamChannel, StreamFeeder, StreamFetchError, StreamPort, StreamQuality, StreamQualityTracker,
    StreamReply, StreamState,
};
use crate::ids::RequestId;
use std::time::{Duration, Instant};

/// The server's clock in these tests.
const SERVER_NOW_MS: i64 = 1_700_000_042_750;

#[derive(Default)]
struct FakePort {
    next_id: u64,
    fail_sends: bool,
    streams_requests: Vec<i32>,
    segment_requests: Vec<(RequestId, SegmentFetch)>,
    times: Vec<(i64, BroadcastRequestId, i64)>,
    parts: Vec<(i64, BroadcastPartAnswer)>,
}

impl FakePort {
    fn id(&mut self) -> Option<RequestId> {
        if self.fail_sends {
            return None;
        }
        self.next_id += 1;
        Some(RequestId(self.next_id))
    }

    /// The answered time per request, in order.
    fn time_answers(&self) -> Vec<(u64, i64)> {
        self.times
            .iter()
            .map(|(chat, request, time)| {
                assert_eq!(*chat, 51);
                (request.id, *time)
            })
            .collect()
    }
}

impl StreamPort for FakePort {
    fn request_streams(&mut self, group_call_id: i32) -> Option<RequestId> {
        let id = self.id()?;
        self.streams_requests.push(group_call_id);
        Some(id)
    }

    fn request_segment(&mut self, fetch: &SegmentFetch) -> Option<RequestId> {
        let id = self.id()?;
        self.segment_requests.push((id, *fetch));
        Some(id)
    }

    fn server_time_ms(&self) -> i64 {
        SERVER_NOW_MS
    }

    fn answer_time(&mut self, chat_id: i64, request: BroadcastRequestId, timestamp_ms: i64) {
        self.times.push((chat_id, request, timestamp_ms));
    }

    fn answer_part(&mut self, chat_id: i64, answer: BroadcastPartAnswer) {
        self.parts.push((chat_id, answer));
    }
}

fn engine_id(id: u64) -> BroadcastRequestId {
    BroadcastRequestId { instance: 7, id }
}

fn video(segment: i64, quality: StreamQuality) -> BroadcastPartRequest {
    BroadcastPartRequest {
        timestamp_ms: 1_700_000_000_000 + segment * 1000,
        duration_ms: 1000,
        channel: BroadcastChannel::Video {
            channel_id: 1,
            quality,
        },
    }
}

fn audio(segment: i64) -> BroadcastPartRequest {
    BroadcastPartRequest {
        timestamp_ms: 1_700_000_000_000 + segment * 1000,
        duration_ms: 1000,
        channel: BroadcastChannel::Audio,
    }
}

fn feeder() -> StreamFeeder {
    StreamFeeder::new(555, 51, GroupCallMode::Rtmp)
}

fn ask(feeder: &mut StreamFeeder, port: &mut FakePort, id: u64, part: BroadcastPartRequest) {
    feeder.on_part_request(engine_id(id), part, Instant::now(), port);
}

fn last_part(port: &FakePort) -> &BroadcastPartAnswer {
    &port.parts.last().expect("a part answer").1
}

#[test]
fn an_rtmp_time_request_answers_the_first_channels_time_offset() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    feeder.on_time_request(engine_id(1), &mut port);
    assert_eq!(port.streams_requests, vec![555]);
    assert_eq!(feeder.state(), FeederState::Syncing);
    assert_eq!(feeder.stream_state(), StreamState::Connecting);
    let channels = [
        StreamChannel {
            channel_id: 1,
            scale: 0,
            time_offset: 1_700_000_123_456,
        },
        StreamChannel {
            channel_id: 2,
            scale: 0,
            time_offset: 99,
        },
    ];
    let outcome = feeder.on_reply(
        RequestId(1),
        StreamReply::Streams(&channels),
        Instant::now(),
        &mut port,
    );
    assert_eq!(outcome, ReplyOutcome::Handled);
    assert_eq!(port.time_answers(), vec![(1, 1_700_000_123_456)]);
    // A stream exists, but it is Live only from its first segment on.
    assert_eq!(feeder.stream_state(), StreamState::Connecting);
    assert_eq!(feeder.channels().len(), 2);
    assert_eq!(feeder.last_timestamp_ms(), 1_700_000_123_456);
}

#[test]
fn other_streams_get_the_servers_time_at_once() {
    for mode in [GroupCallMode::Stream, GroupCallMode::Rtc] {
        let mut port = FakePort::default();
        let mut feeder = StreamFeeder::new(555, 51, mode);
        feeder.on_time_request(engine_id(4), &mut port);
        assert!(port.streams_requests.is_empty(), "{mode:?}");
        assert_eq!(port.time_answers(), vec![(4, SERVER_NOW_MS)], "{mode:?}");
    }
}

#[test]
fn empty_stream_list_answers_zero_and_shows_no_stream() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    feeder.on_time_request(engine_id(1), &mut port);
    feeder.on_reply(
        RequestId(1),
        StreamReply::Streams(&[]),
        Instant::now(),
        &mut port,
    );
    assert_eq!(port.time_answers(), vec![(1, 0)]);
    assert_eq!(feeder.state(), FeederState::NoStream);
    assert_eq!(feeder.stream_state(), StreamState::NoStream);
    // tgcalls asks again a second after a 0; each ask is a fresh
    // `getGroupCallStreams`, so the stream is picked up once it starts.
    feeder.on_time_request(engine_id(2), &mut port);
    assert_eq!(port.streams_requests.len(), 2);
    let live = [StreamChannel {
        channel_id: 1,
        scale: 0,
        time_offset: 5_000,
    }];
    feeder.on_reply(
        RequestId(2),
        StreamReply::Streams(&live),
        Instant::now(),
        &mut port,
    );
    assert_eq!(feeder.stream_state(), StreamState::Connecting);
    assert_eq!(port.time_answers().last(), Some(&(2, 5_000)));
}

#[test]
fn time_requests_share_one_streams_request() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    feeder.on_time_request(engine_id(1), &mut port);
    feeder.on_time_request(engine_id(2), &mut port);
    assert_eq!(port.streams_requests.len(), 1);
    feeder.on_reply(
        RequestId(1),
        StreamReply::Streams(&[]),
        Instant::now(),
        &mut port,
    );
    assert_eq!(port.time_answers(), vec![(1, 0), (2, 0)]);
}

#[test]
fn a_cancelled_time_request_is_not_answered() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    feeder.on_time_request(engine_id(1), &mut port);
    feeder.on_time_request(engine_id(2), &mut port);
    assert!(feeder.on_cancel(engine_id(1)));
    assert!(!feeder.on_cancel(engine_id(1)), "cancelled once");
    feeder.on_reply(
        RequestId(1),
        StreamReply::Streams(&[]),
        Instant::now(),
        &mut port,
    );
    assert_eq!(port.time_answers(), vec![(2, 0)]);
}

#[test]
fn failed_streams_send_answers_zero() {
    let mut port = FakePort {
        fail_sends: true,
        ..FakePort::default()
    };
    let mut feeder = feeder();
    feeder.on_time_request(engine_id(3), &mut port);
    assert_eq!(port.time_answers(), vec![(3, 0)]);
}

#[test]
fn streams_membership_error_answers_zero_and_rejoins() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    feeder.on_time_request(engine_id(1), &mut port);
    let outcome = feeder.on_reply(
        RequestId(1),
        StreamReply::Error(StreamFetchError::RejoinNeeded),
        Instant::now(),
        &mut port,
    );
    assert_eq!(outcome, ReplyOutcome::Rejoin);
    assert_eq!(port.time_answers(), vec![(1, 0)]);
    assert_eq!(feeder.state(), FeederState::Failed);
}

#[test]
fn a_video_part_fetches_its_channel_and_quality() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    let part = video(7, StreamQuality::Full);
    ask(&mut feeder, &mut port, 1, part);
    let (_, fetch) = port.segment_requests.first().expect("segment request");
    assert_eq!(
        *fetch,
        SegmentFetch {
            group_call_id: 555,
            time_offset: part.timestamp_ms,
            scale: 0,
            channel_id: 1,
            quality: Some(StreamQuality::Full),
        }
    );
    assert_eq!(feeder.pending_parts(), 1);
}

#[test]
fn channels_qualities_and_scales_follow_the_request() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    ask(&mut feeder, &mut port, 1, audio(1));
    let mut thumbnail = video(2, StreamQuality::Thumbnail);
    thumbnail.channel = BroadcastChannel::Video {
        channel_id: 3,
        quality: StreamQuality::Thumbnail,
    };
    thumbnail.duration_ms = 250;
    ask(&mut feeder, &mut port, 2, thumbnail);
    let mut medium = video(3, StreamQuality::Medium);
    medium.duration_ms = 500;
    ask(&mut feeder, &mut port, 3, medium);
    let fetched: Vec<_> = port
        .segment_requests
        .iter()
        .map(|(_, fetch)| (fetch.channel_id, fetch.quality, fetch.scale))
        .collect();
    assert_eq!(
        fetched,
        vec![
            // Audio parts carry no video fields.
            (0, None, 0),
            (3, Some(StreamQuality::Thumbnail), 2),
            (1, Some(StreamQuality::Medium), 1),
        ]
    );
}

#[test]
fn a_part_for_now_loads_the_servers_current_second() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    let mut part = audio(0);
    part.timestamp_ms = 0;
    ask(&mut feeder, &mut port, 1, part);
    let (id, fetch) = *port.segment_requests.first().expect("segment request");
    assert_eq!(fetch.time_offset, 1_700_000_042_000);
    feeder.on_reply(id, StreamReply::Data("AQID"), Instant::now(), &mut port);
    assert_eq!(last_part(&port).timestamp_ms, 1_700_000_042_000);
}

#[test]
fn an_unknown_duration_answers_resync_needed_without_a_fetch() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    let mut part = audio(1);
    part.duration_ms = 2000;
    ask(&mut feeder, &mut port, 1, part);
    assert!(port.segment_requests.is_empty());
    assert_eq!(last_part(&port).status, BroadcastPartStatus::ResyncNeeded);
}

#[test]
fn data_answers_success_with_the_decoded_bytes() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    let start = Instant::now();
    let part = video(4, StreamQuality::Full);
    feeder.on_part_request(engine_id(9), part, start, &mut port);
    let (id, _) = *port.segment_requests.first().expect("segment request");
    // "AQID" is base64 for [1, 2, 3].
    let outcome = feeder.on_reply(
        id,
        StreamReply::Data("AQID"),
        start + Duration::from_millis(40),
        &mut port,
    );
    assert_eq!(outcome, ReplyOutcome::Handled);
    assert_eq!(
        last_part(&port),
        &BroadcastPartAnswer {
            request: engine_id(9),
            status: BroadcastPartStatus::Success,
            timestamp_ms: part.timestamp_ms,
            response_time: SERVER_NOW_MS as f64 / 1000.0,
            data: vec![1, 2, 3],
        }
    );
    assert_eq!(feeder.pending_parts(), 0);
    assert_eq!(feeder.stream_state(), StreamState::Live);
}

#[test]
fn a_cancelled_part_is_forgotten_and_its_reply_dropped() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    ask(&mut feeder, &mut port, 1, audio(1));
    ask(&mut feeder, &mut port, 2, audio(2));
    let (first, _) = port.segment_requests[0];
    assert!(feeder.on_cancel(engine_id(1)));
    assert_eq!(feeder.pending_parts(), 1);
    assert!(!feeder.owns(first));
    let outcome = feeder.on_reply(first, StreamReply::Data("AQID"), Instant::now(), &mut port);
    assert_eq!(outcome, ReplyOutcome::NotMine);
    assert!(port.parts.is_empty());
    // A request from another instance with the same id is not this one.
    assert!(!feeder.on_cancel(BroadcastRequestId { instance: 8, id: 2 }));
    assert_eq!(feeder.pending_parts(), 1);
}

#[test]
fn bad_base64_answers_resync_needed() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    ask(&mut feeder, &mut port, 1, audio(4));
    let (id, _) = *port.segment_requests.first().expect("segment request");
    feeder.on_reply(
        id,
        StreamReply::Data("not base64!"),
        Instant::now(),
        &mut port,
    );
    assert_eq!(last_part(&port).status, BroadcastPartStatus::ResyncNeeded);
    assert!(last_part(&port).data.is_empty());
}

#[test]
fn empty_data_is_still_a_success() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    ask(&mut feeder, &mut port, 1, audio(4));
    let (id, _) = *port.segment_requests.first().expect("segment request");
    feeder.on_reply(id, StreamReply::Data(""), Instant::now(), &mut port);
    assert_eq!(last_part(&port).status, BroadcastPartStatus::Success);
    assert!(last_part(&port).data.is_empty());
}

#[test]
fn the_first_successful_segment_makes_the_stream_live() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    ask(&mut feeder, &mut port, 1, audio(1));
    ask(&mut feeder, &mut port, 2, audio(2));
    let (first, _) = port.segment_requests[0];
    let (second, _) = port.segment_requests[1];
    feeder.on_reply(
        first,
        StreamReply::Error(StreamFetchError::TimeTooBig),
        Instant::now(),
        &mut port,
    );
    assert_eq!(feeder.stream_state(), StreamState::Connecting);
    feeder.on_reply(second, StreamReply::Data("AQID"), Instant::now(), &mut port);
    assert_eq!(feeder.stream_state(), StreamState::Live);
}

#[test]
fn call_invalid_counts_as_left() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    ask(&mut feeder, &mut port, 1, audio(1));
    let (id, _) = *port.segment_requests.first().expect("segment request");
    let outcome = feeder.on_reply(
        id,
        StreamReply::Error(StreamFetchError::CallLeft),
        Instant::now(),
        &mut port,
    );
    assert_eq!(outcome, ReplyOutcome::Left);
    assert_eq!(last_part(&port).status, BroadcastPartStatus::ResyncNeeded);
    assert_eq!(feeder.stream_state(), StreamState::Ended);

    let mut feeder = StreamFeeder::new(555, 51, GroupCallMode::Rtmp);
    feeder.on_time_request(engine_id(5), &mut port);
    let outcome = feeder.on_reply(
        RequestId(port.next_id),
        StreamReply::Error(StreamFetchError::CallLeft),
        Instant::now(),
        &mut port,
    );
    assert_eq!(outcome, ReplyOutcome::Left);
    assert_eq!(port.time_answers().last(), Some(&(5, 0)));
}

#[test]
fn part_errors_map_like_tdesktop() {
    let cases = [
        (
            StreamFetchError::Flood,
            BroadcastPartStatus::NotReady,
            ReplyOutcome::Handled,
        ),
        (
            StreamFetchError::TimeTooBig,
            BroadcastPartStatus::NotReady,
            ReplyOutcome::Handled,
        ),
        (
            StreamFetchError::RejoinNeeded,
            BroadcastPartStatus::NotReady,
            ReplyOutcome::Rejoin,
        ),
        (
            StreamFetchError::CallLeft,
            BroadcastPartStatus::ResyncNeeded,
            ReplyOutcome::Left,
        ),
        (
            StreamFetchError::Other,
            BroadcastPartStatus::ResyncNeeded,
            ReplyOutcome::Handled,
        ),
    ];
    for (error, status, outcome) in cases {
        let mut port = FakePort::default();
        let mut feeder = feeder();
        ask(&mut feeder, &mut port, 1, audio(1));
        let (id, _) = *port.segment_requests.first().expect("segment request");
        let got = feeder.on_reply(id, StreamReply::Error(error), Instant::now(), &mut port);
        assert_eq!(got, outcome, "{error:?}");
        assert_eq!(last_part(&port).status, status, "{error:?}");
        // Every answer carries the server's time for tgcalls' resync.
        assert_eq!(
            last_part(&port).response_time,
            SERVER_NOW_MS as f64 / 1000.0
        );
    }
}

#[test]
fn parts_over_the_pending_cap_answer_not_ready() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    for segment in 0..MAX_PENDING_PARTS {
        ask(
            &mut feeder,
            &mut port,
            segment as u64,
            audio(segment as i64),
        );
    }
    assert_eq!(port.segment_requests.len(), MAX_PENDING_PARTS);
    assert!(port.parts.is_empty());
    ask(&mut feeder, &mut port, 99, audio(99));
    assert_eq!(port.segment_requests.len(), MAX_PENDING_PARTS);
    assert_eq!(last_part(&port).request, engine_id(99));
    assert_eq!(last_part(&port).status, BroadcastPartStatus::NotReady);
}

#[test]
fn failed_segment_send_answers_not_ready() {
    let mut port = FakePort {
        fail_sends: true,
        ..FakePort::default()
    };
    let mut feeder = feeder();
    ask(&mut feeder, &mut port, 1, audio(3));
    assert_eq!(last_part(&port).status, BroadcastPartStatus::NotReady);
    assert_eq!(feeder.pending_parts(), 0);
}

#[test]
fn drain_answers_every_pending_part_resync_needed() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    for segment in [5, 3, 4] {
        ask(&mut feeder, &mut port, segment as u64, audio(segment));
    }
    feeder.drain(&mut port);
    let answered: Vec<_> = port
        .parts
        .iter()
        .map(|(chat, answer)| (*chat, answer.request.id, answer.status))
        .collect();
    assert_eq!(
        answered,
        vec![
            (51, 3, BroadcastPartStatus::ResyncNeeded),
            (51, 4, BroadcastPartStatus::ResyncNeeded),
            (51, 5, BroadcastPartStatus::ResyncNeeded),
        ]
    );
    assert_eq!(feeder.pending_parts(), 0);
}

#[test]
fn unknown_request_ids_are_ignored() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    let outcome = feeder.on_reply(
        RequestId(404),
        StreamReply::Data("AQID"),
        Instant::now(),
        &mut port,
    );
    assert_eq!(outcome, ReplyOutcome::NotMine);
    assert!(port.parts.is_empty() && port.times.is_empty());
    assert!(!feeder.owns(RequestId(404)));
}

#[test]
fn quality_is_reported_only_when_the_bucket_changes() {
    let mut tracker = StreamQualityTracker::default();
    assert_eq!(tracker.observe(720.0), Some(StreamQuality::Full));
    assert_eq!(tracker.observe(700.0), None);
    assert_eq!(tracker.observe(640.0), None);
    assert_eq!(tracker.observe(500.0), Some(StreamQuality::Medium));
    assert_eq!(tracker.observe(320.0), None);
    assert_eq!(tracker.observe(240.0), Some(StreamQuality::Thumbnail));
    assert_eq!(tracker.observe(100.0), None);
    assert_eq!(tracker.observe(1280.0), Some(StreamQuality::Full));
}
