//! State reducer tests: groups.
use super::common::*;
use super::*;

#[test]
fn custom_title_failure_surfaces_in_member_dialog() {
    // Slice G1 replay: a TDLib `error` for `setChatMemberTag` lands
    // in `member_action_error` (the member-management dialog reads
    // the member-list fetch states, not `admin_lists`) as well as
    // `admin_lists` for the info panel.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::SetChatMemberTag { user_id: 42 }),
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
            extra.0
        ),
    );
    let message = session
        .groups
        .member_action_error
        .get(&13)
        .expect("member action error recorded");
    assert!(message.contains("Could not set custom title"));
    assert!(matches!(
        session.groups.admin_lists.get(&13),
        Some(AdminListFetch::Failed(_))
    ));
}

#[test]
fn basic_group_add_member_failures_accumulate() {
    // Slice G1 fix-up replay: every per-user `addChatMember` answers
    // `failedToAddMembers` (schema 1.8.67, line 13578) — the real
    // shape, not `ok`/`error`. Two per-user responses must
    // accumulate (1 + 0), and a request-level error counts one more,
    // so the dialog's partial-add line is honest.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let failed_member = |user_id: i64| {
        format!(
            r#"{{"@type":"failedToAddMember","user_id":{user_id},"premium_would_allow_invite":false,"premium_required_to_send_messages":false}}"#
        )
    };
    for (user_id, extra_count) in [(7, 1), (8, 0)] {
        let extra = session.request(RequestPurpose::AddChatMember, Some(ChatId(13)));
        let members = (0..extra_count)
            .map(|_| failed_member(user_id))
            .collect::<Vec<_>>()
            .join(",");
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"failedToAddMembers","@extra":"{}","failed_to_add_members":[{members}]}}"#,
                extra.0
            ),
        );
    }
    assert_eq!(session.groups.add_members_failed.get(&13), Some(&1));
    let extra = session.request(RequestPurpose::AddChatMember, Some(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"USER_PRIVACY_RESTRICTED"}}"#,
            extra.0
        ),
    );
    assert_eq!(session.groups.add_members_failed.get(&13), Some(&2));
}

#[test]
fn bulk_add_members_response_replaces_count() {
    // Slice G1: the single bulk `addChatMembers` response replaces
    // the failure count (no accumulation across attempts).
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for count in [3, 1] {
        let extra = session.request(RequestPurpose::AddChatMembers, Some(ChatId(13)));
        let members = (0..count)
            .map(|_| r#"{"@type":"failedToAddMember","user_id":9,"premium_would_allow_invite":false,"premium_required_to_send_messages":false}"#)
            .collect::<Vec<_>>()
            .join(",");
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"failedToAddMembers","@extra":"{}","failed_to_add_members":[{members}]}}"#,
                extra.0
            ),
        );
    }
    assert_eq!(session.groups.add_members_failed.get(&13), Some(&1));
}

#[test]
fn invite_link_replace_failure_surfaces_and_broadcast_rolls_back() {
    // Slice G1 fix-up replay: a failed `replacePrimaryChatInviteLink`
    // keeps the previously loaded list (no cache poisoning) and
    // surfaces the error via `invite_link_error` for the status
    // note; a failed `toggleSupergroupIsBroadcastGroup` removes the
    // optimistic broadcast flag so the panel doesn't lie.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.groups.invite_links.insert(
        13,
        InviteLinkFetch::Loaded(InviteLinkList {
            total_count: 1,
            links: Vec::new(),
        }),
    );

    let extra = session.request(
        RequestPurpose::ReplacePrimaryChatInviteLink,
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"INVITE_LINK_INVALID"}}"#,
            extra.0
        ),
    );
    assert!(matches!(
        session.groups.invite_links.get(&13),
        Some(InviteLinkFetch::Loaded(_))
    ));
    assert!(
        session
            .groups
            .invite_link_error
            .as_ref()
            .is_some_and(|m| m.contains("Could not replace primary invite link"))
    );

    let mut chat = placeholder_chat(ChatId(14));
    chat.kind = ChatKind::Supergroup {
        supergroup_id: 14,
        is_channel: false,
    };
    session.chats.insert(14, chat);
    session.groups.supergroup_is_broadcast.insert(14, true);
    let extra = session.request(RequestPurpose::ToggleBroadcastGroup, Some(ChatId(14)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
            extra.0
        ),
    );
    assert!(!session.groups.supergroup_is_broadcast.contains_key(&14));
}

