use crate::composer::SendOptions;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::telegram::envelope::OrderInfoData;
use crate::telegram::requests::*;

#[test]
fn get_storage_statistics_shape_matches_1_8_67() {
    // Phase S2: `getStorageStatistics chat_limit:int32 =
    // StorageStatistics` (schema 1.8.67, line 15781).
    let json = get_storage_statistics(RequestId(41), 0);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getStorageStatistics");
    assert_eq!(v["@extra"], "41");
    assert_eq!(v["chat_limit"], 0);
}

/// M1 fix-up: `send_text` strips blockquote entities for secret chats.
#[test]
fn secret_chat_text_strips_blockquote_only() {
    let json = send_text(
        RequestId(64),
        ChatId(7),
        None,
        "> quoted\n**bold**",
        None,
        &SendOptions {
            is_secret: true,
            ..SendOptions::default()
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let entities = v["input_message_content"]["text"]["entities"]
        .as_array()
        .unwrap();
    assert!(
        entities
            .iter()
            .all(|e| e["type"]["@type"] != "textEntityTypeBlockQuote"),
        "blockquote must be stripped for secret chats: {entities:?}"
    );
    assert!(
        entities
            .iter()
            .any(|e| e["type"]["@type"] == "textEntityTypeBold"),
        "non-blockquote entities survive: {entities:?}"
    );
}

#[test]
fn get_contacts_shape_matches_1_8_67() {
    // `getContacts = Users;` — no parameters (schema 1.8.67, line 14520).
    let json = get_contacts(RequestId(60));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getContacts");
    assert_eq!(v["@extra"], "60");
    assert_eq!(v.as_object().unwrap().len(), 2);
    assert!(!json.contains("CANARY"));
}

#[test]
fn add_contact_shape_matches_1_8_67() {
    // `addContact user_id:int53 contact:importedContact
    // share_phone_number:Bool = Ok;` (schema 1.8.67, line 14513) with
    // `importedContact phone_number:string first_name:string
    // last_name:string note:formattedText` (line 7382).
    let json = add_contact(
        RequestId(61),
        31,
        "+15550131",
        "CANARY-first",
        "CANARY-last",
        "",
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "addContact");
    assert_eq!(v["@extra"], "61");
    assert_eq!(v["user_id"], 31);
    let contact = &v["contact"];
    assert_eq!(contact["@type"], "importedContact");
    assert_eq!(contact["phone_number"], "+15550131");
    assert_eq!(contact["first_name"], "CANARY-first");
    assert_eq!(contact["last_name"], "CANARY-last");
    assert_eq!(contact["note"]["@type"], "formattedText");
    assert_eq!(contact["note"]["text"], "");
    assert_eq!(v["share_phone_number"], false);
}

#[test]
fn a6_remove_contacts_shape_matches_1_8_67() {
    // Slice A6: `removeContacts user_ids:vector<int53> = Ok;`
    // (schema 1.8.67, line 14528).
    let json = remove_contacts(RequestId(61), &[31, 32]);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "removeContacts");
    assert_eq!(v["@extra"], "61");
    assert_eq!(v["user_ids"], serde_json::json!([31, 32]));
}

#[test]
fn a6_clear_imported_contacts_shape_matches_1_8_67() {
    // Slice A6: `clearImportedContacts = Ok;` (schema 1.8.67,
    // line 14539).
    let json = clear_imported_contacts(RequestId(62));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "clearImportedContacts");
    assert_eq!(v["@extra"], "62");
}

#[test]
fn a6_import_contacts_shape_matches_1_8_67() {
    // Slice A6: `importContacts contacts:vector<importedContact> =
    // ImportedContacts;` (schema 1.8.67, line 14517) with
    // `importedContact` (line 7382).
    let contacts = vec![ImportedContact {
        phone_number: "+15550131".to_string(),
        first_name: "CANARY-first".to_string(),
        last_name: "CANARY-last".to_string(),
        note: "met at the demo".to_string(),
    }];
    let json = import_contacts(RequestId(63), &contacts);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "importContacts");
    assert_eq!(v["@extra"], "63");
    let c = &v["contacts"][0];
    assert_eq!(c["@type"], "importedContact");
    assert_eq!(c["phone_number"], "+15550131");
    assert_eq!(c["first_name"], "CANARY-first");
    assert_eq!(c["last_name"], "CANARY-last");
    assert_eq!(c["note"]["@type"], "formattedText");
    assert_eq!(c["note"]["text"], "met at the demo");
    assert_eq!(c["note"]["entities"], serde_json::json!([]));
}

