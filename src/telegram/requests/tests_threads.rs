use crate::composer::SendOptions;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::telegram::requests::*;
use serde_json::Value;

#[test]
fn message_thread_requests_match_1_8_67() {
    // getMessageThread (schema line 11566) and the thread topic of
    // sendMessage / sendChatAction (messageTopicThread, line 3001).
    let json = get_message_thread(RequestId(4), ChatId(13), MessageId(101));
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getMessageThread");
    assert_eq!(v["chat_id"], 13);
    assert_eq!(v["message_id"], 101);

    let send = send_text(
        RequestId(5),
        ChatId(14),
        None,
        "hi",
        None,
        &SendOptions::default(),
    );
    let routed: Value = serde_json::from_str(&route_into_thread(&send, 501)).unwrap();
    assert_eq!(routed["@type"], "sendMessage");
    assert_eq!(routed["topic_id"]["@type"], "messageTopicThread");
    assert_eq!(routed["topic_id"]["message_thread_id"], 501);
    assert_eq!(routed["reply_to"]["@type"], "inputMessageReplyToMessage");
    assert_eq!(routed["reply_to"]["message_id"], 501);

    // An explicit reply is kept.
    let reply = send_text(
        RequestId(6),
        ChatId(14),
        None,
        "hi",
        Some(SendReply {
            message_id: MessageId(520),
            quote: None,
        }),
        &SendOptions::default(),
    );
    let routed: Value = serde_json::from_str(&route_into_thread(&reply, 501)).unwrap();
    assert_eq!(routed["reply_to"]["message_id"], 520);
    assert_eq!(routed["topic_id"]["message_thread_id"], 501);

    // Typing has no `reply_to`: only the topic is set.
    let typing = send_chat_action(RequestId(7), ChatId(14), true);
    let typing: Value = serde_json::from_str(&route_into_thread(&typing, 501)).unwrap();
    assert_eq!(typing["topic_id"]["message_thread_id"], 501);
    assert!(typing.get("reply_to").is_none());

    // Requests without a topic pass through untouched.
    let view = view_messages(
        RequestId(8),
        ChatId(14),
        &[MessageId(1)],
        "messageSourceChatHistory",
        true,
    );
    assert_eq!(route_into_thread(&view, 501), view);
}
