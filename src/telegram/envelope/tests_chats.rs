use super::*;
use crate::ids::{ChatId, MessageId, UserId};

#[test]
fn chat_folder_spec_parsed() {
    // Parity slice: `getChatFolder` response (schema 1.8.67 line 13355)
    // carries the full `chatFolder` spec (line 3476).
    let env = parse_envelope(
            r#"{"@type":"chatFolder","name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]},"animate_custom_emoji":false},"icon":null,"color_id":-1,"is_shareable":false,"pinned_chat_ids":[11],"included_chat_ids":[12,13],"excluded_chat_ids":[14],"exclude_muted":true,"exclude_read":false,"exclude_archived":true,"include_contacts":true,"include_non_contacts":false,"include_bots":true,"include_groups":false,"include_channels":true}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::ChatFolder { spec } => {
            assert_eq!(spec.name, "Work");
            assert_eq!(spec.pinned_chat_ids, vec![11]);
            assert_eq!(spec.included_chat_ids, vec![12, 13]);
            assert_eq!(spec.excluded_chat_ids, vec![14]);
            assert!(spec.exclude_muted);
            assert!(!spec.exclude_read);
            assert!(spec.exclude_archived);
            assert!(spec.include_contacts);
            assert!(!spec.include_non_contacts);
            assert!(spec.include_bots);
            assert!(!spec.include_groups);
            assert!(spec.include_channels);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn chat_folder_info_response_parsed() {
    // Parity slice: `createChatFolder` / `editChatFolder` responses
    // (schema 1.8.67 lines 13358 / 13361) are `chatFolderInfo`.
    let env = parse_envelope(
            r#"{"@type":"chatFolderInfo","id":5,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"New","entities":[]},"animate_custom_emoji":false},"icon":null,"color_id":-1,"is_shareable":false,"has_my_invite_links":false}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::ChatFolderInfo(info) => {
            assert_eq!(info.id, 5);
            assert_eq!(info.name, "New");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn chat_lists_response_parsed() {
    // Parity slice: `getChatListsToAddChat` response (schema 1.8.67
    // line 13347) is `chatLists`.
    let env = parse_envelope(
            r#"{"@type":"chatLists","chat_lists":[{"@type":"chatListMain"},{"@type":"chatListFolder","chat_folder_id":2}]}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::ChatLists { lists } => {
            assert_eq!(lists, vec![ChatList::Main, ChatList::Folder(2)]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn send_message_uses_topic_id_field_in_schema() {
    // Guard against obsolete message_thread_id examples.
    let schema = include_str!("../../../schema/td_api.tl");
    let send = schema
        .lines()
        .find(|l| l.starts_with("sendMessage "))
        .expect("sendMessage");
    assert!(send.contains("topic_id:MessageTopic"));
    assert!(!send.contains("message_thread_id"));
}

#[test]
fn parse_message_topic_forum_yields_forum_topic_id() {
    // Parity slice 4: `message.topic_id` (schema 1.8.67, lines 3001–3010
    // and 3165) — only `messageTopicForum` maps to `Some`.
    let forum = parse_envelope(
            r#"{"@type":"message","id":7,"chat_id":16,"is_outgoing":false,"topic_id":{"@type":"messageTopicForum","forum_topic_id":2},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}"#,
        )
        .unwrap();
    match forum.payload {
        EnvelopePayload::Message(message) => assert_eq!(message.topic_id, Some(2)),
        other => panic!("{other:?}"),
    }
    let thread = parse_envelope(
            r#"{"@type":"message","id":8,"chat_id":16,"is_outgoing":false,"topic_id":{"@type":"messageTopicThread","message_thread_id":5},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}"#,
        )
        .unwrap();
    match thread.payload {
        EnvelopePayload::Message(message) => assert_eq!(message.topic_id, None),
        other => panic!("{other:?}"),
    }
    let plain = parse_envelope(
            r#"{"@type":"message","id":9,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}"#,
        )
        .unwrap();
    match plain.payload {
        EnvelopePayload::Message(message) => assert_eq!(message.topic_id, None),
        other => panic!("{other:?}"),
    }
}

#[test]
fn cl2_bare_chat_answer_parses_as_update_new_chat() {
    // Slice CL2: the `createPrivateChat` answer is a bare `chat`
    // object (schema 1.8.67, line 13312), not wrapped in
    // `updateNewChat`. It parses exactly like the inner chat so
    // the reducer inserts it into the model.
    let env = parse_envelope(
            r#"{"@type":"chat","@extra":"58","id":777001,"title":"Saved Messages","type":{"@type":"chatTypePrivate","user_id":777},"unread_count":0}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewChat { chat_id, title, .. } => {
            assert_eq!(chat_id, ChatId(777001));
            assert_eq!(title, "Saved Messages");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_new_chat_parses_send_permission() {
    // Parity slice 4: `chat.permissions.can_send_basic_messages`
    // (schema 1.8.67, line 1070).
    let env = parse_envelope(
            r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Demo forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"permissions":{"@type":"chatPermissions","can_send_basic_messages":false},"unread_count":0}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewChat {
            can_send_basic_messages,
            ..
        } => assert!(!can_send_basic_messages),
        other => panic!("{other:?}"),
    }
    // Absent block defaults to true (lenient parsing).
    let env = parse_envelope(
            r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Demo forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewChat {
            can_send_basic_messages,
            ..
        } => assert!(can_send_basic_messages),
        other => panic!("{other:?}"),
    }
    // `updateChatPermissions` (schema 1.8.67, line 10500).
    let env = parse_envelope(
            r#"{"@type":"updateChatPermissions","chat_id":16,"permissions":{"@type":"chatPermissions","can_send_basic_messages":true}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateChatPermissions {
            chat_id,
            can_send_basic_messages,
            permissions,
        } => {
            assert_eq!(chat_id, ChatId(16));
            assert!(can_send_basic_messages);
            // Slice G1: the full block is kept for the editor.
            assert!(permissions.unwrap().can_send_basic_messages);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn forum_topics_parse_keeps_needed_fields() {
    let json = r#"{"@type":"forumTopics","@extra":"9","total_count":2,"topics":[{"info":{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":1,"name":"General","icon":{"@type":"forumTopicIcon","color":7322096,"custom_emoji_id":"0"},"creation_date":1700000000,"creator_id":{"@type":"messageSenderUser","user_id":5},"is_general":true,"is_outgoing":false,"is_closed":false,"is_hidden":false,"is_name_implicit":false},"last_message":{"id":50,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_TOPIC_welcome","entities":[]}}},"order":"500","is_pinned":true,"unread_count":3,"last_read_inbox_message_id":50,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":true,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false},"draft_message":null},{"info":{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":2,"name":"Random","icon":{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"},"creation_date":1700000100,"creator_id":{"@type":"messageSenderUser","user_id":6},"is_general":false,"is_outgoing":false,"is_closed":true,"is_hidden":false,"is_name_implicit":false},"last_message":null,"order":"100","is_pinned":false,"unread_count":0,"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":true,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false},"draft_message":null}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::ForumTopics {
            total_count,
            topics,
        } => {
            assert_eq!(total_count, 2);
            assert_eq!(topics.len(), 2);
            let general = &topics[0];
            assert_eq!(general.forum_topic_id, 1);
            assert_eq!(general.name, "General");
            assert!(general.is_general);
            assert!(!general.is_closed);
            assert!(general.is_pinned);
            assert_eq!(general.unread_count, 3);
            assert_eq!(general.order, 500);
            assert_eq!(general.last_message_preview, "CANARY_TOPIC_welcome");
            let random = &topics[1];
            assert_eq!(random.forum_topic_id, 2);
            assert!(!random.is_general);
            assert!(random.is_closed);
            assert_eq!(random.last_message_preview, "");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn forum_topic_without_info_is_skipped() {
    let json = r#"{"@type":"forumTopics","total_count":1,"topics":[{"order":"1"}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::ForumTopics { topics, .. } => assert!(topics.is_empty()),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn update_supergroup_parses_admin_restrict_right() {
    // Phase A1: `chatMemberStatusAdministrator` carries `rights`
    // (schema 1.8.67 line 1092); `can_restrict_members` (line 1092) is
    // what `setChatSlowModeDelay` requires (line 13551).
    let json = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":25,"is_forum":false,"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":false,"rights":{"@type":"chatAdministratorRights","can_manage_chat":false,"can_change_info":false,"can_post_messages":false,"can_edit_messages":false,"can_delete_messages":false,"can_invite_users":false,"can_restrict_members":true,"can_pin_messages":false,"can_promote_members":false,"can_manage_video_chats":false,"can_post_stories":false,"can_edit_stories":false,"can_delete_stories":false,"can_manage_direct_messages":false,"can_manage_tags":false,"can_send_welcome_messages":false,"is_anonymous":false}}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateSupergroup {
            status,
            can_restrict_members,
            can_promote_members,
            can_manage_tags,
            ..
        } => {
            assert_eq!(status, ChannelMemberStatus::Administrator);
            assert_eq!(can_restrict_members, Some(true));
            // Phase D3b: `can_promote_members` rides the same rights block.
            assert_eq!(can_promote_members, Some(false));
            // Slice G1: `can_manage_tags` gates custom-title changes.
            assert_eq!(can_manage_tags, Some(false));
        }
        other => panic!("unexpected {other:?}"),
    }
    // Admin without the right → `Some(false)`.
    let json = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":25,"is_forum":false,"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":false,"rights":{"@type":"chatAdministratorRights","can_restrict_members":false}}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateSupergroup {
            can_restrict_members,
            ..
        } => assert_eq!(can_restrict_members, Some(false)),
        other => panic!("unexpected {other:?}"),
    }
    // Admin with no rights block → `None` (treated as lacking the right).
    let json = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":25,"is_forum":false,"status":{"@type":"chatMemberStatusAdministrator"}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateSupergroup {
            can_restrict_members,
            ..
        } => assert_eq!(can_restrict_members, None),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn update_supergroup_parses_forum_flag() {
    let json = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"usernames":null,"date":1700000000,"status":{"@type":"chatMemberStatusMember"},"member_count":42,"boost_level":0,"has_automatic_translation":false,"has_linked_chat":false,"has_location":false,"sign_messages":false,"show_message_sender":false,"join_to_send_messages":false,"join_by_request":false,"is_slow_mode_enabled":false,"is_channel":false,"is_broadcast_group":false,"is_forum":true,"is_direct_messages_group":false,"is_administered_direct_messages_group":false,"verification_status":{"@type":"verificationStatus","is_verified":false,"is_scam":false,"is_fake":false},"has_direct_messages_group":false,"has_forum_tabs":false,"restriction_info":null,"paid_message_star_count":0,"active_story_state":null}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateSupergroup {
            supergroup_id,
            verification: _,
            member_count: _,
            is_forum,
            has_forum_tabs: _,
            username,
            status,
            can_restrict_members,
            can_invite_users,
            can_promote_members,
            can_manage_tags: _,
            can_manage_topics: _,
            can_change_info: _,
            can_send_welcome_messages: _,
            join_by_request,
            sign_messages: _,
            show_message_sender: _,
            is_broadcast_group,
        } => {
            assert_eq!(supergroup_id, 16);
            assert!(is_forum);
            // Parity slice: null `usernames` → empty username.
            assert_eq!(username, "");
            // Phase A1: own status parsed (`chatMemberStatusMember`).
            assert_eq!(status, ChannelMemberStatus::Member);
            // Members carry no admin rights.
            assert_eq!(can_restrict_members, None);
            // Phase D3a: no invite right either.
            assert_eq!(can_invite_users, None);
            // Phase D3b: no promote right either.
            assert_eq!(can_promote_members, None);
            // Slice G1: flags parsed (both false in this fixture).
            assert!(!join_by_request);
            assert!(!is_broadcast_group);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn update_supergroup_parses_username() {
    // Parity slice: first active username is kept for the
    // channel/supergroup header (schema 1.8.67 lines 2746/2372).
    let json = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":18,"usernames":{"@type":"usernames","active_usernames":["demochannel","backupname"],"disabled_usernames":[],"editable_username":"demochannel","collectible_usernames":[]},"is_forum":false}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateSupergroup {
            supergroup_id,
            verification: _,
            member_count: _,
            is_forum,
            has_forum_tabs: _,
            username,
            status,
            can_restrict_members,
            can_invite_users,
            can_promote_members,
            can_manage_tags: _,
            can_manage_topics: _,
            can_change_info: _,
            can_send_welcome_messages: _,
            join_by_request,
            sign_messages: _,
            show_message_sender: _,
            is_broadcast_group,
        } => {
            assert_eq!(supergroup_id, 18);
            assert!(!is_forum);
            assert_eq!(username, "demochannel");
            assert_eq!(status, ChannelMemberStatus::Unknown);
            assert_eq!(can_restrict_members, None);
            // Phase D3a: no `status` block → no invite right either.
            assert_eq!(can_invite_users, None);
            // Phase D3b: no `status` block → no promote right either.
            assert_eq!(can_promote_members, None);
            // Slice G1: missing flags default to false.
            assert!(!join_by_request);
            assert!(!is_broadcast_group);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn supergroup_response_parses_forum_flag() {
    let json = r#"{"@type":"supergroup","@extra":"4","id":17,"is_forum":false}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Supergroup {
            supergroup_id,
            is_forum,
            has_forum_tabs: _,
            username,
            status,
            can_restrict_members,
            can_invite_users,
            can_promote_members,
            can_manage_tags: _,
            can_manage_topics: _,
            can_change_info: _,
            can_send_welcome_messages: _,
            join_by_request,
            sign_messages: _,
            show_message_sender: _,
            is_broadcast_group,
        } => {
            assert_eq!(supergroup_id, 17);
            assert!(!is_forum);
            assert_eq!(username, "");
            assert_eq!(status, ChannelMemberStatus::Unknown);
            assert_eq!(can_restrict_members, None);
            // No `status` block → no admin rights for either gate.
            assert_eq!(can_invite_users, None);
            assert_eq!(can_promote_members, None);
            // Slice G1: missing flags default to false.
            assert!(!join_by_request);
            assert!(!is_broadcast_group);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn read_inbox_and_outbox_are_typed() {
    let inbox = parse_envelope(
            r#"{"@type":"updateChatReadInbox","chat_id":4,"last_read_inbox_message_id":88,"unread_count":3}"#,
        )
        .unwrap();
    match inbox.payload {
        EnvelopePayload::UpdateChatReadInbox {
            chat_id,
            last_read_inbox_message_id,
            unread_count,
        } => {
            assert_eq!(chat_id.0, 4);
            assert_eq!(last_read_inbox_message_id.0, 88);
            assert_eq!(unread_count, 3);
        }
        other => panic!("{other:?}"),
    }
    let outbox = parse_envelope(
        r#"{"@type":"updateChatReadOutbox","chat_id":4,"last_read_outbox_message_id":91}"#,
    )
    .unwrap();
    match outbox.payload {
        EnvelopePayload::UpdateChatReadOutbox {
            chat_id,
            last_read_outbox_message_id,
        } => {
            assert_eq!(chat_id.0, 4);
            assert_eq!(last_read_outbox_message_id.0, 91);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn user_full_info_bot_info_parsed() {
    // `getUserFullInfo` response (schema 1.8.67 lines 11501 / 2430 /
    // 2468): `bot_info` carries `description` and a bare
    // `vector<botCommand>` of `commands`.
    let json = r#"{"@type":"userFullInfo","@extra":"7","block_list":null,"bio":{"@type":"formattedText","text":"","entities":[]},"birthdate":null,"bot_info":{"@type":"botInfo","short_description":"A demo bot","description":"This bot demonstrates the info panel.","commands":[{"@type":"botCommand","command":"start","description":"Start the bot","is_ephemeral":false},{"@type":"botCommand","command":"help","description":"Show help","is_ephemeral":false}]}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UserFullInfo {
            extras: _,
            bot_info,
            bio,
            photo,
            photo_id: _,
            blocked,
        } => {
            let info = bot_info.expect("bot_info");
            assert_eq!(info.short_description, "A demo bot");
            assert_eq!(info.description, "This bot demonstrates the info panel.");
            assert_eq!(info.commands.len(), 2);
            assert_eq!(info.commands[0].command, "start");
            assert_eq!(info.commands[0].description, "Start the bot");
            assert_eq!(info.commands[1].command, "help");
            assert!(bio.is_empty());
            assert!(photo.is_none());
            // Slice A6: the fixture's `block_list` is null → not
            // blocked.
            assert!(!blocked);
        }
        other => panic!("{other:?}"),
    }
    // Non-bot full info: `bot_info` null → None.
    let env = parse_envelope(
        r#"{"@type":"userFullInfo","@extra":"8","block_list":null,"bot_info":null}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::UserFullInfo { bot_info, .. } => assert!(bot_info.is_none()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn user_full_info_block_list_main_parsed() {
    // Slice A6: `userFullInfo.block_list:BlockList` (schema 1.8.67,
    // line 2468) — `blockListMain` (line 9692) means blocked.
    let env = parse_envelope(
            r#"{"@type":"userFullInfo","@extra":"11","block_list":{"@type":"blockListMain"},"bot_info":null}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UserFullInfo { blocked, .. } => assert!(blocked),
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_user_full_info_parsed() {
    // `updateUserFullInfo` (schema 1.8.67 line 10744): user id explicit,
    // `bot_info` nested under `user_full_info`.
    let json = r#"{"@type":"updateUserFullInfo","user_id":21,"user_full_info":{"@type":"userFullInfo","bot_info":{"@type":"botInfo","short_description":"","description":"Refreshed description.","commands":[{"@type":"botCommand","command":"ping","description":"","is_ephemeral":false}]}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateUserFullInfo {
            user_id, bot_info, ..
        } => {
            assert_eq!(user_id.0, 21);
            let info = bot_info.expect("bot_info");
            assert_eq!(info.description, "Refreshed description.");
            assert_eq!(info.commands.len(), 1);
            assert_eq!(info.commands[0].command, "ping");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn draft_message_text_and_same_chat_reply() {
    let json = r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0,"draft_message":{"@type":"draftMessage","reply_to":{"@type":"inputMessageReplyToMessage","message_id":101,"quote":null,"checklist_task_id":0,"poll_option_id":""},"date":1700000000,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"meet at 6","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewChat { draft, .. } => {
            let draft = draft.expect("draft");
            assert_eq!(draft.text, "meet at 6");
            assert_eq!(draft.reply_to_message_id, Some(MessageId(101)));
            assert_eq!(draft.quote, None);
        }
        other => panic!("{other:?}"),
    }
    let update = parse_envelope(
            r#"{"@type":"updateChatDraftMessage","chat_id":11,"draft_message":null,"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}]}"#,
        )
        .unwrap();
    match update.payload {
        EnvelopePayload::UpdateChatDraftMessage {
            draft, positions, ..
        } => {
            assert!(draft.is_none());
            assert_eq!(positions.len(), 1);
            assert_eq!(positions[0].order, 9);
        }
        other => panic!("{other:?}"),
    }
    let external = parse_envelope(
            r#"{"@type":"updateChatDraftMessage","chat_id":11,"draft_message":{"@type":"draftMessage","reply_to":{"@type":"inputMessageReplyToExternalMessage","chat_id":12,"message_id":4,"quote":null,"checklist_task_id":0,"poll_option_id":""},"date":1,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"hi","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null},"positions":[]}"#,
        )
        .unwrap();
    match external.payload {
        EnvelopePayload::UpdateChatDraftMessage { draft, .. } => {
            let draft = draft.expect("text kept");
            assert_eq!(draft.text, "hi");
            assert_eq!(draft.reply_to_message_id, None);
            assert_eq!(draft.quote, None);
        }
        other => panic!("{other:?}"),
    }
    let bot = parse_envelope(
            r#"{"@type":"updateUser","user":{"id":11,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":true,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
        )
        .unwrap();
    match bot.payload {
        EnvelopePayload::UpdateUser { user_id, user } => {
            assert_eq!(user_id, UserId(11));
            assert!(user.is_bot);
            assert_eq!(user.first_name, "Bot");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn new_chat_carries_read_cursors() {
    let json = r#"{"@type":"updateNewChat","chat":{"id":9,"title":"n","type":{"@type":"chatTypePrivate","user_id":9},"unread_count":2,"last_read_inbox_message_id":10,"last_read_outbox_message_id":11}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewChat {
            unread_count,
            last_read_inbox_message_id,
            last_read_outbox_message_id,
            ..
        } => {
            assert_eq!(unread_count, 2);
            assert_eq!(last_read_inbox_message_id.0, 10);
            assert_eq!(last_read_outbox_message_id.0, 11);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn draft_message_parses_partial_quote() {
    // Slice G1: a draft saved with `inputTextQuote` (schema 1.8.67
    // line 3056) restores the quote text and UTF-16 position.
    let env = parse_envelope(
            r#"{"@type":"updateChatDraftMessage","chat_id":11,"draft_message":{"@type":"draftMessage","reply_to":{"@type":"inputMessageReplyToMessage","message_id":101,"quote":{"@type":"inputTextQuote","text":{"@type":"formattedText","text":"meet at","entities":[]},"position":7},"checklist_task_id":0,"poll_option_id":""},"date":1,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"sounds good","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null},"positions":[]}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateChatDraftMessage { draft, .. } => {
            let draft = draft.expect("draft");
            assert_eq!(draft.text, "sounds good");
            assert_eq!(draft.reply_to_message_id, Some(MessageId(101)));
            assert_eq!(draft.quote, Some(("meet at".to_string(), 7)));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn chats_and_found_messages_are_typed() {
    let chats =
        parse_envelope(r#"{"@type":"chats","@extra":"4","total_count":2,"chat_ids":[11,"12"]}"#)
            .unwrap();
    match chats.payload {
        EnvelopePayload::Chats {
            total_count,
            chat_ids,
        } => {
            assert_eq!(total_count, 2);
            assert_eq!(chat_ids, vec![ChatId(11), ChatId(12)]);
        }
        other => panic!("{other:?}"),
    }
    let found = parse_envelope(
            r#"{"@type":"foundMessages","@extra":"5","total_count":1,"next_offset":"n1","messages":[{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_FOUND_hi","entities":[]}}}]}"#,
        )
        .unwrap();
    match found.payload {
        EnvelopePayload::FoundMessages {
            total_count,
            messages,
            next_offset,
        } => {
            assert_eq!(total_count, 1);
            assert_eq!(next_offset, "n1");
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].id.0, 101);
            assert_eq!(messages[0].chat_id.0, 11);
            assert_eq!(messages[0].content.preview(), "CANARY_FOUND_hi");
        }
        other => panic!("{other:?}"),
    }
    let schema = include_str!("../../../schema/td_api.tl");
    assert!(schema.lines().any(|l| l.starts_with("searchChats ")));
    assert!(schema.lines().any(|l| l.starts_with("searchMessages ")));
    assert!(
        schema
            .lines()
            .any(|l| l.starts_with("searchRecentlyFoundChats "))
    );
    assert!(
        schema
            .lines()
            .any(|l| l.starts_with("addRecentlyFoundChat "))
    );
    assert!(schema.lines().any(|l| l.starts_with("chats ")));
    assert!(schema.lines().any(|l| l.starts_with("foundMessages ")));
    let in_chat = parse_envelope(
            r#"{"@type":"foundChatMessages","@extra":"6","total_count":2,"next_from_message_id":"40","messages":[{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_CHAT_FOUND","entities":[]}}}]}"#,
        )
        .unwrap();
    match in_chat.payload {
        EnvelopePayload::FoundChatMessages {
            total_count,
            messages,
            next_from_message_id,
        } => {
            assert_eq!(total_count, 2);
            assert_eq!(next_from_message_id.0, 40);
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].id.0, 101);
            assert_eq!(messages[0].content.preview(), "CANARY_CHAT_FOUND");
        }
        other => panic!("{other:?}"),
    }
    assert!(schema.lines().any(|l| l.starts_with("searchChatMessages ")));
    assert!(schema.lines().any(|l| l.starts_with("foundChatMessages ")));
}

/// MED4: `updateOption` for `message_caption_length_max` (schema:10926)
/// parses to a typed option value; other options parse but are ignored.
#[test]
fn update_option_parses_caption_length_max() {
    let json = r#"{"@type":"updateOption","name":"message_caption_length_max","value":{"@type":"optionValueInteger","value":1024}}"#;
    let payload = parse_payload("updateOption", json).unwrap();
    assert_eq!(
        payload,
        EnvelopePayload::UpdateOption {
            name: "message_caption_length_max".to_string(),
            value: OptionValue::Integer(1024),
        }
    );
    let json = r#"{"@type":"updateOption","name":"some_unknown_option","value":{"@type":"optionValueBoolean","value":true}}"#;
    let payload = parse_payload("updateOption", json).unwrap();
    assert!(matches!(
        payload,
        EnvelopePayload::UpdateOption {
            value: OptionValue::Boolean(true),
            ..
        }
    ));
}
