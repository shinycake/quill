use super::*;
use crate::data_settings::{AutoDownloadNetSettings, StorageChatStats};
use crate::ids::{ChatId, MessageId, UserId};
use crate::privacy::PrivacyRule;
use crate::rich::parse_rich_message;
use crate::telegram::envelope_story::parse_story_album;
use crate::telegram::profile_accent::parse_profile_accent_color;
use crate::telegram::requests::ArchiveChatListSettings;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;

/// Slice CL3: true when a `BlockList` JSON value is `blockListMain`
/// (schema 1.8.67, lines 9692–9695).
pub(crate) fn is_block_list_main(block_list: Option<&Value>) -> bool {
    block_list
        .and_then(|b| b.get("@type"))
        .and_then(Value::as_str)
        == Some("blockListMain")
}

pub(crate) fn parse_payload(type_name: &str, json: &str) -> Result<EnvelopePayload, ParseError> {
    let value: Value = serde_json::from_str(json).map_err(|_| ParseError::InvalidJson)?;
    match type_name {
        "updatePendingMessage" => {
            let (content, files) = parse_content(value.get("content"));
            Ok(EnvelopePayload::UpdatePendingMessage {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                forum_topic_id: i32::try_from(int53(value.get("forum_topic_id"))?)
                    .map_err(|_| ParseError::BadInt)?,
                draft_id: int53(value.get("draft_id"))?,
                can_stop: value
                    .get("can_stop")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                keep_on_stop: value
                    .get("keep_on_stop")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                content,
                files,
            })
        }
        "count" => Ok(EnvelopePayload::Count {
            count: value.get("count").and_then(Value::as_i64).unwrap_or(0) as i32,
        }),
        "updateStopMessageDraft" => Ok(EnvelopePayload::UpdateStopMessageDraft {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            forum_topic_id: i32::try_from(int53(value.get("forum_topic_id"))?)
                .map_err(|_| ParseError::BadInt)?,
            draft_id: int53(value.get("draft_id"))?,
        }),
        "updateAuthorizationState" => {
            let state = value
                .get("authorization_state")
                .ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateAuthorizationState(parse_auth(state)))
        }
        // MED4: `updateOption` (schema:10926). TDLib pushes all options
        // after authorization; Quill keeps `message_caption_length_max`.
        "updateOption" => {
            let name = json_field_str(&value, "name");
            let raw = value.get("value").unwrap_or(&Value::Null);
            let value = match raw.get("@type").and_then(Value::as_str).unwrap_or("") {
                "optionValueBoolean" => OptionValue::Boolean(json_bool(raw.get("value"), false)),
                "optionValueInteger" => OptionValue::Integer(int53_or_zero(raw.get("value"))),
                "optionValueString" => OptionValue::String(json_field_str(raw, "value")),
                _ => OptionValue::Empty,
            };
            Ok(EnvelopePayload::UpdateOption { name, value })
        }
        "authorizationStateWaitTdlibParameters"
        | "authorizationStateWaitPhoneNumber"
        | "authorizationStateWaitPremiumPurchase"
        | "authorizationStateWaitEmailAddress"
        | "authorizationStateWaitEmailCode"
        | "authorizationStateWaitCode"
        | "authorizationStateWaitOtherDeviceConfirmation"
        | "authorizationStateWaitRegistration"
        | "authorizationStateWaitPassword"
        | "authorizationStateReady"
        | "authorizationStateLoggingOut"
        | "authorizationStateClosing"
        | "authorizationStateClosed" => Ok(EnvelopePayload::UpdateAuthorizationState(parse_auth(
            &value,
        ))),
        "updateNewMessage" => {
            let message = value.get("message").ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateNewMessage(parse_message(message)?))
        }
        "updateMessageSendSucceeded" => Ok(EnvelopePayload::UpdateMessageSendSucceeded {
            message: parse_message(value.get("message").ok_or(ParseError::MissingField)?)?,
            old_message_id: MessageId(int53(value.get("old_message_id"))?),
        }),
        "updateMessageSendFailed" => Ok(EnvelopePayload::UpdateMessageSendFailed {
            message: parse_message(value.get("message").ok_or(ParseError::MissingField)?)?,
            old_message_id: MessageId(int53(value.get("old_message_id"))?),
            error: parse_error(value.get("error")),
        }),
        "updateMessageSendAcknowledged" => Ok(EnvelopePayload::UpdateMessageSendAcknowledged {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            message_id: MessageId(int53(value.get("message_id"))?),
        }),
        "updateDeleteMessages" => Ok(EnvelopePayload::UpdateDeleteMessages {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            message_ids: int53_array(value.get("message_ids")),
            is_permanent: value
                .get("is_permanent")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            from_cache: value
                .get("from_cache")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "updateMessageContent" => {
            let (content, files) = parse_content(value.get("new_content"));
            Ok(EnvelopePayload::UpdateMessageContent {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                message_id: MessageId(int53(value.get("message_id"))?),
                content,
                files,
            })
        }
        "updateMessageEphemeralContent" => {
            // Schema: `ephemeral_content` may be explicit null ("none") —
            // that clears the stored ephemeral content. Only a missing or
            // mistyped field is a parse error.
            let ephemeral = match value.get("ephemeral_content") {
                None => return Err(ParseError::MissingField),
                Some(v) if v.is_null() => None,
                Some(v) => {
                    Some(parse_ephemeral_message_content(Some(v)).ok_or(ParseError::MissingField)?)
                }
            };
            Ok(EnvelopePayload::UpdateMessageEphemeralContent {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                message_id: MessageId(int53(value.get("message_id"))?),
                ephemeral,
            })
        }
        "updateMessageContentOpened" => Ok(EnvelopePayload::UpdateMessageContentOpened {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            message_id: MessageId(int53(value.get("message_id"))?),
        }),
        "updateMessageEdited" => Ok(EnvelopePayload::UpdateMessageEdited {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            message_id: MessageId(int53(value.get("message_id"))?),
            edit_date: value.get("edit_date").and_then(Value::as_i64).unwrap_or(0) as i32,
            reply_markup: parse_reply_markup(value.get("reply_markup")),
        }),
        "updatePoll" => Ok(EnvelopePayload::UpdatePoll {
            poll: parse_poll(value.get("poll")).ok_or(ParseError::MissingField)?,
        }),
        "updateChatPosition" => Ok(EnvelopePayload::UpdateChatPosition(parse_position(&value)?)),
        "updateChatTitle" => Ok(EnvelopePayload::UpdateChatTitle {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            title: value
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        "updateChatLastMessage" => {
            let chat_id = ChatId(int53(value.get("chat_id"))?);
            Ok(EnvelopePayload::UpdateChatLastMessage {
                chat_id,
                last_message: match value.get("last_message") {
                    None | Some(Value::Null) => None,
                    Some(message) => parse_message(message).ok(),
                },
                positions: parse_position_list(chat_id, value.get("positions")),
            })
        }
        "updateChatAddedToList" => Ok(EnvelopePayload::UpdateChatAddedToList {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            list: parse_chat_list(value.get("chat_list")),
        }),
        "updateChatRemovedFromList" => Ok(EnvelopePayload::UpdateChatRemovedFromList {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            list: parse_chat_list(value.get("chat_list")),
        }),
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
            Ok(EnvelopePayload::UpdateChatFolders {
                folders,
                are_tags_enabled,
            })
        }
        "updateChatActiveStories" => {
            let active_stories = value
                .get("active_stories")
                .ok_or(ParseError::MissingField)?;
            parse_chat_active_stories(active_stories)
                .map(|active_stories| EnvelopePayload::UpdateChatActiveStories { active_stories })
                .ok_or(ParseError::MissingField)
        }
        "chatActiveStories" => parse_chat_active_stories(&value)
            .map(|active_stories| EnvelopePayload::ChatActiveStories { active_stories })
            .ok_or(ParseError::MissingField),
        "story" => {
            let (story, files) = parse_story(&value).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Story { story, files })
        }
        "storyAlbums" => {
            let albums = value
                .get("albums")
                .and_then(Value::as_array)
                .map(|albums| albums.iter().filter_map(parse_story_album).collect())
                .unwrap_or_default();
            Ok(EnvelopePayload::StoryAlbums { albums })
        }
        "storyAlbum" => {
            let album = parse_story_album(&value).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::StoryAlbum { album })
        }
        "stories" => {
            let total_count = value
                .get("total_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32;
            let stories = value
                .get("stories")
                .and_then(Value::as_array)
                .map(|stories| stories.iter().filter_map(parse_story).collect())
                .unwrap_or_default();
            let pinned_story_ids = value
                .get("pinned_story_ids")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| id.as_i64().map(|id| id as i32))
                        .collect()
                })
                .unwrap_or_default();
            Ok(EnvelopePayload::Stories {
                total_count,
                stories,
                pinned_story_ids,
            })
        }
        "updateStory" => {
            let story = value.get("story").ok_or(ParseError::MissingField)?;
            let (story, files) = parse_story(story).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Story { story, files })
        }
        "updateStoryDeleted" => Ok(EnvelopePayload::UpdateStoryDeleted {
            poster_chat_id: int53(value.get("story_poster_chat_id"))?,
            story_id: value
                .get("story_id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)? as i32,
        }),
        // Phase B1: secret chat lifecycle (schema 1.8.67, lines 10741 /
        // 2816). `updateSecretChat` carries the full `secretChat` object in
        // its `secret_chat` field; the bare `secretChat` object is the
        // `getSecretChat` answer.
        "updateSecretChat" => Ok(EnvelopePayload::UpdateSecretChat {
            secret_chat: parse_secret_chat(value.get("secret_chat"))
                .ok_or(ParseError::MissingField)?,
        }),
        "secretChat" => Ok(EnvelopePayload::SecretChat {
            secret_chat: parse_secret_chat(Some(&value)).ok_or(ParseError::MissingField)?,
        }),
        // Phase C1: call signaling updates (schema 1.8.67, lines
        // 10816 / 10862) and the `createCall` answer (`callId`,
        // line 7034).
        "updateCall" => Ok(EnvelopePayload::UpdateCall {
            call: parse_call(value.get("call")).ok_or(ParseError::MissingField)?,
        }),
        "updateNewCallSignalingData" => Ok(EnvelopePayload::UpdateNewCallSignalingData {
            call_id: value
                .get("call_id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)? as i32,
            data: value
                .get("data")
                .and_then(Value::as_str)
                .and_then(|s| STANDARD.decode(s).ok())
                .unwrap_or_default(),
        }),
        "callId" => Ok(EnvelopePayload::CallId {
            id: value
                .get("id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)? as i32,
        }),
        // Phase C3a: `groupCallId` (schema 1.8.67, line 7037) — the
        // `createVideoChat` answer. The driver fetches the full
        // `groupCall` via `getGroupCall`; live state arrives as
        // `updateGroupCall`.
        "groupCallId" => Ok(EnvelopePayload::GroupCallId {
            id: value
                .get("id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)? as i32,
        }),
        // Phase C2f: `groupCallInfo` (schema 1.8.67, line 7190) — the
        // `joinGroupCall` answer.
        "groupCallInfo" => Ok(EnvelopePayload::GroupCallInfo {
            group_call_id: value
                .get("group_call_id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)? as i32,
            join_payload: value
                .get("join_payload")
                .and_then(Value::as_str)
                .ok_or(ParseError::MissingField)?
                .to_string(),
        }),
        // Phase C3a: group-call signaling updates (schema 1.8.67,
        // lines 10819 / 10824 / 10830 / 10836 / 10576). All
        // signaling-only: no media transport until Phase C2.
        // `getGroupCall` (schema :14274) answers with a bare `groupCall`
        // object — route it through the same handling as
        // `updateGroupCall` so the fetch path can create the tracker.
        "groupCall" => Ok(EnvelopePayload::UpdateGroupCall {
            group_call: parse_group_call(Some(&value)).ok_or(ParseError::MissingField)?,
        }),
        "updateGroupCall" => Ok(EnvelopePayload::UpdateGroupCall {
            group_call: parse_group_call(value.get("group_call"))
                .ok_or(ParseError::MissingField)?,
        }),
        "updateGroupCallParticipant" => Ok(EnvelopePayload::UpdateGroupCallParticipant {
            group_call_id: value
                .get("group_call_id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)? as i32,
            participant: parse_group_call_participant(value.get("participant"))
                .ok_or(ParseError::MissingField)?,
        }),
        "updateGroupCallParticipants" => Ok(EnvelopePayload::UpdateGroupCallParticipants {
            group_call_id: value
                .get("group_call_id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)? as i32,
            participant_user_ids: value
                .get("participant_user_ids")
                .and_then(Value::as_array)
                .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect::<Vec<i64>>())
                .ok_or(ParseError::MissingField)?,
        }),
        "updateGroupCallVerificationState" => {
            Ok(EnvelopePayload::UpdateGroupCallVerificationState {
                group_call_id: value
                    .get("group_call_id")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)? as i32,
                generation: value
                    .get("generation")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)? as i32,
                emojis: value
                    .get("emojis")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect::<Vec<String>>()
                    })
                    .ok_or(ParseError::MissingField)?,
            })
        }
        // Phase C2h: group-call message updates (schema 1.8.67,
        // lines 10839 / 10851 / 10856).
        "updateNewGroupCallMessage" => Ok(EnvelopePayload::UpdateNewGroupCallMessage {
            group_call_id: value
                .get("group_call_id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)? as i32,
            message: parse_group_call_message(value.get("message"))
                .ok_or(ParseError::MissingField)?,
        }),
        "updateGroupCallMessageSendFailed" => {
            Ok(EnvelopePayload::UpdateGroupCallMessageSendFailed {
                group_call_id: value
                    .get("group_call_id")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)? as i32,
                message_id: value
                    .get("message_id")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)? as i32,
                error: parse_error(value.get("error")),
            })
        }
        "updateGroupCallMessagesDeleted" => Ok(EnvelopePayload::UpdateGroupCallMessagesDeleted {
            group_call_id: value
                .get("group_call_id")
                .and_then(Value::as_i64)
                .ok_or(ParseError::MissingField)? as i32,
            message_ids: value
                .get("message_ids")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_i64())
                        .map(|id| id as i32)
                        .collect::<Vec<i32>>()
                })
                .ok_or(ParseError::MissingField)?,
        }),
        "updateChatVideoChat" => Ok(EnvelopePayload::UpdateChatVideoChat {
            chat_id: int53(value.get("chat_id"))?,
            video_chat: parse_video_chat(value.get("video_chat"))
                .ok_or(ParseError::MissingField)?,
        }),
        "updateStoryPostSucceeded" => {
            let story = value.get("story").ok_or(ParseError::MissingField)?;
            let (story, files) = parse_story(story).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateStoryPostSucceeded {
                story,
                files,
                old_story_id: value
                    .get("old_story_id")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)? as i32,
            })
        }
        "updateStoryPostFailed" => {
            let story = value.get("story").ok_or(ParseError::MissingField)?;
            let (story, _files) = parse_story(story).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateStoryPostFailed {
                story,
                error: parse_error(value.get("error")),
            })
        }
        "availableReactions" => {
            let list = |key: &str| -> Vec<_> {
                value
                    .get(key)
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(parse_story_available_reaction)
                            .collect()
                    })
                    .unwrap_or_default()
            };
            Ok(EnvelopePayload::StoryAvailableReactions {
                reactions: list("top_reactions"),
                recent: list("recent_reactions"),
                popular: list("popular_reactions"),
                allow_custom_emoji: value
                    .get("allow_custom_emoji")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        }
        // Phase 9.3: `canPostStory` answer — one of the
        // `canPostStoryResult*` variants (TDLib 1.8.67, `schema/td_api.tl:8535`
        // – `td_api.tl:8553`).
        "canPostStoryResultOk"
        | "canPostStoryResultPremiumNeeded"
        | "canPostStoryResultBoostNeeded"
        | "canPostStoryResultActiveStoryLimitExceeded"
        | "canPostStoryResultWeeklyLimitExceeded"
        | "canPostStoryResultMonthlyLimitExceeded"
        | "canPostStoryResultLiveStoryIsActive" => {
            let result = parse_can_post_story_result(&value).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::CanPostStoryResult { result })
        }
        "updateUnreadMessageCount" => Ok(EnvelopePayload::UpdateUnreadMessageCount {
            list: parse_chat_list(value.get("chat_list")),
            unread_count: unread_total(&value, "unread_count"),
            unread_unmuted_count: unread_total(&value, "unread_unmuted_count"),
        }),
        "updateUnreadChatCount" => Ok(EnvelopePayload::UpdateUnreadChatCount {
            list: parse_chat_list(value.get("chat_list")),
            total_count: unread_total(&value, "total_count"),
            marked_as_unread_count: unread_total(&value, "marked_as_unread_count"),
            marked_as_unread_unmuted_count: unread_total(&value, "marked_as_unread_unmuted_count"),
            unread_count: unread_total(&value, "unread_count"),
            unread_unmuted_count: unread_total(&value, "unread_unmuted_count"),
        }),
        "updateChatReadInbox" => Ok(EnvelopePayload::UpdateChatReadInbox {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            last_read_inbox_message_id: MessageId(int53_or_zero(
                value.get("last_read_inbox_message_id"),
            )),
            unread_count: value
                .get("unread_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        }),
        // Slice CL3: mention / reaction badge counts (schema 1.8.67,
        // lines 10567/10570). Reading a mention or a reaction reports the
        // chat's new counter on `updateMessageMentionRead` /
        // `updateMessageUnreadReactions` instead (lines 10443/10450);
        // Quill keeps no per-message unread flags, so both fold into the
        // chat counter (Telegram X `Tdlib.updateMessageMentionRead` /
        // `Tdlib.updateMessageUnreadReactions`).
        "updateChatUnreadMentionCount" | "updateMessageMentionRead" => {
            Ok(EnvelopePayload::UpdateChatUnreadMentionCount {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                unread_mention_count: value
                    .get("unread_mention_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
            })
        }
        "updateChatUnreadReactionCount" | "updateMessageUnreadReactions" => {
            Ok(EnvelopePayload::UpdateChatUnreadReactionCount {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                unread_reaction_count: value
                    .get("unread_reaction_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
            })
        }
        // Slice CL3: `updateChatBlockList` (schema 1.8.67, line 10594).
        "updateChatBlockList" => Ok(EnvelopePayload::UpdateChatBlockList {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            blocked: is_block_list_main(value.get("block_list")),
        }),
        // Batch 8: `updateChatActionBar` (schema 1.8.67, line 10526).
        "updateChatActionBar" => Ok(EnvelopePayload::UpdateChatActionBar {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            action_bar: parse_chat_action_bar(value.get("action_bar")),
        }),
        // Slice CL3: `reportChat` result (schema 1.8.67, lines
        // 9210–9219) — collapsed to Ok vs "more info required".
        "reportChatResultOk" => Ok(EnvelopePayload::ReportChatResult(ReportChatOutcome::Ok)),
        "reportChatResultOptionRequired" => Ok(EnvelopePayload::ReportChatResult(
            ReportChatOutcome::OptionRequired {
                title: json_field_str(&value, "title"),
                options: parse_report_options(value.get("options")),
            },
        )),
        "reportChatResultTextRequired" => Ok(EnvelopePayload::ReportChatResult(
            ReportChatOutcome::TextRequired {
                option_id: json_field_str(&value, "option_id"),
                is_optional: value
                    .get("is_optional")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
        )),
        "reportChatResultMessagesRequired" => Ok(EnvelopePayload::ReportChatResult(
            ReportChatOutcome::MessagesRequired,
        )),
        // Message menu "N Seen" / "N Reacted" rows (schema lines 2859-2879,
        // 7315-7318).
        "messageViewers" => Ok(EnvelopePayload::MessageViewers(parse_message_viewers(
            &value,
        ))),
        "addedReactions" => Ok(EnvelopePayload::AddedReactions(parse_added_reactions(
            &value,
        ))),
        "updateChatReadOutbox" => Ok(EnvelopePayload::UpdateChatReadOutbox {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            last_read_outbox_message_id: MessageId(int53_or_zero(
                value.get("last_read_outbox_message_id"),
            )),
        }),
        "updateChatNotificationSettings" => Ok(EnvelopePayload::UpdateChatNotificationSettings {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            notification_settings: parse_chat_notification_settings(
                value.get("notification_settings"),
            ),
        }),
        "updateChatIsMarkedAsUnread" => Ok(EnvelopePayload::UpdateChatIsMarkedAsUnread {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            is_marked_as_unread: value
                .get("is_marked_as_unread")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "updateChatAction" => Ok(EnvelopePayload::UpdateChatAction {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            sender: parse_message_sender(value.get("sender_id"))?,
            action: parse_chat_action(value.get("action")),
        }),
        // Parity slice: `updateChatPhoto` (schema 1.8.67, line 10488).
        "updateChatPhoto" => Ok(EnvelopePayload::UpdateChatPhoto {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            photo: parse_chat_photo_small(value.get("photo")),
        }),
        "updateChatDraftMessage" => {
            let chat_id = ChatId(int53(value.get("chat_id"))?);
            Ok(EnvelopePayload::UpdateChatDraftMessage {
                chat_id,
                draft: parse_chat_draft(value.get("draft_message")),
                positions: parse_position_list(chat_id, value.get("positions")),
            })
        }
        "updateUser" => {
            let user = value.get("user").ok_or(ParseError::MissingField)?;
            let parsed = parse_user(user).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateUser {
                user_id: UserId(parsed.id),
                user: parsed,
            })
        }
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
            Ok(EnvelopePayload::UpdateProfileAccentColors {
                colors,
                available_ids,
            })
        }
        "updateUserStatus" => Ok(EnvelopePayload::UpdateUserStatus {
            user_id: UserId(int53(value.get("user_id"))?),
            status: parse_user_status(value.get("status")),
        }),
        "users" => Ok(EnvelopePayload::Users {
            user_ids: value
                .get("user_ids")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(|id| int53(Some(id)).ok()).collect())
                .unwrap_or_default(),
        }),
        "updateUnconfirmedSession" => Ok(parse_unconfirmed_session_update(&value)),
        "updateServiceNotification" => Ok(parse_service_notification(&value)),
        "updateTermsOfService" => parse_terms_of_service(&value)
            .map(|terms| EnvelopePayload::UpdateTermsOfService { terms })
            .ok_or(ParseError::MissingField),
        "emailAddressAuthenticationCodeInfo" => Ok(EnvelopePayload::EmailCodeInfo {
            pattern: json_field_str(&value, "email_address_pattern"),
            length: json_i32(value.get("length"), 0).max(0),
        }),
        "resetPasswordResultOk" | "resetPasswordResultPending" | "resetPasswordResultDeclined" => {
            Ok(parse_reset_password_result(type_name, &value))
        }
        "updateConnectionState" => Ok(EnvelopePayload::UpdateConnectionState(parse_connection(
            value.get("state"),
        ))),
        "updateNewChat" => {
            let chat = value.get("chat").ok_or(ParseError::MissingField)?;
            parse_new_chat(chat)
        }
        "updateChatPermissions" => {
            // Parity slice 4: `updateChatPermissions` (schema 1.8.67, line
            // 10500) — keep the send-permission gate fresh.
            let permissions = value.get("permissions");
            Ok(EnvelopePayload::UpdateChatPermissions {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                can_send_basic_messages: permissions
                    .and_then(|p| p.get("can_send_basic_messages"))
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                // Slice G1: full `chatPermissions` block (schema 1.8.67,
                // line 1070) for the permissions editor.
                permissions: parse_chat_permissions(permissions),
            })
        }
        "updateChatMessageAutoDeleteTime" => {
            // Phase B4: `updateChatMessageAutoDeleteTime` (schema 1.8.67,
            // line 10549) — the chat-level auto-delete or self-destruct
            // timer changed.
            Ok(EnvelopePayload::UpdateChatMessageAutoDeleteTime {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                message_auto_delete_time: value
                    .get("message_auto_delete_time")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
            })
        }
        "ok" => Ok(EnvelopePayload::Ok),
        // `parity:proxy-settings`: schema 1.8.67 :10118 / :10121 / :10077.
        "addedProxies" => Ok(EnvelopePayload::AddedProxies {
            proxies: crate::proxy::parse_added_proxies(&value),
        }),
        "addedProxy" => Ok(EnvelopePayload::AddedProxy {
            proxy: crate::proxy::parse_added_proxy(&value),
        }),
        "seconds" => Ok(EnvelopePayload::Seconds {
            seconds: value.get("seconds").and_then(Value::as_f64).unwrap_or(0.0),
        }),
        // A5: `checkChatUsernameResult*` (schema 1.8.67, lines 8583–8598).
        "checkChatUsernameResultOk" => Ok(EnvelopePayload::CheckChatUsernameResult(
            UsernameCheckResult::Available,
        )),
        "checkChatUsernameResultUsernameOccupied" => Ok(EnvelopePayload::CheckChatUsernameResult(
            UsernameCheckResult::Occupied,
        )),
        "checkChatUsernameResultUsernameInvalid" => Ok(EnvelopePayload::CheckChatUsernameResult(
            UsernameCheckResult::Invalid,
        )),
        "checkChatUsernameResultUsernamePurchasable" => Ok(
            EnvelopePayload::CheckChatUsernameResult(UsernameCheckResult::Purchasable),
        ),
        "checkChatUsernameResultPublicChatsTooMany" => Ok(
            EnvelopePayload::CheckChatUsernameResult(UsernameCheckResult::PublicChatsTooMany),
        ),
        "checkChatUsernameResultPublicGroupsUnavailable" => Ok(
            EnvelopePayload::CheckChatUsernameResult(UsernameCheckResult::PublicGroupsUnavailable),
        ),
        // Slice A6: `importedContacts` (schema 1.8.67, line 14517) —
        // the `importContacts` answer (NOT `ok`).
        "importedContacts" => {
            let user_ids = value
                .get("user_ids")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default();
            Ok(EnvelopePayload::ImportedContacts { user_ids })
        }
        "richMessage" => {
            let (blocks, is_full) = parse_rich_message(&value);
            Ok(EnvelopePayload::RichMessage {
                rich: RichMessageContent { blocks, is_full },
            })
        }
        // Slice msg-richtext-ai-tools: `fixedText` (schema:157) — the
        // `fixTextWithAi` answer.
        "fixedText" => Ok(EnvelopePayload::FixedText {
            text: parse_formatted_text(value.get("text")),
        }),
        // Slice msg-richtext-ai-tools: bare `formattedText` (schema:3046)
        // — the `composeTextWithAi` answer.
        "formattedText" => Ok(EnvelopePayload::FormattedText {
            text: parse_formatted_text(Some(&value)),
        }),
        // MED4: `webPageInstantView` (schema:4377) — same `blocks` /
        // `is_full` shape as `richMessage`, so the M2 parser applies.
        "webPageInstantView" => {
            let (blocks, is_full) = parse_rich_message(&value);
            Ok(EnvelopePayload::WebPageInstantView {
                rich: RichMessageContent { blocks, is_full },
            })
        }
        // MED4b: `getLinkPreview` answer (schema:14792) — the full
        // `linkPreview` object. Thumbnail `ParsedFile`s are dropped: the
        // composer chip shows title/description + a media glyph, never
        // the image (downloading transient preview files is out of
        // slice — DECISIONS.md).
        "linkPreview" => {
            let (preview, _files) = parse_link_preview(Some(&value));
            Ok(EnvelopePayload::LinkPreview { preview })
        }
        "messageLink" => Ok(EnvelopePayload::MessageLink {
            link: value
                .get("link")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            is_public: value
                .get("is_public")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        // M1 fix-up: only `can_get_link` is kept (see the
        // `MessageProperties` payload docs).
        "messageReadDateRead"
        | "messageReadDateUnread"
        | "messageReadDateTooOld"
        | "messageReadDateUserPrivacyRestricted"
        | "messageReadDateMyPrivacyRestricted" => parse_message_read_date(&value)
            .map(EnvelopePayload::MessageReadDate)
            .ok_or(ParseError::MissingField),
        "messageProperties" => {
            let flag = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
            Ok(EnvelopePayload::MessageProperties(MessageActions {
                can_be_copied: flag("can_be_copied"),
                can_be_deleted_only_for_self: flag("can_be_deleted_only_for_self"),
                can_be_deleted_for_all_users: flag("can_be_deleted_for_all_users"),
                can_be_edited: flag("can_be_edited"),
                can_be_forwarded: flag("can_be_forwarded"),
                can_be_pinned: flag("can_be_pinned"),
                can_be_replied: flag("can_be_replied"),
                can_get_link: flag("can_get_link"),
                can_get_message_thread: flag("can_get_message_thread"),
                can_be_saved: flag("can_be_saved"),
                can_report_chat: flag("can_report_chat"),
                can_get_viewers: flag("can_get_viewers"),
                can_get_read_date: flag("can_get_read_date"),
                can_report_supergroup_spam: flag("can_report_supergroup_spam"),
                can_delete_reactions: flag("can_delete_reactions"),
                can_edit_scheduling_state: flag("can_edit_scheduling_state"),
            }))
        }
        // B4: `pollVoters` — the `getPollVoters` answer. Unparseable
        // senders are dropped; the list never misattributes a vote.
        "pollVoters" => Ok(EnvelopePayload::PollVoters {
            total_count: value
                .get("total_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            voters: value
                .get("voters")
                .and_then(Value::as_array)
                .map(|voters| {
                    voters
                        .iter()
                        .filter_map(|voter| parse_message_sender(voter.get("voter_id")).ok())
                        .collect()
                })
                .unwrap_or_default(),
        }),
        // Bots slice: `inlineQueryResults` — the `getInlineQueryResults`
        // answer (schema 1.8.67, line 7716). Missing fields degrade to
        // empty strings; thumbnails are dropped (no URL on the wire).
        // Slice S9: `inlineQueryResultAnimation` entries (schema line
        // 7658) additionally yield parsed `AnimationItem`s (+ files) for
        // the GIF panel search; Loop 3's summaries are untouched.
        "inlineQueryResults" => {
            let mut results = Vec::new();
            let mut animations = Vec::new();
            let mut files: Vec<ParsedFile> = Vec::new();
            if let Some(entries) = value.get("results").and_then(Value::as_array) {
                for entry in entries {
                    results.push(parse_inline_query_result(entry));
                    if entry.get("@type").and_then(Value::as_str)
                        == Some("inlineQueryResultAnimation")
                    {
                        let (item, item_files) = parse_animation_value(entry.get("animation"));
                        files.extend(item_files);
                        if let Some(item) = item {
                            animations.push(item);
                        }
                    }
                }
            }
            files.retain(|file| file.id.0 != 0);
            Ok(EnvelopePayload::InlineQueryResults(
                InlineQueryResultsPage {
                    inline_query_id: value
                        .get("inline_query_id")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    button: value
                        .get("button")
                        .filter(|button| !button.is_null())
                        .map(parse_inline_query_results_button),
                    results,
                    animations,
                    files,
                    next_offset: value
                        .get("next_offset")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                },
            ))
        }
        // Phase C3a: `text` (schema 1.8.67, line 10071) — the
        // `joinVideoChat` / `joinGroupCall` answer ("join response
        // payload for tgcalls"). Quill stores it, never consumes it
        // (no media transport until Phase C2).
        "text" => Ok(EnvelopePayload::Text {
            text: value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        // Phase C3a: `httpUrl` (schema 1.8.67, line 7458) — the
        // `getVideoChatInviteLink` answer.
        "httpUrl" => Ok(EnvelopePayload::HttpUrl {
            url: value
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        // Phase C2h: `rtmpUrl` (schema 1.8.67, line 7113) — the
        // `getVideoChatRtmpUrl` / `replaceVideoChatRtmpUrl` answer.
        "rtmpUrl" => Ok(EnvelopePayload::RtmpUrl {
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
        }),
        // Phase C2f: `inviteGroupCallParticipantResult*` (schema 1.8.67,
        // lines 7216-7227) — the `inviteGroupCallParticipant` answer.
        // Note: the success variant is
        // `inviteGroupCallParticipantResultSuccess`, not `...ResultOk`.
        "inviteGroupCallParticipantResultSuccess" => {
            Ok(EnvelopePayload::InviteGroupCallParticipantResult(
                InviteGroupCallParticipantResult::Success {
                    chat_id: value.get("chat_id").and_then(Value::as_i64).unwrap_or(0),
                    message_id: value.get("message_id").and_then(Value::as_i64).unwrap_or(0),
                },
            ))
        }
        "inviteGroupCallParticipantResultUserPrivacyRestricted" => {
            Ok(EnvelopePayload::InviteGroupCallParticipantResult(
                InviteGroupCallParticipantResult::UserPrivacyRestricted,
            ))
        }
        "inviteGroupCallParticipantResultUserAlreadyParticipant" => {
            Ok(EnvelopePayload::InviteGroupCallParticipantResult(
                InviteGroupCallParticipantResult::UserAlreadyParticipant,
            ))
        }
        "inviteGroupCallParticipantResultUserWasBanned" => {
            Ok(EnvelopePayload::InviteGroupCallParticipantResult(
                InviteGroupCallParticipantResult::UserWasBanned,
            ))
        }
        "callbackQueryAnswer" => Ok(EnvelopePayload::CallbackQueryAnswer(
            parse_callback_query_answer(&value),
        )),
        // Slice bots-games: `getGameHighScores` answer (TDLib 1.8.67,
        // `schema/td_api.tl:13174`).
        "gameHighScores" => Ok(EnvelopePayload::GameHighScores(parse_game_high_scores(
            &value,
        ))),
        // B1: `getLoginUrlInfo` answers (TDLib 1.8.67, `schema/td_api.tl:3862`
        // / `:3869`).
        "loginUrlInfoOpen" => Ok(EnvelopePayload::LoginUrlInfo(LoginUrlInfo::Open {
            url: json_field_str(&value, "url"),
        })),
        "loginUrlInfoRequestConfirmation" => Ok(EnvelopePayload::LoginUrlInfo(
            LoginUrlInfo::RequestConfirmation {
                domain: json_field_str(&value, "domain"),
                request_write_access: value
                    .get("request_write_access")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
        )),
        // Slice P1: payment answers (TDLib 1.8.67, `schema/td_api.tl:4734` /
        // `:4737` / `:4740` / `:4765`).
        "upgradedGift" => Ok(EnvelopePayload::MarketplaceGift(
            crate::marketplace::GiftQuote::parse(&value),
        )),
        "optionValueInteger" => Ok(EnvelopePayload::GiftTextLimit(int53_or_zero(
            value.get("value"),
        ))),
        "giftResaleResultOk" => Ok(EnvelopePayload::GiftPurchaseResult(
            crate::marketplace::GiftPurchaseResult::Sent(json_field_str(
                &value,
                "received_gift_id",
            )),
        )),
        "giftResaleResultPriceIncreased" => Ok(EnvelopePayload::GiftPurchaseResult(
            crate::marketplace::GiftPurchaseResult::PriceIncreased(
                value
                    .get("price")
                    .and_then(crate::marketplace::GiftPrice::parse),
            ),
        )),
        "paymentForm" => parse_payment_form(&value)
            .map(EnvelopePayload::PaymentForm)
            .ok_or(ParseError::MissingField),
        "validatedOrderInfo" => Ok(EnvelopePayload::ValidatedOrderInfo(
            ValidatedOrderInfoData {
                order_info_id: json_field_str(&value, "order_info_id"),
                shipping_options: value
                    .get("shipping_options")
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().map(parse_shipping_option).collect())
                    .unwrap_or_default(),
            },
        )),
        "paymentResult" => Ok(EnvelopePayload::PaymentResult(PaymentResultData {
            success: value
                .get("success")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            verification_url: json_field_str(&value, "verification_url"),
        })),
        "paymentReceipt" => parse_payment_receipt(&value)
            .map(EnvelopePayload::PaymentReceipt)
            .ok_or(ParseError::MissingField),
        // Slice `parity:bots-payment-recurring`: `getStarSubscriptions`
        // answer (TDLib 1.8.67, `schema/td_api.tl:1269`).
        "starSubscriptions" => parse_star_subscriptions(&value)
            .map(EnvelopePayload::StarSubscriptions)
            .ok_or(ParseError::MissingField),
        // Parity slice: `createChatFolder` / `editChatFolder` responses
        // (TDLib 1.8.67, `schema/td_api.tl:13358` / `:13361`).
        "chatFolderInfo" => parse_chat_folder_info(&value)
            .map(EnvelopePayload::ChatFolderInfo)
            .ok_or(ParseError::MissingField),
        // Parity slice: `getChatFolder` response (TDLib 1.8.67,
        // `schema/td_api.tl:13355`) — the full editable folder spec.
        "chatFolder" => parse_chat_folder(&value)
            .map(|spec| EnvelopePayload::ChatFolder { spec })
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
            Ok(EnvelopePayload::ChatLists { lists })
        }
        "error" => Ok(EnvelopePayload::Error(parse_error(Some(&value)))),
        "messages" => {
            let messages = value
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let parsed = messages
                .iter()
                .filter_map(|m| parse_message(m).ok())
                .collect();
            Ok(EnvelopePayload::Messages(parsed))
        }
        // `getMessageThread` answer (schema 1.8.67, line 3897).
        "messageThreadInfo" => Ok(EnvelopePayload::MessageThreadInfo(Box::new(
            parse_message_thread_info(&value)?,
        ))),
        "chats" => Ok(EnvelopePayload::Chats {
            total_count: value
                .get("total_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            chat_ids: value
                .get("chat_ids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| int53(Some(v)).ok())
                .map(ChatId)
                .collect(),
        }),
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
            Ok(EnvelopePayload::FoundMessages {
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                messages: parsed,
                next_offset: value
                    .get("next_offset")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        }
        // Phase C2i: `userPrivacySettingRules` (schema 1.8.67, :8976)
        // answers `getUserPrivacySettingRules` (:15620). Only the rule
        // constructor names are kept — enough to map Everybody /
        // Contacts / Nobody.
        "userPrivacySettingRules" => {
            let rules = value
                .get("rules")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(EnvelopePayload::UserPrivacySettingRules {
                rules: rules.iter().map(PrivacyRule::parse).collect(),
            })
        }
        // Slice S3: `updateUserPrivacySettingRules` (schema 1.8.67,
        // :10871) — rules changed on another device.
        "updateUserPrivacySettingRules" => {
            let rules = value
                .get("rules")
                .and_then(|v| v.get("rules"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(EnvelopePayload::UpdateUserPrivacySettingRules {
                setting: value
                    .get("setting")
                    .and_then(|v| v.get("@type"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                rules: rules.iter().map(PrivacyRule::parse).collect(),
            })
        }
        // Slice S3: `readDatePrivacySettings` (schema 1.8.67, :9026) —
        // the `getReadDatePrivacySettings` answer.
        "readDatePrivacySettings" => Ok(EnvelopePayload::ReadDatePrivacySettings {
            show_read_date: value
                .get("show_read_date")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        // Slice S3: `messageSenders` (schema 1.8.67, :14505) — the
        // `getBlockedMessageSenders` answer; only user senders kept.
        "messageSenders" => {
            let senders = value
                .get("senders")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(EnvelopePayload::BlockedMessageSenders {
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                sender_ids: senders
                    .iter()
                    .filter(|s| s.get("@type").and_then(Value::as_str) == Some("messageSenderUser"))
                    .filter_map(|s| s.get("user_id"))
                    .filter_map(Value::as_i64)
                    .collect(),
            })
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
                            let date = message.get("date").and_then(Value::as_i64)? as i32;
                            (message_id > 0).then(|| CalendarDay {
                                total_count: day
                                    .get("total_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    as i32,
                                message_id: MessageId(message_id),
                                date,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(EnvelopePayload::MessageCalendar {
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                days,
            })
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
            Ok(EnvelopePayload::FoundChatMessages {
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                messages: parsed,
                next_from_message_id: MessageId(int53_or_zero(value.get("next_from_message_id"))),
            })
        }
        // Phase 5.1: `updateSupergroup` (schema line 10738) and the
        // `getSupergroup` response both carry `supergroup.is_forum` (schema
        // line 2746). Parity slice: also keep the first active username
        // (`supergroup.usernames`, schema lines 2746/2372) for the
        // channel/supergroup header.
        "updateBasicGroup" => {
            let group = value.get("basic_group").ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateBasicGroup {
                basic_group_id: int53(group.get("id"))?,
                member_count: int53(group.get("member_count")).unwrap_or(0) as i32,
            })
        }
        "updateChatOnlineMemberCount" => Ok(EnvelopePayload::UpdateChatOnlineMemberCount {
            chat_id: int53(value.get("chat_id"))?,
            online_member_count: int53(value.get("online_member_count")).unwrap_or(0) as i32,
        }),
        "updateSupergroup" => {
            let supergroup = value.get("supergroup").ok_or(ParseError::MissingField)?;
            let verification_flag = |name: &str| {
                supergroup
                    .get("verification_status")
                    .and_then(|v| v.get(name))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            };
            Ok(EnvelopePayload::UpdateSupergroup {
                supergroup_id: int53(supergroup.get("id"))?,
                verification: crate::peer_badge::VerificationStatus {
                    is_verified: verification_flag("is_verified"),
                    is_scam: verification_flag("is_scam"),
                    is_fake: verification_flag("is_fake"),
                },
                member_count: int53(supergroup.get("member_count")).unwrap_or(0) as i32,
                is_forum: supergroup
                    .get("is_forum")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                has_forum_tabs: supergroup
                    .get("has_forum_tabs")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                username: parse_first_active_username(supergroup.get("usernames")),
                // Phase A1: own `chatMemberStatus*` (schema 1.8.67 line
                // 2746); unknown/missing → `Unknown` (gated, no bypass).
                // `can_restrict_members` gates the slow-mode admin control
                // (schema line 13551).
                status: parse_channel_member_status(supergroup.get("status"))
                    .map(|(status, _)| status)
                    .unwrap_or(ChannelMemberStatus::Unknown),
                can_restrict_members: parse_restrict_members_right(supergroup.get("status")),
                can_invite_users: parse_invite_users_right(supergroup.get("status")),
                can_promote_members: parse_promote_members_right(supergroup.get("status")),
                can_manage_tags: parse_manage_tags_right(supergroup.get("status")),
                // Slice G2: forum-topic / sign-messages / welcome-message
                // rights (schema 1.8.67, lines 1090/1092).
                can_manage_topics: parse_manage_topics_right(supergroup.get("status")),
                can_change_info: parse_change_info_right(supergroup.get("status")),
                can_send_welcome_messages: parse_send_welcome_messages_right(
                    supergroup.get("status"),
                ),
                // Slice G1: `supergroup.join_by_request` /
                // `supergroup.is_broadcast_group` (schema 1.8.67, lines
                // 2733/2736/2746).
                join_by_request: supergroup
                    .get("join_by_request")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                is_broadcast_group: supergroup
                    .get("is_broadcast_group")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                // Slice G2: `supergroup.sign_messages` /
                // `supergroup.show_message_sender` (schema 1.8.67, lines
                // 2731/2746).
                sign_messages: supergroup
                    .get("sign_messages")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                show_message_sender: supergroup
                    .get("show_message_sender")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        }
        "supergroup" => Ok(EnvelopePayload::Supergroup {
            supergroup_id: int53(value.get("id"))?,
            is_forum: value
                .get("is_forum")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            has_forum_tabs: value
                .get("has_forum_tabs")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            username: parse_first_active_username(value.get("usernames")),
            // Phase A1: own `chatMemberStatus*` (schema 1.8.67 line 2746).
            status: parse_channel_member_status(value.get("status"))
                .map(|(status, _)| status)
                .unwrap_or(ChannelMemberStatus::Unknown),
            can_restrict_members: parse_restrict_members_right(value.get("status")),
            can_invite_users: parse_invite_users_right(value.get("status")),
            can_promote_members: parse_promote_members_right(value.get("status")),
            can_manage_tags: parse_manage_tags_right(value.get("status")),
            // Slice G2: forum-topic / sign-messages / welcome-message
            // rights (schema 1.8.67, lines 1090/1092).
            can_manage_topics: parse_manage_topics_right(value.get("status")),
            can_change_info: parse_change_info_right(value.get("status")),
            can_send_welcome_messages: parse_send_welcome_messages_right(value.get("status")),
            // Slice G1: `supergroup.join_by_request` /
            // `supergroup.is_broadcast_group` (schema 1.8.67, lines
            // 2733/2736/2746).
            join_by_request: value
                .get("join_by_request")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_broadcast_group: value
                .get("is_broadcast_group")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            // Slice G2: `supergroup.sign_messages` /
            // `supergroup.show_message_sender` (schema 1.8.67, lines
            // 2731/2746).
            sign_messages: value
                .get("sign_messages")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            show_message_sender: value
                .get("show_message_sender")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        // Phase 5.1: `forumTopics` (schema line 3976). Topics keep their
        // response order; the UI sorts by `order` descending per the schema
        // ("Topics must be sorted by the order in descending order").
        "forumTopics" => {
            let topics = value
                .get("topics")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let parsed = topics.iter().filter_map(parse_forum_topic).collect();
            Ok(EnvelopePayload::ForumTopics {
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                topics: parsed,
            })
        }
        "message" => Ok(EnvelopePayload::Message(parse_message(&value)?)),
        // Slice G2: `forumTopicInfo` — the `createForumTopic` answer
        // (schema 1.8.67, line 12665). Only the chat id is kept; the
        // topic list is refetched on success.
        // Subsection tabs: live topic changes (schema 1.8.67, lines
        // 10652 / 10665) — new topics, renames, pins, reads, mutes.
        "updateForumTopicInfo" => value
            .get("info")
            .and_then(parse_forum_topic_info)
            .map(EnvelopePayload::UpdateForumTopicInfo)
            .ok_or(ParseError::MissingField),
        "forumTopic" => parse_forum_topic(&value)
            .map(EnvelopePayload::ForumTopicAnswer)
            .ok_or(ParseError::MissingField),
        "updateForumTopic" => parse_forum_topic_update(&value)
            .map(EnvelopePayload::UpdateForumTopic)
            .ok_or(ParseError::MissingField),
        "forumTopicInfo" => Ok(EnvelopePayload::ForumTopic {
            chat_id: value.get("chat_id").and_then(Value::as_i64).unwrap_or(0),
        }),
        "updateFile" => Ok(EnvelopePayload::UpdateFile(parse_file(value.get("file"))?)),
        // Slice media-downloads-pause: `updateFileDownload` (schema 1.8.67,
        // line 10795) — pause state / completion for a listed download.
        "updateFileDownload" => Ok(EnvelopePayload::UpdateFileDownload {
            file_id: int53_or_zero(value.get("file_id")) as i32,
            is_paused: value
                .get("is_paused")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            complete_date: int53_or_zero(value.get("complete_date")) as i32,
        }),
        "file" => Ok(EnvelopePayload::File(parse_file(Some(&value))?)),
        "stickerSets" => Ok(parse_sticker_sets(&value)),
        "stickerSet" => Ok(parse_sticker_set(&value)),
        "updateStickerSet" => Ok(EnvelopePayload::UpdateStickerSet {
            id: int64(value.get("sticker_set").and_then(|set| set.get("id"))).unwrap_or(0),
            is_custom_emoji: value
                .pointer("/sticker_set/sticker_type/@type")
                .and_then(Value::as_str)
                == Some("stickerTypeCustomEmoji"),
        }),
        "trendingStickerSets" => Ok(parse_trending_sticker_sets(&value)),
        "stickers" => Ok(parse_stickers(&value)),
        // Slice S10: emoji backend payloads (parsers live in envelope_emoji).
        "emojiStatuses" => Ok(crate::telegram::envelope_emoji::parse_emoji_statuses(
            &value,
        )),
        "emojiStatusCustomEmojis" => {
            Ok(crate::telegram::envelope_emoji::parse_emoji_status_custom_emojis(&value))
        }
        "animatedEmoji" => Ok(crate::telegram::envelope_emoji::parse_animated_emoji(
            &value,
        )),
        "emojiKeywords" => Ok(crate::telegram::envelope_emoji::parse_emoji_keywords(
            &value,
        )),
        "emojiCategories" => Ok(crate::telegram::envelope_emoji::parse_emoji_categories(
            &value,
        )),
        "animations" => Ok(parse_animations(&value)),
        "sponsoredMessages" => Ok(parse_sponsored_messages(&value)?),
        "reportSponsoredResultOk" => Ok(EnvelopePayload::ReportSponsoredResult(
            ReportSponsoredResult::Ok,
        )),
        "reportSponsoredResultFailed" => Ok(EnvelopePayload::ReportSponsoredResult(
            ReportSponsoredResult::Failed,
        )),
        "reportSponsoredResultOptionRequired" => Ok(EnvelopePayload::ReportSponsoredResult(
            ReportSponsoredResult::OptionRequired {
                title: json_field_str(&value, "title"),
                options: parse_report_options(value.get("options")),
            },
        )),
        "reportSponsoredResultAdsHidden" => Ok(EnvelopePayload::ReportSponsoredResult(
            ReportSponsoredResult::AdsHidden,
        )),
        "reportSponsoredResultPremiumRequired" => Ok(EnvelopePayload::ReportSponsoredResult(
            ReportSponsoredResult::PremiumRequired,
        )),
        "reportStoryResultOk" => Ok(EnvelopePayload::ReportStoryResult(ReportStoryResult::Ok)),
        "reportStoryResultOptionRequired" => Ok(EnvelopePayload::ReportStoryResult(
            ReportStoryResult::OptionRequired {
                title: json_field_str(&value, "title"),
                options: parse_report_options(value.get("options")),
            },
        )),
        "reportStoryResultTextRequired" => Ok(EnvelopePayload::ReportStoryResult(
            ReportStoryResult::TextRequired {
                option_id: json_field_str(&value, "option_id"),
                is_optional: value
                    .get("is_optional")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
        )),
        "storyInteractions" => Ok(EnvelopePayload::StoryInteractions {
            interactions: parse_story_interactions(&value),
        }),
        "updateStoryStealthMode" => Ok(EnvelopePayload::UpdateStoryStealthMode {
            active_until_date: value
                .get("active_until_date")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            cooldown_until_date: value
                .get("cooldown_until_date")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        }),
        "updateSavedAnimations" => Ok(EnvelopePayload::UpdateSavedAnimations {
            animation_ids: value
                .get("animation_ids")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| id.as_i64())
                        .map(|id| id as i32)
                        .filter(|id| *id != 0)
                        .collect()
                })
                .unwrap_or_default(),
        }),
        // Slice S9: `updateAnimationSearchParameters` (schema 1.8.67,
        // line 11064) — server-pushed; provider is the upstream search
        // provider name, emojis the new suggested search emojis.
        "updateAnimationSearchParameters" => Ok(EnvelopePayload::UpdateAnimationSearchParameters {
            provider: value
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            emojis: value
                .get("emojis")
                .and_then(Value::as_array)
                .map(|emojis| {
                    emojis
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        }),
        // Slice S15: `updateInstalledStickerSets` (schema 1.8.67, line
        // 10932). Ids arrive as int64s (string or number); a missing or
        // malformed list parses to empty, which the reducer treats as a
        // no-op.
        "updateInstalledStickerSets" => Ok(EnvelopePayload::UpdateInstalledStickerSets {
            sticker_set_ids: value
                .get("sticker_set_ids")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(|id| int64(Some(id))).collect())
                .unwrap_or_default(),
            is_regular: value
                .get("sticker_type")
                .and_then(|t| t.get("@type"))
                .and_then(Value::as_str)
                == Some("stickerTypeRegular"),
        }),
        "notificationSounds" => {
            let sounds = value
                .get("notification_sounds")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(parse_notification_sound).collect())
                .unwrap_or_default();
            Ok(EnvelopePayload::NotificationSounds { sounds })
        }
        // Phase S2: `storageStatistics` — aggregate `by_chat[].by_file_type[]`
        // into per-`fileType` totals (TGX `TGStorageStats` aggregates the
        // same way; schema 1.8.67 lines 9780/9787/9793). Zero-size entries
        // are kept: the UI orders by a fixed category list, not by size.
        // Slice S4: per-chat rows are also kept verbatim (the usage
        // screen's per-chat breakdown; present when `chat_limit` > 0).
        "storageStatistics" => {
            let total_size = int53_or_zero(value.get("size"));
            let mut totals: Vec<StorageFileTypeStats> = Vec::new();
            let mut by_chat: Vec<StorageChatStats> = Vec::new();
            for chat in value
                .get("by_chat")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let chat_id = chat.get("chat_id").and_then(Value::as_i64).unwrap_or(0);
                let chat_size = int53_or_zero(chat.get("size"));
                let chat_count = chat
                    .get("count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .clamp(0, i32::MAX as i64) as i32;
                by_chat.push(StorageChatStats {
                    chat_id,
                    size: chat_size,
                    count: chat_count,
                });
                for entry in chat
                    .get("by_file_type")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let Some(file_type) = entry
                        .get("file_type")
                        .and_then(|t| t.get("@type"))
                        .and_then(Value::as_str)
                    else {
                        continue;
                    };
                    let size = int53_or_zero(entry.get("size"));
                    let count = entry
                        .get("count")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                        .clamp(0, i32::MAX as i64) as i32;
                    match totals.iter_mut().find(|t| t.file_type == file_type) {
                        Some(existing) => {
                            existing.size = existing.size.saturating_add(size);
                            existing.count = existing.count.saturating_add(count);
                        }
                        None => totals.push(StorageFileTypeStats {
                            file_type: file_type.to_string(),
                            size,
                            count,
                        }),
                    }
                }
            }
            Ok(EnvelopePayload::StorageStatistics {
                total_size,
                by_file_type: totals,
                by_chat,
            })
        }
        // Slice S4: `autoDownloadSettingsPresets` — the
        // `getAutoDownloadSettingsPresets` response (schema 1.8.67, line
        // 9862). Missing low/medium/high is a malformed answer, not a
        // default — the reducer never sees it.
        "autoDownloadSettingsPresets" => {
            let Some((low, medium, high)) = AutoDownloadNetSettings::parse_presets(&value) else {
                return Err(ParseError::MissingField);
            };
            Ok(EnvelopePayload::AutoDownloadSettingsPresets { low, medium, high })
        }
        // Slice A2: `passwordState` — the `getPasswordState` /
        // `setPassword` / `setRecoveryEmailAddress` /
        // `resendRecoveryEmailAddressCode` /
        // `cancelRecoveryEmailAddressVerification` response (schema
        // 1.8.67, line 273). `recovery_email_address_code_info` is null
        // unless a recovery-email confirmation is pending (schema line
        // 83); a non-object there is treated as absent, never an error.
        "passwordState" => {
            let bool_field = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
            let str_field = |name: &str| {
                value
                    .get(name)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };
            let (pending_email_pattern, pending_email_code_length) = value
                .get("recovery_email_address_code_info")
                .and_then(Value::as_object)
                .map(|info| {
                    (
                        info.get("email_address_pattern")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        info.get("length")
                            .and_then(Value::as_i64)
                            .unwrap_or(0)
                            .clamp(0, i32::MAX as i64) as i32,
                    )
                })
                .unwrap_or((None, 0));
            Ok(EnvelopePayload::PasswordState {
                state: PasswordState {
                    has_password: bool_field("has_password"),
                    password_hint: str_field("password_hint"),
                    has_recovery_email_address: bool_field("has_recovery_email_address"),
                    has_passport_data: bool_field("has_passport_data"),
                    pending_email_pattern,
                    pending_email_code_length,
                    login_email_address_pattern: str_field("login_email_address_pattern"),
                    pending_reset_date: value
                        .get("pending_reset_date")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                        .clamp(0, i32::MAX as i64) as i32,
                },
            })
        }
        // Slice A8: `authenticationCodeInfo` — the
        // `sendPhoneNumberCode` / `resendPhoneNumberCode` answer (schema
        // 1.8.67, line 78). A missing `phone_number` / `timeout` degrades
        // to empty / 0 rather than failing the parse; the reducer only
        // trusts it when it answers our own in-flight request.
        "authenticationCodeInfo" => {
            let phone_number = value
                .get("phone_number")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let timeout = value
                .get("timeout")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .clamp(0, i32::MAX as i64) as i32;
            Ok(EnvelopePayload::AuthenticationCodeInfo {
                phone_number,
                timeout,
            })
        }
        // Slice A7: `accountTtl` — the `getAccountTtl` answer (schema
        // 1.8.67, line 9053). A missing/invalid `days` degrades to 0
        // rather than failing the parse; the authoritative refetch
        // decides.
        "accountTtl" => {
            let days = value
                .get("days")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .clamp(0, i32::MAX as i64) as i32;
            Ok(EnvelopePayload::AccountTtl { days })
        }
        // Slice A3: `sessions` — the `getActiveSessions` answer (schema
        // 1.8.67, lines 9144/9147). Malformed entries are dropped rather
        // than failing the whole list (a session id is required).
        "session" => Ok(EnvelopePayload::DeviceLoginResult {
            result: parse_session(&value)
                .map(|session| {
                    if session.is_password_pending {
                        crate::auth::DeviceLoginResult::PasswordRequired
                    } else {
                        crate::auth::DeviceLoginResult::Linked
                    }
                })
                .unwrap_or(crate::auth::DeviceLoginResult::Failed),
        }),
        "sessions" => {
            let sessions = value
                .get("sessions")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(parse_session).collect())
                .unwrap_or_default();
            Ok(EnvelopePayload::Sessions { sessions })
        }
        // Slice A4: `connectedWebsites` — the `getConnectedWebsites`
        // answer (schema 1.8.67, lines 9171/15124). Unparseable websites
        // are skipped rather than failing the whole list (a website id
        // is required).
        "connectedWebsites" => {
            let websites = value
                .get("websites")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(parse_website).collect())
                .unwrap_or_default();
            Ok(EnvelopePayload::ConnectedWebsites { websites })
        }
        "updateSavedNotificationSounds" => Ok(EnvelopePayload::UpdateSavedNotificationSounds {
            sound_ids: value
                .get("notification_sound_ids")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| id.as_i64().or_else(|| id.as_str()?.parse().ok()))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "scopeNotificationSettings" => {
            // The response to `getScopeNotificationSettings` carries no scope
            // field — the scope is correlated via the pending request.
            Ok(EnvelopePayload::ScopeNotificationSettings {
                scope: NotificationSettingsScope::PrivateChats,
                settings: parse_scope_notification_settings(Some(&value)),
            })
        }
        "updateScopeNotificationSettings" => {
            let scope = value
                .get("scope")
                .and_then(|s| s.get("@type"))
                .and_then(Value::as_str);
            match parse_notification_settings_scope(scope) {
                Some(scope) => Ok(EnvelopePayload::UpdateScopeNotificationSettings {
                    scope,
                    settings: parse_scope_notification_settings(value.get("notification_settings")),
                }),
                None => Err(ParseError::MissingField),
            }
        }
        "updateReactionNotificationSettings" => {
            Ok(EnvelopePayload::UpdateReactionNotificationSettings {
                settings: parse_reaction_notification_settings(value.get("notification_settings")),
            })
        }
        // Slice CL2: `createPrivateChat` answer — a bare `chat`
        // object (schema 1.8.67, line 13312). Parsed exactly like
        // `updateNewChat`'s inner chat so the reducer inserts it into
        // the model; the driver opens it through the normal
        // `select_chat` flow when the `@extra` matches our
        // `CreatePrivateChat` request.
        "chat" => parse_new_chat(&value),
        // Slice CL2: `archiveChatListSettings` — the
        // `getArchiveChatListSettings` answer (schema 1.8.67, line
        // 3512). Missing fields default to false (never fail the
        // parse — a partial answer still beats no settings).
        "archiveChatListSettings" => Ok(EnvelopePayload::ArchiveChatListSettings {
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
        }),
        "updateMessageInteractionInfo" => Ok(EnvelopePayload::UpdateMessageInteractionInfo {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            message_id: MessageId(int53(value.get("message_id"))?),
            interaction_info: parse_interaction_info(value.get("interaction_info")),
        }),
        "updateChatMember" => {
            let member = value
                .get("new_chat_member")
                .and_then(|m| parse_chat_member(Some(m)))
                .ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateChatMember {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                member,
            })
        }
        "chatMember" => {
            let member = parse_chat_member(Some(&value)).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::ChatMember { member })
        }
        "user" => Ok(EnvelopePayload::Me {
            user_id: int53(value.get("id"))?,
        }),
        "userFullInfo" => Ok(EnvelopePayload::UserFullInfo {
            extras: super::envelope_types::parse_user_profile_extras(Some(&value)),
            bot_info: parse_bot_info(value.get("bot_info")),
            bio: parse_formatted_text(value.get("bio")),
            photo: parse_user_full_info_photo(&value),
            photo_id: int53(value.get("photo").and_then(|p| p.get("id"))).ok(),
            blocked: is_block_list_main(value.get("block_list")),
        }),
        "updateUserFullInfo" => Ok(EnvelopePayload::UpdateUserFullInfo {
            user_id: UserId(int53(value.get("user_id"))?),
            extras: super::envelope_types::parse_user_profile_extras(value.get("user_full_info")),
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
        }),
        "supergroupFullInfo" => Ok(EnvelopePayload::SupergroupFullInfo {
            description: value
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            member_count: value
                .get("member_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            // Parity slice: `linked_chat_id` (schema 1.8.67, line 2792) —
            // the discussion-group chat id (0 = none).
            linked_chat_id: int53_or_zero(value.get("linked_chat_id")),
            // Phase A1: slow-mode fields (schema 1.8.67, lines 2758–2759)
            // plus the boost bypass counts (lines 2779–2780). The expiry is
            // `double` in the schema; `as_f64` accepts integer JSON too.
            slow_mode_delay: value
                .get("slow_mode_delay")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            slow_mode_delay_expires_in: value
                .get("slow_mode_delay_expires_in")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            my_boost_count: value
                .get("my_boost_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            unrestrict_boost_count: value
                .get("unrestrict_boost_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            // Phase D2: `can_get_statistics` (schema 1.8.67, line 2792) —
            // gates the statistics entry point in the info panel.
            can_get_statistics: value
                .get("can_get_statistics")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            // Slice G2: anti-spam fields (schema 1.8.67, line 2792).
            has_aggressive_anti_spam_enabled: value
                .get("has_aggressive_anti_spam_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            can_toggle_aggressive_anti_spam: value
                .get("can_toggle_aggressive_anti_spam")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            // Slice S11: sticker-set fields (schema 1.8.67, lines 2765 and
            // 2792); int64 ids arrive as JSON strings.
            can_set_sticker_set: value
                .get("can_set_sticker_set")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            sticker_set_id: int64(value.get("sticker_set_id")).unwrap_or(0),
            custom_emoji_sticker_set_id: int64(value.get("custom_emoji_sticker_set_id"))
                .unwrap_or(0),
        }),
        // Parity slice: `updateSupergroupFullInfo` (schema 1.8.67, line
        // 10750) — same fields as the `supergroupFullInfo` response, with
        // an explicit `supergroup_id` so no pending-request correlation
        // is needed.
        "updateSupergroupFullInfo" => Ok(EnvelopePayload::UpdateSupergroupFullInfo {
            supergroup_id: int53(value.get("supergroup_id"))?,
            description: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("description"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            member_count: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("member_count"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            linked_chat_id: int53_or_zero(
                value
                    .get("supergroup_full_info")
                    .and_then(|info| info.get("linked_chat_id")),
            ),
            // Phase A1: slow-mode + boost fields (schema 1.8.67,
            // lines 2758–2759 / 2779–2780), nested like the other fields.
            slow_mode_delay: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("slow_mode_delay"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            slow_mode_delay_expires_in: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("slow_mode_delay_expires_in"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            my_boost_count: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("my_boost_count"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            unrestrict_boost_count: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("unrestrict_boost_count"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            // Phase D2: `can_get_statistics` (schema 1.8.67, line 2792),
            // nested like the other fields.
            can_get_statistics: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("can_get_statistics"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            // Slice G2: anti-spam fields (schema 1.8.67, line 2792),
            // nested like the other fields.
            has_aggressive_anti_spam_enabled: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("has_aggressive_anti_spam_enabled"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            can_toggle_aggressive_anti_spam: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("can_toggle_aggressive_anti_spam"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            // Slice S11: sticker-set fields (schema 1.8.67, lines 2765 and
            // 2792), nested like the other fields.
            can_set_sticker_set: value
                .get("supergroup_full_info")
                .and_then(|info| info.get("can_set_sticker_set"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            sticker_set_id: int64(
                value
                    .get("supergroup_full_info")
                    .and_then(|info| info.get("sticker_set_id")),
            )
            .unwrap_or(0),
            custom_emoji_sticker_set_id: int64(
                value
                    .get("supergroup_full_info")
                    .and_then(|info| info.get("custom_emoji_sticker_set_id")),
            )
            .unwrap_or(0),
        }),
        // Slice (communities backend core): `updateCommunity` (schema
        // 1.8.67, line 10726) — the update carries the full `community`
        // object; guaranteed to come before the community identifier is
        // returned, so no pending-request correlation is needed.
        "updateCommunity" => Ok(EnvelopePayload::UpdateCommunity {
            community: value
                .get("community")
                .and_then(parse_community)
                .ok_or(ParseError::MissingField)?,
        }),
        // Slice (communities backend core): `updateCommunityFullInfo`
        // (schema 1.8.67, line 10753) — the arrival path for
        // `loadCommunityFullInfo` (line 11799 answers `ok`; the data is
        // sent through update). Carries its own `community_id`.
        "updateCommunityFullInfo" => Ok(EnvelopePayload::UpdateCommunityFullInfo {
            community_id: int53(value.get("community_id"))?,
            full_info: value
                .get("community_full_info")
                .and_then(parse_community_full_info)
                .ok_or(ParseError::MissingField)?,
        }),
        // Slice (communities backend core): `communityId` (schema 1.8.67,
        // line 2264) — the `createCommunity` response (line 11806).
        "communityId" => Ok(EnvelopePayload::CommunityId {
            id: int53(value.get("id"))?,
        }),
        // Phase D2: `getChatStatistics` response (schema 1.8.67, line
        // 15760) — `chatStatisticsChannel` / `chatStatisticsSupergroup`.
        // The response carries no chat id; `Session::apply` correlates it
        // via the pending `GetChatStatistics` request.
        "chatStatisticsChannel" | "chatStatisticsSupergroup" => {
            Ok(EnvelopePayload::ChatStatistics {
                statistics: parse_chat_statistics(&value)?,
            })
        }
        // Slice G2: welcome-message updates (schema 1.8.67, lines
        // 10599/10649) and boost responses (lines 6943/6968).
        "updateChatWelcomeMessages" => Ok(EnvelopePayload::UpdateChatWelcomeMessages {
            chat_id: int53(value.get("chat_id"))?,
            messages: value
                .get("messages")
                .and_then(Value::as_array)
                .map(|messages| messages.iter().filter_map(parse_welcome_message).collect())
                .unwrap_or_default(),
        }),
        "updateChatHasWelcomeMessages" => Ok(EnvelopePayload::UpdateChatHasWelcomeMessages {
            chat_id: int53(value.get("chat_id"))?,
            has_welcome_messages: value
                .get("has_welcome_messages")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "updateChatHasProtectedContent" => Ok(EnvelopePayload::UpdateChatHasProtectedContent {
            chat_id: int53(value.get("chat_id"))?,
            has_protected_content: value
                .get("has_protected_content")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "chatBoostStatus" => Ok(EnvelopePayload::ChatBoostStatus {
            level: value.get("level").and_then(Value::as_i64).unwrap_or(0) as i32,
            boost_count: value
                .get("boost_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        }),
        "chatBoostSlots" => Ok(EnvelopePayload::ChatBoostSlots {
            slots: value
                .get("slots")
                .and_then(Value::as_array)
                .map(|slots| {
                    slots
                        .iter()
                        .filter_map(|slot| slot.get("slot_id").and_then(Value::as_i64))
                        .map(|id| id as i32)
                        .collect()
                })
                .unwrap_or_default(),
        }),
        // Phase D3a: invite-link / join-request responses and updates
        // (schema 1.8.67, lines 2627/2630/2688/2691/10555/11210). The
        // responses carry no chat id; `Session::apply` correlates them via
        // the pending request. The updates carry their own `chat_id`.
        // Phase D3b: admin-list / member-list responses (schema 1.8.67,
        // lines 2485/2529) — same correlation, no chat id on the wire.
        "chatAdministrators" => Ok(EnvelopePayload::ChatAdministrators {
            administrators: value
                .get("administrators")
                .and_then(Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|entry| parse_chat_administrator(Some(entry)))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        // Phase D3c: `chatEvents` (schema 1.8.67, line 7938) — the
        // `getChatEventLog` response. Events whose actor fails to parse
        // are dropped in `parse_chat_event`, never misattributed.
        "chatEvents" => Ok(EnvelopePayload::ChatEvents {
            events: value
                .get("events")
                .and_then(Value::as_array)
                .map(|events| events.iter().filter_map(parse_chat_event).collect())
                .unwrap_or_default(),
        }),
        "chatMembers" => Ok(EnvelopePayload::SupergroupMembers {
            total_count: int53(value.get("total_count")).map(|v| v as i32)?,
            members: value
                .get("members")
                .and_then(Value::as_array)
                .map(|members| {
                    members
                        .iter()
                        .filter_map(|member| parse_chat_member(Some(member)))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "chatInviteLink" => Ok(EnvelopePayload::ChatInviteLink {
            link: parse_chat_invite_link(Some(&value)).ok_or(ParseError::MissingField)?,
        }),
        // Checked invite preview; joining requires a separate confirmation.
        "chatInviteLinkInfo" => Ok(EnvelopePayload::ChatInviteLinkInfo {
            title: json_field_str(&value, "title"),
            member_count: int53(value.get("member_count")).unwrap_or(0) as i32,
            creates_join_request: value
                .get("creates_join_request")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_channel: value
                .get("type")
                .and_then(|t| t.get("@type"))
                .and_then(Value::as_str)
                == Some("inviteLinkChatTypeChannel"),
        }),
        // `getDeepLinkInfo` answer (schema 1.8.67, line 10087).
        "deepLinkInfo" => {
            let text = parse_formatted_text(value.get("text"));
            Ok(EnvelopePayload::DeepLinkInfo {
                entities: parse_text_entities(&text, value.get("text")),
                text,
                need_update: value
                    .get("need_update_application")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        }
        "chatInviteLinks" => Ok(EnvelopePayload::ChatInviteLinks {
            total_count: int53(value.get("total_count")).map(|v| v as i32)?,
            links: value
                .get("invite_links")
                .and_then(Value::as_array)
                .map(|links| {
                    links
                        .iter()
                        .filter_map(|link| parse_chat_invite_link(Some(link)))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "chatJoinRequests" => Ok(EnvelopePayload::ChatJoinRequests {
            total_count: int53(value.get("total_count")).map(|v| v as i32)?,
            requests: value
                .get("requests")
                .and_then(Value::as_array)
                .map(|requests| {
                    requests
                        .iter()
                        .filter_map(|request| parse_chat_join_request(Some(request)))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        // Slice G1: `createdBasicGroupChat` (schema 1.8.67, line 3644).
        "createdBasicGroupChat" => Ok(EnvelopePayload::CreatedBasicGroupChat {
            chat_id: int53(value.get("chat_id"))?,
        }),
        // Slice G1: `failedToAddMembers` (schema 1.8.67, line 3640).
        "failedToAddMembers" => Ok(EnvelopePayload::FailedToAddMembers {
            failed_count: value
                .get("failed_to_add_members")
                .and_then(Value::as_array)
                .map(|members| members.len() as i32)
                .unwrap_or(0),
        }),
        // Slice G1: `basicGroupFullInfo` (schema 1.8.67, line 2714).
        "basicGroupFullInfo" => Ok(EnvelopePayload::BasicGroupFullInfo {
            members: value
                .get("members")
                .and_then(Value::as_array)
                .map(|members| {
                    members
                        .iter()
                        .filter_map(|member| parse_chat_member(Some(member)))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "updateNewChatJoinRequest" => Ok(EnvelopePayload::UpdateNewChatJoinRequest {
            chat_id: int53(value.get("chat_id"))?,
            request: parse_chat_join_request(value.get("request"))
                .ok_or(ParseError::MissingField)?,
            user_chat_id: int53(value.get("user_chat_id"))?,
            invite_link: parse_chat_invite_link(value.get("invite_link"))
                .ok_or(ParseError::MissingField)?,
            query_id: int53(value.get("query_id"))?,
        }),
        "updateChatPendingJoinRequests" => {
            let pending = value
                .get("pending_join_requests")
                .filter(|v| v.get("@type").and_then(Value::as_str) == Some("chatJoinRequestsInfo"));
            Ok(EnvelopePayload::UpdateChatPendingJoinRequests {
                chat_id: int53(value.get("chat_id"))?,
                total_count: int53(pending.and_then(|v| v.get("total_count"))).map(|v| v as i32)?,
                user_ids: pending
                    .and_then(|v| v.get("user_ids"))
                    .and_then(Value::as_array)
                    .map(|ids| ids.iter().filter_map(|id| int53(Some(id)).ok()).collect())
                    .unwrap_or_default(),
            })
        }
        "botCommands" => Ok(EnvelopePayload::BotCommands {
            bot_user_id: UserId(int53(value.get("bot_user_id"))?),
            commands: value
                .get("commands")
                .and_then(Value::as_array)
                .map(|commands| {
                    commands
                        .iter()
                        .filter_map(|command| parse_bot_command(Some(command)))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "chatJoinResultSuccess" => Ok(EnvelopePayload::JoinChatResult(ChatJoinResult::Success {
            chat_id: ChatId(int53(value.get("chat_id"))?),
        })),
        "chatJoinResultRequestSent" => {
            Ok(EnvelopePayload::JoinChatResult(ChatJoinResult::RequestSent))
        }
        "chatJoinResultGuardBotApprovalRequired" => Ok(EnvelopePayload::JoinChatResult(
            ChatJoinResult::GuardBotApprovalRequired,
        )),
        "chatJoinResultDeclined" => Ok(EnvelopePayload::JoinChatResult(ChatJoinResult::Declined)),
        "updateMessageIsPinned" => Ok(EnvelopePayload::UpdateMessageIsPinned {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            message_id: MessageId(int53(value.get("message_id"))?),
            is_pinned: value
                .get("is_pinned")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        other => {
            // Phase D2: an unknown future `ChatStatistics` constructor must
            // fail parsing rather than silently becoming `Unknown` and
            // dropping the statistics response. All other unknown types
            // keep the existing `Unknown` convention.
            if other.starts_with("chatStatistics") {
                return Err(ParseError::MissingField);
            }
            Ok(EnvelopePayload::Unknown(UnknownKind {
                type_name: other.to_string(),
            }))
        }
    }
}

/// A non-negative `int32` unread total from an update object.
fn unread_total(value: &Value, key: &str) -> i32 {
    value
        .get(key)
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .clamp(0, i64::from(i32::MAX)) as i32
}
