//! Driver tests: group-call native transport, video routing and screen sharing.
use super::*;

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
        vec![(555, "join-answer".to_string(), false)]
    );
    assert!(
        driver
            .session
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
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .screen_sharing = true;
    driver
        .session
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

/// Phase C2g: frames with `participant_user_id` land in the group
/// slots (camera vs screen), never in the 1:1 slots.
#[test]
fn group_video_frames_route_to_participant_slots() {
    let (dir, driver, _recorder, handle, _sink, _seq) = ready_group_call_driver();
    let frame = |screen: bool| VideoFrame {
        seq: 0,
        width: 2,
        height: 2,
        rgba: vec![0u8; 16],
        is_local: false,
        participant_user_id: Some(42),
        is_screen: screen,
    };
    handle.emit_video_frame(555, frame(false));
    handle.emit_video_frame(555, frame(true));
    assert!(driver.latest_group_video_frame(555, 42, false).is_some());
    assert!(driver.latest_group_video_frame(555, 42, true).is_some());
    assert!(driver.latest_group_video_frame(555, 43, false).is_none());
    assert!(driver.latest_video_frame(555, false).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice calls-group-self-tile: a group-local frame (`is_local`,
/// no participant — what the engine now delivers for group
/// CAPTURE frames) lands in the shared (call id, is_local) slots,
/// never in the participant slots.
#[test]
fn group_local_frame_routes_to_shared_local_slot() {
    let (dir, driver, _recorder, handle, _sink, _seq) = ready_group_call_driver();
    handle.emit_video_frame(
        555,
        VideoFrame {
            seq: 0,
            width: 2,
            height: 2,
            rgba: vec![0u8; 16],
            is_local: true,
            participant_user_id: None,
            is_screen: false,
        },
    );
    assert!(driver.latest_video_frame(555, true).is_some());
    assert!(driver.latest_video_frame(555, false).is_none());
    assert!(driver.latest_group_video_frame(555, 42, false).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice calls-group-self-tile: leaving the group call clears the
/// self-tile slot along with the participant slots.
#[test]
fn group_call_leave_clears_local_frame_slot() {
    let (dir, mut driver, _recorder, handle, _sink, _seq) = ready_group_call_driver();
    handle.emit_video_frame(
        555,
        VideoFrame {
            seq: 0,
            width: 2,
            height: 2,
            rgba: vec![0u8; 16],
            is_local: true,
            participant_user_id: None,
            is_screen: false,
        },
    );
    assert!(driver.latest_video_frame(555, true).is_some());
    driver.leave_group_call().expect("leave");
    assert!(driver.latest_video_frame(555, true).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g: leaving tears the native transport down and drops the
/// retained frames.
#[test]
fn group_call_leave_tears_down_transport_and_frames() {
    let (dir, mut driver, _recorder, mut handle, _sink, _seq) = ready_group_call_driver();
    handle.emit_video_frame(
        555,
        VideoFrame {
            seq: 0,
            width: 2,
            height: 2,
            rgba: vec![0u8; 16],
            is_local: false,
            participant_user_id: Some(42),
            is_screen: false,
        },
    );
    assert!(driver.latest_group_video_frame(555, 42, false).is_some());
    // Review fix: a live presentation is stopped before the call
    // (privacy — capture ends first).
    handle.start_screen_share(555).expect("mock presentation");
    driver.leave_group_call().expect("leave");
    assert_eq!(handle.group_leaves(), vec![555]);
    assert_eq!(handle.screen_share_stops(), vec![555]);
    assert!(driver.latest_group_video_frame(555, 42, false).is_none());
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
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.transport_ready)
    );
    // TDLib confirms the join via `updateGroupCall is_joined`; the
    // toggle gates on it.
    driver
        .session
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
    if let Some(call) = driver.session.active_group_call.as_mut() {
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
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .is_joined = true;
    assert!(driver.toggle_group_call_screen_share().is_err());
    assert!(
        !driver
            .session
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
    driver.toggle_group_call_self_mute();
    assert_eq!(handle.group_mutes(), vec![(555, true)]);
    let sent = sent_request(&recorder, "toggleGroupCallParticipantIsMuted");
    assert_eq!(sent["participant_id"]["user_id"], 7);
    assert_eq!(sent["is_muted"], true);
    driver.toggle_group_call_self_mute();
    assert_eq!(handle.group_mutes(), vec![(555, true), (555, false)]);
    let _ = std::fs::remove_dir_all(&dir);
}
