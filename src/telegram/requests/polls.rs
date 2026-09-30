use super::{SendReply, message_topic_value, send_reply_value};
use crate::ids::{ChatId, MessageId, RequestId};
use serde_json::{Value, json};

/// `getPollVoters` (TDLib 1.8.67, `schema/td_api.tl:12941`):
/// `getPollVoters chat_id:int53 message_id:int53 option_id:int32
/// offset:int32 limit:int32 = PollVoters;`
/// `option_id` is the 0-based option index (like `setPollAnswer`,
/// schema line 12932); `limit` must be positive and ≤ 50. Response is
/// `pollVoters`; voters arrive page by page (offset = items already
/// loaded). Only called when `poll.can_get_voters` (schema line 698).
pub fn get_poll_voters(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    option_id: i32,
    offset: i32,
    limit: i32,
) -> String {
    json!({
        "@type": "getPollVoters",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "option_id": option_id,
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

/// `stopPoll` (TDLib 1.8.67, `schema/td_api.tl:12953`):
/// `stopPoll chat_id:int53 message_id:int53 reply_markup:ReplyMarkup = Ok;`
/// `reply_markup` is "for bots only; pass null if none" (schema doc), so
/// the human client always sends null. Response is `ok`; the poll closes
/// via `updatePoll` (and `chatEventPollStopped` lands in the event log).
pub fn stop_poll(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "stopPoll",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reply_markup": Value::Null,
    })
    .to_string()
}

/// `getInlineQueryResults` (TDLib 1.8.67, `schema/td_api.tl:13019`):
/// `getInlineQueryResults bot_user_id:int53 chat_id:int53
/// user_location:location query:string offset:string = InlineQueryResults;`
/// `user_location` is null (schema doc: "pass null if unknown"); `offset`
/// is "" for the first chunk, the previous answer's `next_offset` after.
pub fn get_inline_query_results(
    extra: RequestId,
    bot_user_id: i64,
    chat_id: ChatId,
    query: &str,
    offset: &str,
) -> String {
    json!({
        "@type": "getInlineQueryResults",
        "@extra": extra.as_extra(),
        "bot_user_id": bot_user_id,
        "chat_id": chat_id.0,
        "user_location": Value::Null,
        "query": query,
        "offset": offset,
    })
    .to_string()
}

/// `sendInlineQueryResultMessage` (TDLib 1.8.67, `schema/td_api.tl:12226`):
/// `sendInlineQueryResultMessage chat_id:int53 topic_id:MessageTopic
/// reply_to:InputMessageReplyTo options:messageSendOptions query_id:int64
/// result_id:string hide_via_bot:Bool = Message;`
/// Sends the picked inline result as a normal chat message; `options` is
/// null for the defaults (like `send_document`). `hide_via_bot` may only
/// be used with the search bots (schema doc), so the UI defaults it off.
pub fn send_inline_query_result_message(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    reply_to: Option<SendReply>,
    query_id: i64,
    result_id: &str,
    hide_via_bot: bool,
) -> String {
    json!({
        "@type": "sendInlineQueryResultMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": Value::Null,
        "query_id": query_id,
        "result_id": result_id,
        "hide_via_bot": hide_via_bot,
    })
    .to_string()
}

/// `setPollAnswer` (TDLib 1.8.67, `schema/td_api.tl:12932`): `option_ids` are
/// 0-based indexes into the poll's option list (not the `pollOption.id`
/// strings). Response is `ok`; the new counts arrive via `updatePoll`.
pub fn set_poll_answer(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    option_ids: &[i32],
) -> String {
    json!({
        "@type": "setPollAnswer",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "option_ids": option_ids,
    })
    .to_string()
}

/// Fields for `inputMessagePoll` (TDLib 1.8.67, `schema/td_api.tl:6193`).
/// `description` empty = sent as null; `open_period` 0 = no auto-close.
pub struct PollSend<'a> {
    pub question: &'a str,
    pub options: &'a [&'a str],
    pub description: &'a str,
    pub is_anonymous: bool,
    pub allows_multiple_answers: bool,
    pub allows_revoting: bool,
    pub shuffle_options: bool,
    pub country_codes: &'a [&'a str],
    pub poll_type: PollTypeSend<'a>,
    pub open_period: i32,
    pub reply_to: Option<SendReply>,
    /// Parity slice 4: forum topic the send is addressed to (`None` = no topic).
    pub topic_id: Option<i32>,
}

/// `InputPollType` for `inputMessagePoll` (TDLib 1.8.67).
pub enum PollTypeSend<'a> {
    Regular,
    Quiz {
        correct_option_ids: &'a [i32],
        explanation: &'a str,
    },
}

/// `sendMessage` + `inputMessagePoll` / `inputPollOption` /
/// `inputPollTypeRegular` (schema line 481) / `inputPollTypeQuiz`
/// (schema line 488) (TDLib 1.8.67). Options must already be trimmed and
/// non-empty (2–10); the question 1–255 chars — validated by
/// `PollDraft::validate` before this is called. `members_only`,
/// `hide_results_until_closes`, `close_date` stay at the zero value
/// (out of the B3 slice); `media`/`explanation_media` are null.
pub fn send_poll(extra: RequestId, chat_id: ChatId, poll: PollSend<'_>) -> String {
    let options: Vec<Value> = poll
        .options
        .iter()
        .map(|text| {
            json!({
                "@type": "inputPollOption",
                "text": {
                    "@type": "formattedText",
                    "text": text,
                    "entities": []
                },
                "media": Value::Null
            })
        })
        .collect();
    let description = poll.description.trim();
    let poll_type = match poll.poll_type {
        PollTypeSend::Regular => json!({
            "@type": "inputPollTypeRegular",
            "allow_adding_options": false
        }),
        PollTypeSend::Quiz {
            correct_option_ids,
            explanation,
        } => json!({
            "@type": "inputPollTypeQuiz",
            "correct_option_ids": correct_option_ids,
            "explanation": {
                "@type": "formattedText",
                "text": explanation,
                "entities": []
            },
            "explanation_media": Value::Null
        }),
    };
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(poll.topic_id),
        "reply_to": send_reply_value(poll.reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessagePoll",
            "question": {
                "@type": "formattedText",
                "text": poll.question,
                "entities": []
            },
            "options": options,
            "description": if description.is_empty() {
                Value::Null
            } else {
                json!({
                    "@type": "formattedText",
                    "text": description,
                    "entities": []
                })
            },
            "media": Value::Null,
            "is_anonymous": poll.is_anonymous,
            "allows_multiple_answers": poll.allows_multiple_answers,
            "allows_revoting": poll.allows_revoting,
            "members_only": false,
            "country_codes": poll.country_codes,
            "shuffle_options": poll.shuffle_options,
            "hide_results_until_closes": false,
            "type": poll_type,
            "open_period": poll.open_period,
            "close_date": 0,
            "is_closed": false
        }
    })
    .to_string()
}
