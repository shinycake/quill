use crate::composer::{ComposerScheduling, PreviewMediaSize, SendOptions};
use crate::ids::{ChatId, MessageId, RequestId};
use crate::telegram::requests::*;
use serde_json::Value;

#[test]
fn send_text_includes_topic_id_null() {
    let json = send_text(
        RequestId(9),
        ChatId(1),
        None,
        "hi",
        None,
        &SendOptions::default(),
    );
    assert!(json.contains("\"topic_id\":null"));
    assert!(!json.contains("message_thread_id"));
    assert!(json.contains("\"@extra\":\"9\""));
    assert!(json.contains("\"reply_to\":null"));
}

#[test]
fn send_rich_message_shape_matches_1_8_67() {
    // M2: `sendMessage` + `inputMessageRichMessage message:inputRichMessage
    // clear_draft:Bool = InputMessageContent` (schema 1.8.67, line 6084).
    let rich = crate::rich::input_rich_message(&[crate::rich::RichBlock::Paragraph {
        text: "hi".into(),
        entities: Vec::new(),
        buttons: Vec::new(),
    }])
    .expect("blocks");
    let json = send_rich_message(
        RequestId(60),
        ChatId(11),
        None,
        &rich,
        None,
        &SendOptions {
            disable_notification: true,
            ..SendOptions::default()
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["@extra"], "60");
    let content = &v["input_message_content"];
    assert_eq!(content["@type"], "inputMessageRichMessage");
    assert_eq!(content["clear_draft"], true);
    assert_eq!(content["message"]["@type"], "inputRichMessage");
    assert_eq!(
        content["message"]["source"]["blocks"][0]["@type"],
        "inputPageBlockParagraph"
    );
    assert_eq!(v["options"]["disable_notification"], true);
    assert!(v["reply_markup"].is_null());
}

#[test]
fn get_full_rich_message_shape_matches_1_8_67() {
    // M2: `getFullRichMessage chat_id:int53 message_id:int53 =
    // RichMessage` (schema 1.8.67, line 11554).
    let json = get_full_rich_message(RequestId(61), ChatId(11), MessageId(22));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getFullRichMessage");
    assert_eq!(v["@extra"], "61");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["message_id"], 22);
}

#[test]
fn send_text_topic_id_uses_message_topic_forum() {
    // Parity slice 4: `sendMessage.topic_id` (schema 1.8.67, line 12200)
    // takes `messageTopicForum{forum_topic_id}` (line 3004).
    let json = send_text(
        RequestId(9),
        ChatId(16),
        Some(2),
        "hi",
        None,
        &SendOptions::default(),
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["chat_id"], 16);
    assert_eq!(v["topic_id"]["@type"], "messageTopicForum");
    assert_eq!(v["topic_id"]["forum_topic_id"], 2);
}

