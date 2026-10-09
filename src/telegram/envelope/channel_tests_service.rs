use super::*;

#[test]
fn service_message_screenshot_taken_parsed() {
    // Phase S1: `messageScreenshotTaken` (schema 1.8.67, line 5375)
    // parses to the service-row variant; no fields are kept.
    let json = r#"{"id":503,"chat_id":41,"is_outgoing":false,"content":{"@type":"messageScreenshotTaken"}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    assert!(matches!(parsed.content, MessageContent::ScreenshotTaken));
    assert_eq!(parsed.content.preview(), "Took a screenshot");
}

#[test]
fn service_message_chat_added_to_community_parsed() {
    // Slice C2k: `messageChatAddedToCommunity` (schema 1.8.67, line
    // 5360) keeps only the community id; the preview carries no
    // name because the envelope layer has no name lookup.
    let json = r#"{"id":503,"chat_id":41,"is_outgoing":false,"content":{"@type":"messageChatAddedToCommunity","community_id":123}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    assert!(matches!(
        parsed.content,
        MessageContent::ChatAddedToCommunity { community_id: 123 }
    ));
    assert_eq!(
        parsed.content.preview(),
        "This chat was added to a community"
    );
}

#[test]
fn service_message_chat_added_to_community_missing_id_defaults_to_zero() {
    // Slice C2k: a missing `community_id` must not panic — the parse
    // arm defaults it to 0.
    let json = r#"{"id":505,"chat_id":41,"is_outgoing":false,"content":{"@type":"messageChatAddedToCommunity"}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    assert!(matches!(
        parsed.content,
        MessageContent::ChatAddedToCommunity { community_id: 0 }
    ));
}

#[test]
fn service_message_chat_removed_from_community_parsed() {
    // Slice C2k: `messageChatRemovedFromCommunity` (schema 1.8.67,
    // line 5363) has no fields.
    let json = r#"{"id":504,"chat_id":41,"is_outgoing":false,"content":{"@type":"messageChatRemovedFromCommunity"}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    assert!(matches!(
        parsed.content,
        MessageContent::ChatRemovedFromCommunity
    ));
    assert_eq!(
        parsed.content.preview(),
        "This chat was removed from a community"
    );
}

#[test]
fn service_message_chat_join_from_community_parsed() {
    // Slice G9: `messageChatJoinFromCommunity` (schema 1.8.67, line
    // 5354) keeps only the community id; the preview carries no name
    // because the envelope layer has no name lookup.
    let json = r#"{"id":506,"chat_id":41,"is_outgoing":false,"content":{"@type":"messageChatJoinFromCommunity","community_id":123}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    assert!(matches!(
        parsed.content,
        MessageContent::ChatJoinFromCommunity { community_id: 123 }
    ));
    assert_eq!(
        parsed.content.preview(),
        "Joined the group from the community"
    );
}

#[test]
fn service_message_chat_join_from_community_missing_id_defaults_to_zero() {
    // Slice G9: a missing `community_id` must not panic — the parse
    // arm defaults it to 0.
    let json = r#"{"id":507,"chat_id":41,"is_outgoing":false,"content":{"@type":"messageChatJoinFromCommunity"}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    assert!(matches!(
        parsed.content,
        MessageContent::ChatJoinFromCommunity { community_id: 0 }
    ));
}

#[test]
fn message_date_parsed_and_defaults_to_zero() {
    // kit Phase 4: schema `message.date` (1.8.67, line 3165).
    let json = r#"{"id":504,"chat_id":41,"date":1790631720,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    assert_eq!(parsed.date, 1790631720);
    let json = r#"{"id":505,"chat_id":41,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    assert_eq!(parsed.date, 0);
}

