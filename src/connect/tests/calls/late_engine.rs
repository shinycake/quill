//! A call that rings on an account in the background: its engine is
//! loaded on demand, after the call is already tracked.
use super::*;
use crate::notify_accounts::ringing_incoming_call;

const RINGING: &str = r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":true,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#;

#[test]
fn an_engine_installed_while_a_call_rings_starts_that_call() {
    let (dir, mut driver, _recorder, sink, seq) = call_driver();
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    ingest_call_json(&mut driver, &seq, &sink, RINGING);
    assert_eq!(ringing_incoming_call(&driver.session), Some((77, 41, true)));
    let mock = MockEngine::new();
    let handle = mock.clone();
    driver.set_call_engine(Box::new(mock));
    assert!(handle.started_calls().is_empty(), "nothing started yet");
    driver.start_engine_for_tracked_call();
    assert_eq!(handle.started_calls(), vec![(77, 41, false)]);
    // Now it can be answered.
    driver.accept_call().unwrap();
    assert_eq!(handle.accepted_calls(), vec![77]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn declining_a_background_call_stops_the_ring() {
    let (dir, mut driver, recorder, sink, seq) = call_driver();
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    ingest_call_json(&mut driver, &seq, &sink, RINGING);
    driver.discard_call().unwrap();
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|json| json.contains("discardCall"))
    );
    assert_eq!(ringing_incoming_call(&driver.session), None);
    // No engine: starting the tracked call is a no-op.
    driver.start_engine_for_tracked_call();
    let _ = std::fs::remove_dir_all(&dir);
}
