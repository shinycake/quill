//! Driver tests: group-call native transport, video routing and screen sharing.
use super::*;
use crate::connect::SelfMuteError;

/// Phase C2g: the join carries the native offer (not the no-device
/// fallback) and the audio SSRC parsed from it.
#[test]
fn group_join_carries_engine_offer() {
    let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
    driver.join_video_chat(555).expect("join tracked call");
    assert_eq!(handle.group_offers(), vec![(555, 51)]);
    let join = sent_request(&recorder, "joinVideoChat");
    assert_eq!(join["join_parameters"]["payload"], "mock-group-offer-555");
    // The mock offer carries no `a=ssrc:` lines: honest 0.
    assert_eq!(join["join_parameters"]["audio_source_id"], 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g: the `joinVideoChat` Text answer finishes the native
/// handshake exactly once.
#[test]
fn group_join_answer_connects_native_transport() {
    let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
    let extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
            extra.0
        ),
    );
    assert_eq!(
        handle.group_connects(),
        vec![(555, "join-answer".to_string(), None)]
    );
    assert!(
        driver
            .session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.transport_ready)
    );
    // A second pump (no new answer) must not reconnect.
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":true,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#,
    );
    assert_eq!(handle.group_connects().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g (review fix): a rejoin resets the transport handshake —
/// the new `joinVideoChat` answer reconnects the fresh native context
/// instead of being dropped by the pump's `transport_ready` filter,
/// and an orphaned presentation is stopped.
#[test]
fn group_rejoin_answer_reconnects_native_transport() {
    let (dir, mut driver, recorder, mut handle, sink, seq) = ready_group_call_driver();
    let need_rejoin = r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":true,"need_rejoin":true,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#;
    // Join → answer → the native transport is connected.
    let extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
            extra.0
        ),
    );
    assert!(
        driver
            .session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.transport_ready)
    );
    assert_eq!(handle.group_connects().len(), 1);
    // A live presentation plus tracked screen-share state: the
    // rejoin must stop the orphaned native presentation and clear
    // the stale flags.
    handle.start_screen_share(555).expect("mock presentation");
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .screen_sharing = true;
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .screen_share_answer = "old-answer".into();
    // `need_rejoin` auto-rejoins with a fresh offer and a reset
    // handshake gate.
    ingest_call_json(&mut driver, &seq, &sink, need_rejoin);
    assert_eq!(handle.group_offers().len(), 2);
    assert_eq!(handle.screen_share_stops(), vec![555]);
    let call = driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .expect("tracked call");
    assert!(!call.transport_ready);
    assert!(call.join_payload.is_empty());
    assert!(!call.screen_sharing && !call.screen_share_pending);
    assert!(call.screen_share_answer.is_empty());
    // The new answer reconnects the fresh native context.
    let extra = sent_request(&recorder, "joinVideoChat")["@extra"]
        .as_str()
        .unwrap()
        .to_string();
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"text","text":"rejoin-answer","@extra":"{extra}"}}"#),
    );
    assert_eq!(handle.group_connects().len(), 2);
    assert!(
        driver
            .session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.transport_ready)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g: participant `video_info` / `screen_sharing_video_info`