#[test]
fn a6_parse_vcard_maps_the_honest_subset() {
    // What the slice ultimately validates: the RFC 6350 fields
    // that have an `importedContact` home land correctly, and the
    // rest are dropped without breaking the parse.
    let text = "BEGIN:VCARD\r\n\
            VERSION:3.0\r\n\
            N:Doe;John;;;\r\n\
            FN:John Doe\r\n\
            TEL;TYPE=CELL:+1 (555) 013-1\r\n\
            TEL;TYPE=WORK:+1-555-0142\r\n\
            EMAIL:john@example.com\r\n\
            ADR:;;123 Main St;;;;\r\n\
            NOTE:met at the demo\\, likes ponytail\r\n\
            END:VCARD\r\n\
            BEGIN:VCARD\r\n\
            VERSION:3.0\r\n\
            FN:Jane\r\n\
            TEL:+15550155\r\n\
            END:VCARD\r\n\
            BEGIN:VCARD\r\n\
            VERSION:3.0\r\n\
            FN:No Phone\r\n\
            END:VCARD\r\n";
    let (contacts, skipped, _truncated) = parse_vcard(text);
    // John: two numbers → two contacts, CELL first; Jane: one.
    assert_eq!(contacts.len(), 3);
    assert_eq!(contacts[0].phone_number, "+15550131");
    assert_eq!(contacts[0].first_name, "John");
    assert_eq!(contacts[0].last_name, "Doe");
    assert_eq!(contacts[0].note, "met at the demo, likes ponytail");
    assert_eq!(contacts[1].phone_number, "+15550142");
    assert_eq!(contacts[1].first_name, "John");
    assert_eq!(contacts[2].phone_number, "+15550155");
    assert_eq!(contacts[2].first_name, "Jane");
    assert_eq!(contacts[2].last_name, "");
    // The phoneless card is skipped, not imported nameless.
    assert_eq!(skipped, 1);
}

#[test]
fn a6_parse_vcard_handles_folding_and_21_types() {
    // Folded NOTE line + vCard 2.1 bare `TEL;VOICE` param style.
    let text = "BEGIN:VCARD\nVERSION:2.1\nN:Smith;Ada;;;\nTEL;VOICE:+15550199\nNOTE:long note that\n continues here\nEND:VCARD\n";
    let (contacts, skipped, truncated) = parse_vcard(text);
    assert_eq!(skipped, 0);
    assert_eq!(truncated, 0);
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].first_name, "Ada");
    assert_eq!(contacts[0].last_name, "Smith");
    assert_eq!(contacts[0].phone_number, "+15550199");
    assert_eq!(contacts[0].note, "long note thatcontinues here");
}

#[test]
fn a6_parse_vcard_reports_truncation_at_the_limit() {
    // What the slice ultimately validates: an over-limit paste loses
    // cards, and the parser reports exactly how many were dropped so
    // the dialog can say so honestly instead of failing silently.
    let mut text = String::new();
    for i in 0..VCARD_IMPORT_LIMIT + 100 {
        text.push_str(&format!(
            "BEGIN:VCARD\r\nFN:Person {i}\r\nTEL:+1555{i:07}\r\nEND:VCARD\r\n"
        ));
    }
    let (contacts, skipped, truncated) = parse_vcard(&text);
    assert_eq!(contacts.len(), VCARD_IMPORT_LIMIT);
    assert_eq!(skipped, 0);
    assert_eq!(truncated, 100);
}

#[test]
fn get_forum_topics_shape_matches_1_8_67() {
    let json = get_forum_topics(RequestId(31), ChatId(12), "", 0, MessageId(0), 0, 100);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getForumTopics");
    assert_eq!(v["@extra"], "31");
    assert_eq!(v["chat_id"], 12);
    assert_eq!(v["query"], "");
    assert_eq!(v["offset_date"], 0);
    assert_eq!(v["offset_message_id"], 0);
    assert_eq!(v["offset_forum_topic_id"], 0);
    assert_eq!(v["limit"], 100);
    assert!(!json.contains("CANARY"));
}

