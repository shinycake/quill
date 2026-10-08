use crate::ids::{ChatId, MessageId, RequestId, TopicId};
use crate::telegram::requests::*;
use serde_json::{Value, json};

#[test]
fn cl2_set_pinned_chats_shape_matches_1_8_67() {
    // Slice CL2: `setPinnedChats chat_list:ChatList
    // chat_ids:vector<int53> = Ok;` (schema 1.8.67, line 13681) —
    // the full new pinned order, not a delta.
    let json = set_pinned_chats(RequestId(51), false, &[11, 12, 13]);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setPinnedChats");
    assert_eq!(v["@extra"], "51");
    assert_eq!(v["chat_list"]["@type"], "chatListMain");
    assert_eq!(v["chat_ids"], serde_json::json!([11, 12, 13]));

    let archived = set_pinned_chats(RequestId(52), true, &[7]);
    let v: serde_json::Value = serde_json::from_str(&archived).unwrap();
    assert_eq!(v["chat_list"]["@type"], "chatListArchive");
    assert_eq!(v["chat_ids"], serde_json::json!([7]));
}

#[test]
fn cl2_read_chat_list_shape_matches_1_8_67() {
    // Slice CL2: `readChatList chat_list:ChatList = Ok;` (schema
    // 1.8.67, line 13684).
    let json = read_chat_list(RequestId(53), false);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "readChatList");
    assert_eq!(v["@extra"], "53");
    assert_eq!(v["chat_list"]["@type"], "chatListMain");

    let archived = read_chat_list(RequestId(54), true);
    let v: serde_json::Value = serde_json::from_str(&archived).unwrap();
    assert_eq!(v["chat_list"]["@type"], "chatListArchive");
}

#[test]
fn cl2_clear_recently_found_chats_shape_matches_1_8_67() {
    // Slice CL2: `clearRecentlyFoundChats = Ok;` (schema 1.8.67,
    // line 11671) — no fields.
    let json = clear_recently_found_chats(RequestId(55));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "clearRecentlyFoundChats");
    assert_eq!(v["@extra"], "55");
    assert!(v.get("query").is_none());
}

#[test]
fn cl2_create_private_chat_shape_matches_1_8_67() {
    // Slice CL2: `createPrivateChat user_id:int53 force:Bool =
    // Chat;` (schema 1.8.67, line 13312).
    let json = create_private_chat(RequestId(58), 777, false);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "createPrivateChat");
    assert_eq!(v["@extra"], "58");
    assert_eq!(v["user_id"], 777);
    assert_eq!(v["force"], false);
}

#[test]
fn cl2_archive_chat_list_settings_shapes_match_1_8_67() {
    // Slice CL2: `getArchiveChatListSettings =
    // ArchiveChatListSettings;` (schema 1.8.67, line 13421) and
    // `setArchiveChatListSettings settings:archiveChatListSettings =
    // Ok;` (line 13424); the settings object shape is line 3512.
    let json = get_archive_chat_list_settings(RequestId(56));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getArchiveChatListSettings");
    assert_eq!(v["@extra"], "56");

    let settings = ArchiveChatListSettings {
        archive_and_mute_new_chats_from_unknown_users: true,
        keep_unmuted_chats_archived: false,
        keep_chats_from_folders_archived: true,
    };
    let json = set_archive_chat_list_settings(RequestId(57), settings);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setArchiveChatListSettings");
    assert_eq!(v["@extra"], "57");
    assert_eq!(v["settings"]["@type"], "archiveChatListSettings");
    assert_eq!(
        v["settings"]["archive_and_mute_new_chats_from_unknown_users"],
        true
    );
    assert_eq!(v["settings"]["keep_unmuted_chats_archived"], false);
    assert_eq!(v["settings"]["keep_chats_from_folders_archived"], true);
}