/// become engine subscriptions; paused endpoints and the local user
/// are skipped.
#[test]
fn group_participant_video_syncs_subscriptions() {
    let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
    let extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
            extra.0
        ),
    );
    let camera = r#"{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[111,112]}],"endpoint_id":"ep-42","is_paused":false}"#;
    let screen = r#"{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[222]}],"endpoint_id":"ep-42-screen","is_paused":false}"#;
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &group_participant_json(42, false, camera, screen),
    );
    // Paused endpoint: skipped.
    let paused = r#"{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[333]}],"endpoint_id":"ep-43","is_paused":true}"#;
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &group_participant_json(43, false, paused, "null"),
    );
    // Local user: skipped even with video info.
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &group_participant_json(777, true, camera, "null"),
    );
    let syncs = handle.group_video_syncs();
    let (call_id, sources) = syncs.last().expect("sync recorded");
    assert_eq!(*call_id, 555);
    assert_eq!(sources.len(), 2);
    let camera_source = sources
        .iter()
        .find(|source| source.endpoint == "ep-42")
        .expect("camera source");
    assert_eq!(camera_source.user_id, 42);
    assert_eq!(camera_source.ssrc_groups.len(), 1);
    assert_eq!(camera_source.ssrc_groups[0].semantics, "SIM");
    assert_eq!(camera_source.ssrc_groups[0].ssrcs, vec![111, 112]);
    let screen_source = sources
        .iter()
        .find(|source| source.endpoint == "ep-42-screen")
        .expect("screen source");
    assert_eq!(screen_source.user_id, 42);
    assert_eq!(screen_source.ssrc_groups[0].ssrcs, vec![222]);
    let _ = std::fs::remove_dir_all(&dir);
}

fn planes() -> LiveFrame {
    LiveFrame::new(0, 2, 2, LiveRotation::None, vec![0; 6], Instant::now()).expect("frame")
}

fn video(user_id: i64, screen: bool) -> LiveKey {
    LiveKey::Video {
        call_id: 555,
        user_id,
        screen,
    }
}

/// Call 555 as TDLib reports it, with your camera on or off.
fn my_video_json(on: bool) -> String {
    r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":false,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":ON,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#
        .replace("ON", if on { "true" } else { "false" })
}

/// Participant 42's camera and screen on, and your camera on.
fn turn_videos_on(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &std::sync::atomic::AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
) {
    ingest_call_json(
        driver,
        seq,
        sink,
        &group_participant_json(42, false, CAMERA_INFO, SCREEN_INFO),
    );
    ingest_call_json(driver, seq, sink, &my_video_json(true));
}

