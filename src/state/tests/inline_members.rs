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
    session.groups.note_member_more(key, more);
    apply_json(&mut session, &seq, &sink, &page(&[3, 4, 5], 7, more.0));
    assert_eq!(loaded_ids(&session), (vec![1, 2, 3, 4, 5], 7));
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
    session
        .groups
        .supergroup_member_queries
        .insert(key, (first, "al".into()));
    apply_json(&mut session, &seq, &sink, &page(&[1, 2, 3], 6, first.0));
    let more = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembersMore {
            filter: MemberListFilter::Recent,
            offset: 3,
        }),
        Some(ChatId(13)),
    );
    session.groups.note_member_more(key, more);
    // A search for "bob" fills the list before the "al" page lands.
    let second = session.request(purpose, Some(ChatId(13)));
    session
        .groups
        .supergroup_member_queries
        .insert(key, (second, "bob".into()));
    apply_json(&mut session, &seq, &sink, &page(&[8, 9], 2, second.0));
    apply_json(&mut session, &seq, &sink, &page(&[3, 4, 5], 7, more.0));
    assert_eq!(loaded_ids(&session), (vec![8, 9], 2));
    assert!(session.groups.member_more_bases.is_empty());
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
