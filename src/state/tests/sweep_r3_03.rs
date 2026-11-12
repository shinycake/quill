//! Sweep fixes (r3-03): answers land only on the stage that waits for their
//! own request, and a one-shot line leaves with the success that follows.
use super::common::*;
use super::*;

fn error(extra: RequestId) -> String {
    format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"X"}}"#,
        extra.0
    )
}

fn members(extra: RequestId, ids: &[i64]) -> String {
    let rows: Vec<String> = ids
        .iter()
        .map(|id| {
            format!(
                r#"{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":{id}}},"tag":"","inviter_user_id":0,"joined_chat_date":0,"status":{{"@type":"chatMemberStatusMember","member_until_date":0}}}}"#
            )
        })
        .collect();
    format!(
        r#"{{"@type":"chatMembers","@extra":"{}","total_count":{},"members":[{}]}}"#,
        extra.0,
        ids.len(),
        rows.join(",")
    )
}

const MEMBERS: RequestPurpose = RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers {
    filter: MemberListFilter::Search,
});
const RECENT: RequestPurpose = RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers {
    filter: MemberListFilter::Recent,
});

/// 08-001 / 08-003: a search that could not be sent leaves the list
/// `NotSent` under the new query, waiting for no request, so the answer of
/// an older search (still in flight) does not fill it with the old rows.
#[test]
fn a_refused_member_search_is_not_filled_by_an_older_answer() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let key = (13, MemberListFilter::Search);
    let older = session.request(MEMBERS, Some(ChatId(13)));
    session.groups.note_member_first(key, older, "al");
    session.groups.note_member_refused(key, "bob");
    apply_json(&mut session, &seq, &sink, &members(older, &[1, 2]));
    assert!(matches!(
        session.groups.member_fetch(key),
        Some(SupergroupMembersFetch::NotSent(_))
    ));
    assert_eq!(session.groups.member_query(key), "bob");
    // And its error does not end a later stage either.
    let newer = session.request(MEMBERS, Some(ChatId(13)));
    session.groups.note_member_first(key, newer, "cy");
    apply_json(&mut session, &seq, &sink, &error(older));
    assert!(
        session
            .groups
            .member_fetch(key)
            .is_some_and(SupergroupMembersFetch::in_flight)
    );
    apply_json(&mut session, &seq, &sink, &members(newer, &[7]));
    assert!(matches!(
        session.groups.member_fetch(key),
        Some(SupergroupMembersFetch::Loaded { members, .. }) if members.len() == 1
    ));
}

/// 08-002: a membership change leaves a page `Stale` (no request out, so
/// no answer can land on it), the administrators `Stale` only when asked
/// before, and the next ask moves the list to `Loading` under its own id.
#[test]
fn a_membership_change_leaves_stale_lists_that_ask_again() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let key = (13, MemberListFilter::Recent);
    let first = session.request(RECENT, Some(ChatId(13)));
    session.groups.note_member_first(key, first, "");
    apply_json(&mut session, &seq, &sink, &members(first, &[1, 2]));
    session.groups.members_changed(13);
    assert!(session.groups.member_list_is_stale(key));
    assert!(session.groups.stale_admin_chats().is_empty());
    // A late answer of the first request finds nothing waiting for it.
    apply_json(&mut session, &seq, &sink, &members(first, &[9]));
    assert!(session.groups.member_list_is_stale(key));
    // The next ask keeps the page drawn and waits for its own request.
    let again = session.request(RECENT, Some(ChatId(13)));
    session.groups.note_member_first(key, again, "");
    assert!(matches!(
        session.groups.member_fetch(key),
        Some(SupergroupMembersFetch::Loading { request, last: Some(_) }) if *request == again
    ));
}

