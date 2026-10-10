//! Parses TDLib objects for users, contacts, profiles and secret chats.
use crate::ids::UserId;
use crate::telegram::envelope::*;
use serde_json::Value;

/// The users domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_users_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        // Phase B1: secret chat lifecycle (schema 1.8.67, lines 10741 /
        // 2816). `updateSecretChat` carries the full `secretChat` object in
        // its `secret_chat` field; the bare `secretChat` object is the
        // `getSecretChat` answer.
        "updateSecretChat" => Ok(EnvelopePayload::Users(UsersPayload::UpdateSecretChat {
            secret_chat: parse_secret_chat(value.get("secret_chat"))
                .ok_or(ParseError::MissingField)?,
        })),
        "secretChat" => Ok(EnvelopePayload::Users(UsersPayload::SecretChat {
            secret_chat: parse_secret_chat(Some(value)).ok_or(ParseError::MissingField)?,
        })),
        "updateUser" => {
            let user = value.get("user").ok_or(ParseError::MissingField)?;
            let parsed = parse_user(user).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Users(UsersPayload::UpdateUser {
                user_id: UserId(parsed.id),
                user: parsed,
            }))
        }
        "updateUserStatus" => Ok(EnvelopePayload::Users(UsersPayload::UpdateUserStatus {
            user_id: UserId(int53(value.get("user_id"))?),
            status: parse_user_status(value.get("status")),
        })),
        "users" => Ok(EnvelopePayload::Users(UsersPayload::Users {
            user_ids: value
                .get("user_ids")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(|id| int53(Some(id)).ok()).collect())
                .unwrap_or_default(),
        })),
        // A5: `checkChatUsernameResult*` (schema 1.8.67, lines 8583–8598).
        "checkChatUsernameResultOk" => Ok(EnvelopePayload::Users(
            UsersPayload::CheckChatUsernameResult(UsernameCheckResult::Available),
        )),
        "checkChatUsernameResultUsernameOccupied" => Ok(EnvelopePayload::Users(
            UsersPayload::CheckChatUsernameResult(UsernameCheckResult::Occupied),
        )),
        "checkChatUsernameResultUsernameInvalid" => Ok(EnvelopePayload::Users(
            UsersPayload::CheckChatUsernameResult(UsernameCheckResult::Invalid),
        )),
        "checkChatUsernameResultUsernamePurchasable" => Ok(EnvelopePayload::Users(
            UsersPayload::CheckChatUsernameResult(UsernameCheckResult::Purchasable),
        )),
        "checkChatUsernameResultPublicChatsTooMany" => Ok(EnvelopePayload::Users(
            UsersPayload::CheckChatUsernameResult(UsernameCheckResult::PublicChatsTooMany),
        )),
        "checkChatUsernameResultPublicGroupsUnavailable" => Ok(EnvelopePayload::Users(
            UsersPayload::CheckChatUsernameResult(UsernameCheckResult::PublicGroupsUnavailable),
        )),
        // Slice A6: `importedContacts` (schema 1.8.67, line 14517) —
        // the `importContacts` answer (NOT `ok`).
        "importedContacts" => {
            let user_ids = value
                .get("user_ids")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default();
            Ok(EnvelopePayload::Users(UsersPayload::ImportedContacts {
                user_ids,
            }))
        }
        "user" => Ok(EnvelopePayload::Users(UsersPayload::Me {
            user_id: int53(value.get("id"))?,
        })),
        "userFullInfo" => Ok(EnvelopePayload::Users(UsersPayload::UserFullInfo {
            extras: crate::telegram::envelope::envelope_types::parse_user_profile_extras(Some(
                value,
            )),
            bot_info: parse_bot_info(value.get("bot_info")),
            bio: parse_formatted_text(value.get("bio")),
            photo: parse_user_full_info_photo(value),
            photo_id: int53(value.get("photo").and_then(|p| p.get("id"))).ok(),
            blocked: is_block_list_main(value.get("block_list")),
        })),
        "updateUserFullInfo" => Ok(EnvelopePayload::Users(UsersPayload::UpdateUserFullInfo {
            user_id: UserId(int53(value.get("user_id"))?),
            extras: crate::telegram::envelope::envelope_types::parse_user_profile_extras(
                value.get("user_full_info"),
            ),
            bot_info: parse_bot_info(
                value
                    .get("user_full_info")
                    .and_then(|info| info.get("bot_info")),
            ),
            bio: parse_formatted_text(value.get("user_full_info").and_then(|info| info.get("bio"))),
            photo: value
                .get("user_full_info")
                .and_then(parse_user_full_info_photo),
            photo_id: int53(
                value
                    .get("user_full_info")
                    .and_then(|info| info.get("photo"))
                    .and_then(|p| p.get("id")),
            )
            .ok(),
            blocked: is_block_list_main(
                value
                    .get("user_full_info")
                    .and_then(|info| info.get("block_list")),
            ),
        })),
        _ => return Ok(None),
    };
    payload.map(Some)
}