#[test]
fn channel_is_supported() {
    let kind = ChatKind::Supergroup {
        supergroup_id: 1,
        is_channel: true,
    };
    assert!(kind.is_supported_chat());
    assert!(kind.gate_reason().is_none());
    assert!(kind.is_channel());
    let group = ChatKind::Supergroup {
        supergroup_id: 2,
        is_channel: false,
    };
    assert!(!group.is_channel());
}

#[test]
fn forum_topics_response_is_cached_per_chat() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
    );
    let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(16)));
    let json = format!(
        r#"{{"@type":"forumTopics","@extra":"{}","total_count":2,"topics":[{{"info":{{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":2,"name":"Random","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"creation_date":1,"creator_id":{{"@type":"messageSenderUser","user_id":6}},"is_general":false,"is_outgoing":false,"is_closed":false,"is_hidden":false,"is_name_implicit":false}},"last_message":null,"order":"100","is_pinned":false,"unread_count":5,"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{{"@type":"chatNotificationSettings"}},"draft_message":null}},{{"info":{{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":1,"name":"General","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"creation_date":1,"creator_id":{{"@type":"messageSenderUser","user_id":5}},"is_general":true,"is_outgoing":false,"is_closed":false,"is_hidden":false,"is_name_implicit":false}},"last_message":null,"order":"900","is_pinned":false,"unread_count":0,"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{{"@type":"chatNotificationSettings"}},"draft_message":null}}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
        extra.0
    );
    apply_json(&mut session, &seq, &sink, &json);
    let ordered = session.ordered_forum_topics(ChatId(16));
    assert_eq!(ordered.len(), 2);
    assert_eq!(ordered[0].forum_topic_id, 1); // order 900 first
    assert_eq!(ordered[1].forum_topic_id, 2);
    assert_eq!(ordered[1].unread_count, 5);
    // A response for a different purpose must not populate the cache.
    let extra2 = session.request(RequestPurpose::GetHistory, Some(ChatId(16)));
    let json2 = json.replace(
        &format!("\"@extra\":\"{}\"", extra.0),
        &format!("\"@extra\":\"{}\"", extra2.0),
    );
    session.threads.forum_topics.clear();
    apply_json(&mut session, &seq, &sink, &json2);
    assert!(!session.threads.forum_topics.contains_key(&16));
}

#[test]
fn supergroup_full_info_resolves_supergroup() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, 77);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"supergroupFullInfo","@extra":"{}","description":"CANARY desc","member_count":4321}}"#,
            extra.0
        ),
    );
    let info = session.supergroup_full_info(77).expect("cached");
    assert_eq!(info.description, "CANARY desc");
    assert_eq!(info.member_count, 4321);
    assert!(session.supergroup_full_info(78).is_none());
}