/// Phase C2g: a participant's camera and screen each get their own live
/// feed, never the 1:1 slots; a frame for a track nobody asked for, or
/// one with no video on, makes no feed.
#[test]
fn group_video_frames_route_to_participant_feeds() {
    let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
    handle.emit_live_frame(video(42, false), planes());
    turn_videos_on(&mut driver, &seq, &sink);
    let camera = driver.live_sink(video(42, false)).expect("camera feed");
    assert!(camera.latest().is_none(), "the early frame made no feed");
    let screen = driver.live_sink(video(42, true)).expect("screen feed");
    handle.emit_live_frame(video(42, false), planes());
    handle.emit_live_frame(video(42, true), planes());
    handle.emit_live_frame(video(43, false), planes());
    assert!(camera.latest().is_some());
    assert!(screen.latest().is_some());
    assert!(
        driver.live_sink(video(43, false)).is_none(),
        "43 has no video"
    );
    // A group call's frames never reach a 1:1 call's feed of the same id.
    assert!(
        driver
            .live_sink(LiveKey::Call {
                call_id: 555,
                local: false,
                screen: false
            })
            .is_none()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice calls-group-self-tile: your camera preview in a group call goes
/// to the call's local feed, not a 1:1 call's or a participant's feed.
#[test]
fn group_local_frame_routes_to_the_local_feed() {
    let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
    turn_videos_on(&mut driver, &seq, &sink);
    let local = driver
        .live_sink(LiveKey::Local { call_id: 555 })
        .expect("feed");
    let camera = driver.live_sink(video(42, false)).expect("feed");
    handle.emit_live_frame(LiveKey::Local { call_id: 555 }, planes());
    assert!(local.latest().is_some());
    assert!(camera.latest().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice calls-group-self-tile: leaving the group call ends the self
/// tile's feed along with the participants'.
#[test]
fn group_call_leave_ends_the_local_feed() {
    let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
    turn_videos_on(&mut driver, &seq, &sink);
    let local = driver
        .live_sink(LiveKey::Local { call_id: 555 })
        .expect("feed");
    handle.emit_live_frame(LiveKey::Local { call_id: 555 }, planes());
    driver.leave_group_call().expect("leave");
    assert_eq!(local.snapshot().end, Some(LiveEnd::Left));
    assert!(driver.live_sink(LiveKey::Local { call_id: 555 }).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g: leaving tears the native transport down and ends the
/// call's feeds.
#[test]
fn group_call_leave_tears_down_transport_and_frames() {
    let (dir, mut driver, _recorder, mut handle, sink, seq) = ready_group_call_driver();
    turn_videos_on(&mut driver, &seq, &sink);
    let camera = driver.live_sink(video(42, false)).expect("feed");
    handle.emit_live_frame(video(42, false), planes());
    assert!(camera.latest().is_some());
    // Review fix: a live presentation is stopped before the call
    // (privacy — capture ends first).
    handle.start_screen_share(555).expect("mock presentation");
    driver.leave_group_call().expect("leave");
    assert_eq!(handle.group_leaves(), vec![555]);
    assert_eq!(handle.screen_share_stops(), vec![555]);
    assert_eq!(camera.snapshot().end, Some(LiveEnd::Left));
    assert!(driver.live_sink(video(42, false)).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A camera or screen TDLib reports off ends its feed at once; turned on
/// again it gets a new feed, with no frame from before.
#[test]
fn a_video_turned_off_ends_its_feed_and_comes_back_clean() {
    let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
    turn_videos_on(&mut driver, &seq, &sink);
    let camera = driver.live_sink(video(42, false)).expect("camera");
    let screen = driver.live_sink(video(42, true)).expect("screen");
    let local = driver
        .live_sink(LiveKey::Local { call_id: 555 })
        .expect("local");
    handle.emit_live_frame(video(42, false), planes());
    assert!(camera.latest().is_some());
    // 42 turns the camera off, keeping the screen.
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &group_participant_json(42, false, "null", SCREEN_INFO),
    );
    assert_eq!(camera.snapshot().end, Some(LiveEnd::Left));
    assert_eq!(screen.snapshot().end, None, "the screen goes on");
    assert!(driver.live_sink(video(42, false)).is_none());
    // A late frame from the engine brings nothing back.
    handle.emit_live_frame(video(42, false), planes());
    assert!(driver.live_sink(video(42, false)).is_none());
    // On again: a new feed, empty until the next frame.
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &group_participant_json(42, false, CAMERA_INFO, SCREEN_INFO),
    );
    let again = driver.live_sink(video(42, false)).expect("camera again");
    assert!(!again.same_as(&camera));
    assert!(again.latest().is_none());
    // Your camera off: the self tile's feed ends too.
    ingest_call_json(&mut driver, &seq, &sink, &my_video_json(false));
    assert_eq!(local.snapshot().end, Some(LiveEnd::Left));
    assert!(driver.live_sink(LiveKey::Local { call_id: 555 }).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g: screen-share toggle runs the presentation handshake —
/// offer into `startGroupCallScreenSharing`, answer into the engine,
/// stop pairs `endGroupCallScreenSharing` with the engine stop.
#[test]
fn group_screen_share_toggle_handshake() {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    // The engine enumerates a screen source; the driver picks it up
    // on connect.
    handle.set_devices(vec![MediaDevice {
        id: "screen-0".into(),
        name: "Test screen".into(),
        kind: MediaDeviceKind::Screen,
    }]);
    // Join and finish the native handshake first, as in reality.
    let join_extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
            join_extra.0
        ),
    );
    assert!(
        driver
            .session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.transport_ready)
    );
    // TDLib confirms the join via `updateGroupCall is_joined`; the
    // toggle gates on it.
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .is_joined = true;
    let start_extra = driver
        .toggle_group_call_screen_share()
        .expect("start sharing");
    assert_eq!(handle.screen_share_offers(), vec![555]);
    let start = sent_request(&recorder, "startGroupCallScreenSharing");
    assert_eq!(start["group_call_id"], 555);
    assert_eq!(start["payload"], "mock-presentation-offer-555");
    assert!(
        driver
            .session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.screen_share_pending && !call.screen_sharing)
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"share-answer","@extra":{}}}"#,
            start_extra.0
        ),
    );
    assert_eq!(
        handle.screen_share_connects(),
        vec![(555, "share-answer".to_string())]
    );
    assert!(
        driver
            .session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.screen_sharing && !call.screen_share_pending)
    );
    driver
        .toggle_group_call_screen_share()
        .expect("stop sharing");
    let end = sent_request(&recorder, "endGroupCallScreenSharing");
    assert_eq!(end["group_call_id"], 555);
    assert_eq!(handle.screen_share_stops(), vec![555]);
    assert!(
        driver
            .session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|call| !call.screen_sharing && !call.screen_share_pending)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g: a failed screen-share handshake (the reducer clears
/// the tracked flags on a `startGroupCallScreenSharing` error)
/// leaves no stray native presentation: the pump stops it.
#[test]
fn group_screen_share_failure_stops_native_presentation() {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    handle.set_devices(vec![MediaDevice {
        id: "screen-0".into(),
        name: "Test screen".into(),
        kind: MediaDeviceKind::Screen,
    }]);
    let join_extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
            join_extra.0
        ),
    );
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .is_joined = true;
    driver
        .toggle_group_call_screen_share()
        .expect("start sharing");
    assert!(handle.presentation_active(555));
    // Simulate the reducer's error arm: flags cleared, error
    // surfaced, native presentation untouched.
    if let Some(call) = driver.session.calls.active_group_call.as_mut() {
        call.screen_share_pending = false;
        call.screen_sharing = false;
        call.screen_share_answer.clear();
    }
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":true,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#,
    );
    assert_eq!(handle.screen_share_stops(), vec![555]);
    assert!(!handle.presentation_active(555));
    assert!(recorder.snapshot().len() >= 2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g: without an enumerated screen source the toggle is
/// rejected and no request goes out ("No screen source available").
#[test]
fn group_screen_share_rejected_without_screen_source() {
    let (dir, mut driver, recorder, _handle, _sink, _seq) = ready_group_call_driver();
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .is_joined = true;
    assert!(driver.toggle_group_call_screen_share().is_err());
    assert!(
        !driver
            .session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.screen_share_pending || call.screen_sharing)
    );
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|json| !json.contains("startGroupCallScreenSharing"))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Muting yourself in a voice chat silences the microphone and tells
/// Telegram, so everyone sees it (it used to only flip a local flag).
#[test]
fn group_call_self_mute_reaches_the_microphone_and_telegram() {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &group_participant_json(7, true, "null", "null"),
    );
    driver.toggle_group_call_self_mute().expect("mute");
    assert_eq!(handle.group_mutes(), vec![(555, true)]);
    let sent = sent_request(&recorder, "toggleGroupCallParticipantIsMuted");
    assert_eq!(sent["participant_id"]["user_id"], 7);
    assert_eq!(sent["is_muted"], true);
    driver.toggle_group_call_self_mute().expect("mute");
    assert_eq!(handle.group_mutes(), vec![(555, true), (555, false)]);
    let _ = std::fs::remove_dir_all(&dir);
}