#[test]
fn send_text_reply_uses_input_message_reply_to_message() {
    let json = send_text(
        RequestId(10),
        ChatId(11),
        None,
        "sounds good",
        Some(SendReply::plain(MessageId(101))),
        &SendOptions::default(),
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
    assert_eq!(v["reply_to"]["message_id"], 101);
    assert_eq!(v["reply_to"]["quote"], Value::Null);
    assert_eq!(v["reply_to"]["checklist_task_id"], 0);
    assert_eq!(v["reply_to"]["poll_option_id"], "");
    assert!(!json.contains("inputMessageReplyToExternalMessage"));
    assert!(!json.contains("CANARY"));
}

#[test]
fn send_text_disables_link_preview_for_secret_chats() {
    // Phase S1: secret chats never get link previews (TGX default-off).
    let json = send_text(
        RequestId(11),
        ChatId(41),
        None,
        "see https://example.com",
        None,
        &SendOptions {
            link_preview_disabled: true,
            ..SendOptions::default()
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let opts = &v["input_message_content"]["link_preview_options"];
    assert_eq!(opts["@type"], "linkPreviewOptions");
    assert_eq!(opts["is_disabled"], true);

    let json = send_text(
        RequestId(12),
        ChatId(11),
        None,
        "see https://example.com",
        None,
        &SendOptions::default(),
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["input_message_content"]["link_preview_options"],
        Value::Null
    );
}

/// M1 fix-up: captions parse composer markup into entities, exactly
/// like message text (`**bold**` in a caption must not go out
/// literal).
#[test]
fn photo_caption_parses_markup_into_entities() {
    let json = send_photo(
        RequestId(61),
        ChatId(7),
        None,
        "/tmp/picked.png",
        "**bold** and plain",
        false,
        None,
        None,
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let caption = &v["input_message_content"]["caption"];
    assert_eq!(caption["text"], "bold and plain");
    let entities = caption["entities"].as_array().unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0]["type"]["@type"], "textEntityTypeBold");
    assert_eq!(entities[0]["offset"], 0);
    assert_eq!(entities[0]["length"], 4);
}

/// M1 fix-up: `editMessageCaption` parses markup too.
#[test]
fn edit_caption_parses_markup_into_entities() {
    let json = edit_message_caption(
        RequestId(62),
        ChatId(11),
        MessageId(60),
        "*italic* cap",
        false,
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let caption = &v["caption"];
    assert_eq!(caption["text"], "italic cap");
    let entities = caption["entities"].as_array().unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0]["type"]["@type"], "textEntityTypeItalic");
}

/// M1 fix-up: `textEntityTypeBlockQuote` is stripped for secret chats
/// (schema: unsupported there); other entities survive.
#[test]
fn secret_chat_captions_strip_blockquote_only() {
    let json = send_photo(
        RequestId(63),
        ChatId(7),
        None,
        "/tmp/picked.png",
        "> quoted\n**bold**",
        false,
        None,
        None,
        true,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let caption = &v["input_message_content"]["caption"];
    let entities = caption["entities"].as_array().unwrap();
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

/// MED4: `getWebPageInstantView` (schema 1.8.67, line 14797) —
/// `only_local: false`; a 404 from TDLib means "no Instant View"
/// and the caller falls back to the browser.
#[test]
fn get_web_page_instant_view_shape() {
    let json = get_web_page_instant_view(RequestId(5), "https://example.com/article");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getWebPageInstantView");
    assert_eq!(v["@extra"], "5");
    assert_eq!(v["url"], "https://example.com/article");
    assert_eq!(v["only_local"], false);
}

/// MED4: `show_caption_above_media` rides `inputMessagePhoto` /
/// `inputMessageVideo` (schema 1.8.67, lines 6117/6128).
#[test]
fn caption_above_media_wire() {
    let json = send_photo(
        RequestId(11),
        ChatId(7),
        None,
        "/tmp/picked.png",
        "cap",
        true,
        None,
        None,
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["input_message_content"]["show_caption_above_media"], true);
    let json = send_video(
        RequestId(17),
        ChatId(7),
        None,
        "/tmp/picked.mp4",
        &VideoSend {
            duration: 1,
            width: 320,
            height: 180,
            supports_streaming: true,
            self_destruct: None,
        },
        "cap",
        true,
        None,
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["input_message_content"]["show_caption_above_media"], true);
}

#[test]
fn extra_is_decimal_string_not_float() {
    let json = get_authorization_state(RequestId(9007199254740993));
    assert!(json.contains("\"@extra\":\"9007199254740993\""));
    assert!(!json.contains("\"@extra\":9007199254740993"));
}

#[test]
fn get_callback_query_answer_shape_matches_1_8_67() {
    // `getCallbackQueryAnswer chat_id:int53 message_id:int53
    // payload:CallbackQueryPayload = CallbackQueryAnswer` (schema line
    // 13138); `callbackQueryPayloadData data:bytes` (line 7737) with
    // base64 `bytes` in JSON.
    let json = get_callback_query_answer(RequestId(51), ChatId(21), MessageId(301), &[1, 2, 3]);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getCallbackQueryAnswer");
    assert_eq!(v["@extra"], "51");
    assert_eq!(v["chat_id"], 21);
    assert_eq!(v["message_id"], 301);
    assert_eq!(v["payload"]["@type"], "callbackQueryPayloadData");
    assert_eq!(v["payload"]["data"], "AQID");
    assert!(!json.contains("answerCallbackQuery"));
    assert!(!json.contains("CANARY"));
}

#[test]
fn edit_message_text_shape_matches_1_8_67() {
    let json = edit_message_text(
        RequestId(31),
        ChatId(11),
        MessageId(102),
        "edited body",
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editMessageText");
    assert_eq!(v["@extra"], "31");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["message_id"], 102);
    assert_eq!(v["reply_markup"], Value::Null);
    assert_eq!(v["input_message_content"]["@type"], "inputMessageText");
    assert_eq!(v["input_message_content"]["text"]["@type"], "formattedText");
    assert_eq!(v["input_message_content"]["text"]["text"], "edited body");
    assert_eq!(
        v["input_message_content"]["text"]["entities"],
        serde_json::json!([])
    );
    assert_eq!(
        v["input_message_content"]["link_preview_options"],
        Value::Null
    );
    assert_eq!(v["input_message_content"]["clear_draft"], false);
    assert!(!json.contains("CANARY"));
    assert!(!json.contains("message_thread_id"));
}

#[test]
fn edit_message_caption_shape_matches_1_8_67() {
    let json = edit_message_caption(
        RequestId(32),
        ChatId(11),
        MessageId(60),
        "new cap",
        false,
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editMessageCaption");
    assert_eq!(v["@extra"], "32");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["message_id"], 60);
    assert_eq!(v["reply_markup"], Value::Null);
    assert_eq!(v["caption"]["@type"], "formattedText");
    assert_eq!(v["caption"]["text"], "new cap");
    assert_eq!(v["caption"]["entities"], serde_json::json!([]));
    assert_eq!(v["show_caption_above_media"], false);
    assert!(!json.contains("CANARY"));
}

#[test]
fn delete_messages_shape_matches_1_8_67() {
    let json = delete_messages(RequestId(33), ChatId(11), &[MessageId(102)], true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "deleteMessages");
    assert_eq!(v["@extra"], "33");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["message_ids"], serde_json::json!([102]));
    assert_eq!(v["revoke"], true);
    assert!(!json.contains("CANARY"));
}

#[test]
fn forward_messages_shape_matches_1_8_67() {
    let json = forward_messages(
        RequestId(34),
        ChatId(12),
        ChatId(11),
        &[MessageId(101), MessageId(102)],
        false,
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "forwardMessages");
    assert_eq!(v["@extra"], "34");
    assert_eq!(v["chat_id"], 12);
    assert_eq!(v["topic_id"], Value::Null);
    assert_eq!(v["from_chat_id"], 11);
    assert_eq!(v["message_ids"], serde_json::json!([101, 102]));
    assert_eq!(v["options"], Value::Null);
    assert_eq!(v["send_copy"], false);
    assert_eq!(v["remove_caption"], false);
    assert!(!json.contains("CANARY"));
    assert!(!json.contains("message_thread_id"));
    assert!(!json.contains("inputMessageForwarded"));
}

#[test]
fn add_and_remove_message_reaction_shapes_match_1_8_67() {
    let add = add_message_reaction(RequestId(35), ChatId(11), MessageId(101), "❤", false, true);
    let v: serde_json::Value = serde_json::from_str(&add).unwrap();
    assert_eq!(v["@type"], "addMessageReaction");
    assert_eq!(v["@extra"], "35");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["message_id"], 101);
    assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
    assert_eq!(v["reaction_type"]["emoji"], "❤");
    assert_eq!(v["is_big"], false);
    assert_eq!(v["update_recent_reactions"], true);
    assert!(!add.contains("CANARY"));
    assert!(!add.contains("setMessageReactions"));
    assert!(!add.contains("reactionTypeCustomEmoji"));
    assert!(!add.contains("reactionTypePaid"));

    let remove = remove_message_reaction(RequestId(36), ChatId(11), MessageId(101), "❤");
    let v: serde_json::Value = serde_json::from_str(&remove).unwrap();
    assert_eq!(v["@type"], "removeMessageReaction");
    assert_eq!(v["@extra"], "36");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["message_id"], 101);
    assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
    assert_eq!(v["reaction_type"]["emoji"], "❤");
    assert!(!remove.contains("is_big"));
    assert!(!remove.contains("update_recent_reactions"));
    assert!(!remove.contains("CANARY"));
}

