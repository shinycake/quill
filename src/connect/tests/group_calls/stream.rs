//! Driver tests: stream-mode group calls (RTMP live streams) — the
//! broadcast requests from ntgcalls go to TDLib through the recording
//! sender and the answers back to the engine.
use super::*;
use crate::calls::engine::{
    BroadcastPartRequest, BroadcastPartStatus, EngineEvent, GroupCallMode, StreamQuality,
    StreamState,
};
use ntgcalls_sys::NTG_ERR_RTMP_STREAMING_UNSUPPORTED;
use std::time::Instant;

fn group_call_json(participant_count: i32, need_rejoin: bool) -> String {
    format!(
        r#"{{"@type":"updateGroupCall","group_call":{{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":true,"is_joined":true,"need_rejoin":{need_rejoin},"is_owned":false,"can_be_managed":false,"participant_count":{participant_count},"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":false,"mute_new_participants":false,"can_toggle_mute_new_participants":false,"can_send_messages":false,"are_messages_allowed":false,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}}}"#
    )
}

/// A joined call whose native connect reported `mode`.
fn joined_stream_call(mode: GroupCallMode) -> GroupCallDriverHarness {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    handle.set_group_call_mode(51, mode);
    let extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"{{\"rtmp\":true}}","@extra":{}}}"#,
            extra.0
        ),
    );
    (dir, driver, recorder, handle, sink, seq)
}

fn extra_of(request: &Value) -> String {
    request["@extra"]
        .as_str()
        .expect("string @extra")
        .to_string()
}

fn part(segment_id: i64) -> BroadcastPartRequest {
    BroadcastPartRequest {
        segment_id,
        part_id: 0,
        limit: 131_072,
        timestamp: 1_700_000_000_000 + segment_id * 1000,
        quality_update: false,
        channel_id: 1,
        quality: Some(StreamQuality::Full),
    }
}

fn request_part(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    handle: &MockEngine,
    segment_id: i64,
) {
    handle.emit_engine_event(EngineEvent::BroadcastPartRequested {
        chat_id: 51,
        request: part(segment_id),
    });
    driver.request_tick(Instant::now());
}

fn stream_state(driver: &ConnectDriver<Arc<RecordingSender>>) -> (GroupCallMode, StreamState) {
    let call = driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .expect("tracked call");
    (call.stream_mode, call.stream)
}

#[test]
fn connect_records_the_stream_mode() {
    let (dir, driver, ..) = joined_stream_call(GroupCallMode::Rtmp);
    assert_eq!(
        stream_state(&driver),
        (GroupCallMode::Rtmp, StreamState::Connecting)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn timestamp_request_asks_tdlib_and_answers_the_engine() {
    let (dir, mut driver, recorder, handle, sink, seq) = joined_stream_call(GroupCallMode::Rtmp);
    handle.emit_engine_event(EngineEvent::BroadcastTimestampRequested { chat_id: 51 });
    assert!(driver.request_tick(Instant::now()));
    let request = sent_request(&recorder, "getGroupCallStreams");
    assert_eq!(request["group_call_id"], 555);
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"groupCallStreams","streams":[{{"@type":"groupCallStream","channel_id":1,"scale":0,"time_offset":1700000123456}}],"@extra":"{}"}}"#,
            extra_of(&request)
        ),
    );
    assert_eq!(handle.broadcast_timestamps(), vec![(51, 1_700_000_123_456)]);
    assert_eq!(stream_state(&driver).1, StreamState::Live);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_stream_list_shows_no_stream() {
    let (dir, mut driver, recorder, handle, sink, seq) = joined_stream_call(GroupCallMode::Rtmp);
    handle.emit_engine_event(EngineEvent::BroadcastTimestampRequested { chat_id: 51 });
    driver.request_tick(Instant::now());
    let request = sent_request(&recorder, "getGroupCallStreams");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"groupCallStreams","streams":[],"@extra":"{}"}}"#,
            extra_of(&request)
        ),
    );
    assert_eq!(handle.broadcast_timestamps(), vec![(51, 0)]);
    assert_eq!(stream_state(&driver).1, StreamState::NoStream);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn part_request_sends_the_segment_request_and_data_answers_success() {
    let (dir, mut driver, recorder, handle, sink, seq) = joined_stream_call(GroupCallMode::Rtmp);
    request_part(&mut driver, &handle, 3);
    let request = sent_request(&recorder, "getGroupCallStreamSegment");
    assert_eq!(request["group_call_id"], 555);
    assert_eq!(request["time_offset"], 1_700_000_003_000_i64);
    assert_eq!(request["scale"], 0);
    assert_eq!(request["channel_id"], 1);
    assert_eq!(
        request["video_quality"]["@type"],
        "groupCallVideoQualityFull"
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"data","data":"AQID","@extra":"{}"}}"#,
            extra_of(&request)
        ),
    );
    let parts = handle.broadcast_parts();
    let (chat_id, answer) = parts.first().expect("part answer");
    assert_eq!(*chat_id, 51);
    assert_eq!(answer.segment_id, 3);
    assert_eq!(answer.status, BroadcastPartStatus::Success);
    assert_eq!(answer.data, vec![1, 2, 3]);
    // The answer never reached the reducer as a pending request.
    assert!(
        driver
            .session
            .requests
            .get(crate::ids::RequestId(
                extra_of(&request).parse().expect("numeric extra")
            ))
            .is_none()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn answer_part_with_error(code: i32, message: &str) -> (BroadcastPartStatus, MockEngine, usize) {
    let (dir, mut driver, recorder, handle, sink, seq) = joined_stream_call(GroupCallMode::Rtmp);
    request_part(&mut driver, &handle, 1);
    let request = sent_request(&recorder, "getGroupCallStreamSegment");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":{code},"message":"{message}","@extra":"{}"}}"#,
            extra_of(&request)
        ),
    );
    let status = handle
        .broadcast_parts()
        .last()
        .expect("part answer")
        .1
        .status;
    let offers = handle.group_offers().len();
    let _ = std::fs::remove_dir_all(&dir);
    (status, handle, offers)
}

