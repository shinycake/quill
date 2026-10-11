//! State reducer tests: groups (continued from groups.rs).
use super::common::*;
use super::*;

#[test]
fn event_log_gate() {
    // Phase D3c: the event-log gate is deny-by-default. Any
    // administrator or the creator qualifies (no `can_promote_members`
    // right needed, unlike D3b): channels probe via the ChatSummary
    // path (`getChatMember`), supergroups via the status block.
    let (mut session, _) = session();

    assert!(!session.chat_can_view_event_log(ChatId(999)));

    let mut channel = placeholder_chat(ChatId(13));
    channel.kind = ChatKind::Supergroup {
        supergroup_id: 13,
        is_channel: true,
    };
    session.chats.insert(13, channel);
    assert!(!session.chat_can_view_event_log(ChatId(13)));

    session
        .chats
        .get_mut(&13)
        .unwrap()
        .set_member_status(ChannelMemberStatus::Administrator, None);
    assert!(session.chat_can_view_event_log(ChatId(13)));

    // An admin without the promote right still qualifies for the log.
    session
        .chats
        .get_mut(&13)
        .unwrap()
        .set_admin_can_promote_members(Some(false));
    assert!(session.chat_can_view_event_log(ChatId(13)));

    session
        .chats
        .get_mut(&13)
        .unwrap()
        .set_member_status(ChannelMemberStatus::Creator, None);
    assert!(session.chat_can_view_event_log(ChatId(13)));

    session
        .chats
        .get_mut(&13)
        .unwrap()
        .set_member_status(ChannelMemberStatus::Member, None);
    assert!(!session.chat_can_view_event_log(ChatId(13)));
    // Non-channel supergroup path: administrator without promote right.
    let mut group = placeholder_chat(ChatId(14));
    group.kind = ChatKind::Supergroup {
        supergroup_id: 14,
        is_channel: false,
    };
    session.chats.insert(14, group);
    assert!(!session.chat_can_view_event_log(ChatId(14)));
    session
        .groups
        .supergroup_member_status
        .insert(14, ChannelMemberStatus::Administrator);
    assert!(session.chat_can_view_event_log(ChatId(14)));

    // Other chat kinds never qualify.
    let private = placeholder_chat(ChatId(15));
    session.chats.insert(15, private);
    assert!(!session.chat_can_view_event_log(ChatId(15)));
}

#[test]
fn event_log_fetch_replaces_appends_and_dedups() {
    // Phase D3c: a first page replaces the cache; older pages append
    // in decreasing id order with duplicates dropped; a full page
    // sets `has_more`, a short one clears it.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let event = |id: i64| {
        format!(
            r#"{{"@type":"chatEvent","id":{id},"date":1700000000,"member_id":{{"@type":"messageSenderUser","user_id":777}},"action":{{"@type":"chatEventMemberJoined"}}}}"#
        )
    };
    let page = |extra: u64, ids: &[i64]| {
        format!(
            r#"{{"@type":"chatEvents","@extra":"{extra}","events":[{}]}}"#,
            ids.iter()
                .map(|id| event(*id))
                .collect::<Vec<_>>()
                .join(",")
        )
    };

    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id: 0 }),
        Some(ChatId(13)),
    );
    apply_json(&mut session, &seq, &sink, &page(extra.0, &[300, 299]));
    let ChatEventLogFetch::Loaded(loaded) = session.groups.event_logs.get(&13).expect("log loaded")
    else {
        panic!("expected loaded event log");
    };
    assert_eq!(
        loaded.events.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![300, 299]
    );
    assert!(!loaded.has_more);

    // Older page appends; the overlapping id dedupes; a full page
    // (100 events) keeps `has_more`.
    let full: Vec<i64> = (200..300).rev().collect();
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id: 299 }),
        Some(ChatId(13)),
    );
    apply_json(&mut session, &seq, &sink, &page(extra.0, &full));
    let ChatEventLogFetch::Loaded(loaded) = session.groups.event_logs.get(&13).expect("log loaded")
    else {
        panic!("expected loaded event log");
    };
    let ids: Vec<i64> = loaded.events.iter().map(|e| e.id).collect();
    assert_eq!(ids.len(), 101);
    assert_eq!(ids[0], 300);
    assert_eq!(ids[100], 200);
    assert!(loaded.has_more);

    // A short final page clears `has_more`.
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id: 200 }),
        Some(ChatId(13)),
    );
    apply_json(&mut session, &seq, &sink, &page(extra.0, &[199]));
    let ChatEventLogFetch::Loaded(loaded) = session.groups.event_logs.get(&13).expect("log loaded")
    else {
        panic!("expected loaded event log");
    };
    assert_eq!(loaded.events.len(), 102);
    assert!(!loaded.has_more);

    // A first-page refetch replaces everything (refresh semantics).
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id: 0 }),
        Some(ChatId(13)),
    );
    apply_json(&mut session, &seq, &sink, &page(extra.0, &[500]));
    let ChatEventLogFetch::Loaded(loaded) = session.groups.event_logs.get(&13).expect("log loaded")
    else {
        panic!("expected loaded event log");
    };
    assert_eq!(
        loaded.events.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![500]
    );
}

