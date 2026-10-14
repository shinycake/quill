//! Requests for suggested posts in channel direct messages
//! (`crate::suggested_posts`): approve, decline, offers and changed
//! content.
use super::{SendReply, formatted_caption, message_send_options, send_reply_value};
use crate::composer::SendOptions;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::suggested_posts::SuggestTerms;
use serde_json::{Value, json};

/// `approveSuggestedPost chat_id message_id send_date` (schema line
/// 14585); `send_date` 0 publishes now. Answer `ok`.
pub fn approve_suggested_post(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    send_date: i32,
) -> String {
    json!({
        "@type": "approveSuggestedPost",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "send_date": send_date.max(0),
    })
    .to_string()
}

/// `declineSuggestedPost chat_id message_id comment` (schema line 14591).
/// The comment is cut to its 128-character limit. Answer `ok`.
pub fn decline_suggested_post(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    comment: &str,
) -> String {
    let comment: String = comment
        .trim()
        .chars()
        .take(crate::suggested_posts::DECLINE_REASON_MAX)
        .collect();
    json!({
        "@type": "declineSuggestedPost",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "comment": comment,
    })
    .to_string()
}

/// `addOffer chat_id message_id options` (schema line 14599): new terms
/// for a suggestion, or an offer for a plain message. The options carry
/// the complete terms. Answer `message`.
pub fn add_offer(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    terms: SuggestTerms,
) -> String {
    let options = SendOptions {
        suggested_post: Some(terms),
        ..SendOptions::default()
    };
    json!({
        "@type": "addOffer",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "options": message_send_options(&options),
    })
    .to_string()
}

/// `sendMessage` replying to a suggestion with changed content and the
/// terms (tdesktop "Edit Message" of a suggestion: a new message, never an
/// edit). Text goes as `inputMessageText`; media is copied from the
/// suggestion with the new caption (`inputMessageForwarded` with
/// `messageCopyOptions`). Answer `message`.
pub fn send_suggested_change(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    text: &str,
    is_media: bool,
    terms: SuggestTerms,
) -> String {
    let options = SendOptions {
        suggested_post: Some(terms),
        ..SendOptions::default()
    };
    let reply = SendReply {
        message_id,
        quote: None,
        source_chat: None,
    };
    let content = if is_media {
        json!({
            "@type": "inputMessageForwarded",
            "from_chat_id": chat_id.0,
            "message_id": message_id.0,
            "in_game_share": false,
            "replace_video_start_timestamp": false,
            "new_video_start_timestamp": 0,
            "copy_options": {
                "@type": "messageCopyOptions",
                "send_copy": true,
                "replace_caption": true,
                "new_caption": formatted_caption(text, false),
                "new_show_caption_above_media": false,
            },
        })
    } else {
        json!({
            "@type": "inputMessageText",
            "text": formatted_caption(text, false),
            "link_preview_options": Value::Null,
            "clear_draft": true,
        })
    };
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "reply_to": send_reply_value(Some(&reply)),
        "options": message_send_options(&options),
        "reply_markup": Value::Null,
        "input_message_content": content,
    })
    .to_string()
}
