//! The voters pages of a poll are matched to the request that asked: a
//! reload's stage is not filled by an older page's answer.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::{PollVotersFetch, VotersMore};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn seed_poll(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
) {
    let option = |id: &str| {
        format!(
            r#"{{"@type":"pollOption","id":"{id}","text":{{"@type":"formattedText","text":"{id}","entities":[]}},"voter_count":1,"vote_percentage":50,"is_chosen":false}}"#
        )
    };
    let message = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":108,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messagePoll","poll":{{"@type":"poll","id":9004,"question":{{"@type":"formattedText","text":"Q","entities":[]}},"options":[{},{}],"total_voter_count":2,"is_anonymous":false,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"can_get_voters":true,"type":{{"@type":"pollTypeRegular"}}}},"description":{{"@type":"formattedText","text":"","entities":[]}},"can_add_option":false}}}}}}"#,
        option("a"),
        option("b"),
    );
    driver
        .ingest(copy_and_parse(&message, seq, sink).unwrap())
        .unwrap();
}

fn voters_page(extra: RequestId, users: &[i64]) -> String {
    let voters: Vec<String> = users
        .iter()
        .map(|id| {
            format!(r#"{{"@type":"pollVoter","voter_id":{{"@type":"messageSenderUser","user_id":{id}}}}}"#)
        })
        .collect();
    format!(
        r#"{{"@type":"pollVoters","@extra":"{}","total_count":{},"voters":[{}]}}"#,
        extra.0,
        users.len(),
        voters.join(",")
    )
}

fn stage(driver: &ConnectDriver<Arc<RecordingSender>>) -> Option<PollVotersFetch> {
    driver
        .session
        .messages
        .poll_voters
        .get(&(7, 108, 0))
        .cloned()
}

fn extra_of_last(recorder: &RecordingSender) -> RequestId {
    let json = recorder.snapshot().last().cloned().unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();
    RequestId(value["@extra"].as_str().unwrap().parse().unwrap())
}

#[test]
fn an_older_voters_page_does_not_fill_a_reloaded_stage() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    seed_poll(&mut driver, &seq, &dyn_sink);
    let (chat, message) = (ChatId(7), MessageId(108));

    let first = driver.fetch_poll_voters(chat, message, 0).unwrap().unwrap();
    assert_eq!(stage(&driver), Some(PollVotersFetch::Loading(first)));
    assert_eq!(driver.fetch_poll_voters(chat, message, 0).unwrap(), None);
    driver
        .ingest(copy_and_parse(&voters_page(first, &[1, 2]), &seq, &dyn_sink).unwrap())
        .unwrap();
    assert!(matches!(
        stage(&driver),
        Some(PollVotersFetch::Loaded {
            more: VotersMore::Ready,
            ..
        })
    ));

    let next = driver
        .load_more_poll_voters(chat, message, 0)
        .unwrap()
        .unwrap();
    assert_eq!(extra_of_last(&recorder), next);
    assert!(matches!(
        stage(&driver),
        Some(PollVotersFetch::Loaded { more: VotersMore::Asking(id), .. }) if id == next
    ));
    assert_eq!(
        driver.load_more_poll_voters(chat, message, 0).unwrap(),
        None
    );

    // The list is reloaded while the next page is out: that page's late
    // answer must not fill the new first page's stage.
    let reload = driver.fetch_poll_voters(chat, message, 0).unwrap().unwrap();
    assert_eq!(stage(&driver), Some(PollVotersFetch::Loading(reload)));
    driver
        .ingest(copy_and_parse(&voters_page(next, &[3]), &seq, &dyn_sink).unwrap())
        .unwrap();
    assert_eq!(stage(&driver), Some(PollVotersFetch::Loading(reload)));
    driver
        .ingest(copy_and_parse(&voters_page(reload, &[1]), &seq, &dyn_sink).unwrap())
        .unwrap();
    assert!(matches!(
        stage(&driver),
        Some(PollVotersFetch::Loaded { voters, .. }) if voters.len() == 1
    ));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_failed_next_page_keeps_the_list_and_frees_the_next_ask() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    seed_poll(&mut driver, &seq, &dyn_sink);
    let (chat, message) = (ChatId(7), MessageId(108));
    let first = driver.fetch_poll_voters(chat, message, 0).unwrap().unwrap();
    driver
        .ingest(copy_and_parse(&voters_page(first, &[1, 2]), &seq, &dyn_sink).unwrap())
        .unwrap();
    let next = driver
        .load_more_poll_voters(chat, message, 0)
        .unwrap()
        .unwrap();
    let error = format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"boom"}}"#,
        next.0
    );
    driver
        .ingest(copy_and_parse(&error, &seq, &dyn_sink).unwrap())
        .unwrap();
    assert!(matches!(
        stage(&driver),
        Some(PollVotersFetch::Loaded {
            more: VotersMore::Ready,
            ..
        })
    ));
    let sent = recorder.snapshot().len();
    assert!(
        driver
            .load_more_poll_voters(chat, message, 0)
            .unwrap()
            .is_some()
    );
    assert_eq!(recorder.snapshot().len(), sent + 1);
    std::fs::remove_dir_all(dir).ok();
}