#[test]
fn event_log_failure_states() {
    // Phase D3c: a failed first page becomes `Failed`; a failed
    // "load more" keeps the loaded page so the retry button stays.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);

    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id: 0 }),
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
    let ChatEventLogFetch::Failed(message) =
        session.groups.event_logs.get(&13).expect("log failed")
    else {
        panic!("expected failed event log");
    };
    assert!(message.contains("Could not load recent actions"));

    // Load one page, then fail the older page: the loaded page stays.
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id: 0 }),
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatEvents","@extra":"{}","events":[{{"@type":"chatEvent","id":50,"date":1700000000,"member_id":{{"@type":"messageSenderUser","user_id":777}},"action":{{"@type":"chatEventMemberJoined"}}}}]}}"#,
            extra.0
        ),
    );
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id: 50 }),
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"INTERNAL"}}"#,
            extra.0
        ),
    );
    let ChatEventLogFetch::Loaded(loaded) = session.groups.event_logs.get(&13).expect("log loaded")
    else {
        panic!("expected loaded event log");
    };
    assert_eq!(loaded.events.len(), 1);
}

#[test]
fn event_log_relative_time_buckets() {
    // Phase D3c: the log only covers 48h, so relative buckets suffice.
    let now = 1_700_000_000i64;
    assert_eq!(event_log_relative_time_for(now as i32 - 5, now), "just now");
    assert_eq!(event_log_relative_time_for(now as i32 - 90, now), "1m ago");
    assert_eq!(
        event_log_relative_time_for(now as i32 - 3599, now),
        "59m ago"
    );
    assert_eq!(
        event_log_relative_time_for(now as i32 - 3600, now),
        "1h ago"
    );
    assert_eq!(
        event_log_relative_time_for(now as i32 - 86_399, now),
        "23h ago"
    );
    assert_eq!(
        event_log_relative_time_for(now as i32 - 86_400, now),
        "1d ago"
    );
    assert_eq!(
        event_log_relative_time_for(now as i32 - 172_800, now),
        "2d ago"
    );
}

#[test]
fn admin_list_fetch_caches() {
    // Phase D3b: `getChatAdministrators` loads and caches the result;
    // a stale `@extra` is ignored.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetChatAdministrators, Some(ChatId(13)));

    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatAdministrators","@extra":"{}","administrators":[{{"@type":"chatAdministrator","user_id":777,"custom_title":"","is_owner":true,"can_be_edited":false}},{{"@type":"chatAdministrator","user_id":888,"custom_title":"News Desk","is_owner":false,"can_be_edited":true}}]}}"#,
            extra.0
        ),
    );

    let AdminListFetch::Loaded(admins) = session.groups.admin_lists.get(&13).unwrap() else {
        panic!("admin list was not loaded");
    };
    assert_eq!(admins.len(), 2);
    assert!(admins[0].is_owner);
    assert_eq!(admins[1].custom_title, "News Desk");
    assert!(admins[1].can_be_edited);

    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"chatAdministrators","@extra":"99999","administrators":[]}"#,
    );
    let AdminListFetch::Loaded(admins) = session.groups.admin_lists.get(&13).unwrap() else {
        panic!("admin list was not loaded");
    };
    assert_eq!(admins.len(), 2);
}

#[test]
fn set_chat_member_status_ok_invalidates_admin_list() {
    // Phase D3b: a confirmed promote/demote/edit drops the cached admin
    // list so the panel refetches; the change itself arrives as
    // `updateChatMember`.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.groups.admin_lists.insert(
        13,
        AdminListFetch::Loaded(vec![ChatAdministratorEntry {
            user_id: 888,
            custom_title: String::new(),
            is_owner: false,
            can_be_edited: true,
        }]),
    );
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::SetChatMemberStatus {
            user_id: 888,
            kind: MemberStatusChange::Demote,
        }),
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(!session.groups.admin_lists.contains_key(&13));
}

