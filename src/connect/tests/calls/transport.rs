//! The 1:1 call transport as one value, and post-ingest pumps that run
//! independently of each other.
use super::*;

const PENDING_CALL: &str = r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#;

#[test]
fn an_engine_start_error_never_shows_as_connecting() {
    let (dir, mut driver, _recorder, sink, seq) = call_driver();
    driver.set_call_engine(Box::new(MockEngine::unavailable()));
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    ingest_call_json(&mut driver, &seq, &sink, PENDING_CALL);
    let unavailable = EngineError::Unavailable.to_string();
    let call = driver.session.calls.active_call.as_ref().unwrap();
    assert_eq!(call.transport.state(), None, "not connected yet");
    assert_eq!(call.transport.error(), Some(unavailable.as_str()));

    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    let call = driver.session.calls.active_call.as_ref().unwrap();
    assert_eq!(call.transport.state(), Some(TransportState::Failed));
    assert_eq!(call.transport.error(), Some(unavailable.as_str()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_call_signaling_send_failure_still_runs_the_later_pumps() {
    let (dir, mut driver, sender, sink, seq) = failing_call_driver();
    let mock = MockEngine::new();
    let handle = mock.clone();
    driver.set_call_engine(Box::new(mock));
    ingest_failing(&mut driver, &seq, &sink, PENDING_CALL).unwrap();
    handle.receive_signaling_data(77, b"emit-bytes");

    // The call pump fails to relay the bytes; the parameters pump after
    // it must still send `setTdlibParameters` for this update.
    let failed = ingest_failing(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitTdlibParameters"}}"#,
    );
    assert!(matches!(failed, Err(ConnectSendError::Native)));
    assert!(driver.parameters_sent());
    assert!(
        sender
            .snapshot()
            .iter()
            .any(|json| json.contains("setTdlibParameters"))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_summary_keeps_the_reconnects_since_the_last_connect() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    for _ in 0..2 {
        handle.emit_transport_state(77, TransportState::Failed);
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    }
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":false,"need_debug_information":false,"need_log":false}}}"#,
    );
    let summary = driver.session.calls.summary.as_ref().unwrap();
    assert_eq!(summary.reconnect_attempts, 2);
    assert_eq!(summary.final_transport, Some(TransportState::Reconnecting));
    let _ = std::fs::remove_dir_all(&dir);
}