#[test]
fn set_chat_message_auto_delete_time_shape_matches_1_8_67() {
    // Phase B4: `setChatMessageAutoDeleteTime chat_id:int53
    // message_auto_delete_time:int32 = Ok` (schema 1.8.67, line
    // 13454).
    let json = set_chat_message_auto_delete_time(RequestId(21), 41, 3600);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setChatMessageAutoDeleteTime");
    assert_eq!(v["@extra"], "21");
    assert_eq!(v["chat_id"], 41);
    assert_eq!(v["message_auto_delete_time"], 3600);

    let off = set_chat_message_auto_delete_time(RequestId(22), 11, 0);
    let v: serde_json::Value = serde_json::from_str(&off).unwrap();
    assert_eq!(v["message_auto_delete_time"], 0);
}

#[test]
fn get_commands_shape_matches_1_8_67() {
    // `getCommands scope:BotCommandScope language_code:string =
    // BotCommands` (schema 1.8.67 line 14953); a null scope selects the
    // default scope (`botCommandScopeDefault`, line 10360) and an empty
    // language code is allowed.
    let json = get_commands(RequestId(9));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getCommands");
    assert_eq!(v["@extra"], "9");
    assert!(v["scope"].is_null());
    assert_eq!(v["language_code"], "");
}

#[test]
fn get_chat_event_log_shape_matches_1_8_67() {
    // Phase D3c: `getChatEventLog chat_id:int53 query:string
    // from_event_id:int64 limit:int32 filters:chatEventLogFilters
    // user_ids:vector<int53> = ChatEvents;` (schema 1.8.67, line
    // 15252). `filters: None` is the schema's `null` = all event
    // types; a filter set serializes as `chatEventLogFilters`
    // (line 7956) with every field present in schema order.
    let json = get_chat_event_log(RequestId(75), 13, "", 0, 100, None, &[]);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getChatEventLog");
    assert_eq!(v["@extra"], "75");
    assert_eq!(v["chat_id"], 13);
    assert_eq!(v["query"], "");
    assert_eq!(v["from_event_id"], 0);
    assert_eq!(v["limit"], 100);
    assert_eq!(v["filters"], Value::Null);
    assert_eq!(v["user_ids"].as_array().unwrap().len(), 0);

    let filters = ChatEventLogFilterSet {
        member_promotions: true,
        invite_link_changes: true,
        ..Default::default()
    };
    let json = get_chat_event_log(RequestId(76), 13, "", 42, 50, Some(filters), &[7]);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["from_event_id"], 42);
    assert_eq!(v["limit"], 50);
    assert_eq!(v["filters"]["@type"], "chatEventLogFilters");
    assert_eq!(v["filters"]["member_promotions"], true);
    assert_eq!(v["filters"]["invite_link_changes"], true);
    assert_eq!(v["filters"]["message_edits"], false);
    // Every schema field of chatEventLogFilters must be present.
    let schema = include_str!("../../../schema/td_api.tl");
    let filters_line = schema
        .lines()
        .find(|l| l.starts_with("chatEventLogFilters "))
        .expect("chatEventLogFilters in schema");
    for field in filters_line
        .split_whitespace()
        .skip(1)
        .take_while(|token| !token.starts_with('='))
    {
        let name = field.split(':').next().unwrap();
        assert!(
            v["filters"][name].is_boolean(),
            "missing filters field {name}"
        );
    }
    assert_eq!(v["user_ids"], serde_json::json!([7]));
}

#[test]
fn view_messages_uses_chat_history_source() {
    let json = view_messages(
        RequestId(6),
        ChatId(7),
        &[MessageId(11), MessageId(12)],
        "messageSourceChatHistory",
        true,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "viewMessages");
    assert_eq!(v["@extra"], "6");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_ids"], serde_json::json!([11, 12]));
    assert_eq!(v["source"]["@type"], "messageSourceChatHistory");
    assert_eq!(v["force_read"], true);
    assert!(!json.contains("CANARY"));
}

