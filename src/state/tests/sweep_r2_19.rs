//! Sweep r2-19: answers and errors land only in the slot that waits for
//! their own request, so an older request, answered or failed late, never
//! covers a newer one.
use super::common::*;
use super::*;
use crate::state::fetch::Fetch;

fn error(extra: RequestId) -> String {
    format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"X"}}"#,
        extra.0
    )
}

/// 05-022: the colors of the charts are the theme the answering request
/// asked for, and an older request's answer is dropped.
#[test]
fn a_statistics_answer_of_an_older_theme_is_dropped() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let light = session.request(RequestPurpose::GetChatStatistics, Some(ChatId(13)));
    let dark = session.request(RequestPurpose::GetChatStatistics, Some(ChatId(13)));
    session.groups.chat_statistics.insert(
        13,
        ChatStatisticsFetch::Loading {
            request: dark,
            is_dark: true,
        },
    );
    let graph = r#"{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""}"#;
    let value = r#"{"@type":"statisticalValue","value":1.0,"previous_value":1.0,"growth_rate_percentage":0.0}"#;
    let answer = |extra: RequestId| {
        format!(
            r#"{{"@type":"chatStatisticsChannel","@extra":"{}","period":{{"@type":"dateRange","start_date":1,"end_date":2}},"member_count":{value},"mean_message_view_count":{value},"mean_message_share_count":{value},"mean_message_reaction_count":{value},"mean_story_view_count":{value},"mean_story_share_count":{value},"mean_story_reaction_count":{value},"enabled_notifications_percentage":1.0,"member_count_graph":{graph},"join_graph":{graph},"mute_graph":{graph},"view_count_by_hour_graph":{graph},"view_count_by_source_graph":{graph},"join_by_source_graph":{graph},"language_graph":{graph},"message_interaction_graph":{graph},"message_reaction_graph":{graph},"story_interaction_graph":{graph},"story_reaction_graph":{graph},"instant_view_interaction_graph":{graph},"recent_interactions":[]}}"#,
            extra.0
        )
    };
    apply_json(&mut session, &seq, &sink, &answer(light));
    assert!(matches!(
        session.groups.chat_statistics.get(&13),
        Some(ChatStatisticsFetch::Loading { .. })
    ));
    apply_json(&mut session, &seq, &sink, &answer(dark));
    assert!(matches!(
        session.groups.chat_statistics.get(&13),
        Some(ChatStatisticsFetch::Loaded { is_dark: true, .. })
    ));
}

/// 05-023 and 05-028: a late error of an older list request does not fail
/// the newer one, and a late answer does not cover it.
#[test]
fn an_older_invite_link_list_neither_lands_nor_fails_over_the_newer() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let older = session.request(RequestPurpose::GetChatInviteLinks, Some(ChatId(13)));
    let newer = session.request(RequestPurpose::GetChatInviteLinks, Some(ChatId(13)));
    session
        .groups
        .invite_links
        .insert(13, InviteLinkFetch::Loading(newer));
    apply_json(&mut session, &seq, &sink, &error(older));
    assert_eq!(
        session.groups.invite_links.get(&13),
        Some(&InviteLinkFetch::Loading(newer))
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":0,"invite_links":[]}}"#,
            older.0
        ),
    );
    assert_eq!(
        session.groups.invite_links.get(&13),
        Some(&InviteLinkFetch::Loading(newer))
    );
    apply_json(&mut session, &seq, &sink, &error(newer));
    assert!(matches!(
        session.groups.invite_links.get(&13),
        Some(InviteLinkFetch::Failed(_))
    ));
}

/// 05-028: administrator lists, basic group members and rights.
#[test]
fn a_late_error_of_an_older_lookup_leaves_a_loaded_list_alone() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let admins = session.request(RequestPurpose::GetChatAdministrators, Some(ChatId(13)));
    let members = session.request(RequestPurpose::GetBasicGroupFullInfo, Some(ChatId(13)));
    session
        .groups
        .admin_lists
        .insert(13, AdminListFetch::Loaded(Vec::new()));
    session.groups.basic_group_members.insert(
        13,
        SupergroupMembersFetch::Loaded {
            members: Vec::new(),
            total_count: 0,
        },
    );
    apply_json(&mut session, &seq, &sink, &error(admins));
    apply_json(&mut session, &seq, &sink, &error(members));
    assert!(matches!(
        session.groups.admin_lists.get(&13),
        Some(AdminListFetch::Loaded(_))
    ));
    assert!(matches!(
        session.groups.basic_group_members.get(&13),
        Some(SupergroupMembersFetch::Loaded { .. })
    ));
}

/// 05-028: a member who is not an administrator ends the rights lookup
/// instead of leaving it loading.
#[test]
fn rights_of_a_member_who_is_no_administrator_end_the_lookup() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetAdminRights { user_id: 888 }),
        Some(ChatId(13)),
    );
    session
        .groups
        .admin_rights
        .insert((13, 888), AdminRightsFetch::Loading(extra));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":888}},"status":{{"@type":"chatMemberStatusMember"}}}}"#,
            extra.0
        ),
    );
    assert!(matches!(
        session.groups.admin_rights.get(&(13, 888)),
        Some(AdminRightsFetch::Failed(_))
    ));
}