#[test]
fn rich_message_parsed_with_blocks() {
    // M2: `messageRichMessage` (schema 1.8.67, line 5143) parses its
    // `pageBlock*` list; a partial `richMessage` keeps `is_full=false`.
    let json = r#"{"id":601,"chat_id":14,"is_outgoing":false,"content":{"@type":"messageRichMessage","message":{"@type":"richMessage","is_full":false,"is_rtl":false,"blocks":[
            {"@type":"pageBlockTitle","title":{"@type":"richTextPlain","text":"Welcome"}},
            {"@type":"pageBlockParagraph","text":{"@type":"richTexts","texts":[
                {"@type":"richTextPlain","text":"pick "},
                {"@type":"richTextBold","text":{"@type":"richTextPlain","text":"one"}}
            ]}},
            {"@type":"pageBlockButtonRow","buttons":[{"@type":"inlineButton",
                "text":{"@type":"richTextPlain","text":"Vote"},
                "style":{"@type":"buttonStylePrimary"},
                "type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AQID"}}]}
        ]}}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    let MessageContent::RichMessage(rich) = &parsed.content else {
        panic!("expected rich message");
    };
    assert!(!rich.is_full);
    assert_eq!(rich.blocks.len(), 3);
    assert!(matches!(
        &rich.blocks[0],
        crate::rich::RichBlock::Heading { level: 1, .. }
    ));
    let crate::rich::RichBlock::Paragraph { text, entities, .. } = &rich.blocks[1] else {
        panic!("expected paragraph");
    };
    assert_eq!(text, "pick one");
    assert_eq!(entities.len(), 1);
    let crate::rich::RichBlock::ButtonRow { buttons } = &rich.blocks[2] else {
        panic!("expected button row");
    };
    assert_eq!(buttons.len(), 1);
    assert_eq!(buttons[0].text, "Vote");
    assert_eq!(parsed.content.preview(), "Welcome");
    // No `ephemeral_content` field → regular content renders.
    assert!(parsed.ephemeral.is_none());
    assert!(std::ptr::eq(
        effective_content(&parsed.content, parsed.ephemeral.as_ref()),
        &parsed.content
    ));
}

#[test]
fn rich_message_copy_text_joins_text_blocks() {
    // M2: "Copy" on a rich message copies the plain-text form of every
    // text-ish block; buttons/dividers contribute nothing.
    let json = r#"{"id":603,"chat_id":14,"is_outgoing":false,"content":{"@type":"messageRichMessage","message":{"@type":"richMessage","is_full":true,"is_rtl":false,"blocks":[
            {"@type":"pageBlockTitle","title":{"@type":"richTextPlain","text":"Welcome"}},
            {"@type":"pageBlockParagraph","text":{"@type":"richTextPlain","text":"pick one"}},
            {"@type":"pageBlockList","is_ordered":false,"items":[
                {"@type":"pageBlockListItem","label":"a","blocks":[]},
                {"@type":"pageBlockListItem","label":"b","blocks":[]}]},
            {"@type":"pageBlockDivider"},
            {"@type":"pageBlockButtonRow","buttons":[]}
        ]}}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    let MessageContent::RichMessage(rich) = &parsed.content else {
        panic!("expected rich message");
    };
    assert_eq!(rich.copy_text(), "Welcome\npick one\na\nb");
}

#[test]
fn ephemeral_content_parsed_and_wins() {
    // M2: `message.ephemeral_content` (schema 1.8.67, lines 3161/3165)
    // parses and `effective_content` prefers it over the regular
    // content; `null` falls back to the regular content.
    let json = r#"{"id":602,"chat_id":14,"is_outgoing":false,
            "content":{"@type":"messageText","text":{"@type":"formattedText","text":"public","entities":[]}},
            "ephemeral_content":{"@type":"ephemeralMessageContent","can_be_saved":false,"has_timestamped_media":false,
            "content":{"@type":"messageText","text":{"@type":"formattedText","text":"secret flow","entities":[]}},
            "reply_markup":null}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    let ephemeral = parsed.ephemeral.as_ref().expect("ephemeral");
    assert!(matches!(
        ephemeral.content.as_ref(),
        MessageContent::Text(_)
    ));
    let effective = effective_content(&parsed.content, parsed.ephemeral.as_ref());
    assert!(std::ptr::eq(effective, ephemeral.content.as_ref()));

    let json = r#"{"id":603,"chat_id":14,"is_outgoing":false,
            "content":{"@type":"messageText","text":{"@type":"formattedText","text":"public","entities":[]}},
            "ephemeral_content":null}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    assert!(parsed.ephemeral.is_none());
}

