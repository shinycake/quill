//! Parses TDLib objects for one-to-one calls, group calls and video chats.
use crate::telegram::envelope::*;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;

/// The calls domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_calls_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        // Phase C1: call signaling updates (schema 1.8.67, lines
        // 10816 / 10862) and the `createCall` answer (`callId`,
        // line 7034).
        "updateCall" => Ok(EnvelopePayload::Calls(CallsPayload::UpdateCall {
            call: parse_call(value.get("call")).ok_or(ParseError::MissingField)?,
        })),
        "updateNewCallSignalingData" => Ok(EnvelopePayload::Calls(
            CallsPayload::UpdateNewCallSignalingData {
                call_id: value
                    .get("call_id")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)?
                    .sat_i32(),
                data: value
                    .get("data")
                    .and_then(Value::as_str)
                    .and_then(|s| STANDARD.decode(s).ok())
                    .unwrap_or_default(),
            },
        )),
        "callId" => Ok(EnvelopePayload::Calls(CallsPayload::CallId {
            id: value
                .get("id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)?
                .sat_i32(),
        })),
        // Phase C3a: `groupCallId` (schema 1.8.67, line 7037) — the
        // `createVideoChat` answer. The driver fetches the full
        // `groupCall` via `getGroupCall`; live state arrives as
        // `updateGroupCall`.
        "groupCallId" => Ok(EnvelopePayload::Calls(CallsPayload::GroupCallId {
            id: value
                .get("id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)?
                .sat_i32(),
        })),
        // Phase C2f: `groupCallInfo` (schema 1.8.67, line 7190) — the
        // `joinGroupCall` answer.
        "groupCallInfo" => Ok(EnvelopePayload::Calls(CallsPayload::GroupCallInfo {
            group_call_id: value
                .get("group_call_id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)?
                .sat_i32(),
            join_payload: value
                .get("join_payload")
                .and_then(Value::as_str)
                .ok_or(ParseError::MissingField)?
                .to_string(),
        })),
        // Phase C3a: group-call signaling updates (schema 1.8.67,
        // lines 10819 / 10824 / 10830 / 10836 / 10576). All
        // signaling-only: no media transport until Phase C2.
        // `getGroupCall` (schema :14274) answers with a bare `groupCall`
        // object — route it through the same handling as
        // `updateGroupCall` so the fetch path can create the tracker.
        "groupCall" => Ok(EnvelopePayload::Calls(CallsPayload::UpdateGroupCall {
            group_call: parse_group_call(Some(value)).ok_or(ParseError::MissingField)?,
        })),
        "updateGroupCall" => Ok(EnvelopePayload::Calls(CallsPayload::UpdateGroupCall {
            group_call: parse_group_call(value.get("group_call"))
                .ok_or(ParseError::MissingField)?,
        })),
        "updateGroupCallParticipant" => Ok(EnvelopePayload::Calls(
            CallsPayload::UpdateGroupCallParticipant {
                group_call_id: value
                    .get("group_call_id")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)?
                    .sat_i32(),
                participant: parse_group_call_participant(value.get("participant"))
                    .ok_or(ParseError::MissingField)?,
            },
        )),
        "updateGroupCallParticipants" => Ok(EnvelopePayload::Calls(
            CallsPayload::UpdateGroupCallParticipants {
                group_call_id: value
                    .get("group_call_id")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)?
                    .sat_i32(),
                participant_user_ids: value
                    .get("participant_user_ids")
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect::<Vec<i64>>())
                    .ok_or(ParseError::MissingField)?,
            },
        )),
        "updateGroupCallVerificationState" => Ok(EnvelopePayload::Calls(
            CallsPayload::UpdateGroupCallVerificationState {
                group_call_id: value
                    .get("group_call_id")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)?
                    .sat_i32(),
                generation: value
                    .get("generation")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)?
                    .sat_i32(),
                emojis: value
                    .get("emojis")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect::<Vec<String>>()
                    })
                    .ok_or(ParseError::MissingField)?,
            },
        )),
        // Phase C2h: group-call message updates (schema 1.8.67,
        // lines 10839 / 10851 / 10856).
        "updateNewGroupCallMessage" => Ok(EnvelopePayload::Calls(
            CallsPayload::UpdateNewGroupCallMessage {
                group_call_id: value
                    .get("group_call_id")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)?
                    .sat_i32(),
                message: parse_group_call_message(value.get("message"))
                    .ok_or(ParseError::MissingField)?,
            },
        )),
        "updateGroupCallMessageSendFailed" => Ok(EnvelopePayload::Calls(
            CallsPayload::UpdateGroupCallMessageSendFailed {
                group_call_id: value
                    .get("group_call_id")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)?
                    .sat_i32(),
                message_id: value
                    .get("message_id")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)?
                    .sat_i32(),
                error: parse_error(value.get("error")),
            },
        )),
        "updateGroupCallMessagesDeleted" => Ok(EnvelopePayload::Calls(
            CallsPayload::UpdateGroupCallMessagesDeleted {
                group_call_id: value
                    .get("group_call_id")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)?
                    .sat_i32(),
                message_ids: value
                    .get("message_ids")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_i64())
                            .map(|id| id.sat_i32())
                            .collect::<Vec<i32>>()
                    })
                    .ok_or(ParseError::MissingField)?,
            },
        )),
        "updateChatVideoChat" => Ok(EnvelopePayload::Calls(CallsPayload::UpdateChatVideoChat {
            chat_id: int53(value.get("chat_id"))?,
            video_chat: parse_video_chat(value.get("video_chat"))
                .ok_or(ParseError::MissingField)?,
        })),
        // Phase C2h: `rtmpUrl` (schema 1.8.67, line 7113) — the
        // `getVideoChatRtmpUrl` / `replaceVideoChatRtmpUrl` answer.
        "rtmpUrl" => Ok(EnvelopePayload::Calls(CallsPayload::RtmpUrl {
            url: value
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            stream_key: value
                .get("stream_key")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })),
        // Phase C2f: `inviteGroupCallParticipantResult*` (schema 1.8.67,
        // lines 7216-7227) — the `inviteGroupCallParticipant` answer.
        // Note: the success variant is
        // `inviteGroupCallParticipantResultSuccess`, not `...ResultOk`.
        "inviteGroupCallParticipantResultSuccess" => Ok(EnvelopePayload::Calls(
            CallsPayload::InviteGroupCallParticipantResult(
                InviteGroupCallParticipantResult::Success {
                    chat_id: value.get("chat_id").and_then(Value::as_i64).unwrap_or(0),
                    message_id: value.get("message_id").and_then(Value::as_i64).unwrap_or(0),
                },
            ),
        )),
        "inviteGroupCallParticipantResultUserPrivacyRestricted" => Ok(EnvelopePayload::Calls(
            CallsPayload::InviteGroupCallParticipantResult(
                InviteGroupCallParticipantResult::UserPrivacyRestricted,
            ),
        )),
        "inviteGroupCallParticipantResultUserAlreadyParticipant" => Ok(EnvelopePayload::Calls(
            CallsPayload::InviteGroupCallParticipantResult(
                InviteGroupCallParticipantResult::UserAlreadyParticipant,
            ),
        )),
        "inviteGroupCallParticipantResultUserWasBanned" => Ok(EnvelopePayload::Calls(
            CallsPayload::InviteGroupCallParticipantResult(
                InviteGroupCallParticipantResult::UserWasBanned,
            ),
        )),
        _ => return Ok(None),
    };
    payload.map(Some)
}