#[test]
fn update_chat_member_invalidates_admin_list_and_own_rights() {
    // Phase D3b: `updateChatMember` (schema 1.8.67, line 11202)
    // invalidates a cached admin list, and refreshes the viewer's own
    // `can_promote_members` when the member is the current user.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.my_user_id = Some(777);
    let mut channel = placeholder_chat(ChatId(13));
    channel.kind = ChatKind::Supergroup {
        supergroup_id: 13,
        is_channel: true,
    };
    session.chats.insert(13, channel);
    session.groups.admin_lists.insert(
        13,
        AdminListFetch::Loaded(vec![ChatAdministratorEntry {
            user_id: 888,
            custom_title: String::new(),
            is_owner: false,
            can_be_edited: true,
        }]),
    );

    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatMember","chat_id":13,"actor_user_id":777,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":888},"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":true}},"new_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":888},"status":{"@type":"chatMemberStatusMember","member_until_date":0}}}"#,
    );
    assert!(!session.groups.admin_lists.contains_key(&13));

    // Own promotion to administrator with the promote right.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatMember","chat_id":13,"actor_user_id":777,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusMember","member_until_date":0}},"new_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{"@type":"chatAdministratorRights","can_promote_members":true}}}}"#,
    );
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(
        chat.my_member_status,
        Some(ChannelMemberStatus::Administrator)
    );
    assert_eq!(chat.my_admin_can_promote_members, Some(true));
    assert!(chat.can_manage_admins());
}

#[test]
fn get_admin_rights_response_caches_rights() {
    // Phase D3b: `getChatMember` tagged `GetAdminRights` stores the
    // administrator's rights for the edit dialog.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetAdminRights { user_id: 888 }),
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":888}},"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_promote_members":true,"can_delete_messages":true}}}}}}"#,
            extra.0
        ),
    );
    let AdminRightsFetch::Loaded(rights) = session.groups.admin_rights.get(&(13, 888)).unwrap()
    else {
        panic!("admin rights were not loaded");
    };
    assert!(rights.can_promote_members);
    assert!(rights.can_delete_messages);
    assert!(!rights.can_pin_messages);
}

#[test]
fn supergroup_members_fetch_caches() {
    // Phase D3b: `getSupergroupMembers` loads the member-picker page.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers {
            filter: MemberListFilter::Recent,
        }),
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatMembers","@extra":"{}","total_count":2,"members":[{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":111}},"tag":"","inviter_user_id":777,"joined_chat_date":1700000000,"status":{{"@type":"chatMemberStatusMember","member_until_date":0}}}},{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":222}},"tag":"","inviter_user_id":777,"joined_chat_date":1700000000,"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true}}}}]}}"#,
            extra.0
        ),
    );
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
    assert_eq!(*total_count, 2);
    assert_eq!(members.len(), 2);
    assert_eq!(members[0].status, ChannelMemberStatus::Member);
    assert_eq!(members[1].status, ChannelMemberStatus::Administrator);
}

#[test]
fn basic_group_full_info_caches_members() {
    // Slice G1: `getBasicGroupFullInfo` loads the basic-group
    // member list into `basic_group_members`.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetBasicGroupFullInfo, Some(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"basicGroupFullInfo","@extra":"{}","creator_user_id":7,"members":[{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":7}},"tag":"boss","status":{{"@type":"chatMemberStatusCreator"}}}},{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":8}},"tag":"","status":{{"@type":"chatMemberStatusMember"}}}}]}}"#,
            extra.0
        ),
    );
    let SupergroupMembersFetch::Loaded {
        members,
        total_count,
    } = session.groups.basic_group_members.get(&13).unwrap()
    else {
        panic!("basic-group members were not loaded");
    };
    assert_eq!(*total_count, 2);
    assert_eq!(members.len(), 2);
    assert_eq!(members[0].status, ChannelMemberStatus::Creator);
    // Slice G1: `chatMember.tag` (schema 1.8.67, line 2526) is the
    // admin custom title.
    assert_eq!(members[0].tag, "boss");
    assert_eq!(members[1].status, ChannelMemberStatus::Member);
    assert_eq!(members[1].tag, "");
}

