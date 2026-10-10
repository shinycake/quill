//! Parses TDLib objects for options, connection state and scalar answers shared by many requests.
use crate::telegram::envelope::*;
use serde_json::Value;

/// The common domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_common_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        "count" => Ok(EnvelopePayload::Common(CommonPayload::Count {
            count: value
                .get("count")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32(),
        })),
        // MED4: `updateOption` (schema:10926). TDLib pushes all options
        // after authorization; Quill keeps `message_caption_length_max`.
        "updateOption" => {
            let name = json_field_str(value, "name");
            let raw = value.get("value").unwrap_or(&Value::Null);
            let value = match raw.get("@type").and_then(Value::as_str).unwrap_or("") {
                "optionValueBoolean" => OptionValue::Boolean(json_bool(raw.get("value"), false)),
                "optionValueInteger" => OptionValue::Integer(int53_or_zero(raw.get("value"))),
                "optionValueString" => OptionValue::String(json_field_str(raw, "value")),
                _ => OptionValue::Empty,
            };
            Ok(EnvelopePayload::Common(CommonPayload::UpdateOption {
                name,
                value,
            }))
        }
        "updateDiceEmojis" => Ok(EnvelopePayload::Common(CommonPayload::UpdateDiceEmojis {
            emojis: parse_dice_emojis(value),
        })),
        "updateFreezeState" => Ok(EnvelopePayload::Common(CommonPayload::UpdateFreezeState(
            parse_freeze_state(value),
        ))),
        "updateSpeechRecognitionTrial" => Ok(EnvelopePayload::Common(
            CommonPayload::UpdateSpeechRecognitionTrial(parse_speech_trial(value)),
        )),
        "updateAgeVerificationParameters" => Ok(EnvelopePayload::Common(
            CommonPayload::UpdateAgeVerificationParameters {
                parameters: parse_age_verification(value),
            },
        )),
        "updateServiceNotification" => Ok(parse_service_notification(value)),
        "updateConnectionState" => Ok(EnvelopePayload::Common(
            CommonPayload::UpdateConnectionState(parse_connection(value.get("state"))),
        )),
        "seconds" => Ok(EnvelopePayload::Common(CommonPayload::Seconds {
            seconds: value.get("seconds").and_then(Value::as_f64).unwrap_or(0.0),
        })),
        // Slice msg-richtext-ai-tools: `fixedText` (schema:157) — the
        // `fixTextWithAi` answer.
        "fixedText" => Ok(EnvelopePayload::Common(CommonPayload::FixedText {
            text: parse_formatted_text(value.get("text")),
        })),
        // Slice msg-richtext-ai-tools: bare `formattedText` (schema:3046)
        // — the `composeTextWithAi` answer.
        "formattedText" => {
            let text = parse_formatted_text(Some(value));
            let entities = parse_text_entities(&text, Some(value));
            Ok(EnvelopePayload::Common(CommonPayload::FormattedText {
                text,
                entities,
            }))
        }
        // Phase C3a: `text` (schema 1.8.67, line 10071) — the
        // `joinVideoChat` / `joinGroupCall` answer ("join response
        // payload for tgcalls"). Quill stores it, never consumes it
        // (no media transport until Phase C2).
        "text" => Ok(EnvelopePayload::Common(CommonPayload::Text {
            text: value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })),
        // Phase C3a: `httpUrl` (schema 1.8.67, line 7458) — the
        // `getVideoChatInviteLink` answer.
        "httpUrl" => Ok(EnvelopePayload::Common(CommonPayload::HttpUrl {
            url: value
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })),
        _ => return Ok(None),
    };
    payload.map(Some)
}