#[test]
fn sponsored_message_requests_match_1_8_67() {
    let fetch = get_chat_sponsored_messages(RequestId(50), ChatId(13));
    let v: serde_json::Value = serde_json::from_str(&fetch).unwrap();
    assert_eq!(v["@type"], "getChatSponsoredMessages");
    assert_eq!(v["@extra"], "50");
    assert_eq!(v["chat_id"], 13);

    let report = report_chat_sponsored_message(RequestId(51), ChatId(13), 777, "");
    let v: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert_eq!(v["@type"], "reportChatSponsoredMessage");
    assert_eq!(v["chat_id"], 13);
    assert_eq!(v["message_id"], 777);
    assert_eq!(v["option_id"], "");

    let with_option = report_chat_sponsored_message(RequestId(52), ChatId(13), 777, "b3B0aW9u");
    let v: serde_json::Value = serde_json::from_str(&with_option).unwrap();
    assert_eq!(v["option_id"], "b3B0aW9u");

    let view = view_sponsored_chat(RequestId(53), 4242);
    let v: serde_json::Value = serde_json::from_str(&view).unwrap();
    assert_eq!(v["@type"], "viewSponsoredChat");
    assert_eq!(v["sponsored_chat_unique_id"], 4242);

    let click = click_chat_sponsored_message(RequestId(54), ChatId(13), 9001, false, false);
    let v: serde_json::Value = serde_json::from_str(&click).unwrap();
    assert_eq!(v["@type"], "clickChatSponsoredMessage");
    assert_eq!(v["chat_id"], 13);
    assert_eq!(v["message_id"], 9001);
    assert_eq!(v["is_media_click"], false);
    assert_eq!(v["from_fullscreen"], false);

    let media_click = click_chat_sponsored_message(RequestId(55), ChatId(13), 9002, true, false);
    let v: serde_json::Value = serde_json::from_str(&media_click).unwrap();
    assert_eq!(v["is_media_click"], true);
}

#[test]
fn b2_send_bot_start_message_and_get_bot_similar_bots_shapes_match_1_8_67() {
    // Slice B2: `sendBotStartMessage bot_user_id:int53 chat_id:int53
    // parameter:string = Message;` (schema 1.8.67, line 12216) and
    // `getBotSimilarBots bot_user_id:int53 = Users;` (line 11640).
    let json = send_bot_start_message(RequestId(64), 21, 21, "demo_xyz");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendBotStartMessage");
    assert_eq!(v["@extra"], "64");
    assert_eq!(v["bot_user_id"], 21);
    assert_eq!(v["chat_id"], 21);
    assert_eq!(v["parameter"], "demo_xyz");

    let json = get_bot_similar_bots(RequestId(65), 21);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getBotSimilarBots");
    assert_eq!(v["@extra"], "65");
    assert_eq!(v["bot_user_id"], 21);
}

