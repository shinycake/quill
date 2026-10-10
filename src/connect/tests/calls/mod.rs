//! Connect-driver tests: 1:1 calls.
use super::super::*;
use super::*;
use crate::calls::engine::CallEngine;
use crate::calls::engine::{EngineError, TransportState};
use crate::calls::engine::{
    MediaDevice, MediaDeviceKind, MockEngine, RemoteVideoState, VideoFrame,
};
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::AccountKey;
use crate::platform::MemorySecretStore;
use crate::state::{RequestPurpose, Session};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

mod media;

/// Phase C3a: `join_video_chat` must accept the tracked, unjoined call
/// (the normal flow: `getGroupCall` creates the tracker, then the
/// overlay's Join button calls this) while rejecting a missing,
/// mismatched, or already-joined call.
#[test]
fn join_video_chat_guard_allows_tracked_unjoined_call() {
    const UNJOINED_CALL: &str = r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Demo voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":false,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#;

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    // No tracked call: rejected.
    assert_invalid(driver.join_video_chat(555));

    // Track call 555 unjoined, as `getGroupCall` would: join allowed.
    driver
        .ingest(copy_and_parse(UNJOINED_CALL, &seq, &dyn_sink).unwrap())
        .unwrap();
    assert!(
        driver
            .session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|c| c.id == 555 && !c.is_joined)
    );
    let extra = driver
        .join_video_chat(555)
        .expect("join tracked unjoined call");
    let sent = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("joinVideoChat sent");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "joinVideoChat");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["group_call_id"], 555);

    // A different tracked call id is rejected…
    assert_invalid(driver.join_video_chat(777));
    // …and so is the same call once joined.
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .unwrap()
        .is_joined = true;
    assert_invalid(driver.join_video_chat(555));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_engine_lifecycle_bridge() {
    let (dir, mut driver, recorder, sink, seq) = call_driver();
    let mock = MockEngine::new();
    let handle = mock.clone();
    driver.set_call_engine(Box::new(mock));
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
    );
    assert_eq!(handle.started_calls(), vec![(77, 41, false)]);

    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateNewCallSignalingData","call_id":77,"data":"aW5ib3VuZA=="}"#,
    );
    assert_eq!(handle.signaling_received(), vec![(77, b"inbound".to_vec())]);

    driver.accept_call().unwrap();
    assert_eq!(handle.accepted_calls(), vec![77]);
    let accept = sent_request(&recorder, "acceptCall");
    assert_eq!(accept["protocol"]["udp_p2p"], true);
    assert_eq!(accept["protocol"]["udp_reflector"], true);
    assert_eq!(accept["protocol"]["min_layer"], 92);
    assert_eq!(accept["protocol"]["max_layer"], 92);
    assert_eq!(
        accept["protocol"]["library_versions"],
        serde_json::json!(["8.0.0", "9.0.0", "12.0.0", "13.0.0"])
    );

    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":false,"need_debug_information":false,"need_log":false}}}"#,
    );
    assert_eq!(handle.hung_up_calls(), vec![77]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_engine_emission_sends_signaling_data() {
    let (dir, mut driver, recorder, sink, seq) = call_driver();
    let mock = MockEngine::new();
    let handle = mock.clone();
    driver.set_call_engine(Box::new(mock));
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
    );

    handle.receive_signaling_data(77, b"emit-bytes");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
    );
    let sent = sent_request(&recorder, "sendCallSignalingData");
    assert_eq!(sent["call_id"], 77);
    assert_eq!(sent["data"], "ZW1pdC1ieXRlcw==");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn create_call_uses_engine_protocol_when_present() {
    let (dir, mut driver, recorder, sink, seq) = call_driver();
    driver.set_call_engine(Box::new(MockEngine::new()));
    seed_ready_call_user(&mut driver, &seq, &sink);

    driver.start_call(41, false).unwrap();
    assert_invalid(driver.start_call(41, false));
    let create = sent_request(&recorder, "createCall");
    assert_eq!(create["protocol"]["udp_p2p"], true);
    assert_eq!(create["protocol"]["udp_reflector"], true);
    assert_eq!(create["protocol"]["min_layer"], 92);
    assert_eq!(create["protocol"]["max_layer"], 92);
    assert_eq!(
        create["protocol"]["library_versions"],
        serde_json::json!(["8.0.0", "9.0.0", "12.0.0", "13.0.0"])
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn create_call_rejects_missing_audio_engine() {
    let (dir, mut driver, recorder, sink, seq) = call_driver();
    seed_ready_call_user(&mut driver, &seq, &sink);

    let before = recorder.snapshot().len();
    assert_invalid(driver.start_call(41, false));
    assert_eq!(
        recorder.snapshot().len(),
        before,
        "unavailable audio must not ring a recipient"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_engine_absent_keeps_signaling_only() {
    let (dir, mut driver, _recorder, sink, seq) = call_driver();
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateNewCallSignalingData","call_id":77,"data":"ZGlhZ25vc3RpYw=="}"#,
    );
    assert_eq!(
        driver
            .session
            .calls
            .active_call
            .as_ref()
            .unwrap()
            .signaling_queue,
        vec![b"diagnostic".to_vec()]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn create_call_rejects_unavailable_audio_engine() {
    let (dir, mut driver, recorder, sink, seq) = call_driver();
    driver.set_call_engine(Box::new(MockEngine::unavailable()));
    seed_ready_call_user(&mut driver, &seq, &sink);

    let before = recorder.snapshot().len();
    assert_invalid(driver.start_call(41, false));
    assert_eq!(
        recorder.snapshot().len(),
        before,
        "unavailable audio must not ring a recipient"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_engine_ignores_signaling_for_untracked_call() {
    let (dir, mut driver, _recorder, sink, seq) = call_driver();
    let mock = MockEngine::new();
    let handle = mock.clone();
    driver.set_call_engine(Box::new(mock));
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    // No tracked call: signaling must stay diagnostic-only.
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateNewCallSignalingData","call_id":77,"data":"aW5ib3VuZA=="}"#,
    );
    assert!(driver.session.calls.active_call.is_none());
    assert!(handle.signaling_received().is_empty());

    // Tracked call 77, then signaling for a different call id: gated.
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateNewCallSignalingData","call_id":78,"data":"aW5ib3VuZA=="}"#,
    );
    assert!(handle.signaling_received().is_empty());

    // Signaling for the tracked call still bridges.
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateNewCallSignalingData","call_id":77,"data":"aW5ib3VuZA=="}"#,
    );
    assert_eq!(handle.signaling_received(), vec![(77, b"inbound".to_vec())]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2c: Ready `updateCall` carrying transport material
/// (reflector + WebRTC servers, base64 key, `allow_p2p`).
pub(crate) const READY_CALL_JSON: &str = r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateReady","protocol":{"@type":"callProtocol","udp_p2p":true,"udp_reflector":true,"min_layer":92,"max_layer":92,"library_versions":["13.0.0"]},"servers":[{"@type":"callServer","id":"7","ip_address":"149.154.167.40","ipv6_address":"2001:b28:f23d:f001::a","port":443,"type":{"@type":"callServerTypeTelegramReflector","peer_tag":"AAEC","is_tcp":true}},{"@type":"callServer","id":"8","ip_address":"203.0.113.1","ipv6_address":"","port":3478,"type":{"@type":"callServerTypeWebrtc","username":"alice","password":"secret","supports_turn":true,"supports_stun":false}}],"config":"{}","custom_parameters":"{\"audio_codec\":\"opus\"}","encryption_key":"AQIDBA==","emojis":[],"allow_p2p":true}}}"#;

#[test]
fn call_ready_connects_transport_once_with_mapped_params() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    // Pre-select devices before the transport exists.
    driver
        .select_call_devices(Some("mic-a".into()), Some("spk-a".into()))
        .unwrap();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);

    let connects = handle.connects();
    assert_eq!(connects.len(), 1);
    let (call_id, params) = &connects[0];
    assert_eq!(*call_id, 77);
    assert_eq!(params.encryption_key, vec![1, 2, 3, 4]);
    assert!(params.is_outgoing);
    assert!(params.p2p_allowed);
    assert_eq!(params.library_versions, vec!["13.0.0"]);
    assert_eq!(params.custom_parameters, "{\"audio_codec\":\"opus\"}");
    assert_eq!(params.mic_input.as_deref(), Some("mic-a"));
    assert_eq!(params.speaker_input.as_deref(), Some("spk-a"));
    assert_eq!(params.servers.len(), 2);
    let reflector = &params.servers[0];
    assert_eq!(reflector.id, 7);
    assert_eq!(reflector.ipv4, "149.154.167.40");
    assert_eq!(reflector.ipv6, "2001:b28:f23d:f001::a");
    assert_eq!(reflector.port, 443);
    assert!(reflector.username.is_empty());
    assert!(reflector.turn);
    assert!(!reflector.stun);
    assert!(reflector.tcp);
    assert_eq!(reflector.peer_tag, vec![0, 1, 2]);
    let webrtc = &params.servers[1];
    assert_eq!(webrtc.id, 8);
    assert_eq!(webrtc.username, "alice");
    assert_eq!(webrtc.password, "secret");
    assert!(webrtc.turn);
    assert!(!webrtc.stun);
    assert!(!webrtc.tcp);
    assert!(webrtc.peer_tag.is_empty());

    let call = driver.session.calls.active_call.as_ref().unwrap();
    assert_eq!(call.transport, Some(TransportState::Connecting));
    assert_eq!(call.transport_error, None);

    // A second Ready update must not reconnect.
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    assert_eq!(handle.connects().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_ready_without_key_fails_transport_honestly() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    let no_key = READY_CALL_JSON.replace(r#""encryption_key":"AQIDBA==","#, "");
    ingest_call_json(&mut driver, &seq, &sink, &no_key);

    assert!(handle.connects().is_empty());
    let call = driver.session.calls.active_call.as_ref().unwrap();
    assert_eq!(call.transport, Some(TransportState::Failed));
    assert_eq!(
        call.transport_error.as_deref(),
        Some("call became ready without an encryption key")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_transport_callback_updates_session() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    handle.emit_transport_state(77, TransportState::Connected);
    // The next pump drains the transport outbox into the session.
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);

    let call = driver.session.calls.active_call.as_ref().unwrap();
    assert_eq!(call.transport, Some(TransportState::Connected));
    assert_eq!(call.transport_error, None);
    assert_eq!(handle.connects().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_transport_retries_same_params_three_times_then_stops() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    driver.set_call_muted(true).unwrap();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    let original = handle.connects()[0].1.clone();

    for expected_connects in 2..=4 {
        handle.emit_transport_state(77, TransportState::Failed);
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        assert_eq!(handle.connects().len(), expected_connects);
        assert_eq!(handle.connects().last().unwrap().1, original);
        assert_eq!(handle.mute_changes(), vec![(77, true); expected_connects]);
        assert_eq!(
            driver.session.calls.active_call.as_ref().unwrap().transport,
            Some(TransportState::Reconnecting)
        );
    }

    handle.emit_transport_state(77, TransportState::Failed);
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    assert_eq!(handle.connects().len(), 4);
    let call = driver.session.calls.active_call.as_ref().unwrap();
    assert_eq!(call.transport, Some(TransportState::Failed));
    assert_eq!(
        call.transport_error.as_deref(),
        Some("reconnect attempts exhausted")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_transport_connected_resets_reconnect_attempts() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    for _ in 0..2 {
        handle.emit_transport_state(77, TransportState::Failed);
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    }
    handle.emit_transport_state(77, TransportState::Connected);
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);

    for _ in 0..3 {
        handle.emit_transport_state(77, TransportState::Failed);
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    }
    assert_eq!(handle.connects().len(), 6);
    assert_eq!(
        driver.session.calls.active_call.as_ref().unwrap().transport,
        Some(TransportState::Reconnecting)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_transport_reconnect_error_is_reported() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    handle.fail_next_connect();
    handle.emit_transport_state(77, TransportState::Failed);
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);

    let call = driver.session.calls.active_call.as_ref().unwrap();
    assert_eq!(call.transport, Some(TransportState::Failed));
    assert_eq!(
        call.transport_error.as_deref(),
        Some("call engine operation connect failed with code -1")
    );
    assert_eq!(handle.connects().len(), 2);
    driver.session.calls.active_call.as_mut().unwrap().muted = true;
    handle.fail_mute();
    handle.emit_transport_state(77, TransportState::Failed);
    handle.emit_transport_state(77, TransportState::Connecting);
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    let call = driver.session.calls.active_call.as_ref().unwrap();
    assert_eq!(call.transport, Some(TransportState::Failed));
    assert!(call.transport_error.as_ref().unwrap().contains("set_muted"));
    assert_eq!(handle.hung_up_calls(), vec![77]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_debug_information_contains_real_driver_fields() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    driver
        .select_call_devices(Some("mic-a".into()), Some("spk-a".into()))
        .unwrap();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    handle.emit_transport_state(77, TransportState::Connected);
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":false,"need_debug_information":true,"need_log":false}}}"#,
    );

    let payload: Value = serde_json::from_str(&driver.call_debug_information().unwrap()).unwrap();
    assert_eq!(payload["app"], "quill");
    assert_eq!(payload["call_id"], 77);
    assert_eq!(payload["engine_available"], true);
    assert_eq!(payload["engine_protocol"]["min_layer"], 92);
    assert_eq!(
        payload["engine_protocol"]["library_versions"],
        serde_json::json!(["8.0.0", "9.0.0", "12.0.0", "13.0.0"])
    );
    assert_eq!(payload["final_transport_state"], "connected");
    assert_eq!(payload["had_audio"], true);
    assert_eq!(payload["microphone_device_id"], "mic-a");
    assert_eq!(payload["speaker_device_id"], "spk-a");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_mute_goes_through_engine_first() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    handle.emit_transport_state(77, TransportState::Connected);
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);

    assert!(driver.set_call_muted(true).is_ok());
    assert_eq!(handle.mute_changes(), vec![(77, true)]);
    assert!(driver.session.calls.active_call.as_ref().unwrap().muted);
    assert!(driver.set_call_muted(false).is_ok());
    assert_eq!(handle.mute_changes(), vec![(77, true), (77, false)]);
    assert!(!driver.session.calls.active_call.as_ref().unwrap().muted);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_mute_failure_leaves_state_unchanged() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    handle.emit_transport_state(77, TransportState::Connected);
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);

    handle.fail_mute();
    let err = driver.set_call_muted(true).unwrap_err();
    assert!(matches!(err, EngineError::Engine { .. }));
    assert!(!driver.session.calls.active_call.as_ref().unwrap().muted);
    assert!(handle.mute_changes().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_mute_without_active_call_is_no_active_call() {
    let (dir, mut driver, _recorder, _sink, _seq) = call_driver();
    driver.set_call_engine(Box::new(MockEngine::new()));
    assert_eq!(driver.set_call_muted(true), Err(EngineError::NoActiveCall));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_device_selection_stored_before_connect_forwarded_after() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    handle.set_devices(vec![crate::calls::engine::MediaDevice {
        id: "mic-a".into(),
        name: "Mic A".into(),
        kind: crate::calls::engine::MediaDeviceKind::Microphone,
    }]);
    // Pending, not Ready: stored only, never forwarded; install does
    // not enumerate either.
    driver
        .select_call_devices(Some("mic-a".into()), Some("spk-a".into()))
        .unwrap();
    assert_eq!(
        driver.selected_call_devices(),
        (Some("mic-a"), Some("spk-a"))
    );
    assert!(handle.device_selections().is_empty());
    assert!(driver.call_devices().is_empty());

    // Ready connects with the stored selection as stream inputs and
    // refreshes the device cache.
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    let connects = handle.connects();
    assert_eq!(connects.len(), 1);
    assert_eq!(connects[0].1.mic_input.as_deref(), Some("mic-a"));
    assert_eq!(connects[0].1.speaker_input.as_deref(), Some("spk-a"));
    assert_eq!(driver.call_devices().len(), 1);

    // After connect the selection forwards to the engine.
    driver
        .select_call_devices(Some("mic-b".into()), None)
        .unwrap();
    assert_eq!(
        handle.device_selections(),
        vec![(77, Some("mic-b".to_string()), None)]
    );
    assert_eq!(driver.selected_call_devices(), (Some("mic-b"), None));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_device_refresh_without_engine_is_best_effort() {
    let (dir, mut driver, _recorder, _sink, _seq) = call_driver();
    driver.refresh_call_devices();
    assert!(driver.call_devices().is_empty());
    assert_eq!(driver.selected_call_devices(), (None, None));
    let _ = std::fs::remove_dir_all(&dir);
}

impl JsonSender for Arc<FailFirstCallSender> {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        if request.contains("sendCallSignalingData")
            && *self.fail_next.lock().expect("failing sender")
        {
            *self.fail_next.lock().expect("failing sender") = false;
            return Err(ConnectSendError::Native);
        }
        self.sent
            .lock()
            .expect("failing sender")
            .push(request.to_string());
        Ok(())
    }
}

impl JsonSender for Arc<FailFirstInlineQuerySender> {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        if request.contains("getInlineQueryResults")
            && *self.fail_next.lock().expect("failing sender")
        {
            *self.fail_next.lock().expect("failing sender") = false;
            return Err(ConnectSendError::Native);
        }
        self.sent
            .lock()
            .expect("failing sender")
            .push(request.to_string());
        Ok(())
    }
}

#[test]
fn signaling_send_failure_requeues_and_cleans_request() {
    let (dir, mut driver, sender, sink, seq) = failing_call_driver();
    let mock = MockEngine::new();
    let handle = mock.clone();
    driver.set_call_engine(Box::new(mock));
    ingest_failing(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        )
        .unwrap();
    ingest_failing(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
        )
        .unwrap();

    handle.receive_signaling_data(77, b"emit-bytes");
    let signaling_sent = || {
        sender
            .snapshot()
            .into_iter()
            .filter(|json| json.contains("sendCallSignalingData"))
            .map(|json| serde_json::from_str::<Value>(&json).unwrap())
            .collect::<Vec<_>>()
    };
    assert!(signaling_sent().is_empty());

    let failed = ingest_failing(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
    );
    assert!(matches!(failed, Err(ConnectSendError::Native)));
    // The failed request was removed from bookkeeping and the unsent
    // bytes returned to the outbox: nothing sent, nothing lost.
    assert!(
        !driver
            .session
            .requests
            .has_purpose(RequestPurpose::SendCallSignalingData)
    );
    assert!(signaling_sent().is_empty());

    // The next ingest retries the same bytes exactly once.
    ingest_failing(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
    )
    .unwrap();
    let sent = signaling_sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["@type"], "sendCallSignalingData");
    assert_eq!(sent[0]["call_id"], 77);
    assert_eq!(sent[0]["data"], "ZW1pdC1ieXRlcw==");
    let _ = std::fs::remove_dir_all(&dir);
}

pub(crate) fn group_call_test_driver() -> (
    std::path::PathBuf,
    Arc<RecordingSender>,
    ConnectDriver<Arc<RecordingSender>>,
    AtomicU64,
) {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    (dir, recorder, driver, seq)
}