/// 05-026: the limit box's list lands only from the request it waits for.
#[test]
fn an_older_public_chats_answer_does_not_cover_the_newer_list() {
    let (mut session, _sink) = session();
    let purpose = RequestPurpose::Groups(GroupsPurpose::GroupType(
        GroupTypePurpose::GetCreatedPublicChats,
    ));
    let older = session.request(purpose, None);
    let newer = session.request(purpose, None);
    session.groups.group_type.public_chats = PublicChatsFetch::Loading(newer);
    let old = session.requests.get(older).cloned().expect("pending");
    let new = session.requests.get(newer).cloned().expect("pending");
    session.apply_created_public_chats(&[], Some(&old));
    assert_eq!(
        session.groups.group_type.public_chats,
        PublicChatsFetch::Loading(newer)
    );
    session.apply_group_type_error(&TdError::from_code(500), Some(&old));
    assert_eq!(
        session.groups.group_type.public_chats,
        PublicChatsFetch::Loading(newer)
    );
    session.apply_created_public_chats(&[], Some(&new));
    assert_eq!(
        session.groups.group_type.public_chats,
        PublicChatsFetch::Loaded(Vec::new())
    );
}

/// 05-024: a level change keeps the status visible and asks again; only
/// the request waited for answers it, and a refused refetch is a failure
/// the held status survives.
#[test]
fn a_boost_status_marked_out_of_date_is_replaced_only_by_its_own_request() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.groups.set_boost_status(
        13,
        crate::telegram::envelope::ParsedBoostStatus {
            level: 1,
            boost_count: 2,
            ..Default::default()
        },
    );
    session.groups.outdate_boost_status(13);
    assert!(session.groups.boost_status_wanted(13));
    assert_eq!(session.groups.boost_level(13), Some(1));
    let older = session.request(RequestPurpose::GetChatBoostStatus, Some(ChatId(13)));
    let newer = session.request(RequestPurpose::GetChatBoostStatus, Some(ChatId(13)));
    let fetch = session.groups.chat_boost_status.entry(13).or_default();
    fetch.begin();
    fetch.sent(newer);
    let status = |extra: RequestId, level: i32| {
        format!(
            r#"{{"@type":"chatBoostStatus","@extra":"{}","level":{level},"boost_count":9}}"#,
            extra.0
        )
    };
    apply_json(&mut session, &seq, &sink, &status(older, 7));
    assert_eq!(session.groups.boost_level(13), Some(1));
    apply_json(&mut session, &seq, &sink, &error(newer));
    assert!(matches!(
        session.groups.chat_boost_status.get(&13),
        Some(Fetch::Failed { .. })
    ));
    assert_eq!(session.groups.boost_level(13), Some(1), "held answer kept");
    assert!(
        !session.groups.boost_status_wanted(13),
        "no retry from render"
    );
}

/// 05-038: poll statistics and voters.
#[test]
fn an_older_poll_answer_does_not_replace_the_newer_fetch() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let key = (7, 106);
    let purpose = RequestPurpose::Messages(MessagesPurpose::GetPollVoteStatistics {
        chat_id: ChatId(7),
        message_id: MessageId(106),
    });
    let older = session.request(purpose, Some(ChatId(7)));
    let newer = session.request(purpose, Some(ChatId(7)));
    session
        .messages
        .poll_stats
        .insert(key, PollStatsFetch::Loading(newer));
    apply_json(&mut session, &seq, &sink, &error(older));
    assert_eq!(
        session.messages.poll_stats.get(&key),
        Some(&PollStatsFetch::Loading(newer))
    );
    apply_json(&mut session, &seq, &sink, &error(newer));
    assert!(matches!(
        session.messages.poll_stats.get(&key),
        Some(PollStatsFetch::Failed(_))
    ));
}

#[test]
fn an_older_voters_error_does_not_fail_the_newer_first_page() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let purpose = RequestPurpose::Messages(MessagesPurpose::GetPollVoters {
        chat_id: ChatId(7),
        message_id: MessageId(106),
        option_id: 0,
        offset: 0,
    });
    let older = session.request(purpose, Some(ChatId(7)));
    let newer = session.request(purpose, Some(ChatId(7)));
    let key = (7, 106, 0);
    session
        .messages
        .poll_voters
        .insert(key, PollVotersFetch::Loading(newer));
    apply_json(&mut session, &seq, &sink, &error(older));
    assert_eq!(
        session.messages.poll_voters.get(&key),
        Some(&PollVotersFetch::Loading(newer))
    );
    apply_json(&mut session, &seq, &sink, &error(newer));
    assert!(matches!(
        session.messages.poll_voters.get(&key),
        Some(PollVotersFetch::Failed(_))
    ));
}
