//! Parses TDLib objects for the chat list: loading, folders, archive and pins.
use crate::ids::ChatId;
use crate::telegram::envelope::*;
use crate::telegram::requests::ArchiveChatListSettings;
use serde_json::Value;

/// The chat list domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_chat_list_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        "updateChatPosition" => Ok(EnvelopePayload::ChatList(
            ChatListPayload::UpdateChatPosition(parse_position(value)?),
        )),
        "updateChatLastMessage" => {
            let chat_id = ChatId(int53(value.get("chat_id"))?);
            Ok(EnvelopePayload::ChatList(
                ChatListPayload::UpdateChatLastMessage {
                    chat_id,
                    last_message: match value.get("last_message") {
                        None | Some(Value::Null) => None,
                        Some(message) => parse_message(message).ok(),
                    },
                    positions: parse_position_list(chat_id, value.get("positions")),
                },
            ))
        }
        "updateChatAddedToList" => Ok(EnvelopePayload::ChatList(
            ChatListPayload::UpdateChatAddedToList {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                list: parse_chat_list(value.get("chat_list")),
            },
        )),
        "updateChatRemovedFromList" => Ok(EnvelopePayload::ChatList(
            ChatListPayload::UpdateChatRemovedFromList {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                list: parse_chat_list(value.get("chat_list")),
            },
        )),
        "updateChatFolders" => {
            let folders = value
                .get("chat_folders")
                .and_then(Value::as_array)
                .map(|arr| arr.iter().filter_map(parse_chat_folder_info).collect())
                .unwrap_or_default();
            let are_tags_enabled = value
                .get("are_tags_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Ok(EnvelopePayload::ChatList(
                ChatListPayload::UpdateChatFolders {
                    folders,
                    are_tags_enabled,
                },
            ))
        }
        "updateUnreadMessageCount" => Ok(EnvelopePayload::ChatList(
            ChatListPayload::UpdateUnreadMessageCount {
                list: parse_chat_list(value.get("chat_list")),
                unread_count: unread_total(value, "unread_count"),
                unread_unmuted_count: unread_total(value, "unread_unmuted_count"),
            },
        )),
        "updateUnreadChatCount" => Ok(EnvelopePayload::ChatList(
            ChatListPayload::UpdateUnreadChatCount {
                list: parse_chat_list(value.get("chat_list")),
                total_count: unread_total(value, "total_count"),
                marked_as_unread_count: unread_total(value, "marked_as_unread_count"),
                marked_as_unread_unmuted_count: unread_total(
                    value,
                    "marked_as_unread_unmuted_count",
                ),
                unread_count: unread_total(value, "unread_count"),
                unread_unmuted_count: unread_total(value, "unread_unmuted_count"),
            },
        )),
        "updateChatIsMarkedAsUnread" => Ok(EnvelopePayload::ChatList(
            ChatListPayload::UpdateChatIsMarkedAsUnread {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                is_marked_as_unread: value
                    .get("is_marked_as_unread")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
        )),
        // Parity slice: `createChatFolder` / `editChatFolder` responses
        // (TDLib 1.8.67, `schema/td_api.tl:13358` / `:13361`).
        "chatFolderInfo" => parse_chat_folder_info(value)
            .map(|value| EnvelopePayload::ChatList(ChatListPayload::ChatFolderInfo(value)))
            .ok_or(ParseError::MissingField),
        // Parity slice: `getChatFolder` response (TDLib 1.8.67,
        // `schema/td_api.tl:13355`) — the full editable folder spec.
        "chatFolder" => parse_chat_folder(value)
            .map(|spec| EnvelopePayload::ChatList(ChatListPayload::ChatFolder { spec }))
            .ok_or(ParseError::MissingField),
        "chatFolderInviteLink" => parse_chat_folder_invite_link(value)
            .map(|value| EnvelopePayload::ChatList(ChatListPayload::ChatFolderInviteLink(value)))
            .ok_or(ParseError::MissingField),
        "chatFolderInviteLinks" => parse_chat_folder_invite_links(value)
            .map(|value| EnvelopePayload::ChatList(ChatListPayload::ChatFolderInviteLinks(value)))
            .ok_or(ParseError::MissingField),
        "premiumLimit" => parse_premium_limit(value)
            .map(|(type_name, default_value, premium_value)| {
                EnvelopePayload::ChatList(ChatListPayload::PremiumLimit {
                    type_name,
                    default_value,
                    premium_value,
                })
            })
            .ok_or(ParseError::MissingField),
        "recommendedChatFolders" => parse_recommended_chat_folders(value)
            .map(|value| EnvelopePayload::ChatList(ChatListPayload::RecommendedChatFolders(value)))
            .ok_or(ParseError::MissingField),
        "chatFolderInviteLinkInfo" => parse_chat_folder_invite_link_info(value)
            .map(|value| {
                EnvelopePayload::ChatList(ChatListPayload::ChatFolderInviteLinkInfo(value))
            })
            .ok_or(ParseError::MissingField),
        // Parity slice: `getChatListsToAddChat` response (TDLib 1.8.67,
        // `schema/td_api.tl:13347`) — the chat lists a chat may be added
        // to via `addChatToList`.
        "chatLists" => {
            let lists = value
                .get("chat_lists")
                .and_then(Value::as_array)
                .map(|arr| arr.iter().map(|v| parse_chat_list(Some(v))).collect())
                .unwrap_or_default();
            Ok(EnvelopePayload::ChatList(ChatListPayload::ChatLists {
                lists,
            }))
        }
        "chats" => Ok(EnvelopePayload::ChatList(ChatListPayload::Chats {
            total_count: value
                .get("total_count")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32(),
            chat_ids: value
                .get("chat_ids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| int53(Some(v)).ok())
                .map(ChatId)
                .collect(),
        })),
        // Slice CL2: `archiveChatListSettings` — the
        // `getArchiveChatListSettings` answer (schema 1.8.67, line
        // 3512). Missing fields default to false (never fail the
        // parse — a partial answer still beats no settings).
        "archiveChatListSettings" => Ok(EnvelopePayload::ChatList(
            ChatListPayload::ArchiveChatListSettings {
                settings: ArchiveChatListSettings {
                    archive_and_mute_new_chats_from_unknown_users: value
                        .get("archive_and_mute_new_chats_from_unknown_users")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    keep_unmuted_chats_archived: value
                        .get("keep_unmuted_chats_archived")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    keep_chats_from_folders_archived: value
                        .get("keep_chats_from_folders_archived")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                },
            },
        )),
        _ => return Ok(None),
    };
    payload.map(Some)
}

/// A non-negative `int32` unread total from an update object.
fn unread_total(value: &Value, key: &str) -> i32 {
    value
        .get(key)
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .clamp(0, i64::from(i32::MAX)) as i32
}
