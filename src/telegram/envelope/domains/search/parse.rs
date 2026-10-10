//! Parses TDLib objects for global and in-chat search, top chats and date jumps.
use crate::ids::MessageId;
use crate::telegram::envelope::*;
use serde_json::Value;

/// The search domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_search_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        "foundPublicPosts" => Ok(EnvelopePayload::Search(SearchPayload::FoundPublicPosts {
            messages: value
                .get("messages")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|m| parse_message(m).ok())
                .collect(),
            next_offset: value
                .get("next_offset")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            are_limits_exceeded: value
                .get("are_limits_exceeded")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })),
        "foundMessages" => {
            let messages = value
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let parsed = messages
                .iter()
                .filter_map(|m| parse_message(m).ok())
                .collect();
            Ok(EnvelopePayload::Search(SearchPayload::FoundMessages {
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                messages: parsed,
                next_offset: value
                    .get("next_offset")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }))
        }
        "messageCalendar" => {
            let days = value
                .get("days")
                .and_then(Value::as_array)
                .map(|days| {
                    days.iter()
                        .filter_map(|day| {
                            let message = day.get("message")?;
                            let message_id = int53_or_zero(message.get("id"));
                            let date = message.get("date").and_then(Value::as_i64)?.sat_i32();
                            (message_id > 0).then(|| CalendarDay {
                                total_count: day
                                    .get("total_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    .sat_i32(),
                                message_id: MessageId(message_id),
                                date,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(EnvelopePayload::Search(SearchPayload::MessageCalendar {
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                days,
            }))
        }
        "foundChatMessages" => {
            let messages = value
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let parsed = messages
                .iter()
                .filter_map(|m| parse_message(m).ok())
                .collect();
            Ok(EnvelopePayload::Search(SearchPayload::FoundChatMessages {
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                messages: parsed,
                next_from_message_id: MessageId(int53_or_zero(value.get("next_from_message_id"))),
            }))
        }
        _ => return Ok(None),
    };
    payload.map(Some)
}
