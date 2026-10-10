use super::*;
use crate::ids::ChatId;

#[test]
fn reply_info_is_parsed_from_interaction_info() {
    // `messageReplyInfo` (schema 1.8.67, line 2967) rides on
    // `messageInteractionInfo.reply_info`.
    let env = parse_envelope(
        r#"{"@type":"updateMessageInteractionInfo","chat_id":13,"message_id":101,"interaction_info":{"@type":"messageInteractionInfo","view_count":5,"forward_count":1,"reply_info":{"@type":"messageReplyInfo","reply_count":12,"recent_replier_ids":[{"@type":"messageSenderUser","user_id":7},{"@type":"messageSenderChat","chat_id":-1002},{"@type":"messageSenderBogus"}],"last_read_inbox_message_id":50,"last_read_outbox_message_id":40,"last_message_id":60},"reactions":null}}"#,
    )
    .unwrap();
    let EnvelopePayload::Messages(MessagesPayload::UpdateMessageInteractionInfo {
        interaction_info,
        ..
    }) = env.payload
    else {
        panic!("interaction info update");
    };
    let reply = interaction_info.unwrap().reply_info.expect("reply info");
    assert_eq!(reply.reply_count, 12);
    assert_eq!(
        reply.recent_repliers,
        vec![
            MessageSender::User { user_id: 7 },
            MessageSender::Chat { chat_id: -1002 }
        ]
    );
    assert_eq!(reply.last_read_inbox_message_id, 50);
    assert_eq!(reply.last_read_outbox_message_id, 40);
    assert_eq!(reply.last_message_id, 60);
    assert!(reply.has_unread());
    // A null `reply_info` and a fully-read thread.
    let env = parse_envelope(
        r#"{"@type":"updateMessageInteractionInfo","chat_id":13,"message_id":102,"interaction_info":{"@type":"messageInteractionInfo","view_count":5,"forward_count":1,"reply_info":null,"reactions":null}}"#,
    )
    .unwrap();
    let EnvelopePayload::Messages(MessagesPayload::UpdateMessageInteractionInfo {
        interaction_info,
        ..
    }) = env.payload
    else {
        panic!("interaction info update");
    };
    assert_eq!(interaction_info.unwrap().reply_info, None);
    let read = MessageReplyInfo {
        reply_count: 2,
        last_read_inbox_message_id: 9,
        last_message_id: 9,
        ..Default::default()
    };
    assert!(!read.has_unread());
}

#[test]
fn message_thread_info_and_thread_topic_are_parsed() {
    // `messageThreadInfo` (schema 1.8.67, line 3897) is the answer to
    // `getMessageThread`; thread messages carry `messageTopicThread`.
    let env = parse_envelope(
        r#"{"@type":"messageThreadInfo","@extra":"4","chat_id":14,"message_thread_id":501,"reply_info":{"@type":"messageReplyInfo","reply_count":3,"recent_replier_ids":[],"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"last_message_id":504},"unread_message_count":2,"messages":[{"id":501,"chat_id":14,"is_outgoing":false,"topic_id":{"@type":"messageTopicThread","message_thread_id":501},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"root","entities":[]}}}],"draft_message":null}"#,
    )
    .unwrap();
    let EnvelopePayload::Threads(ThreadsPayload::MessageThreadInfo(info)) = env.payload else {
        panic!("thread info");
    };
    assert_eq!(info.chat_id, ChatId(14));
    assert_eq!(info.message_thread_id, 501);
    assert_eq!(info.unread_message_count, 2);
    assert_eq!(info.reply_info.unwrap().reply_count, 3);
    assert_eq!(info.messages.len(), 1);
    assert_eq!(info.messages[0].thread_id, Some(501));
    assert_eq!(info.messages[0].topic_id, None, "not a forum topic");
}
