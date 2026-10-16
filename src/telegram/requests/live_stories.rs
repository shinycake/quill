//! Live story requests (TDLib 1.8.68): the streamer, the top donors, the
//! comment senders, paid comments and the pending Star reactions.
use crate::ids::RequestId;
use crate::telegram::envelope::MessageSender;
use crate::telegram::requests::message_sender_json;
use serde_json::json;

/// `getLiveStoryStreamer group_call_id:int32 = GroupCallParticipant`, for
/// live stories that aren't an RTMP stream.
pub fn get_live_story_streamer(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "getLiveStoryStreamer",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// `getLiveStoryTopDonors group_call_id:int32 = LiveStoryDonors`.
pub fn get_live_story_top_donors(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "getLiveStoryTopDonors",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// `getLiveStoryAvailableMessageSenders group_call_id:int32 =
/// ChatMessageSenders`.
pub fn get_live_story_available_message_senders(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "getLiveStoryAvailableMessageSenders",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// `setLiveStoryMessageSender group_call_id:int32
/// message_sender_id:MessageSender = Ok`.
pub fn set_live_story_message_sender(
    extra: RequestId,
    group_call_id: i32,
    sender: MessageSender,
) -> String {
    json!({
        "@type": "setLiveStoryMessageSender",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "message_sender_id": message_sender_json(sender),
    })
    .to_string()
}

/// `sendGroupCallMessage` for a live story comment: no line feeds (the
/// caller flattens them) and the Stars the viewer agreed to pay.
pub fn send_live_story_comment(
    extra: RequestId,
    group_call_id: i32,
    text: &str,
    entities: &[crate::composer::ComposerEntity],
    paid_message_star_count: i64,
) -> String {
    let entities: Vec<serde_json::Value> = entities
        .iter()
        .map(crate::telegram::requests::format_entity_json)
        .collect();
    json!({
        "@type": "sendGroupCallMessage",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "text": { "@type": "formattedText", "text": text, "entities": entities },
        "paid_message_star_count": paid_message_star_count,
    })
    .to_string()
}

/// `addPendingLiveStoryReaction group_call_id:int32 star_count:int53 = Ok`.
pub fn add_pending_live_story_reaction(
    extra: RequestId,
    group_call_id: i32,
    star_count: i64,
) -> String {
    json!({
        "@type": "addPendingLiveStoryReaction",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "star_count": star_count,
    })
    .to_string()
}

/// `commitPendingLiveStoryReactions group_call_id:int32 = Ok`.
pub fn commit_pending_live_story_reactions(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "commitPendingLiveStoryReactions",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// `removePendingLiveStoryReactions group_call_id:int32 = Ok`.
pub fn remove_pending_live_story_reactions(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "removePendingLiveStoryReactions",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// `deleteGroupCallMessages group_call_id:int32 message_ids:vector<int32>
/// report_spam:Bool = Ok`.
pub fn delete_group_call_messages(
    extra: RequestId,
    group_call_id: i32,
    message_ids: &[i32],
    report_spam: bool,
) -> String {
    json!({
        "@type": "deleteGroupCallMessages",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "message_ids": message_ids,
        "report_spam": report_spam,
    })
    .to_string()
}

/// `deleteGroupCallMessagesBySender group_call_id:int32
/// sender_id:MessageSender report_spam:Bool = Ok`.
pub fn delete_group_call_messages_by_sender(
    extra: RequestId,
    group_call_id: i32,
    sender: MessageSender,
    report_spam: bool,
) -> String {
    json!({
        "@type": "deleteGroupCallMessagesBySender",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "sender_id": message_sender_json(sender),
        "report_spam": report_spam,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(json: &str) -> Value {
        serde_json::from_str(json).unwrap_or(Value::Null)
    }

    #[test]
    fn live_story_requests_carry_the_call_and_the_stars() {
        let extra = RequestId(7);
        let streamer = parse(&get_live_story_streamer(extra, 42));
        assert_eq!(streamer["@type"], "getLiveStoryStreamer");
        assert_eq!(streamer["group_call_id"], 42);
        let comment = parse(&send_live_story_comment(extra, 42, "hi", &[], 30));
        assert_eq!(comment["text"]["text"], "hi");
        assert_eq!(comment["paid_message_star_count"], 30);
        let add = parse(&add_pending_live_story_reaction(extra, 42, 5));
        assert_eq!(add["@type"], "addPendingLiveStoryReaction");
        assert_eq!(add["star_count"], 5);
        let sender = parse(&set_live_story_message_sender(
            extra,
            42,
            MessageSender::Chat { chat_id: -100 },
        ));
        assert_eq!(sender["message_sender_id"]["chat_id"], -100);
        let delete = parse(&delete_group_call_messages(extra, 42, &[3, 4], true));
        assert_eq!(delete["message_ids"], serde_json::json!([3, 4]));
        assert_eq!(delete["report_spam"], true);
        for json in [
            get_live_story_top_donors(extra, 42),
            get_live_story_available_message_senders(extra, 42),
            commit_pending_live_story_reactions(extra, 42),
            remove_pending_live_story_reactions(extra, 42),
            delete_group_call_messages_by_sender(
                extra,
                42,
                MessageSender::User { user_id: 9 },
                false,
            ),
        ] {
            assert_eq!(parse(&json)["group_call_id"], 42);
        }
    }
}