#[test]
fn view_messages_chat_list_source_for_mark_as_read() {
    // Slice CL1: "Mark as read" from the chat list views with
    // `messageSourceChatList`, like Telegram X's
    // `Tdlib.markChatAsRead(..., new MessageSourceChatList(), ...)`.
    let json = view_messages(
        RequestId(6),
        ChatId(7),
        &[MessageId(42)],
        "messageSourceChatList",
        true,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["source"]["@type"], "messageSourceChatList");
    assert_eq!(v["message_ids"], serde_json::json!([42]));
    assert_eq!(v["force_read"], true);
}

#[test]
fn search_public_chats_shape_matches_1_8_67() {
    // `searchPublicChats query:string type_filter:SearchChatTypeFilter =
    // Chats` (schema 1.8.67, line 11609); type_filter null = all types.
    let json = search_public_chats(RequestId(23), "quill");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "searchPublicChats");
    assert_eq!(v["@extra"], "23");
    assert_eq!(v["query"], "quill");
    assert!(v["type_filter"].is_null());
    let schema = include_str!("../../../schema/td_api.tl");
    let line = schema
        .lines()
        .find(|l| l.starts_with("searchPublicChats "))
        .expect("searchPublicChats in schema");
    assert_eq!(
        line,
        "searchPublicChats query:string type_filter:SearchChatTypeFilter = Chats;"
    );
}

#[test]
fn load_chats_folder_list_shape_matches_1_8_67() {
    // `loadChats chat_list:ChatList limit:int32 = Ok` (line 11595) with
    // `chatListFolder` (line 3524) — Phase 7.1 folder load.
    let json = load_chats_list(
        RequestId(24),
        json!({ "@type": "chatListFolder", "chat_folder_id": 3 }),
        100,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "loadChats");
    assert_eq!(v["chat_list"]["@type"], "chatListFolder");
    assert_eq!(v["chat_list"]["chat_folder_id"], 3);
    assert_eq!(v["limit"], 100);
}

