//! Methods moved out of `messages.rs` to keep files under 1000 lines.

use super::*;

/// `addMessageReaction` (TDLib 1.8.67). Chip / picker add: `is_big` false
/// (tdesktop InlineList click, not the big-animation double-click).
/// `update_recent_reactions` true matches the official picker.
pub fn add_message_reaction(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    emoji: &str,
    is_big: bool,
    update_recent_reactions: bool,
) -> String {
    json!({
        "@type": "addMessageReaction",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reaction_type": reaction_type_emoji(emoji),
        "is_big": is_big,
        "update_recent_reactions": update_recent_reactions
    })
    .to_string()
}

/// `removeMessageReaction` (TDLib 1.8.67). A chosen reaction can always be
/// removed (schema). Official chip click on `is_chosen` sends this.
pub fn remove_message_reaction(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    emoji: &str,
) -> String {
    json!({
        "@type": "removeMessageReaction",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reaction_type": reaction_type_emoji(emoji)
    })
    .to_string()
}

/// `getCallbackQueryAnswer` (TDLib 1.8.67, `schema/td_api.tl:13138`).
/// Pressing an `inlineKeyboardButtonTypeCallback` button: sends the callback
/// query to the bot; TDLib returns `callbackQueryAnswer`. (Not
/// `answerCallbackQuery` — that one is bots-only per its schema doc
/// comment.) `payload` is `callbackQueryPayloadData` (line 7737); schema
/// `bytes` is base64 in the JSON interface.
pub fn get_callback_query_answer(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    data: &[u8],
) -> String {
    use base64::Engine;
    get_callback_query_answer_payload(
        extra,
        chat_id,
        message_id,
        json!({
            "@type": "callbackQueryPayloadData",
            "data": base64::engine::general_purpose::STANDARD.encode(data),
        }),
    )
}

/// B1: `getCallbackQueryAnswer` for an
/// `inlineKeyboardButtonTypeCallbackWithPassword` button press. `payload`
/// is `callbackQueryPayloadDataWithPassword` (schema 1.8.67, line 7740).
pub fn get_callback_query_answer_with_password(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    password: &str,
    data: &[u8],
) -> String {
    use base64::Engine;
    get_callback_query_answer_payload(
        extra,
        chat_id,
        message_id,
        json!({
            "@type": "callbackQueryPayloadDataWithPassword",
            "password": password,
            "data": base64::engine::general_purpose::STANDARD.encode(data),
        }),
    )
}

/// B1: `getCallbackQueryAnswer` for an `inlineKeyboardButtonTypeCallbackGame`
/// button press. `payload` is `callbackQueryPayloadGame` (schema 1.8.67,
/// line 7743); `game_short_name` comes from the message's `messageGame`
/// content (schema:5234 / game class schema:673). A `sendGame` constructor
/// does not exist in this schema — the game launches through this callback
/// query (TGX `TGInlineKeyboard` does exactly this).
pub fn get_callback_query_answer_game(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    game_short_name: &str,
) -> String {
    get_callback_query_answer_payload(
        extra,
        chat_id,
        message_id,
        json!({
            "@type": "callbackQueryPayloadGame",
            "game_short_name": game_short_name,
        }),
    )
}

pub(super) fn get_callback_query_answer_payload(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    payload: Value,
) -> String {
    json!({
        "@type": "getCallbackQueryAnswer",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "payload": payload,
    })
    .to_string()
}

/// Slice bots-games: `getGameHighScores` (TDLib 1.8.67,
/// `schema/td_api.tl:13174`) — high scores for the game in `message_id`,
/// with the table range around `user_id`. Response is `gameHighScores`.
pub fn get_game_high_scores(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    user_id: i64,
) -> String {
    json!({
        "@type": "getGameHighScores",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "user_id": user_id,
    })
    .to_string()
}

/// Slice bots-games: `sendMessage` + `inputMessageGame` (TDLib 1.8.67,
/// `schema/td_api.tl:6156`) — send the bot's game to the chat. Not
/// supported for channels or secret chats (the driver pre-checks).
/// Rides `RequestPurpose::SendMessage` so the optimistic row flows
/// through the normal send path.
pub fn send_game(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    bot_user_id: i64,
    game_short_name: &str,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(None),
        "options": message_send_options(&SendOptions::default()),
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageGame",
            "bot_user_id": bot_user_id,
            "game_short_name": game_short_name,
        }
    })
    .to_string()
}

/// B1: `getLoginUrlInfo` (TDLib 1.8.67, `schema/td_api.tl:12985`) — resolve
/// an `inlineKeyboardButtonTypeLoginUrl` button (`id`, schema:3780) to the
/// authorized URL. Response is `loginUrlInfo*`.
pub fn get_login_url_info(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    button_id: i64,
) -> String {
    json!({
        "@type": "getLoginUrlInfo",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "button_id": button_id,
    })
    .to_string()
}

/// B1: `getLoginUrl` (TDLib 1.8.67, `schema/td_api.tl:12993`) — the
/// authorized URL after the user consented to a
/// `loginUrlInfoRequestConfirmation`. Response is `httpUrl`.
pub fn get_login_url(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    button_id: i64,
    allow_write_access: bool,
) -> String {
    json!({
        "@type": "getLoginUrl",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "button_id": button_id,
        "allow_write_access": allow_write_access,
    })
    .to_string()
}

/// B1: `deleteChatReplyMarkup` (TDLib 1.8.67, `schema/td_api.tl:13183`).
/// Must be called after a one-time custom keyboard has been used.
pub fn delete_chat_reply_markup(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
) -> String {
    json!({
        "@type": "deleteChatReplyMarkup",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
    })
    .to_string()
}

/// `openMessageContent` — user started listening to a voice note (1.8.67).
pub fn open_message_content(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "openMessageContent",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0
    })
    .to_string()
}

/// MED2: `recognizeSpeech` (TDLib 1.8.67, `schema/td_api.tl:12181`).
/// Recognizes speech in a voice note or video note message. Returns `Ok`;
/// the result arrives later as `updateMessageContent` carrying the new
/// `speech_recognition_result` (`speechRecognitionResultPending` →
/// `speechRecognitionResultText` / `speechRecognitionResultError`).
pub fn recognize_speech(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "recognizeSpeech",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0
    })
    .to_string()
}

/// B11: `setDefaultReactionType reaction_type:ReactionType = Ok` (TDLib
/// 1.8.67, line 12852): Settings, "Quick reaction".
pub fn set_default_reaction_type(extra: RequestId, reaction_type: Value) -> String {
    json!({
        "@type": "setDefaultReactionType",
        "@extra": extra.as_extra(),
        "reaction_type": reaction_type,
    })
    .to_string()
}