#[test]
fn segment_errors_map_to_part_statuses() {
    assert_eq!(
        answer_part_with_error(420, "FLOOD_WAIT_3").0,
        BroadcastPartStatus::NotReady
    );
    assert_eq!(
        answer_part_with_error(429, "Too Many Requests: retry after 3").0,
        BroadcastPartStatus::NotReady
    );
    assert_eq!(
        answer_part_with_error(400, "TIME_TOO_BIG").0,
        BroadcastPartStatus::NotReady
    );
    assert_eq!(
        answer_part_with_error(400, "SOMETHING_ELSE").0,
        BroadcastPartStatus::ResyncNeeded
    );
}

#[test]
fn join_missing_answers_not_ready_and_rejoins() {
    let (status, _handle, offers) = answer_part_with_error(400, "GROUPCALL_JOIN_MISSING");
    assert_eq!(status, BroadcastPartStatus::NotReady);
    // The first join made one offer; the rejoin made a second.
    assert_eq!(offers, 2);
    let (status, _handle, offers) = answer_part_with_error(403, "GROUPCALL_FORBIDDEN");
    assert_eq!(status, BroadcastPartStatus::NotReady);
    assert_eq!(offers, 2);
}

#[test]
fn leaving_answers_pending_parts_resync_needed_before_the_stop() {
    let (dir, mut driver, _recorder, handle, ..) = joined_stream_call(GroupCallMode::Rtmp);
    request_part(&mut driver, &handle, 1);
    request_part(&mut driver, &handle, 2);
    assert!(handle.broadcast_parts().is_empty());
    driver.leave_group_call().expect("leave");
    let statuses: Vec<_> = handle
        .broadcast_parts()
        .iter()
        .map(|(_, answer)| (answer.segment_id, answer.status))
        .collect();
    assert_eq!(
        statuses,
        vec![
            (1, BroadcastPartStatus::ResyncNeeded),
            (2, BroadcastPartStatus::ResyncNeeded),
        ]
    );
    assert_eq!(handle.group_leaves(), vec![555]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn requests_for_a_call_without_a_feeder_are_answered_at_once() {
    let (dir, mut driver, recorder, handle, ..) = joined_stream_call(GroupCallMode::Rtc);
    assert_eq!(
        stream_state(&driver).0,
        GroupCallMode::Rtc,
        "an RTC call has no feeder"
    );
    handle.emit_engine_event(EngineEvent::BroadcastTimestampRequested { chat_id: 51 });
    request_part(&mut driver, &handle, 4);
    assert_eq!(handle.broadcast_timestamps(), vec![(51, 0)]);
    assert_eq!(
        handle.broadcast_parts().last().map(|(_, a)| a.status),
        Some(BroadcastPartStatus::ResyncNeeded)
    );
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|json| json.contains("getGroupCallStream"))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tile_width_reports_quality_only_on_bucket_change() {
    let (dir, mut driver, recorder, handle, ..) = joined_stream_call(GroupCallMode::Rtmp);
    driver.report_stream_tile_width(51, 720.0);
    driver.report_stream_tile_width(51, 700.0);
    driver.report_stream_tile_width(51, 300.0);
    driver.report_stream_tile_width(51, 250.0);
    assert_eq!(
        handle.stream_qualities(),
        vec![(51, StreamQuality::Full), (51, StreamQuality::Thumbnail)]
    );
    request_part(&mut driver, &handle, 2);
    let request = sent_request(&recorder, "getGroupCallStreamSegment");
    assert_eq!(
        request["video_quality"]["@type"],
        "groupCallVideoQualityThumbnail"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unsupported_stream_is_a_join_error_not_a_crash() {
    let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
    handle.fail_next_group_connect(NTG_ERR_RTMP_STREAMING_UNSUPPORTED);
    let extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"{{\"rtmp\":true}}","@extra":{}}}"#,
            extra.0
        ),
    );
    assert_eq!(
        stream_state(&driver),
        (GroupCallMode::Rtmp, StreamState::Unsupported)
    );
    let call = driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .expect("call");
    assert!(!call.transport_ready);
    assert!(call.transport_error.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stream_pictures_land_in_the_chats_stream_slot() {
    let (dir, driver, _recorder, handle, ..) = joined_stream_call(GroupCallMode::Rtmp);
    handle.emit_stream_frame(
        51,
        VideoFrame {
            seq: 0,
            width: 2,
            height: 2,
            rgba: vec![0; 16],
            is_local: false,
            participant_user_id: None,
            is_screen: false,
        },
    );
    let frame = driver.latest_stream_frame(51).expect("stream frame");
    assert_eq!((frame.width, frame.height), (2, 2));
    assert!(driver.latest_stream_frame(52).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn viewer_count_follows_participant_count() {
    let (dir, mut driver, _recorder, _handle, sink, seq) = joined_stream_call(GroupCallMode::Rtmp);
    ingest_call_json(&mut driver, &seq, &sink, &group_call_json(1234, false));
    let call = driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .expect("call");
    assert_eq!(call.viewer_count, 1234);
    ingest_call_json(&mut driver, &seq, &sink, &group_call_json(-1, false));
    let call = driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .expect("call");
    assert_eq!(call.viewer_count, 0, "a negative count is no viewers");
    let _ = std::fs::remove_dir_all(&dir);
}