#[test]
fn folder_request_shapes_match_1_8_67() {
    use crate::telegram::envelope::ChatFolderSpec;
    let spec = ChatFolderSpec {
        name: "Work".into(),
        pinned_chat_ids: vec![11],
        included_chat_ids: vec![12],
        excluded_chat_ids: vec![],
        exclude_muted: true,
        exclude_read: false,
        exclude_archived: false,
        include_contacts: true,
        include_non_contacts: false,
        include_bots: false,
        include_groups: true,
        include_channels: false,
    };
    // `createChatFolder folder:chatFolder = ChatFolderInfo` (line 13358).
    let v: serde_json::Value =
        serde_json::from_str(&create_chat_folder(RequestId(30), &spec)).unwrap();
    assert_eq!(v["@type"], "createChatFolder");
    assert_eq!(v["folder"]["@type"], "chatFolder");
    assert_eq!(v["folder"]["name"]["text"]["text"], "Work");
    assert!(v["folder"]["icon"].is_null());
    assert_eq!(v["folder"]["color_id"], -1);
    assert_eq!(v["folder"]["pinned_chat_ids"], json!([11]));
    assert_eq!(v["folder"]["included_chat_ids"], json!([12]));
    assert!(v["folder"]["exclude_muted"].as_bool().unwrap());
    assert!(v["folder"]["include_contacts"].as_bool().unwrap());
    assert!(v["folder"]["include_groups"].as_bool().unwrap());
    // `editChatFolder chat_folder_id:int32 folder:chatFolder =
    // ChatFolderInfo` (line 13361).
    let v: serde_json::Value =
        serde_json::from_str(&edit_chat_folder(RequestId(31), 7, &spec)).unwrap();
    assert_eq!(v["@type"], "editChatFolder");
    assert_eq!(v["chat_folder_id"], 7);
    assert_eq!(v["folder"]["@type"], "chatFolder");
    // `deleteChatFolder chat_folder_id:int32 leave_chat_ids:vector<int53>
    // = Ok` (line 13364).
    let v: serde_json::Value =
        serde_json::from_str(&delete_chat_folder(RequestId(32), 7, &[12])).unwrap();
    assert_eq!(v["@type"], "deleteChatFolder");
    assert_eq!(v["chat_folder_id"], 7);
    assert_eq!(v["leave_chat_ids"], json!([12]));
    // `reorderChatFolders chat_folder_ids:vector<int32>
    // main_chat_list_position:int32 = Ok` (line 13373).
    let v: serde_json::Value =
        serde_json::from_str(&reorder_chat_folders(RequestId(33), &[2, 1])).unwrap();
    assert_eq!(v["@type"], "reorderChatFolders");
    assert_eq!(v["chat_folder_ids"], json!([2, 1]));
    assert_eq!(v["main_chat_list_position"], 0);
    // `toggleChatFolderTags are_tags_enabled:Bool = Ok` (line 13376).
    let v: serde_json::Value =
        serde_json::from_str(&toggle_chat_folder_tags(RequestId(34), true)).unwrap();
    assert_eq!(v["@type"], "toggleChatFolderTags");
    assert!(v["are_tags_enabled"].as_bool().unwrap());
    // `getChatFolder chat_folder_id:int32 = ChatFolder` (line 13355).
    let v: serde_json::Value = serde_json::from_str(&get_chat_folder(RequestId(35), 7)).unwrap();
    assert_eq!(v["@type"], "getChatFolder");
    assert_eq!(v["chat_folder_id"], 7);
    // `getChatListsToAddChat chat_id:int53 = ChatLists` (line 13347).
    let v: serde_json::Value =
        serde_json::from_str(&get_chat_lists_to_add_chat(RequestId(36), ChatId(12))).unwrap();
    assert_eq!(v["@type"], "getChatListsToAddChat");
    assert_eq!(v["chat_id"], 12);
    // `addChatToList` with `chatListFolder` (line 13352 + 3524).
    let v: serde_json::Value = serde_json::from_str(&add_chat_to_list_value(
        RequestId(37),
        ChatId(12),
        json!({ "@type": "chatListFolder", "chat_folder_id": 7 }),
    ))
    .unwrap();
    assert_eq!(v["@type"], "addChatToList");
    assert_eq!(v["chat_id"], 12);
    assert_eq!(v["chat_list"]["@type"], "chatListFolder");
    assert_eq!(v["chat_list"]["chat_folder_id"], 7);
}

#[test]
fn search_chats_shape_matches_1_8_67() {
    let json = search_chats(RequestId(21), "alice", 20);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "searchChats");
    assert_eq!(v["@extra"], "21");
    assert_eq!(v["query"], "alice");
    assert_eq!(v["type_filter"], Value::Null);
    assert_eq!(v["limit"], 20);
    assert!(!json.contains("CANARY"));
}

#[test]
fn search_messages_shape_matches_1_8_67() {
    let json = search_messages(RequestId(22), "hello", 20, None);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "searchMessages");
    assert_eq!(v["@extra"], "22");
    assert_eq!(v["chat_list"], Value::Null);
    assert_eq!(v["query"], "hello");
    assert_eq!(v["offset"], "");
    assert_eq!(v["limit"], 20);
    assert_eq!(v["filter"], Value::Null);
    assert_eq!(v["chat_type_filter"], Value::Null);
    assert_eq!(v["min_date"], 0);
    assert_eq!(v["max_date"], 0);
    assert!(!json.contains("chatListMain"));
    assert!(!json.contains("CANARY"));
}

#[test]
fn search_recently_found_and_add_shapes_match_1_8_67() {
    let recents = search_recently_found_chats(RequestId(23), "", 50);
    let v: serde_json::Value = serde_json::from_str(&recents).unwrap();
    assert_eq!(v["@type"], "searchRecentlyFoundChats");
    assert_eq!(v["@extra"], "23");
    assert_eq!(v["query"], "");
    assert_eq!(v["type_filter"], Value::Null);
    assert_eq!(v["limit"], 50);
    let add = add_recently_found_chat(RequestId(24), ChatId(11));
    let v: serde_json::Value = serde_json::from_str(&add).unwrap();
    assert_eq!(v["@type"], "addRecentlyFoundChat");
    assert_eq!(v["@extra"], "24");
    assert_eq!(v["chat_id"], 11);
    assert!(!recents.contains("CANARY"));
    assert!(!add.contains("CANARY"));
}

