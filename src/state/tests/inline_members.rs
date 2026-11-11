//! State reducer tests: the info panel's members list pages in.
use super::common::*;
use super::*;

fn page(ids: &[i64], total: i32, extra: u64) -> String {
    let members: Vec<String> = ids
        .iter()
        .map(|id| {
            format!(
                r#"{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":{id}}},"tag":"","inviter_user_id":0,"joined_chat_date":0,"status":{{"@type":"chatMemberStatusMember","member_until_date":0}}}}"#
            )
        })
        .collect();
    format!(
        r#"{{"@type":"chatMembers","@extra":"{extra}","total_count":{total},"members":[{}]}}"#,
        members.join(",")
    )
}

fn loaded_ids(session: &Session) -> (Vec<i64>, i32) {
    let SupergroupMembersFetch::Loaded {
        members,
        total_count,
    } = session
        .groups
        .supergroup_members
        .get(&(13, MemberListFilter::Recent))
        .unwrap()
    else {
        panic!("members were not loaded");
    };
    let ids = members
        .iter()
        .map(|m| match m.member_id {
            MessageSender::User { user_id } => user_id,
            MessageSender::Chat { chat_id } => chat_id,
        })
        .collect();
    (ids, *total_count)
}

#[test]
fn next_page_joins_the_cached_one_without_repeats() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let first = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers {
            filter: MemberListFilter::Recent,
        }),
        Some(ChatId(13)),
    );
    apply_json(&mut session, &seq, &sink, &page(&[1, 2, 3], 6, first.0));
    assert_eq!(loaded_ids(&session), (vec![1, 2, 3], 6));

    // User 3 repeats because someone joined between the two requests.
    let more = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembersMore {
            filter: MemberListFilter::Recent,
            offset: 3,
        }),
        Some(ChatId(13)),
    );
    let key = (13, MemberListFilter::Recent);
    session.groups.note_member_more(key, more, 3);
    apply_json(&mut session, &seq, &sink, &page(&[3, 4, 5], 7, more.0));
    assert_eq!(loaded_ids(&session), (vec![1, 2, 3, 4, 5], 7));
    assert_eq!(
        session.groups.member_more_blocked(key),
        Some(3),
        "an answered page is not asked again at its offset"
    );
}

#[test]
fn a_next_page_of_an_older_query_does_not_join_the_new_list() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let key = (13, MemberListFilter::Recent);
    let purpose = RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers {
        filter: MemberListFilter::Recent,
    });
    let first = session.request(purpose, Some(ChatId(13)));
    session.groups.note_member_first(key, first, "al");
    apply_json(&mut session, &seq, &sink, &page(&[1, 2, 3], 6, first.0));
    let more = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembersMore {
            filter: MemberListFilter::Recent,
            offset: 3,
        }),
        Some(ChatId(13)),
    );
    session.groups.note_member_more(key, more, 3);
    // A search for "bob" fills the list before the "al" page lands.
    let second = session.request(purpose, Some(ChatId(13)));
    session.groups.note_member_first(key, second, "bob");
    apply_json(&mut session, &seq, &sink, &page(&[8, 9], 2, second.0));
    apply_json(&mut session, &seq, &sink, &page(&[3, 4, 5], 7, more.0));
    assert_eq!(loaded_ids(&session), (vec![8, 9], 2));
    assert_eq!(session.groups.member_asks[&key].more, MoreMembers::Idle);
}

#[test]
fn next_page_after_the_cache_was_dropped_is_ignored() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let more = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembersMore {
            filter: MemberListFilter::Recent,
            offset: 3,
        }),
        Some(ChatId(13)),
    );
    apply_json(&mut session, &seq, &sink, &page(&[4, 5], 7, more.0));
    assert!(
        !session
            .groups
            .supergroup_members
            .contains_key(&(13, MemberListFilter::Recent))
    );
}

fn failed(extra: u64) -> String {
    format!(
        r#"{{"@type":"error","@extra":"{extra}","code":429,"message":"Too Many Requests: retry after 3"}}"#
    )
}