#[test]
fn g2_update_supergroup_caches_sign_flags_and_rights() {
    // Slice G2: `updateSupergroup` carries `sign_messages` /
    // `show_message_sender` plus the new admin rights; the session
    // maps feed the channel manage dialog and its gates.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":25,"is_channel":false},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":25,"sign_messages":true,"show_message_sender":true,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_change_info":true,"can_manage_topics":true,"can_send_welcome_messages":true}}}}"#,
    );
    assert_eq!(
        session.groups.supergroup_sign_messages.get(&25),
        Some(&true)
    );
    assert_eq!(
        session.groups.supergroup_show_message_sender.get(&25),
        Some(&true)
    );
    assert_eq!(
        session.groups.supergroup_manage_topics_right.get(&25),
        Some(&true)
    );
    assert_eq!(
        session.groups.supergroup_change_info_right.get(&25),
        Some(&true)
    );
    assert_eq!(
        session.groups.supergroup_send_welcome_right.get(&25),
        Some(&true)
    );
    assert!(session.chat_can_manage_topics(ChatId(13)));
    assert!(session.chat_can_change_info(ChatId(13)));
    assert!(session.chat_can_send_welcome_messages(ChatId(13)));
    assert!(session.chat_sign_messages(ChatId(13)));
    assert!(session.chat_show_message_sender(ChatId(13)));
    // A member without the rights is gated out.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":25,"status":{"@type":"chatMemberStatusMember"}}}"#,
    );
    assert!(!session.chat_can_manage_topics(ChatId(13)));
    assert!(!session.chat_can_change_info(ChatId(13)));
}

#[test]
fn g2_update_supergroup_full_info_caches_anti_spam() {
    // Slice G2: `updateSupergroupFullInfo` carries the anti-spam
    // state + capability; the toggle is gated on the capability.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":25,"is_channel":false},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":25,"supergroup_full_info":{"@type":"supergroupFullInfo","has_aggressive_anti_spam_enabled":true,"can_toggle_aggressive_anti_spam":true}}"#,
    );
    assert!(session.chat_anti_spam_enabled(ChatId(13)));
    assert!(session.chat_can_toggle_anti_spam(ChatId(13)));
}

#[test]
fn s11_supergroup_sticker_set_fields_apply_and_gate() {
    // Slice S11: `supergroupFullInfo` / `updateSupergroupFullInfo`
    // carry the group sticker-set fields; the gate is fail-closed
    // while the full info is unfetched.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":25,"is_channel":false},"unread_count":0}}"#,
    );
    assert!(!session.chat_can_set_sticker_set(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":25,"supergroup_full_info":{"@type":"supergroupFullInfo","can_set_sticker_set":true,"sticker_set_id":"1234567890123","custom_emoji_sticker_set_id":"9876543210987"}}"#,
    );
    let info = session.supergroup_full_info(25).expect("cached");
    assert!(info.can_set_sticker_set);
    assert_eq!(info.sticker_set_id, 1234567890123);
    assert_eq!(info.custom_emoji_sticker_set_id, 9876543210987);
    assert!(session.chat_can_set_sticker_set(ChatId(13)));
    // A later update replaces the whole pack.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":25,"supergroup_full_info":{"@type":"supergroupFullInfo","can_set_sticker_set":false,"sticker_set_id":"0","custom_emoji_sticker_set_id":"0"}}"#,
    );
    let info = session.supergroup_full_info(25).expect("cached");
    assert!(!info.can_set_sticker_set);
    assert_eq!(info.sticker_set_id, 0);
    assert!(!session.chat_can_set_sticker_set(ChatId(13)));
    // Unknown chat → gate stays closed.
    assert!(!session.chat_can_set_sticker_set(ChatId(404)));
}

#[test]
fn g2_welcome_pack_and_flag_cached() {
    // Slice G2: the welcome pack and the `has_welcome_messages` flag
    // land in their caches (pack via the spontaneous update, flag
    // via both the update and `updateNewChat`).
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatWelcomeMessages","chat_id":13,"messages":[{"id":7,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"welcome","entities":[]}}}]}"#,
    );
    let pack = session
        .groups
        .welcome_messages
        .get(&13)
        .expect("welcome pack");
    assert_eq!(pack.len(), 1);
    assert_eq!(pack[0].id, 7);
    assert_eq!(
        session.groups.welcome_message_fetches.get(&13),
        Some(&WelcomeMessagesFetch::Loaded)
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatHasWelcomeMessages","chat_id":13,"has_welcome_messages":true}"#,
    );
    assert!(session.chat_has_welcome_messages_flag(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":26,"is_channel":false},"unread_count":0,"has_welcome_messages":true}}"#,
    );
    assert!(session.chat_has_welcome_messages_flag(ChatId(14)));
}

