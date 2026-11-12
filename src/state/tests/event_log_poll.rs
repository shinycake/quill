//! State reducer tests: the live poll of the recent-actions page.
use super::common::*;
use super::*;

fn page_json(extra: u64, ids: &[i64]) -> String {
    let events: Vec<String> = ids
        .iter()
        .map(|id| {
            format!(
                r#"{{"@type":"chatEvent","id":{id},"date":1700000000,"member_id":{{"@type":"messageSenderUser","user_id":777}},"action":{{"@type":"chatEventMemberJoined"}}}}"#
            )
        })
        .collect();
    format!(
        r#"{{"@type":"chatEvents","@extra":"{extra}","events":[{}]}}"#,
        events.join(",")
    )
}

fn loaded_ids(session: &Session) -> Option<(Vec<i64>, bool)> {
    match session.groups.event_log_fetch(13)? {
        ChatEventLogFetch::Loaded(page) => {
            Some((page.events.iter().map(|e| e.id).collect(), page.has_more()))
        }
        _ => None,
    }
}

#[test]
fn poll_adds_new_events_on_top_and_keeps_older_pages() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let first: Vec<i64> = (101..=200).rev().collect();
    let extra = event_log_request(
        &mut session,
        GroupsPurpose::GetChatEventLog { from_event_id: 0 },
    );
    apply_json(&mut session, &seq, &sink, &page_json(extra.0, &first));
    let older: Vec<i64> = (1..=100).rev().collect();
    let extra = event_log_request(
        &mut session,
        GroupsPurpose::GetChatEventLog { from_event_id: 101 },
    );
    apply_json(&mut session, &seq, &sink, &page_json(extra.0, &older));
    let Some((ids, has_more)) = loaded_ids(&session) else {
        panic!("expected a loaded event log");
    };
    assert_eq!(ids.len(), 200);
    assert!(has_more);

    // The poll answers with the newest page: two new events and overlap.
    let extra = event_log_request(&mut session, GroupsPurpose::PollChatEventLog);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &page_json(extra.0, &[202, 201, 200, 199]),
    );
    let Some((ids, has_more)) = loaded_ids(&session) else {
        panic!("expected a loaded event log");
    };
    assert_eq!(ids.len(), 202);
    assert_eq!(ids.first(), Some(&202));
    assert_eq!(ids.last(), Some(&1));
    assert!(has_more, "a poll never changes whether older pages exist");
}

#[test]
fn poll_answer_without_a_loaded_page_is_dropped() {
    // A filter change clears the page; a poll sent before it must not
    // bring the old filter's events back.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let first = RequestId(9_001);
    session
        .groups
        .set_event_log_fetch(13, ChatEventLogFetch::Loading { request: first });
    let extra = event_log_request(&mut session, GroupsPurpose::PollChatEventLog);
    apply_json(&mut session, &seq, &sink, &page_json(extra.0, &[5, 4]));
    assert_eq!(
        session.groups.event_log_fetch(13),
        Some(&ChatEventLogFetch::Loading { request: first })
    );
}

#[test]
fn a_refresh_drops_the_page_and_poll_asked_before_it() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let first = event_log_request(
        &mut session,
        GroupsPurpose::GetChatEventLog { from_event_id: 0 },
    );
    apply_json(&mut session, &seq, &sink, &page_json(first.0, &[30, 20]));
    let poll = event_log_request(&mut session, GroupsPurpose::PollChatEventLog);

    // The filter changes: the log starts over and asks the first page again.
    session.groups.reset_event_log_fetch(13);
    let again = event_log_request(
        &mut session,
        GroupsPurpose::GetChatEventLog { from_event_id: 0 },
    );
    // The poll of the old filter lands while the new first page is out.
    apply_json(&mut session, &seq, &sink, &page_json(poll.0, &[99]));
    assert_eq!(
        session.groups.event_log_fetch(13),
        Some(&ChatEventLogFetch::Loading { request: again })
    );
    apply_json(&mut session, &seq, &sink, &page_json(again.0, &[7]));
    assert_eq!(loaded_ids(&session), Some((vec![7], false)));
}

#[test]
fn a_poll_and_an_older_page_settle_each_on_its_own_request() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let full: Vec<i64> = (101..=200).rev().collect();
    let first = event_log_request(
        &mut session,
        GroupsPurpose::GetChatEventLog { from_event_id: 0 },
    );
    apply_json(&mut session, &seq, &sink, &page_json(first.0, &full));
    let older = event_log_request(
        &mut session,
        GroupsPurpose::GetChatEventLog { from_event_id: 101 },
    );
    let poll = event_log_request(&mut session, GroupsPurpose::PollChatEventLog);
    // The poll answers first and does not end the older page's wait.
    apply_json(&mut session, &seq, &sink, &page_json(poll.0, &[201]));
    let Some(ChatEventLogFetch::Loaded(page)) = session.groups.event_log_fetch(13) else {
        panic!("expected a loaded event log");
    };
    assert_eq!(page.older, OlderEvents::Loading { request: older });
    assert_eq!(page.poll, LogPoll::Idle);
    // The older page fails: the loaded log stays and draws Retry.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"INTERNAL"}}"#,
            older.0
        ),
    );
    let Some(ChatEventLogFetch::Loaded(page)) = session.groups.event_log_fetch(13) else {
        panic!("expected a loaded event log");
    };
    assert!(matches!(page.older, OlderEvents::Failed(_)));
    assert_eq!(page.events.len(), 101);
}