#[test]
fn set_chat_draft_message_matches_1_8_67() {
    let save = set_chat_draft_message(
        RequestId(41),
        ChatId(11),
        Some("meet at 6"),
        Some(&SendReply::plain(MessageId(101))),
    );
    let v: serde_json::Value = serde_json::from_str(&save).unwrap();
    assert_eq!(v["@type"], "setChatDraftMessage");
    assert_eq!(v["@extra"], "41");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["topic_id"], Value::Null);
    assert_eq!(v["draft_message"]["@type"], "draftMessage");
    assert_eq!(v["draft_message"]["date"], 0);
    assert_eq!(v["draft_message"]["effect_id"], "0");
    assert_eq!(v["draft_message"]["suggested_post_info"], Value::Null);
    assert_eq!(
        v["draft_message"]["content"]["@type"],
        "draftMessageContentText"
    );
    assert_eq!(v["draft_message"]["content"]["text"]["text"], "meet at 6");
    assert_eq!(
        v["draft_message"]["content"]["link_preview_options"],
        Value::Null
    );
    assert_eq!(
        v["draft_message"]["reply_to"]["@type"],
        "inputMessageReplyToMessage"
    );
    assert_eq!(v["draft_message"]["reply_to"]["message_id"], 101);
    assert_eq!(
        v["draft_message"]["reply_to"]["quote"],
        Value::Null,
        "whole-message draft reply carries no quote"
    );
    let quoted = set_chat_draft_message(
        RequestId(43),
        ChatId(11),
        Some("agree"),
        Some(&SendReply {
            message_id: MessageId(101),
            quote: Some(("meet at".to_string(), 0)),
        }),
    );
    let v: serde_json::Value = serde_json::from_str(&quoted).unwrap();
    assert_eq!(
        v["draft_message"]["reply_to"]["quote"]["@type"],
        "inputTextQuote"
    );
    assert_eq!(
        v["draft_message"]["reply_to"]["quote"]["text"]["text"],
        "meet at"
    );
    assert_eq!(v["draft_message"]["reply_to"]["quote"]["position"], 0);
    let clear = set_chat_draft_message(RequestId(42), ChatId(11), None, None);
    let v: serde_json::Value = serde_json::from_str(&clear).unwrap();
    assert_eq!(v["draft_message"], Value::Null);
    assert!(!save.contains("CANARY"));
}

#[test]
fn set_chat_slow_mode_delay_shape_matches_1_8_67() {
    // `setChatSlowModeDelay chat_id:int53 slow_mode_delay:int32 = Ok;`
    // (schema 1.8.67, line 13551).
    let json = set_chat_slow_mode_delay(RequestId(44), ChatId(11), 30);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setChatSlowModeDelay");
    assert_eq!(v["@extra"], "44");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["slow_mode_delay"], 30);
    assert!(!json.contains("CANARY"));
}

#[test]
fn pin_and_unpin_chat_message_shape_matches_1_8_67() {
    let pin = pin_chat_message(RequestId(40), ChatId(11), MessageId(101), false, false);
    let v: serde_json::Value = serde_json::from_str(&pin).unwrap();
    assert_eq!(v["@type"], "pinChatMessage");
    assert_eq!(v["@extra"], "40");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["message_id"], 101);
    assert_eq!(v["disable_notification"], false);
    assert_eq!(v["only_for_self"], false);
    assert!(!pin.contains("CANARY"));
    assert!(!pin.contains("unpinAllChatMessages"));

    let unpin = unpin_chat_message(RequestId(41), ChatId(11), MessageId(101));
    let v: serde_json::Value = serde_json::from_str(&unpin).unwrap();
    assert_eq!(v["@type"], "unpinChatMessage");
    assert_eq!(v["@extra"], "41");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["message_id"], 101);
    assert!(!unpin.contains("disable_notification"));
    assert!(!unpin.contains("only_for_self"));
    assert!(!unpin.contains("CANARY"));
}

