use super::*;

#[test]
fn supergroup_full_info_parsed() {
    // `getSupergroupFullInfo` → `supergroupFullInfo` (schema 1.8.67
    // lines 11513 / 2792): description + member_count kept.
    let env = parse_envelope(
            r#"{"@type":"supergroupFullInfo","@extra":"5","description":"CANARY group description","member_count":1234,"administrator_count":2}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::SupergroupFullInfo {
            description,
            member_count,
            linked_chat_id,
            slow_mode_delay,
            slow_mode_delay_expires_in,
            my_boost_count,
            unrestrict_boost_count,
            // Phase D2: absent → false (gates the statistics entry point).
            can_get_statistics,
            has_aggressive_anti_spam_enabled: _,
            can_toggle_aggressive_anti_spam: _,
            // Slice S11: absent → false / 0.
            can_set_sticker_set,
            sticker_set_id,
            custom_emoji_sticker_set_id,
            admin: _,
        }) => {
            assert_eq!(description, "CANARY group description");
            assert_eq!(member_count, 1234);
            // Parity slice: no `linked_chat_id` → 0 (no discussion group).
            assert_eq!(linked_chat_id, 0);
            assert!(!can_set_sticker_set);
            assert_eq!(sticker_set_id, 0);
            assert_eq!(custom_emoji_sticker_set_id, 0);
            // Phase A1: slow-mode fields default to 0 when absent.
            assert_eq!(slow_mode_delay, 0);
            assert_eq!(slow_mode_delay_expires_in, 0.0);
            assert_eq!(my_boost_count, 0);
            assert_eq!(unrestrict_boost_count, 0);
            assert!(!can_get_statistics);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn supergroup_full_info_parses_slow_mode_fields() {
    // Phase A1: `slow_mode_delay` / `slow_mode_delay_expires_in`
    // (schema 1.8.67, lines 2758–2759) and the boost bypass counts
    // (lines 2779–2780) are parsed from `supergroupFullInfo`.
    let env = parse_envelope(
            r#"{"@type":"supergroupFullInfo","@extra":"5","description":"d","member_count":10,"slow_mode_delay":30,"slow_mode_delay_expires_in":12.5,"my_boost_count":2,"unrestrict_boost_count":5}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::SupergroupFullInfo {
            slow_mode_delay,
            slow_mode_delay_expires_in,
            my_boost_count,
            unrestrict_boost_count,
            ..
        }) => {
            assert_eq!(slow_mode_delay, 30);
            assert_eq!(slow_mode_delay_expires_in, 12.5);
            assert_eq!(my_boost_count, 2);
            assert_eq!(unrestrict_boost_count, 5);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn supergroup_full_info_parses_linked_chat_id() {
    // Parity slice: `linked_chat_id` (schema 1.8.67 line 2792) feeds the
    // channel header's "Discuss" affordance.
    let env = parse_envelope(
            r#"{"@type":"supergroupFullInfo","@extra":"5","description":"d","member_count":10,"linked_chat_id":77,"direct_messages_chat_id":0}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::SupergroupFullInfo { linked_chat_id, .. }) => {
            assert_eq!(linked_chat_id, 77)
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn message_parses_author_signature() {
    // Phase D2: `message.author_signature` (schema 1.8.67, line 3165)
    // — present → Some; absent or empty → None.
    let json = r#"{"@type":"updateNewMessage","message":{"id":8,"chat_id":4,"is_outgoing":false,"author_signature":"News Desk","content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#;
    let env = parse_envelope(json).unwrap();
    let EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) = env.payload else {
        panic!("expected message");
    };
    assert_eq!(message.author_signature.as_deref(), Some("News Desk"));

    let json = r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":4,"is_outgoing":false,"author_signature":"","content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#;
    let env = parse_envelope(json).unwrap();
    let EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) = env.payload else {
        panic!("expected message");
    };
    assert_eq!(message.author_signature, None);

    let json = r#"{"@type":"updateNewMessage","message":{"id":10,"chat_id":4,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#;
    let env = parse_envelope(json).unwrap();
    let EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) = env.payload else {
        panic!("expected message");
    };
    assert_eq!(message.author_signature, None);
}

#[test]
fn supergroup_full_info_parses_can_get_statistics() {
    // Phase D2: `can_get_statistics` (schema 1.8.67, line 2792) gates
    // the statistics entry point; absent → false.
    let env = parse_envelope(
            r#"{"@type":"supergroupFullInfo","@extra":"5","description":"d","member_count":10,"can_get_statistics":true}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::SupergroupFullInfo {
            can_get_statistics, ..
        }) => assert!(can_get_statistics),
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(
        r#"{"@type":"supergroupFullInfo","@extra":"5","description":"d","member_count":10}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::SupergroupFullInfo {
            can_get_statistics, ..
        }) => assert!(!can_get_statistics),
        other => panic!("{other:?}"),
    }
}