#[test]
fn community_updates_apply_create_name_change_full_info_replace() {
    // Slice (communities backend core): `updateCommunity` (schema
    // 1.8.67, line 10726) creates the community on first sight;
    // `updateCommunityFullInfo` (line 10753) lands the full-info pack.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    assert!(!session.groups.communities.contains_key(&42));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":42,"have_access":true,"name":"Rustaceans","date":1759000000}}"#,
    );
    let community = session
        .groups
        .communities
        .get(&42)
        .expect("created on first sight");
    assert_eq!(community.name, "Rustaceans");
    assert!(community.have_access);
    assert_eq!(community.date, 1759000000);
    // A second update with a new name replaces (the rename path;
    // `setCommunityName` also broadcasts `updateCommunity`).
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":42,"have_access":true,"name":"Rustaceans+","date":1759000000}}"#,
    );
    assert_eq!(
        session.groups.communities.get(&42).unwrap().name,
        "Rustaceans+"
    );
    // Full info replaces the whole pack.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateCommunityFullInfo","community_id":42,"community_full_info":{"@type":"communityFullInfo","chats":[{"@type":"communityChat","chat_id":7,"can_view_history":true,"is_hidden":true}],"administrator_count":3,"banned_count":1,"add_chat_request_count":2}}"#,
    );
    let info = session
        .groups
        .community_full_infos
        .get(&42)
        .expect("full info cached");
    assert_eq!(info.administrator_count, 3);
    assert_eq!(info.banned_count, 1);
    assert_eq!(info.add_chat_request_count, 2);
    assert_eq!(info.chats.len(), 1);
    assert!(info.chats[0].is_hidden);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateCommunityFullInfo","community_id":42,"community_full_info":{"@type":"communityFullInfo","chats":[],"administrator_count":4,"banned_count":0,"add_chat_request_count":0}}"#,
    );
    let info = session.groups.community_full_infos.get(&42).unwrap();
    assert_eq!(info.administrator_count, 4);
    assert!(info.chats.is_empty());
    // Unrelated communities are untouched.
    assert!(!session.groups.communities.contains_key(&43));
    assert!(!session.groups.community_full_infos.contains_key(&43));
}

#[test]
fn set_community_name_ok_drops_full_info_for_refetch() {
    // Slice (communities backend core): a confirmed `setCommunityName`
    // drops the cached full-info pack so the driver refetches it (the
    // welcome-message-mutation pattern); an error leaves the cache
    // alone, so nothing refetches.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.groups.community_full_infos.insert(
        42,
        ParsedCommunityFullInfo {
            chats: Vec::new(),
            administrator_count: 3,
            banned_count: 0,
            add_chat_request_count: 0,
        },
    );
    let extra = session.request_for_community(RequestPurpose::SetCommunityName, 42);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(!session.groups.community_full_infos.contains_key(&42));
    // Error path: the cache stays.
    session.groups.community_full_infos.insert(
        42,
        ParsedCommunityFullInfo {
            chats: Vec::new(),
            administrator_count: 3,
            banned_count: 0,
            add_chat_request_count: 0,
        },
    );
    let extra = session.request_for_community(RequestPurpose::SetCommunityName, 42);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"NAME_INVALID"}}"#,
            extra.0
        ),
    );
    assert!(session.groups.community_full_infos.contains_key(&42));
}

#[test]
fn slow_mode_member_wait_countdown_and_expiry() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let fetched = seed_slow_mode_group(
        &mut session,
        &seq,
        &sink,
        17,
        Some(MEMBER_STATUS),
        SLOW_MODE_FIELDS,
        false,
    );
    let chat = ChatId(17);
    // 3s elapsed → ceil(22.0) = 22.
    assert_eq!(session.slow_mode_wait_secs(chat, fetched + 3_000), Some(22));
    // Countdown rounds up: 24.4s remaining → 25.
    assert_eq!(session.slow_mode_wait_secs(chat, fetched + 600), Some(25));
    // Expiry boundary: 25.0s elapsed → remaining 0 → free to send.
    assert_eq!(session.slow_mode_wait_secs(chat, fetched + 25_000), None);
    assert_eq!(session.slow_mode_wait_secs(chat, fetched + 60_000), None);
}

