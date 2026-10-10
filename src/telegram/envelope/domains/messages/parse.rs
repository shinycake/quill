//! Parses TDLib objects for history, sending, editing, reactions, polls, translation and message menus.
use crate::ids::{ChatId, MessageId};
use crate::rich::parse_rich_message;
use crate::telegram::envelope::*;
use serde_json::Value;

/// The messages domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_messages_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        "updatePendingMessage" => {
            let (content, files) = parse_content(value.get("content"));
            Ok(EnvelopePayload::Messages(
                MessagesPayload::UpdatePendingMessage {
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
                },
            ))
        }
        "updateStopMessageDraft" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateStopMessageDraft {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                forum_topic_id: i32::try_from(int53(value.get("forum_topic_id"))?)
                    .map_err(|_| ParseError::BadInt)?,
                draft_id: int53(value.get("draft_id"))?,
            },
        )),
        "updateNewMessage" => {
            let message = value.get("message").ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Messages(
                MessagesPayload::UpdateNewMessage(parse_message(message)?),
            ))
        }
        "updateMessageSendSucceeded" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateMessageSendSucceeded {
                message: parse_message(value.get("message").ok_or(ParseError::MissingField)?)?,
                old_message_id: MessageId(int53(value.get("old_message_id"))?),
            },
        )),
        "updateMessageSendFailed" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateMessageSendFailed {
                message: parse_message(value.get("message").ok_or(ParseError::MissingField)?)?,
                old_message_id: MessageId(int53(value.get("old_message_id"))?),
                error: parse_error(value.get("error")),
            },
        )),
        "updateMessageSendAcknowledged" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateMessageSendAcknowledged {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                message_id: MessageId(int53(value.get("message_id"))?),
            },
        )),
        "updateDeleteMessages" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateDeleteMessages {
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
            },
        )),
        "updateMessageContent" => {
            let (content, files) = parse_content(value.get("new_content"));
            Ok(EnvelopePayload::Messages(
                MessagesPayload::UpdateMessageContent {
                    chat_id: ChatId(int53(value.get("chat_id"))?),
                    message_id: MessageId(int53(value.get("message_id"))?),
                    content,
                    files,
                },
            ))
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
            Ok(EnvelopePayload::Messages(
                MessagesPayload::UpdateMessageEphemeralContent {
                    chat_id: ChatId(int53(value.get("chat_id"))?),
                    message_id: MessageId(int53(value.get("message_id"))?),
                    ephemeral,
                },
            ))
        }
        "updateMessageContentOpened" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateMessageContentOpened {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                message_id: MessageId(int53(value.get("message_id"))?),
            },
        )),
        "updateMessageEdited" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateMessageEdited {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                message_id: MessageId(int53(value.get("message_id"))?),
                edit_date: value
                    .get("edit_date")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                reply_markup: parse_reply_markup(value.get("reply_markup")),
            },
        )),
        "updatePoll" => Ok(EnvelopePayload::Messages(MessagesPayload::UpdatePoll {
            poll: parse_poll(value.get("poll")).ok_or(ParseError::MissingField)?,
        })),
        // B15: `pollVoteStatistics` (schema 1.8.67, line 10263).
        "pollVoteStatistics" => Ok(EnvelopePayload::Messages(
            MessagesPayload::PollVoteStatistics {
                graph: StatisticalGraph::parse(
                    value.get("vote_graph").ok_or(ParseError::MissingField)?,
                )?,
            },
        )),
        "updateMessageUnreadReactions" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateMessageUnreadReactions {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                message_id: MessageId(int53_or_zero(value.get("message_id"))),
                unread_reaction_count: value
                    .get("unread_reaction_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                newest: value
                    .get("unread_reactions")
                    .and_then(Value::as_array)
                    .and_then(|list| list.last())
                    .and_then(parse_unread_reaction),
            },
        )),
        // Message menu "N Seen" / "N Reacted" rows (schema lines 2859-2879,
        // 7315-7318).
        "messageViewers" => Ok(EnvelopePayload::Messages(MessagesPayload::MessageViewers(
            parse_message_viewers(value),
        ))),
        "addedReactions" => Ok(EnvelopePayload::Messages(MessagesPayload::AddedReactions(
            parse_added_reactions(value),
        ))),
        "updateActiveLiveLocationMessages" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateActiveLiveLocationMessages {
                shares: parse_active_live_locations(value),
            },
        )),
        "updateMessageLiveLocationViewed" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateMessageLiveLocationViewed {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                message_id: MessageId(int53(value.get("message_id"))?),
            },
        )),
        "updateChatDraftMessage" => {
            let chat_id = ChatId(int53(value.get("chat_id"))?);
            Ok(EnvelopePayload::Messages(
                MessagesPayload::UpdateChatDraftMessage {
                    chat_id,
                    draft: parse_chat_draft(value.get("draft_message")),
                    positions: parse_position_list(chat_id, value.get("positions")),
                },
            ))
        }
        "richMessage" => {
            let (blocks, is_full) = parse_rich_message(value);
            Ok(EnvelopePayload::Messages(MessagesPayload::RichMessage {
                rich: RichMessageContent { blocks, is_full },
            }))
        }
        // MED4: `webPageInstantView` (schema:4377) — same `blocks` /
        // `is_full` shape as `richMessage`, so the M2 parser applies.
        "webPageInstantView" => {
            let (blocks, is_full) = parse_rich_message(value);
            Ok(EnvelopePayload::Messages(
                MessagesPayload::WebPageInstantView {
                    rich: RichMessageContent { blocks, is_full },
                },
            ))
        }
        // MED4b: `getLinkPreview` answer (schema:14792) — the full
        // `linkPreview` object. Thumbnail `ParsedFile`s are dropped: the
        // composer chip shows title/description + a media glyph, never
        // the image (downloading transient preview files is out of
        // slice — DECISIONS.md).
        "linkPreview" => {
            let (preview, _files) = parse_link_preview(Some(value));
            Ok(EnvelopePayload::Messages(MessagesPayload::LinkPreview {
                preview,
            }))
        }
        "messageLink" => Ok(EnvelopePayload::Messages(MessagesPayload::MessageLink {
            link: value
                .get("link")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            is_public: value
                .get("is_public")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })),
        // M1 fix-up: only `can_get_link` is kept (see the
        // `MessageProperties` payload docs).
        "messageReadDateRead"
        | "messageReadDateUnread"
        | "messageReadDateTooOld"
        | "messageReadDateUserPrivacyRestricted"
        | "messageReadDateMyPrivacyRestricted" => parse_message_read_date(value)
            .map(|value| EnvelopePayload::Messages(MessagesPayload::MessageReadDate(value)))
            .ok_or(ParseError::MissingField),
        "messageProperties" => {
            let flag = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
            Ok(EnvelopePayload::Messages(
                MessagesPayload::MessageProperties(MessageActions {
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
                    can_get_poll_vote_statistics: flag("can_get_poll_vote_statistics"),
                    can_be_replied_in_another_chat: flag("can_be_replied_in_another_chat"),
                    can_set_fact_check: flag("can_set_fact_check"),
                }),
            ))
        }
        // B4: `pollVoters` — the `getPollVoters` answer. Unparseable
        // senders are dropped; the list never misattributes a vote.
        "pollVoters" => Ok(EnvelopePayload::Messages(MessagesPayload::PollVoters {
            total_count: value
                .get("total_count")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32(),
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
        })),
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
            Ok(EnvelopePayload::Messages(MessagesPayload::Messages(parsed)))
        }
        "message" => Ok(EnvelopePayload::Messages(MessagesPayload::Message(
            parse_message(value)?,
        ))),
        "sponsoredMessages" => Ok(parse_sponsored_messages(value)?),
        "reportSponsoredResultOk" => Ok(EnvelopePayload::Messages(
            MessagesPayload::ReportSponsoredResult(ReportSponsoredResult::Ok),
        )),
        "reportSponsoredResultFailed" => Ok(EnvelopePayload::Messages(
            MessagesPayload::ReportSponsoredResult(ReportSponsoredResult::Failed),
        )),
        "reportSponsoredResultOptionRequired" => Ok(EnvelopePayload::Messages(
            MessagesPayload::ReportSponsoredResult(ReportSponsoredResult::OptionRequired {
                title: json_field_str(value, "title"),
                options: parse_report_options(value.get("options")),
            }),
        )),
        "reportSponsoredResultAdsHidden" => Ok(EnvelopePayload::Messages(
            MessagesPayload::ReportSponsoredResult(ReportSponsoredResult::AdsHidden),
        )),
        "reportSponsoredResultPremiumRequired" => Ok(EnvelopePayload::Messages(
            MessagesPayload::ReportSponsoredResult(ReportSponsoredResult::PremiumRequired),
        )),
        // Slice A7: `accountTtl` — the `getAccountTtl` answer (schema
        // 1.8.67, line 9053). A missing/invalid `days` degrades to 0
        // rather than failing the parse; the authoritative refetch
        // decides.
        "messageAutoDeleteTime" => Ok(EnvelopePayload::Messages(
            MessagesPayload::MessageAutoDeleteTime {
                seconds: value
                    .get("time")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .clamp(0, i32::MAX as i64) as i32,
            },
        )),
        "updateMessageFactCheck" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateMessageFactCheck {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                message_id: MessageId(int53(value.get("message_id"))?),
                text: crate::telegram::envelope::message::parse_fact_check_text(
                    value.get("fact_check"),
                ),
            },
        )),
        "updateMessageInteractionInfo" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateMessageInteractionInfo {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                message_id: MessageId(int53(value.get("message_id"))?),
                interaction_info: parse_interaction_info(value.get("interaction_info")),
            },
        )),
        "updateChatHasScheduledMessages" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateChatHasScheduledMessages {
                chat_id: int53(value.get("chat_id"))?,
                has_scheduled_messages: value
                    .get("has_scheduled_messages")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
        )),
        "updateChatMessageSender" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateChatMessageSender {
                chat_id: int53(value.get("chat_id"))?,
                message_sender: parse_message_sender(value.get("message_sender_id")).ok(),
            },
        )),
        "chatMessageSenders" => Ok(EnvelopePayload::Messages(
            MessagesPayload::ChatMessageSenders {
                senders: value
                    .get("senders")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| {
                        Some(AvailableMessageSender {
                            sender: parse_message_sender(entry.get("sender")).ok()?,
                            needs_premium: entry
                                .get("needs_premium")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                        })
                    })
                    .collect(),
            },
        )),
        "updateChatIsTranslatable" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateChatIsTranslatable {
                chat_id: int53(value.get("chat_id"))?,
                is_translatable: value
                    .get("is_translatable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
        )),
        "updateMessageIsPinned" => Ok(EnvelopePayload::Messages(
            MessagesPayload::UpdateMessageIsPinned {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                message_id: MessageId(int53(value.get("message_id"))?),
                is_pinned: value
                    .get("is_pinned")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
        )),
        _ => return Ok(None),
    };
    payload.map(Some)
}