#[test]
fn g2_supergroup_parses_sign_and_anti_spam_fields() {
    // Slice G2: `supergroup.sign_messages` / `show_message_sender`
    // (schema 1.8.67, lines 2731/2746) and
    // `supergroupFullInfo.has_aggressive_anti_spam_enabled` /
    // `can_toggle_aggressive_anti_spam` (line 2792); absent → false.
    let env = parse_envelope(
        r#"{"@type":"supergroup","id":25,"sign_messages":true,"show_message_sender":true}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::Supergroup {
            sign_messages,
            show_message_sender,
            ..
        }) => {
            assert!(sign_messages);
            assert!(show_message_sender);
        }
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(
        r#"{"@type":"updateSupergroup","supergroup":{"id":25,"sign_messages":false}}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::UpdateSupergroup {
            sign_messages,
            show_message_sender,
            ..
        }) => {
            assert!(!sign_messages);
            assert!(!show_message_sender);
        }
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(
            r#"{"@type":"supergroupFullInfo","@extra":"5","description":"d","member_count":10,"has_aggressive_anti_spam_enabled":true,"can_toggle_aggressive_anti_spam":true}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::SupergroupFullInfo {
            has_aggressive_anti_spam_enabled,
            can_toggle_aggressive_anti_spam,
            ..
        }) => {
            assert!(has_aggressive_anti_spam_enabled);
            assert!(can_toggle_aggressive_anti_spam);
        }
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(
            r#"{"@type":"updateSupergroupFullInfo","supergroup_id":13,"supergroup_full_info":{"has_aggressive_anti_spam_enabled":true}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::UpdateSupergroupFullInfo {
            has_aggressive_anti_spam_enabled,
            can_toggle_aggressive_anti_spam,
            ..
        }) => {
            assert!(has_aggressive_anti_spam_enabled);
            assert!(!can_toggle_aggressive_anti_spam);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn g2_welcome_and_boost_payloads_parse() {
    // Slice G2: `updateChatWelcomeMessages` (schema 1.8.67, line
    // 10649) and `updateChatHasWelcomeMessages` (line 10600).
    let env = parse_envelope(
            r#"{"@type":"updateChatWelcomeMessages","chat_id":7,"messages":[{"id":3,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello, newcomer!","entities":[]}}}]}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::UpdateChatWelcomeMessages { chat_id, messages }) => {
            assert_eq!(chat_id, 7);
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].id, 3);
            assert_eq!(messages[0].content.preview(), "Hello, newcomer!");
        }
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(
        r#"{"@type":"updateChatHasWelcomeMessages","chat_id":7,"has_welcome_messages":true}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::UpdateChatHasWelcomeMessages {
            chat_id,
            has_welcome_messages,
        }) => {
            assert_eq!(chat_id, 7);
            assert!(has_welcome_messages);
        }
        other => panic!("{other:?}"),
    }
    // Slice G2: `chatBoostStatus` (line 6943) and `chatBoostSlots`
    // (line 6968).
    let env = parse_envelope(
            r#"{"@type":"chatBoostStatus","@extra":"9","boost_url":"https://t.me/x","level":3,"boost_count":42}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::ChatBoostStatus { level, boost_count }) => {
            assert_eq!(level, 3);
            assert_eq!(boost_count, 42);
        }
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(
            r#"{"@type":"chatBoostSlots","@extra":"9","slots":[{"slot_id":1,"currently_boosted_chat_id":0,"start_date":0,"expiration_date":0,"cooldown_until_date":0}]}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::ChatBoostSlots { slots }) => {
            assert_eq!(slots, vec![1]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_supergroup_full_info_parsed() {
    // Parity slice: `updateSupergroupFullInfo` (schema 1.8.67 line
    // 10750) carries its own `supergroup_id`.
    let env = parse_envelope(
            r#"{"@type":"updateSupergroupFullInfo","supergroup_id":13,"supergroup_full_info":{"@type":"supergroupFullInfo","description":"CANARY channel","member_count":12345,"linked_chat_id":14}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::UpdateSupergroupFullInfo {
            supergroup_id,
            description,
            member_count,
            linked_chat_id,
            slow_mode_delay,
            ..
        }) => {
            assert_eq!(supergroup_id, 13);
            assert_eq!(description, "CANARY channel");
            assert_eq!(member_count, 12345);
            assert_eq!(linked_chat_id, 14);
            // Absent slow-mode fields default to 0.
            assert_eq!(slow_mode_delay, 0);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_supergroup_full_info_parses_slow_mode_fields() {
    // Phase A1: the nested `supergroup_full_info` also carries the
    // slow-mode fields (schema 1.8.67, lines 2758–2759).
    let env = parse_envelope(
            r#"{"@type":"updateSupergroupFullInfo","supergroup_id":13,"supergroup_full_info":{"@type":"supergroupFullInfo","description":"d","member_count":1,"slow_mode_delay":60,"slow_mode_delay_expires_in":44.0}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::UpdateSupergroupFullInfo {
            slow_mode_delay,
            slow_mode_delay_expires_in,
            ..
        }) => {
            assert_eq!(slow_mode_delay, 60);
            assert_eq!(slow_mode_delay_expires_in, 44.0);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn b7_full_info_parses_the_admin_toggle_flags() {
    // B7: `can_hide_members`, `has_hidden_members`, `is_all_history_available`
    // and `can_enable_paid_reaction` (schema 1.8.67, line 2792).
    let env = parse_envelope(
        r#"{"@type":"supergroupFullInfo","@extra":"5","description":"d","member_count":10,"can_hide_members":true,"has_hidden_members":true,"is_all_history_available":true,"can_enable_paid_reaction":true}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::SupergroupFullInfo { admin, .. }) => {
            assert!(admin.can_hide_members);
            assert!(admin.has_hidden_members);
            assert!(admin.is_all_history_available);
            assert!(admin.can_enable_paid_reaction);
        }
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":9,"supergroup_full_info":{"description":"d","member_count":10,"is_all_history_available":true}}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::UpdateSupergroupFullInfo { admin, .. }) => {
            assert!(admin.is_all_history_available);
            // Absent flags read as false.
            assert!(!admin.can_hide_members);
            assert!(!admin.has_hidden_members);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn b7_supergroup_parses_join_to_send_messages() {
    let env = parse_envelope(
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":9,"join_to_send_messages":true}}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::UpdateSupergroup {
            join_to_send_messages,
            ..
        }) => assert!(join_to_send_messages),
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(r#"{"@type":"supergroup","@extra":"1","id":9}"#).unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::Supergroup {
            join_to_send_messages,
            ..
        }) => assert!(!join_to_send_messages),
        other => panic!("{other:?}"),
    }
}

#[test]
fn b7_basic_group_keeps_status_and_activity() {
    let env = parse_envelope(
        r#"{"@type":"updateBasicGroup","basic_group":{"@type":"basicGroup","id":7,"member_count":4,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_change_info":true}},"is_active":false}}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::UpdateBasicGroup {
            status,
            can_change_info,
            is_active,
            ..
        }) => {
            assert_eq!(status, ChannelMemberStatus::Administrator);
            assert_eq!(can_change_info, Some(true));
            assert!(!is_active);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn b7_chat_available_reactions_parse_all_some_and_junk() {
    use crate::telegram::envelope::{ChatAvailableReactions, ReactionType};
    let env = parse_envelope(
        r#"{"@type":"updateChatAvailableReactions","chat_id":5,"available_reactions":{"@type":"chatAvailableReactionsSome","reactions":[{"@type":"reactionTypeEmoji","emoji":"🔥"},{"@type":"reactionTypeCustomEmoji","custom_emoji_id":"42"},{"@type":"reactionTypePaid"}],"max_reaction_count":4}}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Chats(ChatsPayload::UpdateChatAvailableReactions {
            chat_id,
            available_reactions,
        }) => {
            assert_eq!(chat_id, 5);
            assert_eq!(
                available_reactions,
                ChatAvailableReactions::Some {
                    reactions: vec![
                        ReactionType::emoji("🔥"),
                        ReactionType::CustomEmoji {
                            custom_emoji_id: 42
                        },
                        ReactionType::Paid
                    ],
                    max_reaction_count: 4
                }
            );
        }
        other => panic!("{other:?}"),
    }
    // The limit is clamped to 1..=11; an unknown constructor is ignored.
    let env = parse_envelope(
        r#"{"@type":"updateChatAvailableReactions","chat_id":5,"available_reactions":{"@type":"chatAvailableReactionsAll","max_reaction_count":99}}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Chats(ChatsPayload::UpdateChatAvailableReactions {
            available_reactions,
            ..
        }) => assert_eq!(available_reactions.max_reaction_count(), 11),
        other => panic!("{other:?}"),
    }
    assert!(
        parse_envelope(
            r#"{"@type":"updateChatAvailableReactions","chat_id":5,"available_reactions":{"@type":"somethingNew"}}"#
        )
        .is_err()
    );
}
