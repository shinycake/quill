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
    match session.groups.event_logs.get(&13)? {
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
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id: 0 }),
        Some(ChatId(13)),
    );
    apply_json(&mut session, &seq, &sink, &page_json(extra.0, &first));
    let older: Vec<i64> = (1..=100).rev().collect();
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id: 101 }),
        Some(ChatId(13)),
    );
    apply_json(&mut session, &seq, &sink, &page_json(extra.0, &older));
    let Some((ids, has_more)) = loaded_ids(&session) else {
        panic!("expected a loaded event log");
    };
    assert_eq!(ids.len(), 200);
    assert!(has_more);

    // The poll answers with the newest page: two new events and overlap.
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::PollChatEventLog),
        Some(ChatId(13)),
    );
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
    session
        .groups
        .event_logs
        .insert(13, ChatEventLogFetch::Loading);
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::PollChatEventLog),
        Some(ChatId(13)),
    );
    apply_json(&mut session, &seq, &sink, &page_json(extra.0, &[5, 4]));
    assert_eq!(
        session.groups.event_logs.get(&13),
        Some(&ChatEventLogFetch::Loading)
    );
}
