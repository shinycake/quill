//! Driver tests: stream-mode group calls (RTMP live streams) — the
//! engine's broadcast requests go to TDLib through the recording sender
//! and the answers back to the engine.
use super::*;
use crate::calls::engine::{
    BroadcastChannel, BroadcastPartRequest, BroadcastPartStatus, BroadcastRequestId, EngineEvent,
    GroupCallMode, StreamQuality, StreamState,
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

fn engine_id(id: u64) -> BroadcastRequestId {
    BroadcastRequestId { instance: 1, id }
}

/// The unified part of segment `segment`, as tgcalls asks an RTMP stream.
fn part(segment: i64) -> BroadcastPartRequest {
    BroadcastPartRequest {
        timestamp_ms: 1_700_000_000_000 + segment * 1000,
        duration_ms: 1000,
        channel: BroadcastChannel::Video {
            channel_id: 1,
            quality: StreamQuality::Full,
        },
    }
}

/// Asks for segment `segment`'s part, with the segment as its request id.
fn request_part(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    handle: &MockEngine,
    segment: i64,
) {
    handle.emit_engine_event(EngineEvent::BroadcastPartRequested {
        chat_id: 51,
        request: engine_id(segment as u64),
        part: part(segment),
    });
    driver.request_tick(Instant::now());
}

fn ask_time(handle: &MockEngine, id: u64) {
    handle.emit_engine_event(EngineEvent::BroadcastTimeRequested {
        chat_id: 51,
        request: engine_id(id),
    });
}

/// `(chat, request id, time)` per time answer.
fn times(handle: &MockEngine) -> Vec<(i64, u64, i64)> {
    handle
        .broadcast_times()
        .into_iter()
        .map(|(chat, request, time)| (chat, request.id, time))
        .collect()
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
fn an_engine_stream_request_wakes_the_poll_loop() {
    let (dir, _driver, _recorder, handle, ..) = joined_stream_call(GroupCallMode::Rtmp);
    let seen = crate::wake::UPDATES.epoch();
    ask_time(&handle, 1);
    assert!(
        crate::wake::UPDATES.epoch() != seen,
        "the hook must poke so the request is not left for the idle sleep"
    );
    let _ = std::fs::remove_dir_all(&dir);
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
    ask_time(&handle, 1);
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
    assert_eq!(times(&handle), vec![(51, 1, 1_700_000_123_456)]);
    // Live comes with the first segment, not the stream list.
    assert_eq!(stream_state(&driver).1, StreamState::Connecting);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_stream_list_shows_no_stream() {
    let (dir, mut driver, recorder, handle, sink, seq) = joined_stream_call(GroupCallMode::Rtmp);
    ask_time(&handle, 1);
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
    assert_eq!(times(&handle), vec![(51, 1, 0)]);
    assert_eq!(stream_state(&driver).1, StreamState::NoStream);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_mtproto_stream_gets_the_servers_time_without_asking_tdlib() {
    let (dir, mut driver, recorder, handle, sink, seq) = joined_stream_call(GroupCallMode::Stream);
    // TDLib says the server is an hour ahead of this machine.
    let local_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis() as i64;
    let server_s = local_ms / 1000 + 3600;
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateOption","name":"unix_time","value":{{"@type":"optionValueInteger","value":"{server_s}"}}}}"#
        ),
    );
    ask_time(&handle, 9);
    driver.request_tick(Instant::now());
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|json| json.contains("getGroupCallStreams"))
    );
    let answered = times(&handle);
    let (chat, id, time) = *answered.first().expect("a time answer");
    assert_eq!((chat, id), (51, 9));
    // The server's clock, not this machine's: well past local time.
    assert!(
        time >= server_s * 1000 && time >= local_ms + 3_000_000,
        "{time}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cancelled_part_is_not_answered() {
    let (dir, mut driver, recorder, handle, sink, seq) = joined_stream_call(GroupCallMode::Rtmp);
    request_part(&mut driver, &handle, 3);
    let request = sent_request(&recorder, "getGroupCallStreamSegment");
    handle.emit_engine_event(EngineEvent::BroadcastRequestCancelled {
        chat_id: 51,
        request: engine_id(3),
    });
    driver.request_tick(Instant::now());
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"data","data":"AQID","@extra":"{}"}}"#,
            extra_of(&request)
        ),
    );
    assert!(handle.broadcast_parts().is_empty());
    // The late reply never reaches the reducer either.
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
    assert_eq!(answer.request, engine_id(3));
    assert_eq!(answer.timestamp_ms, 1_700_000_003_000);
    assert_eq!(answer.status, BroadcastPartStatus::Success);
    assert_eq!(answer.data, vec![1, 2, 3]);
    assert_eq!(stream_state(&driver).1, StreamState::Live);
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
    let (status, handle, offers) = answer_part_with_error(400, "GROUPCALL_JOIN_MISSING");
    assert_eq!(status, BroadcastPartStatus::NotReady);
    // The first join made one offer; the rejoin made a second, after
    // stopping the old native call.
    assert_eq!(offers, 2);
    assert_eq!(handle.group_leaves(), vec![555]);
    let (status, handle, offers) = answer_part_with_error(403, "GROUPCALL_FORBIDDEN");
    assert_eq!(status, BroadcastPartStatus::NotReady);
    assert_eq!(offers, 2);
    assert_eq!(handle.group_leaves(), vec![555]);
}

