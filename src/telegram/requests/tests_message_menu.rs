use crate::ids::{ChatId, MessageId, RequestId};
use crate::telegram::envelope::ReactionType;
use crate::telegram::requests::{
    add_profile_audio, delete_chat_messages_by_sender, get_message_added_reactions,
    get_message_read_date, get_message_viewers, report_chat_messages, report_supergroup_spam,
};
use serde_json::Value;

fn parse(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

#[test]
fn report_chat_messages_carries_ids_option_and_text() {
    let value = parse(&report_chat_messages(
        RequestId(5),
        ChatId(-100),
        "b3B0",
        &[MessageId(7), MessageId(8)],
        "details",
    ));
    assert_eq!(value["@type"], "reportChat");
    assert_eq!(value["chat_id"], -100);
    assert_eq!(value["option_id"], "b3B0");
    assert_eq!(value["message_ids"], serde_json::json!([7, 8]));
    assert_eq!(value["text"], "details");
    // The first call has an empty option id, as the schema documents.
    let first = parse(&report_chat_messages(
        RequestId(6),
        ChatId(1),
        "",
        &[MessageId(1)],
        "",
    ));
    assert_eq!(first["option_id"], "");
}

#[test]
fn audience_requests_match_the_schema() {
    let viewers = parse(&get_message_viewers(RequestId(1), ChatId(2), MessageId(3)));
    assert_eq!(viewers["@type"], "getMessageViewers");
    assert_eq!(viewers["message_id"], 3);
    let read = parse(&get_message_read_date(
        RequestId(1),
        ChatId(2),
        MessageId(3),
    ));
    assert_eq!(read["@type"], "getMessageReadDate");
    let all = parse(&get_message_added_reactions(
        RequestId(1),
        ChatId(2),
        MessageId(3),
        None,
        "",
        50,
    ));
    assert_eq!(all["@type"], "getMessageAddedReactions");
    assert!(all["reaction_type"].is_null());
    assert_eq!(all["limit"], 50);
    let one = parse(&get_message_added_reactions(
        RequestId(1),
        ChatId(2),
        MessageId(3),
        Some(&ReactionType::emoji("👍")),
        "next",
        10,
    ));
    assert_eq!(one["reaction_type"]["@type"], "reactionTypeEmoji");
    assert_eq!(one["offset"], "next");
}

#[test]
fn moderation_requests_match_the_schema() {
    let delete = parse(&delete_chat_messages_by_sender(
        RequestId(1),
        ChatId(-100),
        42,
    ));
    assert_eq!(delete["@type"], "deleteChatMessagesBySender");
    assert_eq!(delete["sender_id"]["@type"], "messageSenderUser");
    assert_eq!(delete["sender_id"]["user_id"], 42);
    let spam = parse(&report_supergroup_spam(RequestId(2), 9, &[MessageId(4)]));
    assert_eq!(spam["@type"], "reportSupergroupSpam");
    assert_eq!(spam["supergroup_id"], 9);
    assert_eq!(spam["message_ids"], serde_json::json!([4]));
}

#[test]
fn profile_audio_is_sent_by_file_id() {
    let value = parse(&add_profile_audio(RequestId(3), 12, 200, "Song", "Artist"));
    assert_eq!(value["@type"], "addProfileAudio");
    assert_eq!(value["audio"]["@type"], "inputAudio");
    assert_eq!(value["audio"]["audio"]["@type"], "inputFileId");
    assert_eq!(value["audio"]["audio"]["id"], 12);
    assert_eq!(value["audio"]["performer"], "Artist");
}