#[test]
fn recognize_speech_shape_matches_1_8_67() {
    // MED2: what the request is ultimately validating — the
    // `recognizeSpeech` constructor with chat and message ids.
    let json = recognize_speech(RequestId(21), ChatId(7), MessageId(9));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "recognizeSpeech");
    assert_eq!(v["@extra"], "21");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_id"], 9);
}

// M1: composer markup reaches the wire as `textEntities` with UTF-16
// offsets (schema 1.8.67 lines 5743–5773); the marker syntax never
// leaks into the sent text.
#[test]
fn send_text_converts_markup_to_entities() {
    let json = send_text(
        RequestId(50),
        ChatId(11),
        None,
        "😀 **bold** and `code`",
        None,
        &SendOptions::default(),
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let content = &v["input_message_content"]["text"];
    assert_eq!(content["text"], "😀 bold and code");
    let entities = content["entities"].as_array().unwrap();
    assert_eq!(entities.len(), 2);
    assert_eq!(entities[0]["type"]["@type"], "textEntityTypeBold");
    assert_eq!(entities[0]["offset"], 3); // 😀 = 2 UTF-16 units + space
    assert_eq!(entities[0]["length"], 4);
    assert_eq!(entities[1]["type"]["@type"], "textEntityTypeCode");
    assert_eq!(entities[1]["offset"], 12);
    assert_eq!(entities[1]["length"], 4);
}

#[test]
fn send_text_link_preview_disabled_on_wire() {
    let json = send_text(
        RequestId(51),
        ChatId(11),
        None,
        "see https://example.com",
        None,
        &SendOptions {
            link_preview_disabled: true,
            ..SendOptions::default()
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let opts = &v["input_message_content"]["link_preview_options"];
    assert_eq!(opts["@type"], "linkPreviewOptions");
    assert_eq!(opts["is_disabled"], true);
}

/// MED4b: `getLinkPreview` shape matches the pinned schema
/// (`schema/td_api.tl:14792`).
#[test]
fn get_link_preview_shape_matches_1_8_67() {
    let json = get_link_preview(RequestId(61), "https://example.com/story");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getLinkPreview");
    assert_eq!(v["@extra"], "61");
    assert_eq!(v["text"]["@type"], "formattedText");
    assert_eq!(v["text"]["text"], "https://example.com/story");
    assert_eq!(v["link_preview_options"], Value::Null);
    let schema = include_str!("../../../schema/td_api.tl");
    let line = schema
        .lines()
        .find(|l| l.starts_with("getLinkPreview "))
        .expect("getLinkPreview in schema");
    assert_eq!(
        line,
        "getLinkPreview text:formattedText link_preview_options:linkPreviewOptions = LinkPreview;"
    );
}

/// MED4b: above-text + force-large ride `inputMessageText`;
/// the force flags require the explicit URL (schema:2234-2235).
#[test]
fn send_text_link_preview_full_options_on_wire() {
    let json = send_text(
        RequestId(62),
        ChatId(11),
        None,
        "see https://example.com/story",
        None,
        &SendOptions {
            link_preview_above_text: true,
            link_preview_media: PreviewMediaSize::ForceLarge,
            ..SendOptions::default()
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let opts = &v["input_message_content"]["link_preview_options"];
    assert_eq!(opts["@type"], "linkPreviewOptions");
    assert_eq!(opts["is_disabled"], false);
    assert_eq!(opts["url"], "https://example.com/story");
    assert_eq!(opts["force_small_media"], false);
    assert_eq!(opts["force_large_media"], true);
    assert_eq!(opts["show_above_text"], true);
    // Above-text alone (no force) still sends the options object with
    // the detected URL — equivalent to empty per the schema (first
    // URL is used), and keeps one code path.
    let json = send_text(
        RequestId(63),
        ChatId(11),
        None,
        "see https://example.com/story",
        None,
        &SendOptions {
            link_preview_above_text: true,
            ..SendOptions::default()
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let opts = &v["input_message_content"]["link_preview_options"];
    assert_eq!(opts["show_above_text"], true);
    assert_eq!(opts["url"], "https://example.com/story");
    assert_eq!(opts["force_small_media"], false);
    assert_eq!(opts["force_large_media"], false);
    // Defaults keep the old behavior: null options.
    let json = send_text(
        RequestId(64),
        ChatId(11),
        None,
        "see https://example.com/story",
        None,
        &SendOptions::default(),
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["input_message_content"]["link_preview_options"],
        Value::Null
    );
}

#[test]
fn send_text_silent_and_scheduled_options() {
    let json = send_text(
        RequestId(52),
        ChatId(11),
        None,
        "hi",
        None,
        &SendOptions {
            disable_notification: true,
            scheduling: ComposerScheduling::SendAtDate(1_700_000_000),
            ..SendOptions::default()
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let options = &v["options"];
    assert_eq!(options["@type"], "messageSendOptions");
    assert_eq!(options["disable_notification"], true);
    assert_eq!(
        options["scheduling_state"]["@type"],
        "messageSchedulingStateSendAtDate"
    );
    assert_eq!(options["scheduling_state"]["send_date"], 1_700_000_000);
    assert_eq!(options["scheduling_state"]["repeat_period"], 0);

    let json = send_text(
        RequestId(53),
        ChatId(11),
        None,
        "hi",
        None,
        &SendOptions {
            scheduling: ComposerScheduling::SendWhenOnline,
            ..SendOptions::default()
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["options"]["scheduling_state"]["@type"],
        "messageSchedulingStateSendWhenOnline"
    );

    let json = send_text(
        RequestId(54),
        ChatId(11),
        None,
        "hi",
        None,
        &SendOptions::default(),
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["options"]["disable_notification"], false);
    assert_eq!(v["options"]["scheduling_state"], Value::Null);
}

#[test]
fn forward_messages_send_copy_and_remove_caption() {
    let v: serde_json::Value = serde_json::from_str(&forward_messages(
        RequestId(60),
        ChatId(11),
        ChatId(12),
        &[MessageId(101)],
        true,
        true,
    ))
    .unwrap();
    assert_eq!(v["@type"], "forwardMessages");
    assert_eq!(v["send_copy"], true);
    assert_eq!(v["remove_caption"], true);
    // `remove_caption` is ignored unless `send_copy` is true.
    let v: serde_json::Value = serde_json::from_str(&forward_messages(
        RequestId(61),
        ChatId(11),
        ChatId(12),
        &[MessageId(101)],
        false,
        true,
    ))
    .unwrap();
    assert_eq!(v["send_copy"], false);
    assert_eq!(v["remove_caption"], false);
}

#[test]
fn delete_messages_revoke_flag() {
    let v: serde_json::Value = serde_json::from_str(&delete_messages(
        RequestId(62),
        ChatId(11),
        &[MessageId(101)],
        true,
    ))
    .unwrap();
    assert_eq!(v["@type"], "deleteMessages");
    assert_eq!(v["revoke"], true);
    let v: serde_json::Value = serde_json::from_str(&delete_messages(
        RequestId(63),
        ChatId(11),
        &[MessageId(101)],
        false,
    ))
    .unwrap();
    assert_eq!(v["revoke"], false);
}

#[test]
fn m1_message_action_requests() {
    let v: serde_json::Value =
        serde_json::from_str(&unpin_all_chat_messages(RequestId(64), ChatId(11))).unwrap();
    assert_eq!(v["@type"], "unpinAllChatMessages");
    assert_eq!(v["chat_id"], 11);

    let v: serde_json::Value =
        serde_json::from_str(&get_message_link(RequestId(65), ChatId(11), MessageId(101))).unwrap();
    assert_eq!(v["@type"], "getMessageLink");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["message_id"], 101);

    let v: serde_json::Value = serde_json::from_str(&resend_messages(
        RequestId(66),
        ChatId(11),
        &[MessageId(101)],
    ))
    .unwrap();
    assert_eq!(v["@type"], "resendMessages");
    assert_eq!(v["message_ids"], serde_json::json!([101]));

    let v: serde_json::Value =
        serde_json::from_str(&get_chat_scheduled_messages(RequestId(67), ChatId(11))).unwrap();
    assert_eq!(v["@type"], "getChatScheduledMessages");
    assert_eq!(v["chat_id"], 11);
}

#[test]
fn b1_callback_query_with_password_shape_matches_1_8_67() {
    // B1: `getCallbackQueryAnswer` with `callbackQueryPayloadDataWithPassword`
    // (schema 1.8.67, lines 13138 / 7740); schema `bytes` is base64.
    let json = get_callback_query_answer_with_password(
        RequestId(61),
        ChatId(21),
        MessageId(301),
        "s3cr3t",
        &[1, 2, 3],
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getCallbackQueryAnswer");
    assert_eq!(v["@extra"], "61");
    assert_eq!(v["chat_id"], 21);
    assert_eq!(v["message_id"], 301);
    assert_eq!(
        v["payload"]["@type"],
        "callbackQueryPayloadDataWithPassword"
    );
    assert_eq!(v["payload"]["password"], "s3cr3t");
    assert_eq!(v["payload"]["data"], "AQID");
}

#[test]
fn b1_callback_query_game_shape_matches_1_8_67() {
    // B1: `getCallbackQueryAnswer` with `callbackQueryPayloadGame`
    // (schema 1.8.67, line 7743).
    let json = get_callback_query_answer_game(RequestId(62), ChatId(21), MessageId(301), "chess");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getCallbackQueryAnswer");
    assert_eq!(v["@extra"], "62");
    assert_eq!(v["payload"]["@type"], "callbackQueryPayloadGame");
    assert_eq!(v["payload"]["game_short_name"], "chess");
}

#[test]
fn bots_games_get_high_scores_shape_matches_1_8_67() {
    // Slice bots-games: `getGameHighScores` (schema 1.8.67, line 13174).
    let json = get_game_high_scores(RequestId(64), ChatId(21), MessageId(301), 42);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getGameHighScores");
    assert_eq!(v["@extra"], "64");
    assert_eq!(v["chat_id"], 21);
    assert_eq!(v["message_id"], 301);
    assert_eq!(v["user_id"], 42);
}

#[test]
fn bots_games_send_game_shape_matches_1_8_67() {
    // Slice bots-games: `sendMessage` + `inputMessageGame` (schema 1.8.67,
    // line 6156).
    let json = send_game(RequestId(66), ChatId(21), None, 7, "chess");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["@extra"], "66");
    assert_eq!(v["chat_id"], 21);
    assert_eq!(v["input_message_content"]["@type"], "inputMessageGame");
    assert_eq!(v["input_message_content"]["bot_user_id"], 7);
    assert_eq!(v["input_message_content"]["game_short_name"], "chess");
}

#[test]
fn b1_get_login_url_info_shape_matches_1_8_67() {
    // B1: `getLoginUrlInfo` (schema 1.8.67, line 12985).
    let json = get_login_url_info(RequestId(63), ChatId(21), MessageId(301), 7);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getLoginUrlInfo");
    assert_eq!(v["@extra"], "63");
    assert_eq!(v["chat_id"], 21);
    assert_eq!(v["message_id"], 301);
    assert_eq!(v["button_id"], 7);
}

#[test]
fn b1_get_login_url_shape_matches_1_8_67() {
    // B1: `getLoginUrl` (schema 1.8.67, line 12993).
    let json = get_login_url(RequestId(65), ChatId(21), MessageId(301), 7, true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getLoginUrl");
    assert_eq!(v["@extra"], "65");
    assert_eq!(v["chat_id"], 21);
    assert_eq!(v["message_id"], 301);
    assert_eq!(v["button_id"], 7);
    assert_eq!(v["allow_write_access"], true);
}

#[test]
fn b1_delete_chat_reply_markup_shape_matches_1_8_67() {
    // B1: `deleteChatReplyMarkup` (schema 1.8.67, line 13183).
    let json = delete_chat_reply_markup(RequestId(64), ChatId(21), MessageId(306));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "deleteChatReplyMarkup");
    assert_eq!(v["@extra"], "64");
    assert_eq!(v["chat_id"], 21);
    assert_eq!(v["message_id"], 306);
}