#[test]
fn slow_mode_creator_and_admin_bypass() {
    for (status, label) in [
        (r#"{"@type":"chatMemberStatusCreator"}"#, "creator"),
        (
            r#"{"@type":"chatMemberStatusAdministrator"}"#,
            "administrator",
        ),
    ] {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let fetched = seed_slow_mode_group(
            &mut session,
            &seq,
            &sink,
            18,
            Some(status),
            SLOW_MODE_FIELDS,
            false,
        );
        assert_eq!(
            session.slow_mode_wait_secs(ChatId(18), fetched + 1_000),
            None,
            "{label} bypasses slow mode"
        );
    }
}

#[test]
fn slow_mode_boost_bypass() {
    // `my_boost_count >= unrestrict_boost_count > 0` → exempt.
    let (mut session_a, sink_a) = session();
    let seq_a = AtomicU64::new(0);
    let fetched = seed_slow_mode_group(
        &mut session_a,
        &seq_a,
        &sink_a,
        19,
        Some(MEMBER_STATUS),
        r#""slow_mode_delay":30,"slow_mode_delay_expires_in":25.0,"my_boost_count":5,"unrestrict_boost_count":5"#,
        false,
    );
    assert_eq!(
        session_a.slow_mode_wait_secs(ChatId(19), fetched + 1_000),
        None
    );

    // `unrestrict_boost_count` 0 = unspecified → still gated even with boosts.
    let (mut session2, sink2) = session();
    let seq2 = AtomicU64::new(0);
    let fetched = seed_slow_mode_group(
        &mut session2,
        &seq2,
        &sink2,
        20,
        Some(MEMBER_STATUS),
        r#""slow_mode_delay":30,"slow_mode_delay_expires_in":25.0,"my_boost_count":5,"unrestrict_boost_count":0"#,
        false,
    );
    assert_eq!(
        session2.slow_mode_wait_secs(ChatId(20), fetched + 1_000),
        Some(24)
    );
}

#[test]
fn slow_mode_channel_and_zero_delay_are_ungated() {
    // Broadcast channels ignore slow mode even when the fields are set.
    let (mut session_a, sink_a) = session();
    let seq_a = AtomicU64::new(0);
    let fetched = seed_slow_mode_group(
        &mut session_a,
        &seq_a,
        &sink_a,
        21,
        Some(MEMBER_STATUS),
        SLOW_MODE_FIELDS,
        true,
    );
    assert_eq!(
        session_a.slow_mode_wait_secs(ChatId(21), fetched + 1_000),
        None
    );

    // `slow_mode_delay` 0 = slow mode off.
    let (mut session2, sink2) = session();
    let seq2 = AtomicU64::new(0);
    let fetched = seed_slow_mode_group(
        &mut session2,
        &seq2,
        &sink2,
        22,
        Some(MEMBER_STATUS),
        r#""slow_mode_delay":0,"slow_mode_delay_expires_in":0.0,"my_boost_count":0,"unrestrict_boost_count":0"#,
        false,
    );
    assert_eq!(
        session2.slow_mode_wait_secs(ChatId(22), fetched + 1_000),
        None
    );
}

#[test]
fn slow_mode_unknown_status_is_conservatively_gated() {
    // No `updateSupergroup` seen yet → no bypass, the gate applies.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let fetched =
        seed_slow_mode_group(&mut session, &seq, &sink, 23, None, SLOW_MODE_FIELDS, false);
    assert_eq!(
        session.slow_mode_wait_secs(ChatId(23), fetched + 1_000),
        Some(24)
    );
}

#[test]
fn slow_mode_restrict_right_tracks_admin_rights() {
    // Phase A1: `setChatSlowModeDelay` requires `can_restrict_members`
    // (schema 1.8.67, line 13551); the reducer records it per
    // supergroup from own `chatMemberStatusAdministrator.rights`.
    let (mut session_a, sink_a) = session();
    let seq_a = AtomicU64::new(0);
    let with_right = r#"{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_restrict_members":true}}"#;
    seed_slow_mode_group(
        &mut session_a,
        &seq_a,
        &sink_a,
        30,
        Some(with_right),
        SLOW_MODE_FIELDS,
        false,
    );
    assert_eq!(
        session_a.supergroup_own_status(30),
        Some(ChannelMemberStatus::Administrator)
    );
    assert!(session_a.supergroup_can_restrict_members(30));

    let (mut session_b, sink_b) = session();
    let seq_b = AtomicU64::new(0);
    let without_right = r#"{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_restrict_members":false}}"#;
    seed_slow_mode_group(
        &mut session_b,
        &seq_b,
        &sink_b,
        31,
        Some(without_right),
        SLOW_MODE_FIELDS,
        false,
    );
    assert!(!session_b.supergroup_can_restrict_members(31));
    // Unknown supergroup → treated as lacking the right.
    assert!(!session_b.supergroup_can_restrict_members(999));
}

#[test]
fn slow_mode_ungated_without_full_info() {
    // No cached full info → no gate (can't know the delay).
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":24,"title":"Slow group","type":{"@type":"chatTypeSupergroup","supergroup_id":24,"is_channel":false},"unread_count":0}}"#,
    );
    assert_eq!(session.slow_mode_wait_secs(ChatId(24), unix_ms_now()), None);
}

