use crate::calls::engine::{
    BroadcastPartAnswer, BroadcastPartRequest, BroadcastPartStatus, FeederState, MAX_PENDING_PARTS,
    ReplyOutcome, SegmentFetch, StreamChannel, StreamFeeder, StreamFetchError, StreamPort,
    StreamQuality, StreamQualityTracker, StreamReply, StreamState,
};
use crate::ids::RequestId;
use std::time::{Duration, Instant};

#[derive(Default)]
struct FakePort {
    next_id: u64,
    fail_sends: bool,
    streams_requests: Vec<i32>,
    segment_requests: Vec<(RequestId, SegmentFetch)>,
    timestamps: Vec<(i64, i64)>,
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

    fn answer_timestamp(&mut self, chat_id: i64, timestamp_ms: i64) {
        self.timestamps.push((chat_id, timestamp_ms));
    }

    fn answer_part(&mut self, chat_id: i64, answer: BroadcastPartAnswer) {
        self.parts.push((chat_id, answer));
    }
}

fn part(segment_id: i64, quality: Option<StreamQuality>) -> BroadcastPartRequest {
    BroadcastPartRequest {
        segment_id,
        part_id: 0,
        limit: 131_072,
        timestamp: 1_700_000_000_000 + segment_id * 1000,
        quality_update: false,
        channel_id: 1,
        quality,
    }
}

fn feeder() -> StreamFeeder {
    StreamFeeder::new(555, 51)
}

fn last_part(port: &FakePort) -> &BroadcastPartAnswer {
    &port.parts.last().expect("a part answer").1
}

#[test]
fn timestamp_request_answers_the_first_channels_time_offset() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    feeder.on_timestamp_request(&mut port);
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
    assert_eq!(port.timestamps, vec![(51, 1_700_000_123_456)]);
    assert_eq!(feeder.stream_state(), StreamState::Live);
    assert_eq!(feeder.channels().len(), 2);
    assert_eq!(feeder.last_timestamp_ms(), 1_700_000_123_456);
}

#[test]
fn empty_stream_list_answers_zero_and_shows_no_stream() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    feeder.on_timestamp_request(&mut port);
    feeder.on_reply(
        RequestId(1),
        StreamReply::Streams(&[]),
        Instant::now(),
        &mut port,
    );
    assert_eq!(port.timestamps, vec![(51, 0)]);
    assert_eq!(feeder.state(), FeederState::NoStream);
    assert_eq!(feeder.stream_state(), StreamState::NoStream);
    // ntgcalls asks again a second after a 0; each ask is a fresh
    // `getGroupCallStreams`, so the stream is picked up once it starts.
    feeder.on_timestamp_request(&mut port);
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
    assert_eq!(feeder.stream_state(), StreamState::Live);
    assert_eq!(port.timestamps.last(), Some(&(51, 5_000)));
}

#[test]
fn timestamp_requests_share_one_streams_request() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    feeder.on_timestamp_request(&mut port);
    feeder.on_timestamp_request(&mut port);
    assert_eq!(port.streams_requests.len(), 1);
    feeder.on_reply(
        RequestId(1),
        StreamReply::Streams(&[]),
        Instant::now(),
        &mut port,
    );
    assert_eq!(port.timestamps, vec![(51, 0), (51, 0)]);
}

#[test]
fn failed_streams_send_answers_zero() {
    let mut port = FakePort {
        fail_sends: true,
        ..FakePort::default()
    };
    let mut feeder = feeder();
    feeder.on_timestamp_request(&mut port);
    assert_eq!(port.timestamps, vec![(51, 0)]);
}

#[test]
fn streams_membership_error_answers_zero_and_rejoins() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    feeder.on_timestamp_request(&mut port);
    let outcome = feeder.on_reply(
        RequestId(1),
        StreamReply::Error(StreamFetchError::RejoinNeeded),
        Instant::now(),
        &mut port,
    );
    assert_eq!(outcome, ReplyOutcome::Rejoin);
    assert_eq!(port.timestamps, vec![(51, 0)]);
    assert_eq!(feeder.state(), FeederState::Failed);
}

