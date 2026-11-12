//! r2-14, 02-059: "Show more" asks for one page at a time, and a refusal
//! leaves the button able to ask again.
use super::*;
use crate::state::ParticipantPaging;

fn loads(recorder: &RecordingSender) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains("\"@type\":\"loadGroupCallParticipants\""))
        .count()
}

#[test]
fn show_more_asks_one_page_at_a_time_and_retries_after_a_refusal() {
    let (dir, mut driver, recorder, _engine, sink, seq) = ready_group_call_driver();
    let first = driver.load_more_group_call_participants().unwrap();
    let second = driver.load_more_group_call_participants().unwrap();
    assert_eq!(first, second, "a second click answers with the first");
    assert_eq!(loads(&recorder), 1);
    let paging = |driver: &ConnectDriver<_>| {
        driver
            .session
            .calls
            .active_group_call
            .as_ref()
            .map(|call| call.paging)
    };
    assert_eq!(paging(&driver), Some(ParticipantPaging::Loading(first)));
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"boom"}}"#,
            first.0
        ),
    );
    assert_eq!(paging(&driver), Some(ParticipantPaging::Failed));
    assert!(driver.session.calls.group_call_error.is_some());
    let retry = driver.load_more_group_call_participants().unwrap();
    assert_ne!(retry, first);
    assert_eq!(loads(&recorder), 2);
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, retry.0),
    );
    assert_eq!(paging(&driver), Some(ParticipantPaging::Idle));
    let _ = std::fs::remove_dir_all(&dir);
}