#[test]
fn rejoin_drains_and_stops_the_old_native_call_first() {
    let (dir, mut driver, _recorder, handle, sink, seq) = joined_stream_call(GroupCallMode::Rtmp);
    request_part(&mut driver, &handle, 1);
    request_part(&mut driver, &handle, 2);
    // TDLib says the call needs a rejoin (network loss).
    ingest_call_json(&mut driver, &seq, &sink, &group_call_json(10, true));
    let statuses: Vec<_> = handle
        .broadcast_parts()
        .iter()
        .map(|(_, answer)| answer.status)
        .collect();
    assert_eq!(
        statuses,
        vec![
            BroadcastPartStatus::ResyncNeeded,
            BroadcastPartStatus::ResyncNeeded
        ]
    );
    assert_eq!(handle.group_leaves(), vec![555]);
    assert_eq!(handle.group_offers().len(), 2);
    // The old feeder is gone: a late request is answered at once instead
    // of attaching to the new context.
    request_part(&mut driver, &handle, 3);
    assert_eq!(
        handle.broadcast_parts().last().map(|(_, a)| a.status),
        Some(BroadcastPartStatus::ResyncNeeded)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_invalid_stops_the_stream_without_a_rejoin() {
    let (status, handle, offers) = answer_part_with_error(400, "GROUPCALL_INVALID");
    assert_eq!(status, BroadcastPartStatus::ResyncNeeded);
    assert_eq!(offers, 1, "no rejoin");
    assert_eq!(handle.group_leaves(), vec![555]);
}

#[test]
fn call_invalid_marks_the_stream_ended() {
    let (dir, mut driver, recorder, handle, sink, seq) = joined_stream_call(GroupCallMode::Rtmp);
    request_part(&mut driver, &handle, 1);
    let request = sent_request(&recorder, "getGroupCallStreamSegment");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"GROUPCALL_INVALID","@extra":"{}"}}"#,
            extra_of(&request)
        ),
    );
    assert_eq!(stream_state(&driver).1, StreamState::Ended);
    let call = driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .expect("call");
    assert!(!call.transport_ready);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn data_without_bytes_answers_resync_needed() {
    let (dir, mut driver, recorder, handle, sink, seq) = joined_stream_call(GroupCallMode::Rtmp);
    request_part(&mut driver, &handle, 1);
    let request = sent_request(&recorder, "getGroupCallStreamSegment");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"data","@extra":"{}"}}"#, extra_of(&request)),
    );
    let status = handle.broadcast_parts().last().map(|(_, a)| a.status);
    assert_eq!(status, Some(BroadcastPartStatus::ResyncNeeded));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_queued_request_is_answered() {
    let (dir, mut driver, recorder, handle, ..) = joined_stream_call(GroupCallMode::Rtmp);
    // Far more than one tick handles and more than the old bounded queue
    // held: none may be dropped, since the engine never asks again.
    for segment in 0..600 {
        handle.emit_engine_event(EngineEvent::BroadcastPartRequested {
            chat_id: 51,
            request: engine_id(segment as u64),
            part: part(segment),
        });
    }
    for _ in 0..20 {
        driver.request_tick(Instant::now());
    }
    let sent = recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains("getGroupCallStreamSegment"))
        .count();
    // The feeder sends up to its cap and answers the rest NOT_READY.
    assert_eq!(sent + handle.broadcast_parts().len(), 600);
    let _ = std::fs::remove_dir_all(&dir);
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
        .map(|(_, answer)| (answer.request.id, answer.status))
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
    ask_time(&handle, 1);
    request_part(&mut driver, &handle, 4);
    assert_eq!(times(&handle), vec![(51, 1, 0)]);
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
    // The tile's quality is recorded, but the request keeps what the
    // engine asked for (Full for RTMP, as tgcalls asks for tdesktop).
    request_part(&mut driver, &handle, 2);
    let request = sent_request(&recorder, "getGroupCallStreamSegment");
    assert_eq!(
        request["video_quality"]["@type"],
        "groupCallVideoQualityFull"
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
    // No native call is left connected without a feeder.
    assert_eq!(handle.group_leaves(), vec![555]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unsupported_code_from_another_operation_is_an_ordinary_error() {
    let err = crate::calls::engine::EngineError::Engine {
        op: "ntg_set_stream_sources",
        code: NTG_ERR_RTMP_STREAMING_UNSUPPORTED,
    };
    assert!(!err.is_stream_unsupported());
    let err = crate::calls::engine::EngineError::Engine {
        op: "ntg_connect",
        code: NTG_ERR_RTMP_STREAMING_UNSUPPORTED,
    };
    assert!(err.is_stream_unsupported());
}

fn planes() -> LiveFrame {
    LiveFrame::new(0, 2, 2, LiveRotation::None, vec![0; 6], Instant::now()).expect("frame")
}

#[test]
fn stream_pictures_land_in_the_chats_stream_feed() {
    let (dir, driver, _recorder, handle, ..) = joined_stream_call(GroupCallMode::Rtmp);
    let sink = driver
        .live_sink(LiveKey::Stream { chat_id: 51 })
        .expect("the feeder opened a feed");
    handle.emit_live_frame(LiveKey::Stream { chat_id: 51 }, planes());
    let frame = sink.take().expect("stream frame");
    assert_eq!((frame.width, frame.height), (2, 2));
    assert!(driver.live_sink(LiveKey::Stream { chat_id: 52 }).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_picture_after_leaving_does_not_bring_the_feed_back() {
    let (dir, mut driver, _recorder, handle, ..) = joined_stream_call(GroupCallMode::Rtmp);
    let sink = driver
        .live_sink(LiveKey::Stream { chat_id: 51 })
        .expect("feed");
    driver.leave_group_call().expect("leave");
    assert_eq!(sink.snapshot().end, Some(LiveEnd::Left));
    // ntgcalls can still deliver a frame between the drain and the stop.
    handle.emit_live_frame(LiveKey::Stream { chat_id: 51 }, planes());
    assert!(driver.live_sink(LiveKey::Stream { chat_id: 51 }).is_none());
    assert!(sink.take().is_none());
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

#[test]
fn the_newest_picture_is_known_by_its_sequence() {
    let (dir, driver, _recorder, handle, ..) = joined_stream_call(GroupCallMode::Rtmp);
    let sink = driver
        .live_sink(LiveKey::Stream { chat_id: 51 })
        .expect("feed");
    assert!(sink.latest().is_none());
    let mut seqs = Vec::new();
    for _ in 0..2 {
        handle.emit_live_frame(LiveKey::Stream { chat_id: 51 }, planes());
        // The engine numbers its pictures.
        seqs.push(sink.latest().map(|frame| frame.seq));
    }
    assert_ne!(seqs.first(), seqs.get(1), "each picture has its own number");
    // Only the newest is kept: the first was replaced before it was shown.
    assert_eq!(sink.snapshot().dropped, 1);
    assert_eq!(
        sink.take().map(|frame| frame.seq),
        seqs.get(1).copied().flatten()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_rtmp_join_answer_for_the_admin_keeps_the_server_url() {
    use crate::connect::group_calls::stream::rtmp_info_from_join_answer;
    let answer =
        r#"{"rtmp":true,"rtmp_stream_url":"rtmps://dc4-1.rtmp.t.me/s/","rtmp_stream_key":"1:abc"}"#;
    assert_eq!(
        rtmp_info_from_join_answer(answer),
        Some(("rtmps://dc4-1.rtmp.t.me/s/".into(), "1:abc".into()))
    );
    assert_eq!(
        rtmp_info_from_join_answer(r#"{"rtmp":true}"#),
        Some((String::new(), String::new()))
    );
    assert_eq!(rtmp_info_from_join_answer(r#"{"stream":true}"#), None);
    assert_eq!(rtmp_info_from_join_answer("not json"), None);

    let (dir, mut driver, _recorder, _handle, sink, seq) = ready_group_call_driver();
    let extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":{},"@extra":{}}}"#,
            serde_json::to_string(answer).expect("json"),
            extra.0
        ),
    );
    let call = driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .expect("tracked call");
    assert_eq!(call.rtmp_url.as_deref(), Some("rtmps://dc4-1.rtmp.t.me/s/"));
    assert_eq!(call.rtmp_stream_key.as_deref(), Some("1:abc"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_chosen_speaker_plays_the_stream_and_follows_a_change() {
    let (dir, mut driver, _recorder, handle, ..) = ready_group_call_driver();
    driver
        .select_call_devices(None, Some("usb-headset".into()))
        .expect("select before the call");
    handle.set_group_call_mode(51, GroupCallMode::Rtmp);
    driver.join_video_chat(555).expect("join tracked call");
    // The pick reaches the tracked call before the join (a change while
    // a join is on its way), and the join creates the call with it.
    assert_eq!(
        handle.group_playbacks(),
        vec![(555, Some("usb-headset".to_string()), false); 2],
        "the join creates the call with the chosen output"
    );
    if let Some(call) = driver.session.calls.active_group_call.as_mut() {
        call.is_joined = true;
    }
    driver
        .select_call_devices(None, Some("speakers".into()))
        .expect("switch output");
    driver
        .set_group_call_playback_muted(555, true)
        .expect("mute for me");
    assert!(driver.group_call_playback_muted(555));
    assert_eq!(
        handle.group_playbacks().get(2..).map(<[_]>::to_vec),
        Some(vec![
            (555, Some("speakers".to_string()), false),
            (555, Some("speakers".to_string()), true),
        ]),
        "a change and the local mute reach the joined call"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