#[test]
fn invite_link_fetch_flow_loads_and_caches() {
    // Phase D3a: Fetching invite links loads and caches the result.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetChatInviteLinks, Some(ChatId(13)));

    let link1 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods","name":"Mods","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":8,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;
    let link2 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods2","name":"Mods 2","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":4,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;

    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":2,"invite_links":[{},{}]}}"#,
            extra.0, link1, link2
        ),
    );

    let InviteLinkFetch::Loaded(list) = session.groups.invite_links.get(&13).unwrap() else {
        panic!("invite links were not loaded");
    };
    assert_eq!(list.total_count, 2);
    assert_eq!(list.links.len(), 2);
    assert_eq!(list.links[0].name, "Mods");

    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatInviteLinks","@extra":"99999","total_count":1,"invite_links":[{}]}}"#,
            link1
        ),
    );

    let InviteLinkFetch::Loaded(list) = session.groups.invite_links.get(&13).unwrap() else {
        panic!("invite links were not loaded");
    };
    assert_eq!(list.total_count, 2);
    assert_eq!(list.links.len(), 2);
}

#[test]
fn invite_link_create_upsert_bumps_total() {
    // Phase D3a: A created invite link is appended and increments the total.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let fetch_extra = session.request(RequestPurpose::GetChatInviteLinks, Some(ChatId(13)));

    let link1 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods","name":"Mods","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":8,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;
    let link2 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods2","name":"Mods 2","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":4,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;

    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":2,"invite_links":[{},{}]}}"#,
            fetch_extra.0, link1, link2
        ),
    );

    let create_extra = session.request(RequestPurpose::CreateChatInviteLink, Some(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatInviteLink","@extra":"{}","invite_link":"https://t.me/+new","name":"New","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":0,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}}"#,
            create_extra.0
        ),
    );

    let InviteLinkFetch::Loaded(list) = session.groups.invite_links.get(&13).unwrap() else {
        panic!("invite links were not loaded");
    };
    assert_eq!(list.total_count, 3);
    assert_eq!(list.links.len(), 3);
    assert_eq!(list.links.last().unwrap().name, "New");
}

#[test]
fn invite_link_edit_replaces_in_place() {
    // Phase D3a: Editing an invite link replaces the matching cached entry.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let fetch_extra = session.request(RequestPurpose::GetChatInviteLinks, Some(ChatId(13)));

    let link1 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods","name":"Mods","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":8,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;
    let link2 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods2","name":"Mods 2","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":4,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;

    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":2,"invite_links":[{},{}]}}"#,
            fetch_extra.0, link1, link2
        ),
    );

    let edit_extra = session.request(RequestPurpose::EditChatInviteLink, Some(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatInviteLink","@extra":"{}","invite_link":"https://t.me/+mods","name":"Mods!","creator_user_id":777,"date":1788000000,"edit_date":1788100000,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":9,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}}"#,
            edit_extra.0
        ),
    );

    let InviteLinkFetch::Loaded(list) = session.groups.invite_links.get(&13).unwrap() else {
        panic!("invite links were not loaded");
    };
    assert_eq!(list.total_count, 2);
    assert_eq!(list.links.len(), 2);
    assert_eq!(list.links[0].name, "Mods!");
    assert_eq!(list.links[0].member_count, 9);
}

