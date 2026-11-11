//! The call-end card's uploads: one request at a time, and an answer
//! lands only on the upload it was for.
use super::*;
use crate::state::CallUpload;

const DISCARDED_WITH_LOG: &str = r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":false,"need_debug_information":false,"need_log":true}}}"#;

fn answer(request: crate::ids::RequestId, json: &str) -> String {
    json.replace("EXTRA", &request.as_extra())
}

#[test]
fn the_call_log_goes_out_once_and_a_failure_offers_it_again() {
    let (dir, mut driver, _handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, DISCARDED_WITH_LOG);
    let request = driver.send_call_log().expect("first send");
    assert!(driver.send_call_log().is_err(), "one upload in flight");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &answer(
            request,
            r#"{"@type":"error","@extra":"EXTRA","code":500,"message":"X"}"#,
        ),
    );
    let summary = driver.session.calls.summary.as_ref().expect("summary");
    assert!(matches!(summary.log, CallUpload::Failed(_)));
    assert!(summary.log.can_send());
    let retry = driver.send_call_log().expect("retry after the failure");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &answer(retry, r#"{"@type":"ok","@extra":"EXTRA"}"#),
    );
    let summary = driver.session.calls.summary.as_ref().expect("summary");
    assert_eq!(summary.log, CallUpload::Sent);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_answer_for_another_calls_upload_is_dropped() {
    let (dir, mut driver, _handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, DISCARDED_WITH_LOG);
    let request = driver.send_call_log().expect("send");
    // Another call ended meanwhile: its summary has no upload yet.
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &DISCARDED_WITH_LOG.replace(r#""id":77"#, r#""id":78"#),
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &answer(request, r#"{"@type":"ok","@extra":"EXTRA"}"#),
    );
    let summary = driver.session.calls.summary.as_ref().expect("summary");
    assert_eq!(summary.call_id, 78);
    assert_eq!(summary.log, CallUpload::Idle);
    let _ = std::fs::remove_dir_all(&dir);
}