#[test]
fn search_chat_messages_shape_matches_1_8_67() {
    let json = search_chat_messages(
        RequestId(25),
        ChatId(11),
        &TopicId::None,
        "hello",
        MessageId(0),
        0,
        50,
        None,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "searchChatMessages");
    assert_eq!(v["@extra"], "25");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["topic_id"], Value::Null);
    assert_eq!(v["query"], "hello");
    assert_eq!(v["sender_id"], Value::Null);
    assert_eq!(v["from_message_id"], 0);
    assert_eq!(v["offset"], 0);
    assert_eq!(v["limit"], 50);
    assert_eq!(v["filter"], Value::Null);
    assert!(!json.contains("CANARY"));
    assert!(!json.contains("message_thread_id"));
}

#[test]
fn search_chat_messages_filter_carries_tab_constructor() {
    // Slice media-shared-gallery: the gallery tabs send the tab's
    // `searchMessagesFilter*` constructor in `filter`
    // (schema/td_api.tl:11864).
    for (tab, constructor) in [
        (
            crate::state::SharedMediaTab::Media,
            "searchMessagesFilterPhotoAndVideo",
        ),
        (
            crate::state::SharedMediaTab::Files,
            "searchMessagesFilterDocument",
        ),
        (
            crate::state::SharedMediaTab::Music,
            "searchMessagesFilterAudio",
        ),
        (
            crate::state::SharedMediaTab::Links,
            "searchMessagesFilterUrl",
        ),
        (
            crate::state::SharedMediaTab::Voice,
            "searchMessagesFilterVoiceNote",
        ),
        (
            crate::state::SharedMediaTab::Gifs,
            "searchMessagesFilterAnimation",
        ),
    ] {
        assert_eq!(tab.filter_constructor(), constructor);
        let json = search_chat_messages(
            RequestId(27),
            ChatId(11),
            &TopicId::None,
            "",
            MessageId(0),
            0,
            50,
            Some(search_messages_filter_json(constructor)),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["filter"]["@type"], constructor);
        assert_eq!(v["query"], "");
    }
}

#[test]
fn search_chat_messages_forum_topic_uses_message_topic_forum() {
    let json = search_chat_messages(
        RequestId(26),
        ChatId(11),
        &TopicId::Forum { forum_topic_id: 7 },
        "",
        MessageId(0),
        0,
        50,
        None,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "searchChatMessages");
    assert_eq!(v["topic_id"]["@type"], "messageTopicForum");
    assert_eq!(v["topic_id"]["forum_topic_id"], 7);
    assert_eq!(v["query"], "");
}

#[test]
fn topic_id_json_variants() {
    assert_eq!(topic_id_json(&TopicId::None), Value::Null);
    let dm = topic_id_json(&TopicId::DirectMessages {
        direct_messages_chat_topic_id: 42,
    });
    assert_eq!(dm["@type"], "messageTopicDirectMessages");
    assert_eq!(dm["direct_messages_chat_topic_id"], 42);
    let sm = topic_id_json(&TopicId::SavedMessages {
        saved_messages_topic_id: 9,
    });
    assert_eq!(sm["@type"], "messageTopicSavedMessages");
    assert_eq!(sm["saved_messages_topic_id"], 9);
}

#[test]
fn open_and_close_chat_are_distinct_from_client_close() {
    let open = open_chat(RequestId(8), ChatId(3));
    let close = close_chat(RequestId(9), ChatId(3));
    let client_close = close_request(RequestId(10));
    assert!(open.contains("\"@type\":\"openChat\""));
    assert!(close.contains("\"@type\":\"closeChat\""));
    assert!(client_close.contains("\"@type\":\"close\""));
    assert!(!client_close.contains("closeChat"));
    assert_eq!(serde_json::from_str::<Value>(&close).unwrap()["chat_id"], 3);
}

