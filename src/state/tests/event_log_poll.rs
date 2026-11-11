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
            Some((page.events.iter().map(|e| e.id).collect(), page.has_more))
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
    let first = event_log_request(
        &mut session,
        GroupsPurpose::GetChatEventLog { from_event_id: 0 },
    );
    let extra = event_log_request(&mut session, GroupsPurpose::PollChatEventLog);
    apply_json(&mut session, &seq, &sink, &page_json(extra.0, &[5, 4]));
    assert_eq!(
        session.groups.event_log_fetch(13),
        Some(&ChatEventLogFetch::Loading(first))
    );
}

fn error_json(extra: u64) -> String {
    format!(r#"{{"@type":"error","@extra":"{extra}","code":500,"message":"X"}}"#)
}

/// Sweep r2-17 (05-065): the log's request ids live in its stage. A refresh
/// drops them with the stage, so a page asked before it lands nowhere, and
/// an older page's or a poll's error ends only its own wait.
#[test]
fn the_log_waits_only_for_the_requests_its_stage_holds() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let stale_first = event_log_request(
        &mut session,
        GroupsPurpose::GetChatEventLog { from_event_id: 0 },
    );
    // A refresh: the stage goes, and a new first page is asked.
    session.groups.reset_event_log_fetch(13);
    let first = event_log_request(
        &mut session,
        GroupsPurpose::GetChatEventLog { from_event_id: 0 },
    );
    apply_json(&mut session, &seq, &sink, &page_json(stale_first.0, &[9]));
    assert_eq!(
        session.groups.event_log_fetch(13),
        Some(&ChatEventLogFetch::Loading(first)),
        "the older first page is dropped"
    );
    apply_json(&mut session, &seq, &sink, &error_json(stale_first.0));
    assert_eq!(
        session.groups.event_log_fetch(13),
        Some(&ChatEventLogFetch::Loading(first)),
        "and so is its error"
    );
    apply_json(&mut session, &seq, &sink, &page_json(first.0, &[8, 7]));
    let Some(ChatEventLogFetch::Loaded(loaded)) = session.groups.event_log_fetch(13) else {
        panic!("loaded");
    };
    assert_eq!((loaded.older, loaded.poll), (None, None));

    // An older page and a poll in flight together; each error ends its own.
    let older = event_log_request(
        &mut session,
        GroupsPurpose::GetChatEventLog { from_event_id: 7 },
    );
    let poll = event_log_request(&mut session, GroupsPurpose::PollChatEventLog);
    apply_json(&mut session, &seq, &sink, &error_json(poll.0));
    let Some(ChatEventLogFetch::Loaded(loaded)) = session.groups.event_log_fetch(13) else {
        panic!("loaded");
    };
    assert_eq!((loaded.older, loaded.poll), (Some(older), None));
    apply_json(&mut session, &seq, &sink, &error_json(older.0));
    let Some(ChatEventLogFetch::Loaded(loaded)) = session.groups.event_log_fetch(13) else {
        panic!("a failed older page keeps the loaded log");
    };
    assert_eq!((loaded.older, loaded.poll), (None, None));
    assert_eq!(loaded.events.len(), 2);
}
