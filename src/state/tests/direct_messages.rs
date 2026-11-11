use super::common::*;
use super::*;
use crate::composer::DraftTopic;

const CHANNEL: i64 = -1_000_000_000_081;
const DIRECT: i64 = -1_000_000_000_082;

fn admin_session() -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for json in [
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{DIRECT},"title":"Quill Weekly","type":{{"@type":"chatTypeSupergroup","supergroup_id":82,"is_channel":false}},"unread_count":4,"permissions":{{"can_send_basic_messages":true}}}}}}"#
        ),
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":82,"is_channel":false,"is_direct_messages_group":true,"is_administered_direct_messages_group":true,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_manage_direct_messages":true}}}}"#.to_owned(),
        format!(
            r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":82,"supergroup_full_info":{{"description":"","direct_messages_chat_id":{CHANNEL}}}}}"#
        ),
        topic_json(501, 501, 9000, 3, 2_000),
        topic_json(502, 502, 8000, 1, 3_000),
    ] {
        apply_json(&mut session, &seq, &sink, &json);
    }
    (session, sink, seq)
}

fn topic_json(id: i64, user: i64, order: i64, unread: i32, last: i64) -> String {
    format!(
        r#"{{"@type":"updateDirectMessagesChatTopic","topic":{{"@type":"directMessagesChatTopic","chat_id":{DIRECT},"id":"{id}","sender_id":{{"@type":"messageSenderUser","user_id":{user}}},"order":"{order}","can_send_unpaid_messages":false,"is_marked_as_unread":false,"unread_count":{unread},"last_read_inbox_message_id":{read},"last_read_outbox_message_id":0,"unread_reaction_count":0,"last_message":{msg},"draft_message":null}}}}"#,
        read = last - 1,
        msg = message_json(last, id, "hi"),
    )
}

fn message_json(id: i64, topic: i64, text: &str) -> String {
    format!(
        r#"{{"id":{id},"chat_id":{DIRECT},"date":1700000000,"is_outgoing":false,"sender_id":{{"@type":"messageSenderUser","user_id":{topic}}},"topic_id":{{"@type":"messageTopicDirectMessages","direct_messages_chat_topic_id":{topic}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
    )
}

fn new_message(id: i64, topic: i64) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{}}}"#,
        message_json(id, topic, "new")
    )
}

#[test]
fn conversations_list_in_order_and_count_unread() {
    let (session, _, _) = admin_session();
    let direct = ChatId(DIRECT);
    assert!(session.is_direct_messages_admin(direct));
    let ids: Vec<i64> = session
        .threads
        .direct
        .ordered(DIRECT)
        .iter()
        .map(|topic| topic.id)
        .collect();
    assert_eq!(ids, vec![501, 502]);
    assert_eq!(session.direct_topic_count(direct), 2);
    assert_eq!(session.threads.direct.unread_topics(DIRECT), 2);
    assert_eq!(session.direct_unread_count(direct), Some(4));
    assert_eq!(session.open_direct_topic(direct), None);
}

#[test]
fn opening_a_conversation_swaps_the_history_window() {
    let (mut session, sink, seq) = admin_session();
    let direct = ChatId(DIRECT);
    apply_json(&mut session, &seq, &sink, &new_message(4_000, 501));
    apply_json(&mut session, &seq, &sink, &new_message(4_100, 502));
    assert_eq!(session.histories[&DIRECT].messages.len(), 2);

    assert!(session.select_direct_topic(direct, Some(501)));
    assert!(!session.select_direct_topic(direct, Some(501)), "no change");
    assert!(session.histories[&DIRECT].messages.is_empty());
    assert_eq!(
        session.histories[&DIRECT].unread_anchor,
        Some(MessageId(1_999)),
        "the conversation's own read position"
    );
    assert_eq!(
        session.direct_unread_count(direct),
        Some(3),
        "its own unread count"
    );
    apply_json(&mut session, &seq, &sink, &new_message(4_200, 502));
    apply_json(&mut session, &seq, &sink, &new_message(4_300, 501));
    let ids: Vec<i64> = session.histories[&DIRECT]
        .messages
        .keys()
        .copied()
        .collect();
    assert_eq!(ids, vec![4_300]);
    assert_eq!(
        session.send_topic(direct),
        Some(crate::ids::TopicId::DirectMessages {
            direct_messages_chat_topic_id: 501
        })
    );
    assert!(session.select_direct_topic(direct, None));
    assert_eq!(session.send_topic(direct), None);
}