#[test]
fn invite_link_revoke_removes_the_revoked_link() {
    // Revoking answers with the revoked link only (plus the replacement
    // for a primary link); it leaves the active list without it.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let fetch_extra = session.request(RequestPurpose::GetChatInviteLinks, Some(ChatId(13)));

    let link1 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods","name":"Mods","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":8,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;
    let link2 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods2","name":"Mods 2","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":4,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;

    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":2,"invite_links":[{},{}]}}"#,
            fetch_extra.0, link1, link2
        ),
    );

    let revoke_extra = session.request(RequestPurpose::RevokeChatInviteLink, Some(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":1,"invite_links":[{}]}}"#,
            revoke_extra.0,
            link1.replace(r#""is_revoked":false"#, r#""is_revoked":true"#)
        ),
    );

    let InviteLinkFetch::Loaded(list) = session.groups.invite_links.get(&13).unwrap() else {
        panic!("invite links were not loaded");
    };
    assert_eq!(list.total_count, 1);
    assert_eq!(list.links.len(), 1);
    assert_eq!(list.links[0].invite_link, "https://t.me/+mods2");
}

#[test]
fn join_request_fetch_flow_loads_and_caches() {
    // Phase D3a: Fetching join requests loads and caches the result.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetChatJoinRequests, Some(ChatId(13)));

    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatJoinRequests","@extra":"{}","total_count":2,"requests":[{{"@type":"chatJoinRequest","user_id":7001,"date":1788500000,"bio":"Hi"}},{{"@type":"chatJoinRequest","user_id":7002,"date":1788550000,"bio":"Yo"}}]}}"#,
            extra.0
        ),
    );

    let JoinRequestFetch::Loaded(list) = session.groups.join_requests.get(&13).unwrap() else {
        panic!("join requests were not loaded");
    };
    assert_eq!(list.total_count, 2);
    assert_eq!(list.requests.len(), 2);
    assert_eq!(list.requests[0].user_id, 7001);
    assert_eq!(list.requests[0].bio, "Hi");
}

#[test]
fn join_request_update_prepends_and_dedupes() {
    // Phase D3a: New join-request updates prepend and deduplicate by user ID.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetChatJoinRequests, Some(ChatId(13)));

    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatJoinRequests","@extra":"{}","total_count":2,"requests":[{{"@type":"chatJoinRequest","user_id":7001,"date":1788500000,"bio":"Hi"}},{{"@type":"chatJoinRequest","user_id":7002,"date":1788550000,"bio":"Yo"}}]}}"#,
            extra.0
        ),
    );

    let link = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+join","name":"Join","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":8,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;
    let update = format!(
        r#"{{"@type":"updateNewChatJoinRequest","chat_id":13,"request":{{"@type":"chatJoinRequest","user_id":7003,"date":1788600000,"bio":"New"}},"user_chat_id":0,"invite_link":{},"query_id":"42"}}"#,
        link
    );

    apply_json(&mut session, &seq, &sink, &update);

    let JoinRequestFetch::Loaded(list) = session.groups.join_requests.get(&13).unwrap() else {
        panic!("join requests were not loaded");
    };
    assert_eq!(list.total_count, 3);
    assert_eq!(list.requests.len(), 3);
    assert_eq!(list.requests[0].user_id, 7003);

    apply_json(&mut session, &seq, &sink, &update);

    let JoinRequestFetch::Loaded(list) = session.groups.join_requests.get(&13).unwrap() else {
        panic!("join requests were not loaded");
    };
    assert_eq!(list.total_count, 3);
    assert_eq!(list.requests.len(), 3);
    assert_eq!(list.requests[0].user_id, 7003);

    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewChatJoinRequest","chat_id":14,"request":{{"@type":"chatJoinRequest","user_id":7004,"date":1788650000,"bio":"Unloaded"}},"user_chat_id":0,"invite_link":{},"query_id":"43"}}"#,
            link
        ),
    );

    assert!(!session.groups.join_requests.contains_key(&14));
}

