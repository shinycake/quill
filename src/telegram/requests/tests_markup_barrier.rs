//! The markup writer puts U+0010 next to a marker that touches a word
//! character; the server would turn it into a space, so no request may carry
//! it.
use crate::composer::{DraftTopic, LinkPreviewChoice, SendOptions};
use crate::ids::{ChatId, MessageId, RequestId};
use crate::telegram::requests::*;
use serde_json::Value;

const MARKUP: &str = "h\u{10}**ell**\u{10}o w\u{10}`or`\u{10}ld";

fn clean(json: &str, what: &str) {
    assert!(!json.contains('\u{10}'), "{what} carries U+0010: {json}");
    assert!(!json.contains("\\u0010"), "{what} carries U+0010: {json}");
}

#[test]
fn no_text_request_carries_the_barrier() {
    let id = RequestId(5);
    let chat = ChatId(7);
    let options = SendOptions::default();
    let preview = LinkPreviewChoice::default();
    clean(
        &send_text(id, chat, None, MARKUP, None, &options),
        "sendMessage",
    );
    clean(
        &edit_message_text(id, chat, MessageId(3), MARKUP, false, &preview),
        "editMessageText",
    );
    clean(
        &edit_message_caption(id, chat, MessageId(3), MARKUP, false, false),
        "editMessageCaption",
    );
    clean(
        &set_chat_draft_message(id, chat, DraftTopic::Chat, Some(MARKUP), None),
        "setChatDraftMessage",
    );
    clean(&fix_text_with_ai(id, MARKUP), "fixTextWithAi");
    clean(
        &compose_text_with_ai(id, MARKUP, "", "", false),
        "composeTextWithAi",
    );
    clean(
        &create_rich_message_with_ai(id, MARKUP, "en", false),
        "createRichMessageWithAi",
    );
}

#[test]
fn the_ai_tools_get_text_and_entities_not_markup() {
    let json = fix_text_with_ai(RequestId(5), MARKUP);
    let value: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
    assert_eq!(value["text"]["text"], "hello world");
    assert_eq!(value["text"]["entities"].as_array().map(Vec::len), Some(2));
    let json = compose_text_with_ai(RequestId(5), "**bold** text", "", "", false);
    let value: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
    assert_eq!(value["text"]["text"], "bold text");
    let json = create_rich_message_with_ai(RequestId(5), "write **a** haiku", "en", false);
    let value: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
    assert_eq!(value["prompt"], "write a haiku");
}