/// Phase C2i: `call_entry_label` follows Telegram X's
/// `TD.getCallName` short form — missed/declined are
/// distinguishable by direction; answered calls show direction +
/// duration.
#[test]
fn call_entry_labels_match_telegram_x_convention() {
    use CallDiscardReason::*;
    assert_eq!(call_entry_label(false, &Missed, 0, false), "Missed call");
    assert_eq!(call_entry_label(false, &Missed, 0, true), "Cancelled call");
    assert_eq!(
        call_entry_label(false, &Declined, 0, false),
        "Declined call"
    );
    assert_eq!(call_entry_label(false, &Declined, 0, true), "Busy call");
    assert_eq!(
        call_entry_label(true, &HungUp, 372, false),
        "Incoming video call · 6:12"
    );
    assert_eq!(
        call_entry_label(false, &HungUp, 65, true),
        "Outgoing call · 1:05"
    );
    assert_eq!(
        call_entry_label(false, &Disconnected, 0, false),
        "Incoming call"
    );
}

/// Phase C2i: `messageCall` parses (schema 1.8.67 :5277) — the
/// service-row data for the Calls tab and in-chat rows.
#[test]
fn message_call_parses() {
    let json = r#"{"@type":"message","id":901,"chat_id":71,"is_outgoing":false,"date":1700000000,"content":{"@type":"messageCall","unique_id":901,"is_video":true,"discard_reason":{"@type":"callDiscardReasonHungUp"},"duration":372}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Message(message) => {
            assert_eq!(
                message.content,
                MessageContent::Call {
                    is_video: true,
                    discard_reason: CallDiscardReason::HungUp,
                    duration: 372,
                }
            );
            assert_eq!(
                call_entry_label(true, &CallDiscardReason::HungUp, 372, false),
                "Incoming video call · 6:12"
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn service_message_chat_ttl_changed_parsed() {
    // Phase B4: `messageChatSetMessageAutoDeleteTime` (schema 1.8.67,
    // line 5387) parses to the service-row variant.
    let json = r#"{"id":501,"chat_id":41,"is_outgoing":true,"content":{"@type":"messageChatSetMessageAutoDeleteTime","message_auto_delete_time":3600,"from_user_id":999}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    assert!(matches!(
        parsed.content,
        MessageContent::ChatTtlChanged { secs: 3600 }
    ));
    // `from_user_id` is intentionally not kept.
    assert!(parsed.auto_delete.is_none());
}

#[test]
fn service_message_chat_ttl_disabled_parsed() {
    let json = r#"{"id":502,"chat_id":41,"is_outgoing":false,"content":{"@type":"messageChatSetMessageAutoDeleteTime","message_auto_delete_time":0,"from_user_id":0}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    assert!(matches!(
        parsed.content,
        MessageContent::ChatTtlChanged { secs: 0 }
    ));
}

#[test]
fn auto_delete_in_parsed_as_countdown() {
    // Phase B4: `message.auto_delete_in` (schema 1.8.67, line 3148) —
    // double seconds → whole milliseconds; 0 / absent / garbage →
    // None.
    let json = r#"{"id":503,"chat_id":41,"is_outgoing":false,"auto_delete_in":3595.5,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    let auto_delete = parsed.auto_delete.expect("auto_delete_in parsed");
    assert_eq!(auto_delete.expires_in_ms, 3_595_500);

    for json in [
        r#"{"id":504,"chat_id":41,"is_outgoing":false,"auto_delete_in":0,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}"#,
        r#"{"id":505,"chat_id":41,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}"#,
        r#"{"id":506,"chat_id":41,"is_outgoing":false,"auto_delete_in":-5.0,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}"#,
        r#"{"id":507,"chat_id":41,"is_outgoing":false,"auto_delete_in":"soon","content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}"#,
    ] {
        let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
        assert!(
            parsed.auto_delete.is_none(),
            "auto_delete_in degraded to None: {json}"
        );
    }
}

