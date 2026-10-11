use super::*;
use serde_json::{Value, json};

const INFO: &str = r#"{"@type":"businessConnectedBotInfo","bot":{"@type":"businessConnectedBot","bot_user_id":42,"recipients":{"@type":"businessRecipients","chat_ids":[7,8],"excluded_chat_ids":[9],"select_existing_chats":false,"select_new_chats":true,"select_contacts":false,"select_non_contacts":false,"exclude_selected":false},"rights":{"@type":"businessBotRights","can_reply":true,"can_read_messages":false,"can_delete_sent_messages":false,"can_delete_all_messages":true,"can_edit_name":false,"can_edit_bio":false,"can_edit_profile_photo":false,"can_edit_username":false,"can_view_gifts_and_stars":false,"can_sell_gifts":false,"can_change_gift_settings":false,"can_transfer_and_upgrade_gifts":false,"can_transfer_stars":false,"can_manage_stories":true}},"connection_date":1700000000,"device_model":"","location":""}"#;

#[test]
fn parses_the_connected_bot_with_exclusions() {
    let value: Value = serde_json::from_str(INFO).unwrap();
    let bot = parse_connected_bot(&value).unwrap();
    assert_eq!(bot.bot_user_id, 42);
    assert!(!bot.recipients.all_but_excluded);
    assert_eq!(bot.recipients.included.chat_ids, vec![7, 8]);
    assert!(bot.recipients.included.types.new_chats);
    assert_eq!(bot.recipients.excluded.chat_ids, vec![9]);
    assert!(bot.rights.can_reply && bot.rights.can_delete_all_messages);
    assert!(bot.rights.has(BotRight::ManageStories));
    assert!(!bot.rights.has(BotRight::MarkAsRead));
}

#[test]
fn set_request_round_trips() {
    let value: Value = serde_json::from_str(INFO).unwrap();
    let bot = parse_connected_bot(&value).unwrap();
    let body: Value = serde_json::from_str(&set_connected_bot_request("5", &bot)).unwrap();
    assert_eq!(body["@type"], "setBusinessConnectedBot");
    assert_eq!(body["bot"]["bot_user_id"], 42);
    assert_eq!(body["bot"]["recipients"]["excluded_chat_ids"], json!([9]));
    assert_eq!(body["bot"]["rights"]["can_manage_stories"], true);
    assert_eq!(parse_connected_bot(&body).unwrap(), bot);
    let delete: Value = serde_json::from_str(&delete_connected_bot_request("6", 42)).unwrap();
    assert_eq!(delete["@type"], "deleteBusinessConnectedBot");
    assert_eq!(delete["bot_user_id"], 42);
}

#[test]
fn all_except_sends_no_separate_exclusions() {
    let mut r = BusinessRecipients::everyone();
    r.excluded.chat_ids = vec![3];
    let json = bot_recipients_to_json(&r);
    assert_eq!(json["chat_ids"], json!([3]));
    assert_eq!(json["excluded_chat_ids"], json!([]));
    assert_eq!(json["exclude_selected"], true);
}

#[test]
fn bot_picker_rules() {
    let mut r = BusinessRecipients::everyone();
    bot_set_included(
        &mut r,
        BusinessChats {
            types: ChatTypes::default(),
            chat_ids: vec![1, 2],
        },
    );
    assert!(!r.all_but_excluded);
    // Excluding in "Only Selected" keeps the selection, drops types and
    // removes the excluded chat from the included list.
    bot_set_excluded(
        &mut r,
        BusinessChats {
            types: ChatTypes {
                contacts: true,
                ..ChatTypes::default()
            },
            chat_ids: vec![2],
        },
    );
    assert_eq!(r.included.chat_ids, vec![1]);
    assert_eq!(r.excluded.chat_ids, vec![2]);
    assert!(r.excluded.types.is_empty());
    // Including it again takes it out of the exclusions.
    bot_set_included(
        &mut r,
        BusinessChats {
            types: ChatTypes::default(),
            chat_ids: vec![1, 2],
        },
    );
    assert!(r.excluded.chat_ids.is_empty());
}

#[test]
fn rights_and_warnings() {
    let mut now = BotRights::default();
    now.set(BotRight::TransferStars, true);
    assert_eq!(
        BotRights::warning(&BotRights::default(), &now),
        Some("The bot {bot} will be able to transfer your stars.")
    );
    now.set(BotRight::SellGifts, true);
    assert!(
        BotRights::warning(&BotRights::default(), &now)
            .unwrap()
            .contains("gifts and stars")
    );
    assert!(BotRights::warning(&now, &now).is_none());
    let mut username = BotRights::default();
    username.set(BotRight::EditUsername, true);
    assert!(
        BotRights::warning(&BotRights::default(), &username)
            .unwrap()
            .contains("usernames")
    );
    let json = now.to_json();
    assert_eq!(BotRights::from_value(&json), now);
    let count: usize = RIGHT_GROUPS.iter().map(|(_, rights)| rights.len()).sum();
    assert_eq!(count, 14);
}

#[test]
fn username_queries() {
    assert_eq!(
        parse_username_query("@shop_bot").as_deref(),
        Some("shop_bot")
    );
    assert_eq!(
        parse_username_query(" shop_bot ").as_deref(),
        Some("shop_bot")
    );
    assert_eq!(
        parse_username_query("https://t.me/shop_bot").as_deref(),
        Some("shop_bot")
    );
    assert_eq!(
        parse_username_query("t.me/shop_bot?start=1").as_deref(),
        Some("shop_bot")
    );
    assert!(parse_username_query("").is_none());
    assert!(parse_username_query("@").is_none());
    assert!(parse_username_query("@bad name").is_none());
}

#[test]
fn found_bots_merge_like_tdesktop() {
    let usable = |id: i64| id != 5;
    assert_eq!(
        merge_found_bots(Some(1), &[2, 1, 5], &[3, 4], usable),
        vec![1, 2, 3]
    );
    assert_eq!(merge_found_bots(None, &[], &[5], usable), Vec::<i64>::new());
}

#[test]
fn lookup_requests() {
    let exact: Value = serde_json::from_str(&search_public_chat_request("1", "shop_bot")).unwrap();
    assert_eq!(exact["@type"], "searchPublicChat");
    let mine: Value = serde_json::from_str(&search_bots_on_server_request("2", "shop")).unwrap();
    assert_eq!(mine["type_filter"]["@type"], "searchChatTypeFilterBot");
    let global: Value = serde_json::from_str(&search_public_bots_request("3", "shop")).unwrap();
    assert_eq!(global["@type"], "searchPublicChats");
}
