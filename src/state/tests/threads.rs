//! Reducer tests: channel comments and group reply threads.
use super::common::*;
use super::*;

fn msg(id: i64, chat: i64, thread: i64, text: &str) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":{chat},"is_outgoing":false,"date":1700000000,"topic_id":{{"@type":"messageTopicThread","message_thread_id":{thread}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
    )
}

/// Session with an open thread on post 101 of channel 13 whose replies live
/// in group 14 (thread id 501), root loaded, nothing else.
fn resolved() -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.begin_thread(ChatId(13), MessageId(101));
    let extra = session.request(
        RequestPurpose::GetMessageThread { message_id: 101 },
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messageThreadInfo","@extra":"{}","chat_id":14,"message_thread_id":501,"reply_info":{{"@type":"messageReplyInfo","reply_count":2,"recent_replier_ids":[],"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"last_message_id":503}},"unread_message_count":0,"messages":[{}]}}"#,
            extra.0,
            msg(501, 14, 501, "root")
        ),
    );
    session.open_chat(ChatId(14));
    (session, sink, seq)
}

#[test]
fn thread_info_fills_the_view_and_hides_the_root_until_complete() {
    let (mut session, sink, seq) = resolved();
    {
        let thread = session.thread_for_chat(ChatId(14)).expect("thread");
        assert_eq!(thread.reply_count, 2);
        assert_eq!(thread.unread_anchor, None, "nothing unread");
        assert!(thread.is_comments());
        // The root is pinned in the bar, not a list row, while replies load.
        assert!(thread.ordered().is_empty());
        assert_eq!(thread.root_message().unwrap().id, MessageId(501));
        assert!(thread.is_root(MessageId(501)));
    }
    let page = session.request(
        RequestPurpose::GetMessageThreadHistory { message_id: 101 },
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{},{}]}}"#,
            page.0,
            msg(503, 14, 501, "b"),
            msg(502, 14, 501, "a")
        ),
    );
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    let ids: Vec<i64> = thread.ordered().iter().map(|m| m.id.0).collect();
    assert_eq!(ids, vec![502, 503], "root joins only at the end of history");
    assert!(!thread.history.loaded_complete);
    assert_eq!(thread.history.next_from_message_id, MessageId(502));
}

#[test]
fn thread_routes_edits_deletes_and_sends() {
    let (mut session, sink, seq) = resolved();
    for json in [
        format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            msg(502, 14, 501, "first")
        ),
        // A reply to the root without a thread topic still belongs to it.
        r#"{"@type":"updateNewMessage","message":{"id":503,"chat_id":14,"is_outgoing":false,"date":1,"reply_to":{"@type":"messageReplyToMessage","chat_id":14,"message_id":501,"origin":null,"origin_send_date":0,"content":null},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"by reply","entities":[]}}}}"#.to_string(),
        // Another thread and another chat are ignored.
        format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            msg(504, 14, 999, "other thread")
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            msg(505, 13, 501, "other chat")
        ),
    ] {
        apply_json(&mut session, &seq, &sink, &json);
    }
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    assert!(thread.history.messages.contains_key(&502));
    assert!(thread.history.messages.contains_key(&503));
    assert!(!thread.history.messages.contains_key(&504));
    assert!(!thread.history.messages.contains_key(&505));

    // Edits reach the thread row.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageContent","chat_id":14,"message_id":502,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"first (edited)","entities":[]}}}"#,
    );
    let row = &session.threads.thread.as_ref().unwrap().history.messages[&502];
    assert!(matches!(
        &row.content,
        MessageContent::Text(text) if text.text == "first (edited)"
    ));

    // A pending own send resolves in place.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":-7,"chat_id":14,"is_outgoing":true,"date":1,"topic_id":{"@type":"messageTopicThread","message_thread_id":501},"sending_state":{"@type":"messageSendingStatePending","sending_id":0},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"mine","entities":[]}}}}"#,
    );
    assert!(
        session
            .threads
            .thread
            .as_ref()
            .unwrap()
            .history
            .messages
            .contains_key(&-7)
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateMessageSendSucceeded","old_message_id":-7,"message":{}}}"#,
            msg(510, 14, 501, "mine").replace("\"is_outgoing\":false", "\"is_outgoing\":true")
        ),
    );
    let thread = session.threads.thread.as_ref().unwrap();
    assert!(!thread.history.messages.contains_key(&-7));
    assert!(thread.history.messages.contains_key(&510));

    // Deletes remove rows.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateDeleteMessages","chat_id":14,"message_ids":[502],"is_permanent":true,"from_cache":false}"#,
    );
    assert!(
        !session
            .threads
            .thread
            .as_ref()
            .unwrap()
            .history
            .messages
            .contains_key(&502)
    );
}

#[test]
fn unread_thread_places_the_divider_anchor_and_failed_pages_keep_ready() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    {
        session.begin_thread(ChatId(14), MessageId(40));
        let extra = session.request(
            RequestPurpose::GetMessageThread { message_id: 40 },
            Some(ChatId(14)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messageThreadInfo","@extra":"{}","chat_id":14,"message_thread_id":40,"reply_info":{{"@type":"messageReplyInfo","reply_count":5,"recent_replier_ids":[],"last_read_inbox_message_id":44,"last_read_outbox_message_id":0,"last_message_id":49}},"unread_message_count":5,"messages":[{}]}}"#,
                extra.0,
                msg(40, 14, 40, "root")
            ),
        );
    }
    session.open_chat(ChatId(14));
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    assert_eq!(thread.unread_anchor, Some(MessageId(44)));
    assert_eq!(thread.unread_count, 5);
    assert!(!thread.is_comments(), "same chat");
    assert_eq!(thread.subtitle(), "5 replies");
    // The first page lands, then a later page fails: the view stays Ready.
    let page = session.request(
        RequestPurpose::GetMessageThreadHistory { message_id: 40 },
        Some(ChatId(14)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{}]}}"#,
            page.0,
            msg(49, 14, 40, "newest")
        ),
    );
    let older = session.request(
        RequestPurpose::GetMessageThreadHistory { message_id: 40 },
        Some(ChatId(14)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"Timeout"}}"#,
            older.0
        ),
    );
    assert_eq!(
        session.threads.thread.as_ref().unwrap().status,
        ThreadStatus::Ready
    );
}

#[test]
fn thread_labels_follow_tdesktop() {
    assert_eq!(thread_count_label(0, true), "Comments");
    assert_eq!(thread_count_label(1, true), "1 comment");
    assert_eq!(thread_count_label(128, true), "128 comments");
    assert_eq!(thread_count_label(0, false), "Replies");
    assert_eq!(thread_count_label(1, false), "1 reply");
    assert_eq!(thread_count_label(7, false), "7 replies");
    assert_eq!(comments_bar_label(0), "Leave a comment");
    assert_eq!(comments_bar_label(1), "1 comment");
    assert_eq!(comments_bar_label(3), "3 comments");
    assert_eq!(replies_link_label(1), "1 reply");
    assert_eq!(replies_link_label(4), "4 replies");
}

#[test]
fn opening_another_chat_leaves_the_thread() {
    let (mut session, _, _) = resolved();
    assert!(session.threads.thread.is_some());
    session.open_chat(ChatId(14));
    assert!(session.threads.thread.is_some(), "its own chat keeps it");
    session.open_chat(ChatId(13));
    assert!(session.threads.thread.is_none(), "another chat drops it");
}