#[test]
fn update_chat_pending_join_requests_sets_count() {
    // Phase D3a: Pending join-request updates cache the reported count.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);

    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPendingJoinRequests","chat_id":13,"pending_join_requests":{"@type":"chatJoinRequestsInfo","total_count":5,"user_ids":[7001]}}"#,
    );

    assert_eq!(
        session.groups.pending_join_request_counts.get(&13),
        Some(&5)
    );
}

#[test]
fn process_join_request_ok_drops_from_list() {
    // Phase D3a: Successfully processing a join request removes it from the cache.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let fetch_extra = session.request(RequestPurpose::GetChatJoinRequests, Some(ChatId(13)));

    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatJoinRequests","@extra":"{}","total_count":2,"requests":[{{"@type":"chatJoinRequest","user_id":7001,"date":1788500000,"bio":"Hi"}},{{"@type":"chatJoinRequest","user_id":7002,"date":1788550000,"bio":"Yo"}}]}}"#,
            fetch_extra.0
        ),
    );

    let process_extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::ProcessChatJoinRequest { user_id: 7001 }),
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, process_extra.0),
    );

    let JoinRequestFetch::Loaded(list) = session.groups.join_requests.get(&13).unwrap() else {
        panic!("join requests were not loaded");
    };
    assert_eq!(list.total_count, 1);
    assert_eq!(list.requests.len(), 1);
    assert_eq!(list.requests[0].user_id, 7002);
}

#[test]
fn can_manage_admins_gate() {
    // Phase D3b: admin-management permissions follow channel and
    // supergroup membership rights; deny-by-default.
    let (mut session, _) = session();

    assert!(!session.chat_can_manage_admins(ChatId(999)));

    let mut channel = placeholder_chat(ChatId(13));
    channel.kind = ChatKind::Supergroup {
        supergroup_id: 13,
        is_channel: true,
    };
    session.chats.insert(13, channel);
    assert!(!session.chat_can_manage_admins(ChatId(13)));

    session
        .chats
        .get_mut(&13)
        .unwrap()
        .set_member_status(ChannelMemberStatus::Creator, None);
    assert!(session.chat_can_manage_admins(ChatId(13)));

    {
        let channel = session.chats.get_mut(&13).unwrap();
        channel.set_member_status(ChannelMemberStatus::Administrator, None);
        channel.set_admin_can_promote_members(Some(true));
    }
    assert!(session.chat_can_manage_admins(ChatId(13)));

    session
        .chats
        .get_mut(&13)
        .unwrap()
        .set_admin_can_promote_members(Some(false));
    assert!(!session.chat_can_manage_admins(ChatId(13)));

    session
        .chats
        .get_mut(&13)
        .unwrap()
        .set_admin_can_promote_members(None);
    assert!(!session.chat_can_manage_admins(ChatId(13)));

    // Non-channel supergroup path: creator always; admin needs the
    // explicit right from the supergroup status block.
    let mut creator_group = placeholder_chat(ChatId(14));
    creator_group.kind = ChatKind::Supergroup {
        supergroup_id: 14,
        is_channel: false,
    };
    session.chats.insert(14, creator_group);
    session
        .groups
        .supergroup_member_status
        .insert(14, ChannelMemberStatus::Creator);
    assert!(session.chat_can_manage_admins(ChatId(14)));

    let mut admin_group = placeholder_chat(ChatId(15));
    admin_group.kind = ChatKind::Supergroup {
        supergroup_id: 15,
        is_channel: false,
    };
    session.chats.insert(15, admin_group);
    session
        .groups
        .supergroup_member_status
        .insert(15, ChannelMemberStatus::Administrator);
    session.groups.supergroup_promote_right.insert(15, true);
    assert!(session.chat_can_manage_admins(ChatId(15)));

    session.groups.supergroup_promote_right.insert(15, false);
    assert!(!session.chat_can_manage_admins(ChatId(15)));
}
