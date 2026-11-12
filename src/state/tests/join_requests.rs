//! State reducer tests: join requests after bulk and single actions.
use super::common::*;
use super::*;
use crate::state::PageStage;

fn requests_json(extra: RequestId, total: i32, users: &[i64]) -> String {
    let rows: Vec<String> = users
        .iter()
        .map(|user| {
            format!(r#"{{"@type":"chatJoinRequest","user_id":{user},"date":1788500000,"bio":""}}"#)
        })
        .collect();
    format!(
        r#"{{"@type":"chatJoinRequests","@extra":"{}","total_count":{total},"requests":[{}]}}"#,
        extra.0,
        rows.join(",")
    )
}

fn ok_json(extra: RequestId) -> String {
    format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0)
}

fn listed(session: &Session, chat: i64) -> Option<(i32, Vec<i64>)> {
    match session.groups.join_requests.get(&chat) {
        Some(JoinRequestFetch::Loaded(list)) => Some((
            list.total_count,
            list.requests.iter().map(|r| r.user_id).collect(),
        )),
        _ => None,
    }
}

/// Approve all while the next page loads: that page was asked before the
/// requests were handled, so it must not bring them back.
#[test]
fn approve_all_drops_the_chat_wide_page_in_flight() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let first = session.request(RequestPurpose::GetChatJoinRequests, Some(ChatId(13)));
    apply_json(&mut session, &seq, &sink, &requests_json(first, 4, &[1, 2]));
    let more = session.request(RequestPurpose::GetMoreChatJoinRequests, Some(ChatId(13)));
    if let Some(JoinRequestFetch::Loaded(list)) = session.groups.join_requests.get_mut(&13) {
        list.more = Some(more);
    }
    let all = session.request(
        RequestPurpose::Groups(GroupsPurpose::ProcessAllChatJoinRequests { approve: true }),
        Some(ChatId(13)),
    );
    apply_json(&mut session, &seq, &sink, &ok_json(all));
    apply_json(&mut session, &seq, &sink, &requests_json(more, 4, &[3, 4]));
    assert_eq!(listed(&session, 13), Some((0, Vec::new())));
    assert!(!matches!(
        session.groups.join_requests.get(&13),
        Some(JoinRequestFetch::Loaded(list)) if list.more.is_some()
    ));
}

/// The same for one link's list: its page in flight is dropped and the
/// list no longer reads as loading.
#[test]
fn approve_all_for_a_link_drops_its_page_in_flight() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let page = session.request(
        RequestPurpose::GetLinkJoinRequests { append: true },
        Some(ChatId(13)),
    );
    session.groups.invite_pages.set_join_requests(
        13,
        LinkRequestsState {
            invite_link: "https://t.me/+abc".into(),
            total_count: 4,
            requests: Vec::new(),
            stage: PageStage::Loading(page),
        },
    );
    let all = session.request_for_invite_link(
        RequestPurpose::Groups(GroupsPurpose::ProcessLinkJoinRequests { approve: false }),
        ChatId(13),
        "https://t.me/+abc",
    );
    apply_json(&mut session, &seq, &sink, &ok_json(all));
    apply_json(&mut session, &seq, &sink, &requests_json(page, 4, &[3, 4]));
    let Some(state) = session.groups.invite_pages.join_requests(13) else {
        panic!("the link's list stays open");
    };
    assert!(state.requests.is_empty());
    assert_eq!(state.total_count, 0);
    assert!(!state.loading());
}

/// Approve all on one link, answered after another link's list opened,
/// leaves that list alone: its first page still lands.
#[test]
fn approve_all_for_one_link_keeps_another_links_list() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let all = session.request_for_invite_link(
        RequestPurpose::Groups(GroupsPurpose::ProcessLinkJoinRequests { approve: true }),
        ChatId(13),
        "https://t.me/+first",
    );
    let page = session.request(
        RequestPurpose::GetLinkJoinRequests { append: false },
        Some(ChatId(13)),
    );
    session.groups.invite_pages.set_join_requests(
        13,
        LinkRequestsState {
            invite_link: "https://t.me/+second".into(),
            total_count: 0,
            requests: Vec::new(),
            stage: PageStage::Loading(page),
        },
    );
    apply_json(&mut session, &seq, &sink, &ok_json(all));
    apply_json(&mut session, &seq, &sink, &requests_json(page, 2, &[5, 6]));
    let Some(state) = session.groups.invite_pages.join_requests(13) else {
        panic!("the second link's list stays open");
    };
    assert_eq!(state.invite_link, "https://t.me/+second");
    assert_eq!(state.requests.len(), 2);
    assert_eq!(state.total_count, 2);
    assert!(!state.loading());
}

/// A refused approve keeps the loaded list: the row goes when the request
/// is gone, and any other refusal leaves every row.
#[test]
fn a_refused_approve_keeps_the_list() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let first = session.request(RequestPurpose::GetChatJoinRequests, Some(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &requests_json(first, 3, &[1, 2, 3]),
    );
    let refuse = |session: &mut Session, user_id: i64, message: &str| {
        let extra = session.request(
            RequestPurpose::Groups(GroupsPurpose::ProcessChatJoinRequest { user_id }),
            Some(ChatId(13)),
        );
        apply_json(
            session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"{message}"}}"#,
                extra.0
            ),
        );
    };
    refuse(&mut session, 1, "HIDE_REQUESTER_MISSING");
    assert_eq!(listed(&session, 13), Some((2, vec![2, 3])));
    assert!(session.groups.invite_link_error.last().is_some());
    session.groups.invite_link_error.clear();
    refuse(&mut session, 2, "CHANNELS_TOO_MUCH");
    assert_eq!(listed(&session, 13), Some((2, vec![2, 3])));
    assert!(session.groups.invite_link_error.last().is_some());
}

#[test]
fn r2_two_invite_link_failures_before_a_drain_are_both_kept() {
    // Sweep r2-19 (05-029): the errors of two mutations are a queue.
    let (mut session, _sink) = session();
    session.groups.invite_link_error.push("first".into());
    session.groups.invite_link_error.push("second".into());
    assert_eq!(
        session.groups.invite_link_error.take_all(),
        ["first", "second"]
    );
}