#[test]
fn p1_payment_request_shapes_match_1_8_67() {
    // `getPaymentForm` (schema 1.8.67, line 15262).
    let json = get_payment_form(RequestId(71), ChatId(21), MessageId(401));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getPaymentForm");
    assert_eq!(v["@extra"], "71");
    assert_eq!(v["input_invoice"]["@type"], "inputInvoiceMessage");
    assert_eq!(v["input_invoice"]["chat_id"], 21);
    assert_eq!(v["input_invoice"]["message_id"], 401);
    assert_eq!(v["theme"]["@type"], "themeParameters");
    // `validateOrderInfo` (schema 1.8.67, line 15268).
    let order = OrderInfoData {
        name: "Ada".into(),
        phone_number: "+1".into(),
        email_address: "a@x.io".into(),
        shipping_address: Default::default(),
    };
    let json = validate_order_info(RequestId(72), ChatId(21), MessageId(401), &order, true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "validateOrderInfo");
    assert_eq!(v["order_info"]["@type"], "orderInfo");
    assert_eq!(v["order_info"]["name"], "Ada");
    assert_eq!(v["order_info"]["shipping_address"]["@type"], "address");
    assert_eq!(v["allow_save"], true);
    // `sendPaymentForm` (schema 1.8.67, line 15277).
    let creds = input_credentials_new("tok_test", true);
    let json = send_payment_form(
        RequestId(73),
        ChatId(21),
        MessageId(401),
        7,
        "oi1",
        "fast",
        creds,
        0,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendPaymentForm");
    assert_eq!(v["payment_form_id"], 7);
    assert_eq!(v["order_info_id"], "oi1");
    assert_eq!(v["shipping_option_id"], "fast");
    assert_eq!(v["credentials"]["@type"], "inputCredentialsNew");
    assert_eq!(v["credentials"]["data"], "tok_test");
    assert_eq!(v["tip_amount"], 0);
    let saved = input_credentials_saved("cred1");
    assert_eq!(saved["@type"], "inputCredentialsSaved");
    assert_eq!(saved["saved_credentials_id"], "cred1");
    // `getPaymentReceipt` (schema 1.8.67, line 15280).
    let json = get_payment_receipt(RequestId(74), ChatId(21), MessageId(401));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getPaymentReceipt");
    assert_eq!(v["chat_id"], 21);
    assert_eq!(v["message_id"], 401);
}

#[test]
fn payment_recurring_request_shapes_match_1_8_67() {
    // Slice `parity:bots-payment-recurring`: `getStarSubscriptions`
    // (schema 1.8.67, line 16075).
    let json = get_star_subscriptions(RequestId(81), false, "");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getStarSubscriptions");
    assert_eq!(v["@extra"], "81");
    assert_eq!(v["only_expiring"], false);
    assert_eq!(v["offset"], "");
    let json = get_star_subscriptions(RequestId(82), true, "50");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["only_expiring"], true);
    assert_eq!(v["offset"], "50");
    // `editStarSubscription` (schema 1.8.67, line 16086).
    let json = edit_star_subscription(RequestId(83), "sub1", true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editStarSubscription");
    assert_eq!(v["subscription_id"], "sub1");
    assert_eq!(v["is_canceled"], true);
    // `reuseStarSubscription` (schema 1.8.67, line 16095).
    let json = reuse_star_subscription(RequestId(84), "sub2");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "reuseStarSubscription");
    assert_eq!(v["subscription_id"], "sub2");
}

/// Batch 4 / 6: `setOption` (:15662) and `optimizeStorage` (:15799).
#[test]
fn b6_option_and_optimize_storage_shapes_match_1_8_67() {
    let parse = |json: String| serde_json::from_str::<serde_json::Value>(&json).unwrap();
    let v = parse(set_option_boolean(RequestId(1), "online", true));
    assert_eq!(v["@type"], "setOption");
    assert_eq!(v["name"], "online");
    assert_eq!(
        v["value"],
        serde_json::json!({"@type": "optionValueBoolean", "value": true})
    );
    let v = parse(set_option_integer(
        RequestId(2),
        "storage_max_files_size",
        Some(1024),
    ));
    assert_eq!(
        v["value"],
        serde_json::json!({"@type": "optionValueInteger", "value": "1024"})
    );
    let v = parse(set_option_integer(
        RequestId(3),
        "storage_max_files_size",
        None,
    ));
    assert_eq!(v["value"]["@type"], "optionValueEmpty");

    let v = parse(optimize_storage(
        RequestId(4),
        &OptimizeStorage::everything(50),
    ));
    assert_eq!(v["@type"], "optimizeStorage");
    for key in ["size", "ttl", "count", "immunity_delay"] {
        assert_eq!(v[key], 0, "{key}");
    }
    assert_eq!(v["file_types"], serde_json::json!([]));
    assert_eq!(v["chat_ids"], serde_json::json!([]));
    assert_eq!(v["exclude_chat_ids"], serde_json::json!([]));
    assert_eq!(v["return_deleted_file_statistics"], true);
    assert_eq!(v["chat_limit"], 50);
}

#[test]
fn toggle_has_sponsored_messages_enabled_matches_1_8_67() {
    let v: serde_json::Value =
        serde_json::from_str(&toggle_has_sponsored_messages_enabled(RequestId(60), false)).unwrap();
    assert_eq!(v["@type"], "toggleHasSponsoredMessagesEnabled");
    assert_eq!(v["has_sponsored_messages_enabled"], false);
}
