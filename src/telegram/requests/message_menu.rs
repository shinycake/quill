//! Requests behind the message context menu: reporting messages, the
//! seen / reacted lists, and the admin moderation actions.

use crate::ids::{ChatId, MessageId, RequestId};
use crate::telegram::envelope::ReactionType;
use serde_json::{Value, json};

/// `reportChat chat_id:int53 option_id:bytes message_ids:vector<int53>
/// text:string = ReportChatResult` (schema 1.8.67, line 15693) with the
/// messages to report. `option_id` is the base64 `reportOption.id` the
/// previous answer carried ("" for the first call); `text` the details of a
/// `reportChatResultTextRequired` step.
pub fn report_chat_messages(
    extra: RequestId,
    chat_id: ChatId,
    option_id: &str,
    message_ids: &[MessageId],
    text: &str,
) -> String {
    json!({
        "@type": "reportChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "option_id": option_id,
        "message_ids": message_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
        "text": text,
    })
    .to_string()
}

/// `getMessageViewers chat_id:int53 message_id:int53 = MessageViewers`
/// (schema 1.8.67, line 11576): who of a small group already viewed it.
pub fn get_message_viewers(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "getMessageViewers",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
    })
    .to_string()
}

/// `getMessageReadDate chat_id:int53 message_id:int53 = MessageReadDate`
/// (schema 1.8.67, line 11571): when the other side of a private chat read
/// an outgoing message.
pub fn get_message_read_date(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "getMessageReadDate",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
    })
    .to_string()
}

/// `getMessageAddedReactions chat_id:int53 message_id:int53
/// reaction_type:ReactionType offset:string limit:int32 = AddedReactions`
/// (schema 1.8.67, line 12849). `reaction_type` `None` lists every
/// reaction.
pub fn get_message_added_reactions(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    reaction_type: Option<&ReactionType>,
    offset: &str,
    limit: i32,
) -> String {
    json!({
        "@type": "getMessageAddedReactions",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reaction_type": reaction_type.map_or(Value::Null, ReactionType::to_tdlib_json),
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

/// `deleteChatMessagesBySender chat_id:int53 sender_id:MessageSender = Ok`
/// (schema 1.8.67, line 12291): Telegram Desktop's "Delete all from user".
pub fn delete_chat_messages_by_sender(extra: RequestId, chat_id: ChatId, user_id: i64) -> String {
    json!({
        "@type": "deleteChatMessagesBySender",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "sender_id": { "@type": "messageSenderUser", "user_id": user_id },
    })
    .to_string()
}

/// `reportSupergroupSpam supergroup_id:int53 message_ids:vector<int53> =
/// Ok` (schema 1.8.67, line 15226): the admin "Report Spam" checkbox.
pub fn report_supergroup_spam(
    extra: RequestId,
    supergroup_id: i64,
    message_ids: &[MessageId],
) -> String {
    json!({
        "@type": "reportSupergroupSpam",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "message_ids": message_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
    })
    .to_string()
}