#[test]
fn format_ttl_setting_cases() {
    // Phase B4: exact-unit labels for the picker values and arbitrary
    // values other clients may set.
    assert_eq!(format_ttl_setting(0), "Off");
    assert_eq!(format_ttl_setting(-5), "Off");
    assert_eq!(format_ttl_setting(5), "5s");
    assert_eq!(format_ttl_setting(30), "30s");
    assert_eq!(format_ttl_setting(60), "1m");
    assert_eq!(format_ttl_setting(90), "90s");
    assert_eq!(format_ttl_setting(3600), "1h");
    assert_eq!(format_ttl_setting(86400), "1d");
    assert_eq!(format_ttl_setting(604800), "7d");
    assert_eq!(format_ttl_setting(2592000), "30d");
}

#[test]
fn chat_ttl_service_label_wording() {
    // Phase B4: secret chats say "Self-destruct", others "Auto-delete".
    assert_eq!(
        chat_ttl_service_label(3600, true),
        "Self-destruct timer set to 1h"
    );
    assert_eq!(
        chat_ttl_service_label(0, true),
        "Self-destruct timer turned off"
    );
    assert_eq!(
        chat_ttl_service_label(86400, false),
        "Auto-delete timer set to 1d"
    );
    assert_eq!(
        chat_ttl_service_label(0, false),
        "Auto-delete timer turned off"
    );
}

#[test]
fn update_chat_folders_parsed() {
    // Phase 7.1: `updateChatFolders` (schema 1.8.67 line 10606) carries
    // `vector<chatFolderInfo>` (line 3485); there is no `getChatFolders`
    // function in 1.8.67, so this update is the folder list.
    let env = parse_envelope(
            r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":3,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]},"animate_custom_emoji":false},"icon":{"@type":"chatFolderIcon","name":"Work"},"color_id":2,"is_shareable":false,"has_my_invite_links":false},{"@type":"chatFolderInfo","id":7,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"News","entities":[]},"animate_custom_emoji":false},"icon":null,"color_id":-1,"is_shareable":false,"has_my_invite_links":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateChatFolders {
            folders,
            are_tags_enabled,
        } => {
            assert_eq!(folders.len(), 2);
            assert!(!are_tags_enabled);
            assert_eq!(
                folders[0],
                ChatFolderInfo {
                    id: 3,
                    name: "Work".into(),
                    icon_name: "Work".into(),
                    color_id: 2,
                }
            );
            assert_eq!(
                folders[1],
                ChatFolderInfo {
                    id: 7,
                    name: "News".into(),
                    icon_name: String::new(),
                    color_id: -1,
                }
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn chat_list_folder_parsed() {
    // `chatListFolder` (schema 1.8.67 line 3524) — folder membership
    // arrives in `chatPosition.list` / added-to / removed-from list.
    let env = parse_envelope(
            r#"{"@type":"updateChatPosition","chat_id":11,"position":{"@type":"chatPosition","list":{"@type":"chatListFolder","chat_folder_id":3},"order":"50","is_pinned":false}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateChatPosition(pos) => {
            assert_eq!(pos.list, ChatList::Folder(3));
            assert_eq!(pos.order, 50);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn suggested_profile_photo_keeps_the_chat_photo_id() {
    // `messageSuggestProfilePhoto` (schema 1.8.67, line 5411): accepting
    // the suggestion needs `chatPhoto.id` for `inputChatPhotoPrevious`.
    let json = r#"{"id":504,"chat_id":41,"is_outgoing":false,"content":{"@type":"messageSuggestProfilePhoto","photo":{"@type":"chatPhoto","id":"987654321","sizes":[{"@type":"photoSize","type":"a","photo":{"@type":"file","id":55,"size":100,"local":{"@type":"localFile","path":"","is_downloading_completed":false},"remote":{"@type":"remoteFile","id":"r55"}},"width":160,"height":160}]}}}"#;
    let parsed = parse_message(&serde_json::from_str(json).unwrap()).unwrap();
    let MessageContent::Action(action) = &parsed.content else {
        panic!("{:?}", parsed.content);
    };
    let ServiceAction::SuggestProfilePhoto { photo, photo_id } = action.as_ref() else {
        panic!("{action:?}");
    };
    assert_eq!(*photo_id, 987_654_321);
    assert!(photo.is_some());
}