#[test]
fn part_request_fetches_the_segment_at_scale_zero() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    let request = part(7, Some(StreamQuality::Full));
    feeder.on_part_request(request, Instant::now(), &mut port);
    let (_, fetch) = port.segment_requests.first().expect("segment request");
    assert_eq!(
        *fetch,
        SegmentFetch {
            group_call_id: 555,
            time_offset: request.timestamp,
            scale: 0,
            channel_id: 1,
            quality: Some(StreamQuality::Full),
        }
    );
    assert_eq!(feeder.pending_parts(), 1);
}

#[test]
fn video_parts_use_the_tiles_quality_and_audio_parts_none() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    feeder.set_quality(StreamQuality::Thumbnail);
    feeder.on_part_request(
        part(1, Some(StreamQuality::Full)),
        Instant::now(),
        &mut port,
    );
    feeder.on_part_request(part(2, None), Instant::now(), &mut port);
    let mut update = part(3, Some(StreamQuality::Medium));
    update.quality_update = true;
    feeder.on_part_request(update, Instant::now(), &mut port);
    let qualities: Vec<_> = port
        .segment_requests
        .iter()
        .map(|(_, fetch)| fetch.quality)
        .collect();
    assert_eq!(
        qualities,
        vec![
            Some(StreamQuality::Thumbnail),
            None,
            // A quality update keeps the quality ntgcalls asked for.
            Some(StreamQuality::Medium),
        ]
    );
}

#[test]
fn data_answers_success_with_the_decoded_bytes() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    let start = Instant::now();
    let mut request = part(4, Some(StreamQuality::Full));
    request.part_id = 2;
    request.quality_update = true;
    feeder.on_part_request(request, start, &mut port);
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
            segment_id: 4,
            part_id: 2,
            status: BroadcastPartStatus::Success,
            quality_update: true,
            data: vec![1, 2, 3],
        }
    );
    assert_eq!(feeder.pending_parts(), 0);
    assert_eq!(feeder.stream_state(), StreamState::Live);
}

#[test]
fn bad_base64_answers_resync_needed() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    feeder.on_part_request(part(4, None), Instant::now(), &mut port);
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
            StreamFetchError::Other,
            BroadcastPartStatus::ResyncNeeded,
            ReplyOutcome::Handled,
        ),
    ];
    for (error, status, outcome) in cases {
        let mut port = FakePort::default();
        let mut feeder = feeder();
        feeder.on_part_request(part(1, None), Instant::now(), &mut port);
        let (id, _) = *port.segment_requests.first().expect("segment request");
        let got = feeder.on_reply(id, StreamReply::Error(error), Instant::now(), &mut port);
        assert_eq!(got, outcome, "{error:?}");
        assert_eq!(last_part(&port).status, status, "{error:?}");
    }
}

#[test]
fn more_than_eight_pending_parts_answer_not_ready() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    for segment in 0..MAX_PENDING_PARTS {
        feeder.on_part_request(part(segment as i64, None), Instant::now(), &mut port);
    }
    assert_eq!(port.segment_requests.len(), MAX_PENDING_PARTS);
    assert!(port.parts.is_empty());
    feeder.on_part_request(part(99, None), Instant::now(), &mut port);
    assert_eq!(port.segment_requests.len(), MAX_PENDING_PARTS);
    assert_eq!(last_part(&port).segment_id, 99);
    assert_eq!(last_part(&port).status, BroadcastPartStatus::NotReady);
}

#[test]
fn failed_segment_send_answers_not_ready() {
    let mut port = FakePort {
        fail_sends: true,
        ..FakePort::default()
    };
    let mut feeder = feeder();
    feeder.on_part_request(part(3, None), Instant::now(), &mut port);
    assert_eq!(last_part(&port).status, BroadcastPartStatus::NotReady);
    assert_eq!(feeder.pending_parts(), 0);
}

#[test]
fn drain_answers_every_pending_part_resync_needed() {
    let mut port = FakePort::default();
    let mut feeder = feeder();
    for segment in [5, 3, 4] {
        feeder.on_part_request(part(segment, None), Instant::now(), &mut port);
    }
    feeder.drain(&mut port);
    let answered: Vec<_> = port
        .parts
        .iter()
        .map(|(chat, answer)| (*chat, answer.segment_id, answer.status))
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
    assert!(port.parts.is_empty() && port.timestamps.is_empty());
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
