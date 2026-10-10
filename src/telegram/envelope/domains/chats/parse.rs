//! Parses TDLib objects for chat-level look and actions: backgrounds, themes, deep links, action bar.
use crate::ids::{ChatId, MessageId};
use crate::telegram::envelope::*;
use crate::telegram::name_accent::parse_name_accent_color;
use crate::telegram::profile_accent::parse_profile_accent_color;
use serde_json::Value;

/// The chats domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_chats_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        "updateChatTitle" => Ok(EnvelopePayload::Chats(ChatsPayload::UpdateChatTitle {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            title: value
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })),
        "updateChatReadInbox" => Ok(EnvelopePayload::Chats(ChatsPayload::UpdateChatReadInbox {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            last_read_inbox_message_id: MessageId(int53_or_zero(
                value.get("last_read_inbox_message_id"),
            )),
            unread_count: value
                .get("unread_count")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32(),
        })),
        // Slice CL3: mention / reaction badge counts (schema 1.8.67,
        // lines 10567/10570). Reading a mention or a reaction reports the
        // chat's new counter on `updateMessageMentionRead` /
        // `updateMessageUnreadReactions` instead (lines 10443/10450);
        // Quill keeps no per-message unread flags, so both fold into the
        // chat counter (Telegram X `Tdlib.updateMessageMentionRead` /
        // `Tdlib.updateMessageUnreadReactions`).
        "updateChatUnreadMentionCount" | "updateMessageMentionRead" => Ok(EnvelopePayload::Chats(
            ChatsPayload::UpdateChatUnreadMentionCount {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                unread_mention_count: value
                    .get("unread_mention_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
            },
        )),
        // B15: poll-vote badge counts (schema 1.8.67, lines 10457/10573).
        // `updateMessageContainsUnreadPollVotes` reports the chat's new
        // counter too, so both fold into the chat counter like mentions.
        "updateChatUnreadPollVoteCount" | "updateMessageContainsUnreadPollVotes" => Ok(
            EnvelopePayload::Chats(ChatsPayload::UpdateChatUnreadPollVoteCount {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                unread_poll_vote_count: value
                    .get("unread_poll_vote_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
            }),
        ),
        "updateChatUnreadReactionCount" => Ok(EnvelopePayload::Chats(
            ChatsPayload::UpdateChatUnreadReactionCount {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                unread_reaction_count: value
                    .get("unread_reaction_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
            },
        )),
        // Slice CL3: `updateChatBlockList` (schema 1.8.67, line 10594).
        "updateChatBlockList" => Ok(EnvelopePayload::Chats(ChatsPayload::UpdateChatBlockList {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            blocked: is_block_list_main(value.get("block_list")),
        })),
        // Batch 8: `updateChatActionBar` (schema 1.8.67, line 10526).
        "updateChatActionBar" => Ok(EnvelopePayload::Chats(ChatsPayload::UpdateChatActionBar {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            action_bar: parse_chat_action_bar(value.get("action_bar")),
        })),
        // Slice CL3: `reportChat` result (schema 1.8.67, lines
        // 9210–9219) — collapsed to Ok vs "more info required".
        "reportChatResultOk" => Ok(EnvelopePayload::Chats(ChatsPayload::ReportChatResult(
            ReportChatOutcome::Ok,
        ))),
        "reportChatResultOptionRequired" => Ok(EnvelopePayload::Chats(
            ChatsPayload::ReportChatResult(ReportChatOutcome::OptionRequired {
                title: json_field_str(value, "title"),
                options: parse_report_options(value.get("options")),
            }),
        )),
        "reportChatResultTextRequired" => Ok(EnvelopePayload::Chats(
            ChatsPayload::ReportChatResult(ReportChatOutcome::TextRequired {
                option_id: json_field_str(value, "option_id"),
                is_optional: value
                    .get("is_optional")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }),
        )),
        "reportChatResultMessagesRequired" => Ok(EnvelopePayload::Chats(
            ChatsPayload::ReportChatResult(ReportChatOutcome::MessagesRequired),
        )),
        "updateChatReadOutbox" => Ok(EnvelopePayload::Chats(ChatsPayload::UpdateChatReadOutbox {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            last_read_outbox_message_id: MessageId(int53_or_zero(
                value.get("last_read_outbox_message_id"),
            )),
        })),
        "updateChatNotificationSettings" => Ok(EnvelopePayload::Chats(
            ChatsPayload::UpdateChatNotificationSettings {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                notification_settings: parse_chat_notification_settings(
                    value.get("notification_settings"),
                ),
            },
        )),
        "updateChatDefaultDisableNotification" => Ok(EnvelopePayload::Chats(
            ChatsPayload::UpdateChatDefaultDisableNotification {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                default_disable_notification: json_bool(
                    value.get("default_disable_notification"),
                    false,
                ),
            },
        )),
        "updateChatAction" => Ok(EnvelopePayload::Chats(ChatsPayload::UpdateChatAction {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            sender: parse_message_sender(value.get("sender_id"))?,
            action: parse_chat_action(value.get("action")),
        })),
        // Parity slice: `updateChatPhoto` (schema 1.8.67, line 10488).
        "updateChatPhoto" => Ok(EnvelopePayload::Chats(ChatsPayload::UpdateChatPhoto {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            photo: parse_chat_photo_small(value.get("photo")),
        })),
        // Slice A12: `updateProfileAccentColors` (schema:10964).
        // Malformed palette entries are skipped; a missing
        // `available_accent_color_ids` just means an empty picker.
        "updateProfileAccentColors" => {
            let colors = value
                .get("colors")
                .and_then(Value::as_array)
                .map(|arr| arr.iter().filter_map(parse_profile_accent_color).collect())
                .unwrap_or_default();
            let available_ids = value
                .get("available_accent_color_ids")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_i64())
                        .filter_map(|n| i32::try_from(n).ok())
                        .collect()
                })
                .unwrap_or_default();
            Ok(EnvelopePayload::Chats(
                ChatsPayload::UpdateProfileAccentColors {
                    colors,
                    available_ids,
                },
            ))
        }
        // Malformed entries are skipped; the palette only recolors names.
        "updateAccentColors" => {
            let colors = value
                .get("colors")
                .and_then(Value::as_array)
                .map(|arr| arr.iter().filter_map(parse_name_accent_color).collect())
                .unwrap_or_default();
            let available_ids = value
                .get("available_accent_color_ids")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_i64)
                        .filter_map(|n| i32::try_from(n).ok())
                        .collect()
                })
                .unwrap_or_default();
            Ok(EnvelopePayload::Chats(ChatsPayload::UpdateAccentColors {
                colors,
                available_ids,
            }))
        }
        "updateNewChat" => {
            let chat = value.get("chat").ok_or(ParseError::MissingField)?;
            parse_new_chat(chat)
        }
        "updateChatPermissions" => {
            // Parity slice 4: `updateChatPermissions` (schema 1.8.67, line
            // 10500) — keep the send-permission gate fresh.
            let permissions = value.get("permissions");
            Ok(EnvelopePayload::Chats(
                ChatsPayload::UpdateChatPermissions {
                    chat_id: ChatId(int53(value.get("chat_id"))?),
                    can_send_basic_messages: permissions
                        .and_then(|p| p.get("can_send_basic_messages"))
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    // Slice G1: full `chatPermissions` block (schema 1.8.67,
                    // line 1070) for the permissions editor.
                    permissions: parse_chat_permissions(permissions),
                },
            ))
        }
        "updateChatMessageAutoDeleteTime" => {
            // Phase B4: `updateChatMessageAutoDeleteTime` (schema 1.8.67,
            // line 10549) — the chat-level auto-delete or self-destruct
            // timer changed.
            Ok(EnvelopePayload::Chats(
                ChatsPayload::UpdateChatMessageAutoDeleteTime {
                    chat_id: ChatId(int53(value.get("chat_id"))?),
                    message_auto_delete_time: value
                        .get("message_auto_delete_time")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                        .sat_i32(),
                },
            ))
        }
        "backgrounds" => parse_backgrounds(value)
            .map(|value| EnvelopePayload::Chats(ChatsPayload::Backgrounds(value)))
            .ok_or(ParseError::MissingField),
        "background" => parse_background(value)
            .map(|value| EnvelopePayload::Chats(ChatsPayload::Background(value)))
            .ok_or(ParseError::MissingField),
        "updateDefaultBackground" => {
            let background = value
                .get("background")
                .and_then(parse_background)
                .ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Chats(
                ChatsPayload::UpdateDefaultBackground {
                    for_dark_theme: value
                        .get("for_dark_theme")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    background,
                },
            ))
        }
        "updateChatBackground" => Ok(EnvelopePayload::Chats(ChatsPayload::UpdateChatBackground {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            background: parse_chat_background(value.get("background")),
        })),
        "updateChatTheme" => Ok(EnvelopePayload::Chats(ChatsPayload::UpdateChatTheme {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            theme_name: parse_chat_theme_name(value.get("theme")),
        })),
        "updateEmojiChatThemes" => Ok(EnvelopePayload::Chats(ChatsPayload::UpdateEmojiChatThemes(
            parse_emoji_chat_themes(value),
        ))),
        "chatPhotos" => Ok(EnvelopePayload::Chats(ChatsPayload::ChatPhotos {
            total_count: value
                .get("total_count")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32(),
            photos: value
                .get("photos")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(crate::telegram::envelope::users::parse_profile_photo)
                .collect(),
        })),
        "updateChatOnlineMemberCount" => Ok(EnvelopePayload::Chats(
            ChatsPayload::UpdateChatOnlineMemberCount {
                chat_id: int53(value.get("chat_id"))?,
                online_member_count: int53(value.get("online_member_count"))
                    .unwrap_or(0)
                    .sat_i32(),
            },
        )),
        // Slice CL2: `createPrivateChat` answer — a bare `chat`
        // object (schema 1.8.67, line 13312). Parsed exactly like
        // `updateNewChat`'s inner chat so the reducer inserts it into
        // the model; the driver opens it through the normal
        // `select_chat` flow when the `@extra` matches our
        // `CreatePrivateChat` request.
        "chat" => parse_new_chat(value),
        // B7: `updateChatAvailableReactions` (schema 1.8.67, line 10532).
        "updateChatAvailableReactions" => Ok(EnvelopePayload::Chats(
            ChatsPayload::UpdateChatAvailableReactions {
                chat_id: int53(value.get("chat_id"))?,
                available_reactions: parse_chat_available_reactions(
                    value.get("available_reactions"),
                )
                .ok_or(ParseError::MissingField)?,
            },
        )),
        "updateChatHasProtectedContent" => Ok(EnvelopePayload::Chats(
            ChatsPayload::UpdateChatHasProtectedContent {
                chat_id: int53(value.get("chat_id"))?,
                has_protected_content: value
                    .get("has_protected_content")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
        )),
        "updateChatAccentColors" => Ok(EnvelopePayload::Chats(
            ChatsPayload::UpdateChatAccentColors {
                chat_id: int53(value.get("chat_id"))?,
                accent: ChatAccent::parse(value),
            },
        )),
        "chatBoostLinkInfo" => Ok(EnvelopePayload::Chats(ChatsPayload::ChatBoostLinkInfo {
            chat_id: int53_or_zero(value.get("chat_id")),
        })),
        "messageLinkInfo" => Ok(EnvelopePayload::Chats(ChatsPayload::MessageLinkInfo {
            chat_id: int53_or_zero(value.get("chat_id")),
            message_id: int53_or_zero(value.get("message").and_then(|m| m.get("id"))),
            media_timestamp: int53_or_zero(value.get("media_timestamp"))
                .try_into()
                .ok()
                .filter(|t| *t > 0),
            thread_id: value
                .pointer("/topic_id/message_thread_id")
                .and_then(Value::as_i64)
                .filter(|id| *id > 0),
        })),
        // `getDeepLinkInfo` answer (schema 1.8.67, line 10087).
        "deepLinkInfo" => {
            let text = parse_formatted_text(value.get("text"));
            Ok(EnvelopePayload::Chats(ChatsPayload::DeepLinkInfo {
                entities: parse_text_entities(&text, value.get("text")),
                text,
                need_update: value
                    .get("need_update_application")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }))
        }
        ty if ty.starts_with("internalLinkType") => {
            crate::deep_link_types::parse_internal_link(value)
                .map(|value| EnvelopePayload::Chats(ChatsPayload::InternalLinkType(value)))
                .ok_or(ParseError::MissingField)
        }
        _ => return Ok(None),
    };
    payload.map(Some)
}