#[test]
fn protected_content_flag_tracks_chat_and_update() {
    // `chat.has_protected_content` arrives with the chat and changes via
    // `updateChatHasProtectedContent`; copying is refused while it's set.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":26,"is_channel":false},"unread_count":0,"has_protected_content":true}}"#,
    );
    assert!(session.chat_has_protected_content(ChatId(14)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatHasProtectedContent","chat_id":14,"has_protected_content":false}"#,
    );
    assert!(!session.chat_has_protected_content(ChatId(14)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatHasProtectedContent","chat_id":15,"has_protected_content":true}"#,
    );
    assert!(session.chat_has_protected_content(ChatId(15)));
}

#[test]
fn g2_chat_boost_status_cached() {
    // Slice G2: the `getChatBoostStatus` answer is cached per chat.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetChatBoostStatus, Some(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatBoostStatus","@extra":"{}","level":3,"boost_count":42}}"#,
            extra.0
        ),
    );
    assert_eq!(session.groups.chat_boost_status.get(&13), Some(&(3, 42)));
}

#[test]
fn g2_boost_slots_stashed_for_chain() {
    // Slice G2: the slots answer is stashed per chat so the driver
    // can chain `boostChat`.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetBoostSlotsForBoost, Some(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatBoostSlots","@extra":"{}","slots":[{{"slot_id":3}},{{"slot_id":7}}]}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.groups.boost_slots_by_chat.get(&13),
        Some(&vec![3, 7])
    );
}

#[test]
fn g2_sign_toggle_error_rolls_back() {
    // Slice G2: the optimistic sign-messages toggle restores the
    // previous flags when TDLib answers `error`.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.groups.supergroup_sign_messages.insert(25, false);
    session
        .groups
        .supergroup_show_message_sender
        .insert(25, false);
    let extra = session.request(
        RequestPurpose::ToggleSupergroupSignMessages,
        Some(ChatId(13)),
    );
    session
        .requests
        .pending_mut(extra)
        .expect("pending")
        .rollback = Some(RequestRollback::SignMessages {
        supergroup_id: 25,
        previous_sign: Some(false),
        previous_show: Some(false),
    });
    session.groups.supergroup_sign_messages.insert(25, true);
    session
        .groups
        .supergroup_show_message_sender
        .insert(25, true);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.groups.supergroup_sign_messages.get(&25),
        Some(&false)
    );
    assert_eq!(
        session.groups.supergroup_show_message_sender.get(&25),
        Some(&false)
    );
}

#[test]
fn g2_anti_spam_toggle_error_rolls_back() {
    // Slice G2: the optimistic anti-spam toggle restores the previous
    // flag when TDLib answers `error`.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session
        .groups
        .supergroup_anti_spam_enabled
        .insert(25, false);
    let extra = session.request(
        RequestPurpose::ToggleSupergroupAggressiveAntiSpam,
        Some(ChatId(13)),
    );
    session
        .requests
        .pending_mut(extra)
        .expect("pending")
        .rollback = Some(RequestRollback::AntiSpam {
        supergroup_id: 25,
        previous: Some(false),
    });
    session.groups.supergroup_anti_spam_enabled.insert(25, true);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.groups.supergroup_anti_spam_enabled.get(&25),
        Some(&false)
    );
}

#[test]
fn g2_welcome_fetch_error_marks_failed() {
    // Slice G2: a failed `loadChatWelcomeMessages` marks the fetch
    // failed so the dialog shows an error, not a spinner.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::LoadChatWelcomeMessages, Some(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
            extra.0
        ),
    );
    assert!(matches!(
        session.groups.welcome_message_fetches.get(&13),
        Some(WelcomeMessagesFetch::Failed(_))
    ));
}

#[test]
fn g2_create_forum_topic_answer_invalidates_topics() {
    // Slice G2: the `createForumTopic` answer (`forumTopicInfo`)
    // drops the cached topic list so the UI refetches it.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.threads.forum_topics.insert(13, Vec::new());
    let extra = session.request(RequestPurpose::CreateForumTopic, Some(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"forumTopicInfo","@extra":"{}","chat_id":13,"forum_topic_id":5,"name":"new"}}"#,
            extra.0
        ),
    );
    assert!(!session.threads.forum_topics.contains_key(&13));
}