#[test]
fn send_chat_action_typing_and_cancel_match_1_8_67() {
    let typing = send_chat_action(RequestId(42), ChatId(11), true);
    let v: serde_json::Value = serde_json::from_str(&typing).unwrap();
    assert_eq!(v["@type"], "sendChatAction");
    assert_eq!(v["@extra"], "42");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["topic_id"], Value::Null);
    assert_eq!(v["business_connection_id"], "");
    assert_eq!(v["action"]["@type"], "chatActionTyping");
    assert!(!typing.contains("CANARY"));
    assert!(!typing.contains("message_thread_id"));

    let cancel = send_chat_action(RequestId(43), ChatId(11), false);
    let v: serde_json::Value = serde_json::from_str(&cancel).unwrap();
    assert_eq!(v["action"]["@type"], "chatActionCancel");
    assert!(!cancel.contains("chatActionTyping"));

    let recording =
        send_chat_action_kind(RequestId(44), ChatId(11), "chatActionRecordingVoiceNote");
    let v: serde_json::Value = serde_json::from_str(&recording).unwrap();
    assert_eq!(v["action"]["@type"], "chatActionRecordingVoiceNote");
}

#[test]
fn cl3_report_chat_shape_matches_1_8_67() {
    // Slice CL3: `reportChat chat_id:int53 option_id:bytes
    // message_ids:vector<int53> text:string = ReportChatResult;`
    // (schema 1.8.67, line 15693) — the simple spam-report flow
    // sends empty option_id/message_ids/text (schema:3667).
    let json = report_chat(RequestId(61), 11);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "reportChat");
    assert_eq!(v["@extra"], "61");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["option_id"], "");
    assert_eq!(v["message_ids"], serde_json::json!([]));
    assert_eq!(v["text"], "");
}

#[test]
fn cl3_set_message_sender_block_list_shapes_match_1_8_67() {
    // Slice CL3: `setMessageSenderBlockList sender_id:MessageSender
    // block_list:BlockList = Ok;` (schema 1.8.67, line 14492) —
    // block uses `blockListMain` (line 9692); unblock passes null
    // `block_list` (TGX `Tdlib.unblockSender`).
    let json = set_message_sender_block_list(RequestId(62), 11, true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setMessageSenderBlockList");
    assert_eq!(v["@extra"], "62");
    assert_eq!(v["sender_id"]["@type"], "messageSenderUser");
    assert_eq!(v["sender_id"]["user_id"], 11);
    assert_eq!(v["block_list"]["@type"], "blockListMain");

    let json = set_message_sender_block_list(RequestId(63), 11, false);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setMessageSenderBlockList");
    assert!(v["block_list"].is_null());
}

#[test]
fn reset_all_notification_settings_shape_matches_1_8_67() {
    // Parity slice: `resetAllNotificationSettings = Ok;` (schema 1.8.67,
    // line 13671) — parameterless; resets all chat and scope notification
    // settings to their default values.
    let json = reset_all_notification_settings(RequestId(64));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "resetAllNotificationSettings");
    assert_eq!(v["@extra"], "64");
}

#[test]
fn batch8_action_bar_request_shapes_match_1_8_67() {
    // `removeChatActionBar chat_id:int53 = Ok;` and
    // `sharePhoneNumber user_id:int53 = Ok;`
    let v: serde_json::Value =
        serde_json::from_str(&remove_chat_action_bar(RequestId(71), 11)).unwrap();
    assert_eq!(v["@type"], "removeChatActionBar");
    assert_eq!(v["@extra"], "71");
    assert_eq!(v["chat_id"], 11);
    let v: serde_json::Value =
        serde_json::from_str(&share_phone_number(RequestId(72), 99)).unwrap();
    assert_eq!(v["@type"], "sharePhoneNumber");
    assert_eq!(v["user_id"], 99);
}
