//! Parses TDLib objects for forum topics, comment threads and Saved Messages.
use crate::ids::ChatId;
use crate::telegram::envelope::*;
use serde_json::Value;

/// The threads domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_threads_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        "updateChatViewAsTopics" => Ok(EnvelopePayload::Threads(
            ThreadsPayload::UpdateChatViewAsTopics {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                view_as_topics: value
                    .get("view_as_topics")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
        )),
        "updateSavedMessagesTopic" => value
            .get("topic")
            .and_then(parse_saved_messages_topic)
            .map(|topic| {
                EnvelopePayload::Threads(ThreadsPayload::UpdateSavedMessagesTopic(Box::new(topic)))
            })
            .ok_or(ParseError::MissingField),
        "updateSavedMessagesTopicCount" => Ok(EnvelopePayload::Threads(
            ThreadsPayload::UpdateSavedMessagesTopicCount {
                topic_count: int53_or_zero(value.get("topic_count")).sat_i32(),
            },
        )),
        "updateSavedMessagesTags" => Ok(EnvelopePayload::Threads(
            ThreadsPayload::UpdateSavedMessagesTags {
                saved_messages_topic_id: int53_or_zero(value.get("saved_messages_topic_id")),
                tags: value
                    .get("tags")
                    .map(parse_saved_messages_tags)
                    .unwrap_or_default(),
            },
        )),
        "savedMessagesTags" => Ok(EnvelopePayload::Threads(
            ThreadsPayload::SavedMessagesTags {
                tags: parse_saved_messages_tags(value),
            },
        )),
        // `getMessageThread` answer (schema 1.8.67, line 3897).
        "messageThreadInfo" => Ok(EnvelopePayload::Threads(ThreadsPayload::MessageThreadInfo(
            Box::new(parse_message_thread_info(value)?),
        ))),
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
            Ok(EnvelopePayload::Threads(ThreadsPayload::ForumTopics {
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                topics: parsed,
            }))
        }
        // Slice G2: `forumTopicInfo` — the `createForumTopic` answer
        // (schema 1.8.67, line 12665). Only the chat id is kept; the
        // topic list is refetched on success.
        // Subsection tabs: live topic changes (schema 1.8.67, lines
        // 10652 / 10665) — new topics, renames, pins, reads, mutes.
        "updateForumTopicInfo" => value
            .get("info")
            .and_then(parse_forum_topic_info)
            .map(|value| EnvelopePayload::Threads(ThreadsPayload::UpdateForumTopicInfo(value)))
            .ok_or(ParseError::MissingField),
        "forumTopic" => parse_forum_topic(value)
            .map(|value| EnvelopePayload::Threads(ThreadsPayload::ForumTopicAnswer(value)))
            .ok_or(ParseError::MissingField),
        "updateForumTopic" => parse_forum_topic_update(value)
            .map(|value| EnvelopePayload::Threads(ThreadsPayload::UpdateForumTopic(value)))
            .ok_or(ParseError::MissingField),
        "forumTopicInfo" => Ok(EnvelopePayload::Threads(ThreadsPayload::ForumTopic {
            chat_id: value.get("chat_id").and_then(Value::as_i64).unwrap_or(0),
        })),
        _ => return Ok(None),
    };
    payload.map(Some)
}
