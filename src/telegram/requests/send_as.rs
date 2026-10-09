//! "Send as" (message sender) requests: which identities the user may post
//! as in a chat, and which one is selected.

use crate::ids::{ChatId, RequestId};
use crate::telegram::envelope::MessageSender;
use crate::telegram::requests::message_sender_json;
use serde_json::json;

/// `getChatAvailableMessageSenders chat_id:int53 = ChatMessageSenders`
/// (TDLib 1.8.67, `schema/td_api.tl:12188`).
pub fn get_chat_available_message_senders(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "getChatAvailableMessageSenders",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `setChatMessageSender chat_id:int53 message_sender_id:MessageSender = Ok`
/// (TDLib 1.8.67, `schema/td_api.tl:12191`). The new choice is announced by
/// `updateChatMessageSender`.
pub fn set_chat_message_sender(extra: RequestId, chat_id: ChatId, sender: MessageSender) -> String {
    json!({
        "@type": "setChatMessageSender",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_sender_id": message_sender_json(sender),
    })
    .to_string()
}