/// A failed next page of the list shown is marked, so the panel can ask
/// for it again; a failed page of an older query is not.
#[test]
fn a_failed_next_page_can_be_asked_for_again() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let key = (13, MemberListFilter::Recent);
    let purpose = RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers {
        filter: MemberListFilter::Recent,
    });
    let more_purpose = RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembersMore {
        filter: MemberListFilter::Recent,
        offset: 3,
    });
    let first = session.request(purpose, Some(ChatId(13)));
    session.groups.note_member_first(key, first, "");
    apply_json(&mut session, &seq, &sink, &page(&[1, 2, 3], 6, first.0));
    let more = session.request(more_purpose, Some(ChatId(13)));
    session.groups.note_member_more(key, more, 3);
    apply_json(&mut session, &seq, &sink, &failed(more.0));
    assert_eq!(
        session.groups.member_asks[&key].more,
        MoreMembers::Failed { offset: 3 }
    );
    assert_eq!(session.groups.member_more_blocked(key), Some(3));
    assert_eq!(loaded_ids(&session), (vec![1, 2, 3], 6), "the list stays");

    session.groups.rearm_failed_member_more(13);
    assert_eq!(session.groups.member_more_blocked(key), None);
    let stale = session.request(more_purpose, Some(ChatId(13)));
    session.groups.note_member_more(key, stale, 3);
    let second = session.request(purpose, Some(ChatId(13)));
    session.groups.note_member_first(key, second, "bob");
    apply_json(&mut session, &seq, &sink, &failed(stale.0));
    assert!(
        session.groups.member_asks[&key].more == MoreMembers::Idle,
        "another query's page says nothing about the list shown"
    );
}

/// A membership change keeps the last page drawn while its refetch runs
/// (`Loading` with the last page), and the refetch's answer replaces it; a list that was
/// not loaded just goes.
#[test]
fn a_membership_change_keeps_the_last_page_until_the_refetch_answers() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let key = (13, MemberListFilter::Recent);
    let purpose = RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers {
        filter: MemberListFilter::Recent,
    });
    let first = session.request(purpose, Some(ChatId(13)));
    session.groups.note_member_first(key, first, "");
    apply_json(&mut session, &seq, &sink, &page(&[1, 2], 2, first.0));
    session.groups.supergroup_members.insert(
        (13, MemberListFilter::Search),
        SupergroupMembersFetch::Loading {
            request: None,
            last: None,
        },
    );
    session.groups.members_added(13);
    assert!(matches!(
        session.groups.supergroup_members.get(&key),
        Some(SupergroupMembersFetch::Loading { request: None, last: Some(last) }) if last.members.len() == 2 && last.total_count == 2
    ));
    assert!(
        !session
            .groups
            .supergroup_members
            .contains_key(&(13, MemberListFilter::Search)),
        "a list that was not loaded goes"
    );
    // Dropped again before the refetch: still the same page.
    session.groups.members_added(13);
    assert!(matches!(
        session.groups.supergroup_members.get(&key),
        Some(SupergroupMembersFetch::Loading { last: Some(_), .. })
    ));
    let second = session.request(purpose, Some(ChatId(13)));
    session.groups.note_member_first(key, second, "");
    apply_json(&mut session, &seq, &sink, &page(&[1, 2, 3], 3, second.0));
    assert_eq!(loaded_ids(&session), (vec![1, 2, 3], 3));
}

/// A failed first page after a membership change replaces the kept page
/// with the failure (the list is not shown stale for good).
#[test]
fn a_failed_refetch_replaces_the_kept_page() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let key = (13, MemberListFilter::Recent);
    let purpose = RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers {
        filter: MemberListFilter::Recent,
    });
    let first = session.request(purpose, Some(ChatId(13)));
    session.groups.note_member_first(key, first, "");
    apply_json(&mut session, &seq, &sink, &page(&[1, 2], 2, first.0));
    session.groups.members_added(13);
    let second = session.request(purpose, Some(ChatId(13)));
    session.groups.note_member_first(key, second, "");
    apply_json(&mut session, &seq, &sink, &failed(second.0));
    assert!(matches!(
        session.groups.supergroup_members.get(&key),
        Some(SupergroupMembersFetch::Failed(_))
    ));
}

/// The revoked-links answer fills the list only while the list waits for
/// that request: a list closed meanwhile (no entry) or reopened (another
/// request) is not filled by the old answer.
#[test]
fn a_late_revoked_links_answer_does_not_reopen_a_closed_list() {
    use crate::state::InviteLinkFetch;
    let (mut session, _sink) = session();
    let asked = session.request(RequestPurpose::GetRevokedChatInviteLinks, Some(ChatId(13)));
    let pending = session.requests.pending.get(&asked.0).cloned();
    let Some(pending) = pending else {
        panic!("the request is pending");
    };
    // Closed: no entry, so the answer fills nothing.
    session.groups.land_revoked(13, &pending, 0, Vec::new());
    assert!(!session.groups.revoked_invite_links.contains_key(&13));
    // Asked again as another request: the old answer is nobody's.
    let again = session.request(RequestPurpose::GetRevokedChatInviteLinks, Some(ChatId(13)));
    session
        .groups
        .revoked_invite_links
        .insert(13, InviteLinkFetch::Loading(again));
    session.groups.land_revoked(13, &pending, 0, Vec::new());
    assert_eq!(
        session.groups.revoked_invite_links.get(&13),
        Some(&InviteLinkFetch::Loading(again))
    );
}
