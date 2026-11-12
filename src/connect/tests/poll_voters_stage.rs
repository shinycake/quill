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

fn update_poll() -> String {
    let option = |id: &str, votes: i32| {
        format!(
            r#"{{"@type":"pollOption","id":"{id}","text":{{"@type":"formattedText","text":"{id}","entities":[]}},"voter_count":{votes},"vote_percentage":50,"is_chosen":false}}"#
        )
    };
    format!(
        r#"{{"@type":"updatePoll","poll":{{"@type":"poll","id":9004,"question":{{"@type":"formattedText","text":"Q","entities":[]}},"options":[{},{}],"total_voter_count":3,"is_anonymous":false,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"can_get_voters":true,"type":{{"@type":"pollTypeRegular"}}}}}}"#,
        option("a", 2),
        option("b", 1),
    )
}

/// Sweep r4-10: a vote moving the poll must not leave the open dialog on
/// "Select an option": the entry goes stale, the dialog's list is asked
/// again, and the answer to the page that was in flight is dropped.
#[test]
fn a_poll_update_makes_the_list_stale_and_the_ask_again_fills_it() {
    let (dir, mut driver, _recorder, _sink, dyn_sink, seq) = poll_driver();
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

    driver
        .ingest(copy_and_parse(&update_poll(), &seq, &dyn_sink).unwrap())
        .unwrap();
    assert_eq!(stage(&driver), Some(PollVotersFetch::Stale));
    // The page that was in flight answers into nothing.
    driver
        .ingest(copy_and_parse(&voters_page(next, &[3]), &seq, &dyn_sink).unwrap())
        .unwrap();
    assert_eq!(stage(&driver), Some(PollVotersFetch::Stale));

    let again = driver.fetch_poll_voters(chat, message, 0).unwrap().unwrap();
    assert_eq!(stage(&driver), Some(PollVotersFetch::Loading(again)));
    driver
        .ingest(copy_and_parse(&voters_page(again, &[1, 2, 3]), &seq, &dyn_sink).unwrap())
        .unwrap();
    assert!(matches!(
        stage(&driver),
        Some(PollVotersFetch::Loaded { voters, .. }) if voters.len() == 3
    ));
    std::fs::remove_dir_all(dir).ok();
}

/// A stale list whose ask cannot be sent ends as `Failed` (drawn with a
/// Retry), not as a stage the dialog asks again every frame; a failed send
/// of a first ask does the same.
#[test]
fn a_stale_list_whose_ask_cannot_go_ends_as_failed() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    seed_poll(&mut driver, &seq, &dyn_sink);
    let (chat, message) = (ChatId(7), MessageId(108));
    let first = driver.fetch_poll_voters(chat, message, 0).unwrap().unwrap();
    driver
        .ingest(copy_and_parse(&voters_page(first, &[1]), &seq, &dyn_sink).unwrap())
        .unwrap();
    driver
        .ingest(copy_and_parse(&update_poll(), &seq, &dyn_sink).unwrap())
        .unwrap();
    recorder.refuse_sends(true);
    assert!(driver.fetch_poll_voters(chat, message, 0).is_err());
    assert!(matches!(stage(&driver), Some(PollVotersFetch::Failed(_))));
    recorder.refuse_sends(false);
    let retry = driver.fetch_poll_voters(chat, message, 0).unwrap().unwrap();
    assert_eq!(stage(&driver), Some(PollVotersFetch::Loading(retry)));
    std::fs::remove_dir_all(dir).ok();
}

/// Sweep r4-10 round 4: the frame upkeep asks a `Stale` list again, once.
#[test]
fn a_stale_list_is_asked_again_by_the_upkeep_once() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    seed_poll(&mut driver, &seq, &dyn_sink);
    let (chat, message) = (ChatId(7), MessageId(108));
    assert!(
        !driver.refresh_stale_poll_voters(chat, message, 0),
        "nothing asked yet: nothing is stale"
    );
    let first = driver.fetch_poll_voters(chat, message, 0).unwrap().unwrap();
    driver
        .ingest(copy_and_parse(&voters_page(first, &[1, 2]), &seq, &dyn_sink).unwrap())
        .unwrap();
    driver
        .session
        .messages
        .poll_voters
        .insert((7, 108, 0), PollVotersFetch::Stale);
    assert!(driver.refresh_stale_poll_voters(chat, message, 0));
    let asked = extra_of_last(&recorder);
    assert_eq!(stage(&driver), Some(PollVotersFetch::Loading(asked)));
    assert!(!driver.refresh_stale_poll_voters(chat, message, 0), "once");
    std::fs::remove_dir_all(dir).ok();
}

/// Sweep r4-10 round 4: the stats Retry's fetch turns a `Failed` entry
/// into `Loading` of the request it sent.
#[test]
fn the_stats_fetch_turns_a_failed_entry_into_loading() {
    use crate::state::PollStatsFetch;
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    seed_poll(&mut driver, &seq, &dyn_sink);
    let (chat, message) = (ChatId(7), MessageId(108));
    driver
        .session
        .messages
        .poll_stats
        .insert((7, 108), PollStatsFetch::Failed("no".into()));
    let asked = driver
        .fetch_poll_vote_statistics(chat, message, false)
        .unwrap()
        .unwrap();
    assert_eq!(extra_of_last(&recorder), asked);
    assert_eq!(
        driver.session.messages.poll_stats.get(&(7, 108)),
        Some(&PollStatsFetch::Loading(asked))
    );
    std::fs::remove_dir_all(dir).ok();
}