const JOINED_GROUP_CALL: &str = r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":true,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#;

fn is_muted_self(driver: &ConnectDriver<Arc<RecordingSender>>) -> bool {
    driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .is_some_and(|call| call.is_muted_self)
}

/// Unmuting is engine-first: a failing engine leaves you muted, nothing is
/// sent to Telegram, and the error reaches the caller (the UI shows it as
/// the status note).
#[test]
fn group_call_unmute_failure_leaves_you_muted() {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, JOINED_GROUP_CALL);
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &group_participant_json(7, true, "null", "null"),
    );
    driver.set_group_call_self_mute(true).expect("mute");
    handle.fail_group_mute();
    let sent_before = recorder.snapshot().len();
    let result = driver.toggle_group_call_self_mute();
    assert!(matches!(result, Err(SelfMuteError::Engine(_))));
    assert!(is_muted_self(&driver), "a failed unmute must stay muted");
    assert_eq!(recorder.snapshot().len(), sent_before, "nothing sent");
    let _ = std::fs::remove_dir_all(&dir);
}

/// When Telegram says you are muted (an admin, or another device) the
/// button follows the server and the engine is muted to match; when it
/// later says unmuted, the engine opens again.
#[test]
fn group_call_server_mute_settles_the_button_and_the_engine() {
    let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
    let extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(&mut driver, &seq, &sink, JOINED_GROUP_CALL);
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
            extra.0
        ),
    );
    let me = group_participant_json(7, true, "null", "null");
    let muted_by_admin = me.replace(
        r#""is_muted_for_all_users":false"#,
        r#""is_muted_for_all_users":true"#,
    );
    ingest_call_json(&mut driver, &seq, &sink, &me);
    assert!(!is_muted_self(&driver));
    ingest_call_json(&mut driver, &seq, &sink, &muted_by_admin);
    assert!(is_muted_self(&driver));
    assert_eq!(handle.group_mutes().last(), Some(&(555, true)));
    ingest_call_json(&mut driver, &seq, &sink, &me);
    assert!(!is_muted_self(&driver));
    assert_eq!(handle.group_mutes().last(), Some(&(555, false)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Push-to-talk press and release go through the same engine-first path.
#[test]
fn group_call_push_to_talk_drives_the_engine_first() {
    let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, JOINED_GROUP_CALL);
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &group_participant_json(7, true, "null", "null"),
    );
    driver.set_group_call_self_mute(true).expect("hold closed");
    driver.set_group_call_self_mute(false).expect("press");
    assert_eq!(handle.group_mutes(), vec![(555, true), (555, false)]);
    assert!(!is_muted_self(&driver));
    handle.fail_group_mute();
    assert!(driver.set_group_call_self_mute(true).is_err());
    assert!(is_muted_self(&driver), "a failed release still closes it");
    let _ = std::fs::remove_dir_all(&dir);
}

