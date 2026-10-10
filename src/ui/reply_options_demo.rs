//! Screenshot fixtures for the reply options: the chat chooser behind
//! "Reply in Another Chat", the quote picker and a reply carried into
//! another chat (injected through the reducer, no live Telegram).

use quill::composer::{ComposerReplyTo, QuoteSelection};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// The group the message lives in and the chat the reply may go to.
pub(super) const SOURCE_CHAT: ChatId = ChatId(16);
pub(super) const SOURCE_MESSAGE: MessageId = MessageId(61);
pub(super) const TARGET_CHAT: ChatId = ChatId(14);

const MESSAGE_TEXT: &str = "Can you send me the release notes? I need them before Friday. Also, which build are we shipping?";

/// Maya's question in the "Studio standup" group, plus a few people and
/// groups to choose from, with English names.
pub(super) fn apply_ready_reply_elsewhere(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let jsons = [
        r#"{"@type":"updateUser","user":{"id":11,"first_name":"Maya","last_name":"Chen","type":{"@type":"userTypeRegular"}}}"#.to_string(),
        r#"{"@type":"updateChatTitle","chat_id":11,"title":"Maya Chen"}"#.to_string(),
        r#"{"@type":"updateChatTitle","chat_id":12,"title":"Jonas Weber"}"#.to_string(),
        r#"{"@type":"updateChatTitle","chat_id":13,"title":"Release notes"}"#.to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Sam Rivera","type":{"@type":"chatTypePrivate","user_id":14},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":15,"title":"Weekend hike","type":{"@type":"chatTypeBasicGroup","basic_group_id":15},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Studio standup","type":{"@type":"chatTypeBasicGroup","basic_group_id":16},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":14,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"18","is_pinned":false}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":15,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"16","is_pinned":false}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":16,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"40","is_pinned":false}}"#.to_string(),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{},"chat_id":{},"sender_id":{{"@type":"messageSenderUser","user_id":11}},"is_outgoing":false,"date":1790632200,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{MESSAGE_TEXT}","entities":[]}}}}}}}}"#,
            SOURCE_MESSAGE.0, SOURCE_CHAT.0
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(SOURCE_CHAT);
}

fn source_reply() -> ComposerReplyTo {
    ComposerReplyTo::new(SOURCE_CHAT, SOURCE_MESSAGE, MESSAGE_TEXT)
}

/// The reply as the chooser sees it: aimed nowhere yet.
pub(super) fn reply_to_choose() -> ComposerReplyTo {
    source_reply()
}

/// The reply with its second sentence quoted.
pub(super) fn reply_with_quote() -> ComposerReplyTo {
    let quote = quill::reply_options::quote_segments(MESSAGE_TEXT)
        .into_iter()
        .nth(1)
        .unwrap_or(QuoteSelection {
            text: String::new(),
            position: 0,
        });
    source_reply().with_new_quote(Some(quote))
}

/// The reply carried into Sam's chat.
pub(super) fn reply_in_target() -> ComposerReplyTo {
    reply_with_quote().into_chat(TARGET_CHAT)
}