#[test]
fn the_all_view_locks_the_field_until_a_reply_is_picked() {
    let (mut session, sink, seq) = admin_session();
    let direct = ChatId(DIRECT);
    apply_json(&mut session, &seq, &sink, &new_message(4_000, 501));
    assert!(session.direct_reply_locked(direct, None));
    assert!(!session.direct_reply_locked(direct, Some(MessageId(4_000))));
    assert!(
        session.direct_reply_locked(direct, Some(MessageId(9_999))),
        "a reply to a message that is not loaded names no conversation"
    );
    session.select_direct_topic(direct, Some(501));
    assert!(!session.direct_reply_locked(direct, None));
    session.select_direct_topic(direct, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":82,"supergroup_full_info":{"description":"","direct_messages_chat_id":0}}"#,
    );
    assert!(session.direct_messages_disabled(direct));
    assert!(!session.direct_reply_locked(direct, None));
}

#[test]
fn drafts_live_on_the_conversation() {
    let (mut session, sink, seq) = admin_session();
    let direct = ChatId(DIRECT);
    assert!(!session.accepts_composer_draft(direct, DraftTopic::Chat));
    assert!(session.accepts_composer_draft(direct, DraftTopic::DirectMessages(501)));
    session.select_direct_topic(direct, Some(502));
    assert_eq!(
        session.open_draft_topic(direct),
        DraftTopic::DirectMessages(502)
    );
    let draft = r#"{"@type":"draftMessage","date":1,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"later","entities":[]}}}"#;
    let json = topic_json(502, 502, 8000, 1, 3_000).replace(
        r#""draft_message":null"#,
        &format!(r#""draft_message":{draft}"#),
    );
    apply_json(&mut session, &seq, &sink, &json);
    assert!(
        session
            .draft_for(direct, DraftTopic::DirectMessages(502))
            .is_some()
    );
    assert!(
        session
            .draft_for(direct, DraftTopic::DirectMessages(501))
            .is_none()
    );
}

#[test]
fn a_member_never_gets_admin_conversations() {
    let (mut session, sink, seq) = admin_session();
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":82,"is_channel":false,"is_direct_messages_group":true,"is_administered_direct_messages_group":false,"status":{"@type":"chatMemberStatusLeft"}}}"#,
    );
    let direct = ChatId(DIRECT);
    assert!(!session.select_direct_topic(direct, Some(501)));
    assert_eq!(session.open_direct_topic(direct), None);
    assert!(!session.direct_reply_locked(direct, None));
    assert_eq!(session.direct_unread_count(direct), None);
}

#[test]
fn a_deleted_conversation_forgets_its_count_and_revenue() {
    let (mut session, _, _) = admin_session();
    let chat = ChatId(DIRECT);
    session
        .threads
        .direct
        .message_counts
        .insert((DIRECT, 501), 12);
    session.set_direct_topic_revenue(chat, 501, 40);
    session
        .threads
        .direct
        .message_counts
        .insert((DIRECT, 502), 3);
    session.forget_direct_topic(chat, 501);
    assert_eq!(session.direct_topic_revenue(chat, 501), None);
    assert!(
        !session
            .threads
            .direct
            .message_counts
            .contains_key(&(DIRECT, 501))
    );
    assert!(
        session
            .threads
            .direct
            .message_counts
            .contains_key(&(DIRECT, 502)),
        "another conversation's stay"
    );
}