/// A membership change must not delete a list that is on its first load
/// or failed: the first goes `Stale` (the poll and readers ask again), the
/// second keeps its Retry. Basic groups the same.
#[test]
fn a_membership_change_keeps_first_loads_and_failures_to_ask_again() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let loading = (14, MemberListFilter::Recent);
    let failed = (14, MemberListFilter::Search);
    let first = session.request(RECENT, Some(ChatId(14)));
    session.groups.note_member_first(loading, first, "");
    let second = session.request(MEMBERS, Some(ChatId(14)));
    session.groups.note_member_first(failed, second, "x");
    apply_json(&mut session, &seq, &sink, &error(second));
    session.groups.members_changed(14);
    assert!(session.groups.member_list_is_stale(loading));
    assert!(matches!(
        session.groups.member_fetch(failed),
        Some(SupergroupMembersFetch::Failed(_))
    ));
    // The stale first load asks again under its own request.
    let again = session.request(RECENT, Some(ChatId(14)));
    session.groups.note_member_first(loading, again, "");
    apply_json(&mut session, &seq, &sink, &members(again, &[4]));
    assert!(matches!(
        session.groups.member_fetch(loading),
        Some(SupergroupMembersFetch::Loaded { members, .. }) if members.len() == 1
    ));
    // A basic group still loading goes stale, not missing.
    session
        .groups
        .basic_group_members
        .insert(15, SupergroupMembersFetch::asked(None, RequestId(900)));
    session.groups.members_changed(15);
    assert!(session.groups.basic_members_are_stale(15));
}

/// 07-005: the wallpaper failure line leaves with the next success.
#[test]
fn a_wallpaper_failure_line_goes_with_the_next_successful_delete() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.chats_state.background_error = Some("offline".into());
    let delete = session.request(
        RequestPurpose::Chats(ChatsPurpose::RemoveInstalledBackground { background_id: 5 }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, delete.0),
    );
    assert_eq!(session.chats_state.background_error, None);
}

fn sharing_lookup(session: &mut Session, chat: ChatId) -> RequestId {
    let request = session.request(
        RequestPurpose::Messages(MessagesPurpose::GetSharingProtection { chat_id: chat }),
        Some(chat),
    );
    session.groups.private_sharing.enable = EnableSharing::Asking { chat, request };
    request
}

fn properties(extra: RequestId, other: bool) -> String {
    format!(
        r#"{{"@type":"messageProperties","@extra":"{}","has_protected_content_by_other_user":{other},"has_protected_content_by_current_user":false}}"#,
        extra.0
    )
}

/// 08-010: only the lookup the flow waits for moves it; an answer or an
/// error with nothing waiting (the box was closed, another lookup is out)
/// writes nothing.
#[test]
fn a_sharing_lookup_answer_needs_the_flow_to_wait_for_it() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let chat = ChatId(31);
    let old = sharing_lookup(&mut session, chat);
    let current = sharing_lookup(&mut session, chat);
    apply_json(&mut session, &seq, &sink, &properties(old, true));
    assert_eq!(
        session.groups.private_sharing.enable,
        EnableSharing::Asking {
            chat,
            request: current
        }
    );
    apply_json(&mut session, &seq, &sink, &error(old));
    assert!(session.groups.private_sharing.asking(chat));
    apply_json(&mut session, &seq, &sink, &properties(current, true));
    assert_eq!(
        session.groups.private_sharing.enable,
        EnableSharing::AskToRequest(chat)
    );
    // The box closed; a late answer of another lookup opens nothing.
    session.groups.private_sharing.enable = EnableSharing::Idle;
    let late = session.request(
        RequestPurpose::Messages(MessagesPurpose::GetSharingProtection { chat_id: chat }),
        Some(chat),
    );
    apply_json(&mut session, &seq, &sink, &properties(late, false));
    apply_json(&mut session, &seq, &sink, &error(late));
    assert_eq!(session.groups.private_sharing.enable, EnableSharing::Idle);
    // A lookup that fails while waited for asks before turning off.
    let failing = sharing_lookup(&mut session, chat);
    apply_json(&mut session, &seq, &sink, &error(failing));
    assert_eq!(
        session.groups.private_sharing.enable,
        EnableSharing::AskToRequest(chat)
    );
}
