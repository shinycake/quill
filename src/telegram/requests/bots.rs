use crate::ids::{ChatId, RequestId};
use serde_json::{Value, json};

/// Phase 3.3: `getCommands` for a bot's global (default) command scope
/// (TDLib 1.8.67, `schema/td_api.tl:14953`):
/// `getCommands scope:BotCommandScope language_code:string = BotCommands;`
/// The schema annotates the method "for bots only", so a user session
/// gets an `error` answer instead of `botCommands`; the driver absorbs it
/// and the `/` menu falls back to the `botInfo` commands. The scope is
/// `botCommandScopeDefault` (line 10360, "a scope covering all users");
/// the chat-specific commands already arrive via `botInfo`.
pub fn get_commands(extra: RequestId) -> String {
    json!({
        "@type": "getCommands",
        "@extra": extra.as_extra(),
        "scope": null,
        "language_code": "",
    })
    .to_string()
}

/// `getChatSponsoredMessages` (TDLib 1.8.67). For channel chats (and chats
/// with bots); rows render Sponsored / Recommended per `is_recommended`.
pub fn get_chat_sponsored_messages(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "getChatSponsoredMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `reportChatSponsoredMessage` (TDLib 1.8.67). `option_id` is the base64
/// `reportOption.id`; empty for the initial request.
pub fn report_chat_sponsored_message(
    extra: RequestId,
    chat_id: ChatId,
    message_id: i64,
    option_id: &str,
) -> String {
    json!({
        "@type": "reportChatSponsoredMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id,
        "option_id": option_id,
    })
    .to_string()
}

/// Slice CL3: `reportChat` (TDLib 1.8.67, `schema/td_api.tl:15693`):
/// `reportChat chat_id:int53 option_id:bytes message_ids:vector<int53>
/// text:string = ReportChatResult;`
/// The simple spam-report flow uses empty option_id/message_ids/text
/// (schema:3667: "The chat can be reported as spam using the method
/// reportChat with an empty option_id and message_ids").
pub fn report_chat(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "reportChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "option_id": "",
        "message_ids": [],
        "text": "",
    })
    .to_string()
}

/// Batch 8: `removeChatActionBar` (TDLib 1.8.67):
/// `removeChatActionBar chat_id:int53 = Ok;` — the bar's close button.
pub fn remove_chat_action_bar(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "removeChatActionBar",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
    })
    .to_string()
}

/// Batch 8: `sharePhoneNumber` (TDLib 1.8.67, `schema/td_api.tl:14584`):
/// `sharePhoneNumber user_id:int53 = Ok;` — "Share my phone number".
pub fn share_phone_number(extra: RequestId, user_id: i64) -> String {
    json!({
        "@type": "sharePhoneNumber",
        "@extra": extra.as_extra(),
        "user_id": user_id,
    })
    .to_string()
}

/// Slice CL3: `setMessageSenderBlockList` (TDLib 1.8.67,
/// `schema/td_api.tl:14492`):
/// `setMessageSenderBlockList sender_id:MessageSender
/// block_list:BlockList = Ok;`
/// `block = false` passes null `block_list` to unblock the sender (TGX
/// `Tdlib.unblockSender`).
pub fn set_message_sender_block_list(extra: RequestId, user_id: i64, block: bool) -> String {
    json!({
        "@type": "setMessageSenderBlockList",
        "@extra": extra.as_extra(),
        "sender_id": { "@type": "messageSenderUser", "user_id": user_id },
        "block_list": if block { json!({ "@type": "blockListMain" }) } else { Value::Null },
    })
    .to_string()
}

/// Slice B2: `sendBotStartMessage bot_user_id:int53 chat_id:int53
/// parameter:string = Message;` (TDLib 1.8.67, `schema/td_api.tl:12216`) —
/// what the START button and "Restart bot" send. `parameter` is the
/// `internalLinkTypeBotStart.start_parameter` (line 9399); empty for a
/// plain restart. Telegram X `Tdlib.sendBotStartMessage`.
pub fn send_bot_start_message(
    extra: RequestId,
    bot_user_id: i64,
    chat_id: i64,
    parameter: &str,
) -> String {
    json!({
        "@type": "sendBotStartMessage",
        "@extra": extra.as_extra(),
        "bot_user_id": bot_user_id,
        "chat_id": chat_id,
        "parameter": parameter,
    })
    .to_string()
}

/// Slice B2: `getBotSimilarBots bot_user_id:int53 = Users;` (TDLib 1.8.67,
/// `schema/td_api.tl:11640`). Powers the similar-bots section of the bot
/// profile (Telegram X `SharedChatsController.Mode.SIMILAR_BOTS`).
pub fn get_bot_similar_bots(extra: RequestId, bot_user_id: i64) -> String {
    json!({
        "@type": "getBotSimilarBots",
        "@extra": extra.as_extra(),
        "bot_user_id": bot_user_id,
    })
    .to_string()
}

/// `viewSponsoredChat` (TDLib 1.8.67). `unique_id` is the `sponsoredChat`
/// unique id (from sponsored search results).
pub fn view_sponsored_chat(extra: RequestId, sponsored_chat_unique_id: i64) -> String {
    json!({
        "@type": "viewSponsoredChat",
        "@extra": extra.as_extra(),
        "sponsored_chat_unique_id": sponsored_chat_unique_id,
    })
    .to_string()
}

/// `clickChatSponsoredMessage` (TDLib 1.8.67). Sent when the user opens a
/// sponsored message's sponsor link/button (`is_media_click = false`) or its
/// media (`is_media_click = true`). `from_fullscreen` is true when the media
/// was opened from the fullscreen viewer.
pub fn click_chat_sponsored_message(
    extra: RequestId,
    chat_id: ChatId,
    message_id: i64,
    is_media_click: bool,
    from_fullscreen: bool,
) -> String {
    json!({
        "@type": "clickChatSponsoredMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id,
        "is_media_click": is_media_click,
        "from_fullscreen": from_fullscreen,
    })
    .to_string()
}

/// Stop the exact currently streaming bot draft; topic uses the pinned MessageTopic API.
pub fn stop_pending_message(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    draft_id: i64,
) -> String {
    json!({"@type":"stopPendingMessage", "@extra":extra.as_extra(), "chat_id":chat_id.0,
        "topic_id":super::message_topic_value(topic_id), "draft_id":draft_id.to_string()})
    .to_string()
}

/// `toggleHasSponsoredMessagesEnabled` (TDLib 1.8.67, `schema/td_api.tl:14854`):
/// the Premium "hide ads" setting; has no effect without Telegram Premium.
pub fn toggle_has_sponsored_messages_enabled(extra: RequestId, enabled: bool) -> String {
    json!({
        "@type": "toggleHasSponsoredMessagesEnabled",
        "@extra": extra.as_extra(),
        "has_sponsored_messages_enabled": enabled,
    })
    .to_string()
}