fn join_tracked_call(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &std::sync::atomic::AtomicU64,
    sink: &Arc<dyn crate::diagnostics::DiagnosticSink>,
) {
    let extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(driver, seq, sink, JOINED_GROUP_CALL);
    ingest_call_json(
        driver,
        seq,
        sink,
        &format!(
            r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
            extra.0
        ),
    );
}

fn transport_error(driver: &ConnectDriver<Arc<RecordingSender>>) -> Option<String> {
    driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .and_then(|call| call.transport_error.clone())
}

/// Muting fails closed: when the engine refuses, the flag still says
/// muted (the capture gate stops the audio), Telegram is told, the error
/// is reported, and the pump keeps retrying the engine, recording the
/// failure once rather than every tick.
#[test]
fn group_call_mute_failure_fails_closed_and_the_pump_retries() {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    join_tracked_call(&mut driver, &seq, &sink);
    let me = group_participant_json(7, true, "null", "null");
    ingest_call_json(&mut driver, &seq, &sink, &me);
    handle.fail_group_mute();
    let result = driver.set_group_call_self_mute(true);
    assert!(matches!(result, Err(SelfMuteError::Engine(_))));
    assert!(is_muted_self(&driver), "the microphone must not stay live");
    let sent = sent_request(&recorder, "toggleGroupCallParticipantIsMuted");
    assert_eq!(sent["is_muted"], true);
    // The pump retries on later updates; the failure is recorded once.
    ingest_call_json(&mut driver, &seq, &sink, &me);
    let first = transport_error(&driver);
    assert!(first.is_some(), "the retry failure is surfaced");
    if let Some(call) = driver.session.calls.active_group_call.as_mut() {
        call.transport_error = Some("kept".into());
    }
    ingest_call_json(&mut driver, &seq, &sink, &me);
    assert_eq!(transport_error(&driver).as_deref(), Some("kept"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// An own-participant update that was already in flight when you muted
/// (it still says unmuted) must not reopen the microphone. Once the toggle
/// request completed, the server's word settles the flag again.
#[test]
fn group_call_stale_own_update_does_not_undo_a_local_mute() {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    join_tracked_call(&mut driver, &seq, &sink);
    let me = group_participant_json(7, true, "null", "null");
    ingest_call_json(&mut driver, &seq, &sink, &me);
    driver.set_group_call_self_mute(true).expect("mute");
    assert!(is_muted_self(&driver));
    ingest_call_json(&mut driver, &seq, &sink, &me);
    assert!(is_muted_self(&driver), "a stale update must not unmute");
    assert_eq!(handle.group_mutes().last(), Some(&(555, true)));
    let toggle = sent_request(&recorder, "toggleGroupCallParticipantIsMuted");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":{}}}"#, toggle["@extra"]),
    );
    ingest_call_json(&mut driver, &seq, &sink, &me);
    assert!(!is_muted_self(&driver), "settled once the request is done");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Nobody can unmute themselves while an admin holds them muted
/// (`can_unmute_self` false, which includes a raised hand).
#[test]
fn group_call_unmute_is_refused_while_force_muted() {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    join_tracked_call(&mut driver, &seq, &sink);
    let held = group_participant_json(7, true, "null", "null").replace(
        r#""is_muted_for_all_users":false"#,
        r#""is_muted_for_all_users":true"#,
    );
    ingest_call_json(&mut driver, &seq, &sink, &held);
    assert!(is_muted_self(&driver));
    let sent_before = recorder.snapshot().len();
    let mutes_before = handle.group_mutes();
    let result = driver.set_group_call_self_mute(false);
    assert!(matches!(result, Err(SelfMuteError::NotAllowed)));
    assert!(is_muted_self(&driver));
    assert_eq!(handle.group_mutes(), mutes_before, "the engine stays shut");
    assert_eq!(recorder.snapshot().len(), sent_before);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A server force-mute that lands while a local toggle is in flight is held
/// back from `is_muted_self` (the toggle is newer), but the microphone must
/// still close: the engine follows `is_muted_self || forced`.
#[test]
fn group_call_force_mute_during_an_in_flight_toggle_mutes_the_engine() {
    let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
    join_tracked_call(&mut driver, &seq, &sink);
    let me = group_participant_json(7, true, "null", "null");
    ingest_call_json(&mut driver, &seq, &sink, &me);
    driver.set_group_call_self_mute(true).expect("mute");
    driver.set_group_call_self_mute(false).expect("unmute");
    assert_eq!(handle.group_mutes().last(), Some(&(555, false)));
    let held = me.replace(
        r#""is_muted_for_all_users":false"#,
        r#""is_muted_for_all_users":true"#,
    );
    ingest_call_json(&mut driver, &seq, &sink, &held);
    assert!(
        !is_muted_self(&driver),
        "the in-flight toggle still owns the flag"
    );
    assert_eq!(
        handle.group_mutes().last(),
        Some(&(555, true)),
        "the engine follows the admin hold"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A toggle request TDLib never answers is swept after the TTL, so own-row
/// updates settle `is_muted_self` again instead of staying held forever.
#[test]
fn group_call_unanswered_mute_toggle_is_swept_and_settling_resumes() {
    let (dir, mut driver, _recorder, _handle, sink, seq) = ready_group_call_driver();
    join_tracked_call(&mut driver, &seq, &sink);
    let me = group_participant_json(7, true, "null", "null");
    ingest_call_json(&mut driver, &seq, &sink, &me);
    driver.set_group_call_self_mute(true).expect("mute");
    ingest_call_json(&mut driver, &seq, &sink, &me);
    assert!(is_muted_self(&driver), "held while the request is pending");
    driver.session.connection = crate::telegram::envelope::ConnectionState::Ready;
    let later = std::time::Instant::now() + crate::state::PENDING_REQUEST_TTL;
    assert!(
        driver
            .session
            .sweep_stale_requests(later, crate::state::PENDING_REQUEST_TTL, |_| false)
    );
    ingest_call_json(&mut driver, &seq, &sink, &me);
    assert!(!is_muted_self(&driver), "settled after the sweep");
    let _ = std::fs::remove_dir_all(&dir);
}
