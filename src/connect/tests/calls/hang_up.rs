//! The engine keeps a call until TDLib took its hang-up: a `discardCall`
//! that fails puts the call back with its sound, one TDLib took (its
//! `ok` or the terminal update) hangs the engine up once.
use super::*;
use crate::telegram::envelope::CallState;

fn answer(request: crate::ids::RequestId, json: &str) -> String {
    json.replace("EXTRA", &request.as_extra())
}

#[test]
fn a_hang_up_that_fails_keeps_the_engine_call() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    let request = driver.discard_call().expect("hang up");
    assert!(
        handle.hung_up_calls().is_empty(),
        "nothing hangs up before TDLib answers"
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &answer(
            request,
            r#"{"@type":"error","@extra":"EXTRA","code":400,"message":"CALL_PEER_INVALID"}"#,
        ),
    );
    assert!(handle.hung_up_calls().is_empty());
    let call = driver
        .session
        .calls
        .active_call
        .as_ref()
        .expect("the call goes on");
    assert!(matches!(call.state, CallState::Ready));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hang_up_tdlib_took_ends_the_engine_call_once() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    let request = driver.discard_call().expect("hang up");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &answer(request, r#"{"@type":"ok","@extra":"EXTRA"}"#),
    );
    assert_eq!(handle.hung_up_calls(), vec![77]);
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":false,"need_debug_information":false,"need_log":false}}}"#,
    );
    assert_eq!(handle.hung_up_calls(), vec![77]);
    assert!(driver.session.calls.active_call.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn closing_the_client_hangs_the_engine_up_at_once() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    driver.hang_up_for_close();
    assert_eq!(handle.hung_up_calls(), vec![77]);
    let _ = std::fs::remove_dir_all(&dir);
}