#[test]
fn g2_forum_mutation_ok_drops_topic_cache() {
    // Slice G2: a confirmed forum-topic mutation (`ok`) drops the
    // cached topic list so the UI refetches it.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.threads.forum_topics.insert(13, Vec::new());
    let extra = session.request(
        RequestPurpose::Threads(ThreadsPurpose::DeleteForumTopic { forum_topic_id: 5 }),
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(!session.threads.forum_topics.contains_key(&13));
}

#[test]
fn g2_welcome_delete_ok_drops_pack() {
    // Slice G2: a confirmed welcome-message deletion (`ok`) drops
    // the cached pack so the dialog refetches it.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.groups.welcome_messages.insert(13, Vec::new());
    session
        .groups
        .welcome_message_fetches
        .insert(13, WelcomeMessagesFetch::Loaded);
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::DeleteChatWelcomeMessage {
            welcome_message_id: 7,
        }),
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(!session.groups.welcome_messages.contains_key(&13));
    assert!(!session.groups.welcome_message_fetches.contains_key(&13));
}

#[test]
fn event_log_admin_ids_dedupes_and_skips_chat_senders() {
    let event = |id: i64, member_id: MessageSender| ParsedChatEvent {
        id,
        date: 1_700_000_000,
        member_id,
        action: ChatEventAction::MemberJoined,
    };
    let page = ChatEventLogPage {
        events: vec![
            event(1, MessageSender::User { user_id: 7 }),
            event(2, MessageSender::Chat { chat_id: 13 }),
            event(3, MessageSender::User { user_id: 9 }),
            event(4, MessageSender::User { user_id: 7 }),
        ],
        has_more: false,
    };
    assert_eq!(page.admin_user_ids(), vec![7, 9]);
}

#[test]
fn pending_join_request_updates_keep_requester_ids() {
    // Batch 8: the requests bar draws avatars for `user_ids`.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPendingJoinRequests","chat_id":13,"pending_join_requests":{"@type":"chatJoinRequestsInfo","total_count":2,"user_ids":[7001,7002]}}"#,
    );
    assert_eq!(
        session.groups.pending_join_request_users.get(&13),
        Some(&vec![7001, 7002])
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPendingJoinRequests","chat_id":13,"pending_join_requests":{"@type":"chatJoinRequestsInfo","total_count":0,"user_ids":[]}}"#,
    );
    assert_eq!(
        session.groups.pending_join_request_counts.get(&13),
        Some(&0)
    );
    assert!(!session.groups.pending_join_request_users.contains_key(&13));
}

#[test]
fn chat_action_bar_from_new_chat_and_updates() {
    // Batch 8: recorded-shape `chat.action_bar`, then `updateChatActionBar`
    // changing and clearing it (schema 1.8.67 lines 3667-3690, 10526).
    use crate::telegram::envelope::ChatActionBar;
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":501,"title":"Stranger","type":{"@type":"chatTypePrivate","user_id":501},"action_bar":{"@type":"chatActionBarReportAddBlock","can_unarchive":true,"account_info":null}}}"#,
    );
    let bar = session.chat_action_bar(ChatId(501)).expect("bar stored");
    assert_eq!(
        bar,
        &ChatActionBar::ReportAddBlock {
            can_unarchive: true
        }
    );
    assert!(bar.can_unarchive() && bar.is_dismissible());

    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatActionBar","chat_id":501,"action_bar":{"@type":"chatActionBarSharePhoneNumber"}}"#,
    );
    assert_eq!(
        session.chat_action_bar(ChatId(501)),
        Some(&ChatActionBar::SharePhoneNumber)
    );

    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatActionBar","chat_id":501,"action_bar":{"@type":"chatActionBarJoinRequest","title":"Cats","is_channel":true,"request_date":1788500000}}"#,
    );
    assert!(
        !session
            .chat_action_bar(ChatId(501))
            .unwrap()
            .is_dismissible()
    );

    // An unknown (newer) constructor shows no bar rather than a wrong one.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatActionBar","chat_id":501,"action_bar":{"@type":"chatActionBarReportUnrelatedLocation"}}"#,
    );
    assert!(session.chat_action_bar(ChatId(501)).is_none());

    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatActionBar","chat_id":501,"action_bar":{"@type":"chatActionBarAddContact"}}"#,
    );
    assert_eq!(
        session.chat_action_bar(ChatId(501)),
        Some(&ChatActionBar::AddContact)
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatActionBar","chat_id":501,"action_bar":null}"#,
    );
    assert!(session.chat_action_bar(ChatId(501)).is_none());
}
