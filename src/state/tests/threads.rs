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
fn the_root_is_followed_by_the_discussion_divider_once_loaded() {
    let (mut session, sink, seq) = resolved();
    // Nothing loaded down to the root yet: no root row, no divider.
    assert!(
        session
            .thread_for_chat(ChatId(14))
            .unwrap()
            .rows()
            .is_empty()
    );
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
    let page = session.request(
        RequestPurpose::GetMessageThreadHistory { message_id: 101 },
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[]}}"#,
            page.0
        ),
    );
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    assert!(thread.history.loaded_complete);
    let rows = thread.rows();
    let ids: Vec<i64> = rows.iter().map(|m| m.id.0).collect();
    assert_eq!(ids, vec![501, THREAD_DIVIDER_ROW_ID, 502, 503]);
    let divider = &rows[1];
    assert_eq!(divider.chat_id, ChatId(14));
    assert_eq!(divider.date, rows[0].date, "same day as the root");
    assert_eq!(
        session.service_text(divider).map(|text| text.plain()),
        Some("Discussion started".to_string())
    );
}

#[test]
fn a_thread_without_replies_says_no_comments_yet() {
    let (mut session, sink, seq) = resolved();
    let page = session.request(
        RequestPurpose::GetMessageThreadHistory { message_id: 101 },
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":0,"messages":[]}}"#,
            page.0
        ),
    );
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    let rows = thread.rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        session.service_text(&rows[1]).map(|text| text.plain()),
        Some("No comments here yet...".to_string())
    );
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

/// Group 14, root 40, five unread replies after 44 (newest 49): the thread
/// opens around the read position, pages forward, holds live replies
/// outside a short window and counts unread replies as rows are viewed.
fn unread_group_thread() -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
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
            r#"{{"@type":"messageThreadInfo","@extra":"{}","chat_id":14,"message_thread_id":40,"reply_info":{{"@type":"messageReplyInfo","reply_count":9,"recent_replier_ids":[],"last_read_inbox_message_id":44,"last_read_outbox_message_id":0,"last_message_id":49}},"unread_message_count":5,"messages":[{}]}}"#,
            extra.0,
            msg(40, 14, 40, "root")
        ),
    );
    session.open_chat(ChatId(14));
    (session, sink, seq)
}

fn thread_page(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    newer: bool,
    ids: &[i64],
) {
    let purpose = if newer {
        RequestPurpose::Threads(ThreadsPurpose::GetMessageThreadHistoryNewer { message_id: 40 })
    } else {
        RequestPurpose::GetMessageThreadHistory { message_id: 40 }
    };
    let extra = session.request(purpose, Some(ChatId(14)));
    let rows: Vec<String> = ids
        .iter()
        .map(|id| msg(*id, 14, 40, &format!("reply {id}")))
        .collect();
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":{},"messages":[{}]}}"#,
            extra.0,
            ids.len(),
            rows.join(",")
        ),
    );
}

#[test]
fn an_unread_thread_opens_around_the_read_position_and_pages_forward() {
    let (mut session, sink, seq) = unread_group_thread();
    assert_eq!(
        session.thread_for_chat(ChatId(14)).unwrap().last_message_id,
        49
    );
    // The first page around 44: the window stops short of 49.
    thread_page(&mut session, &seq, &sink, false, &[46, 45, 44, 43]);
    {
        let thread = session.thread_for_chat(ChatId(14)).unwrap();
        assert!(thread.history.has_newer);
        assert!(!thread.history.loaded_complete);
        assert_eq!(thread.history.next_from_message_id, MessageId(43));
        assert_eq!(thread.unread_anchor, Some(MessageId(44)));
        assert_eq!(
            thread.unread_count, 5,
            "TDLib's count until rows are viewed"
        );
        assert_eq!(thread.newest_loaded_id(), Some(MessageId(46)));
    }
    // A live reply beyond the window only moves the counters.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            msg(50, 14, 40, "live")
        ),
    );
    {
        let thread = session.thread_for_chat(ChatId(14)).unwrap();
        assert!(
            !thread.history.messages.contains_key(&50),
            "outside the window"
        );
        assert_eq!(thread.last_message_id, 50);
        assert_eq!(thread.unread_count, 6);
    }
    // Viewing rows 45 and 46 reads two of them (counted by subtraction
    // while newer rows are unloaded).
    assert!(session.report_visible_messages(ChatId(14), &[MessageId(45), MessageId(46)]));
    {
        let thread = session.thread_for_chat(ChatId(14)).unwrap();
        assert_eq!(thread.last_read_inbox_message_id, 46);
        assert_eq!(thread.unread_count, 4);
        assert_eq!(
            thread.unread_anchor,
            Some(MessageId(44)),
            "the divider stays"
        );
    }
    // The newer page reaches the newest reply.
    thread_page(&mut session, &seq, &sink, true, &[50, 49, 48, 47, 46]);
    {
        let thread = session.thread_for_chat(ChatId(14)).unwrap();
        assert!(!thread.history.has_newer);
        let ids: Vec<i64> = thread.ordered().iter().map(|m| m.id.0).collect();
        assert_eq!(ids, vec![43, 44, 45, 46, 47, 48, 49, 50]);
        assert!(!thread.history.loaded_complete, "older side still open");
    }
    // At the bottom the count follows the loaded rows.
    session.report_visible_messages(ChatId(14), &[MessageId(49), MessageId(50)]);
    assert_eq!(session.thread_for_chat(ChatId(14)).unwrap().unread_count, 0);
    // An empty newer page just closes the newer side.
    let thread = session.threads.thread.as_mut().unwrap();
    thread.last_message_id = 60;
    thread.history.has_newer = true;
    thread_page(&mut session, &seq, &sink, true, &[]);
    assert!(
        !session
            .thread_for_chat(ChatId(14))
            .unwrap()
            .history
            .has_newer
    );
}

#[test]
fn an_own_send_while_newer_replies_are_unloaded_replaces_the_window() {
    let (mut session, sink, seq) = unread_group_thread();
    thread_page(&mut session, &seq, &sink, false, &[46, 45, 44, 43]);
    assert!(
        session
            .thread_for_chat(ChatId(14))
            .unwrap()
            .history
            .has_newer
    );
    // An older page is in flight when the window is replaced: its answer
    // is dropped.
    let stale = session.request(
        RequestPurpose::GetMessageThreadHistory { message_id: 40 },
        Some(ChatId(14)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":-7,"chat_id":14,"is_outgoing":true,"date":1,"topic_id":{"@type":"messageTopicThread","message_thread_id":40},"sending_state":{"@type":"messageSendingStatePending","sending_id":0},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"mine","entities":[]}}}}"#,
    );
    {
        let thread = session.thread_for_chat(ChatId(14)).unwrap();
        let ids: Vec<i64> = thread.history.messages.keys().copied().collect();
        assert_eq!(ids, vec![-7, 40], "root and the pending send only");
        assert!(!thread.history.has_newer);
        assert!(thread.reload_needed);
        assert_eq!(thread.history.window_epoch, 1);
        assert!(!thread.history.loaded_complete);
        assert_eq!(thread.history.next_from_message_id, MessageId(0));
    }
    assert!(session.reset_thread_window());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{}]}}"#,
            stale.0,
            msg(42, 14, 40, "stale")
        ),
    );
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    assert!(
        !thread.history.messages.contains_key(&42),
        "stale page dropped"
    );
    assert_eq!(thread.history.window_epoch, 2);
    // The newest page lands in the fresh window and reaches the root.
    thread_page(
        &mut session,
        &seq,
        &sink,
        false,
        &[50, 49, 48, 47, 46, 45, 44, 43, 42, 41, 40],
    );
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    assert!(thread.history.loaded_complete);
    assert!(!thread.history.has_newer);
    assert!(
        thread.history.messages.contains_key(&-7),
        "the pending row stays"
    );
}

#[test]
fn reading_comments_clears_the_posts_unread_dot_and_follows_server_read_marks() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // The channel post with its comments bar, loaded in the channel.
    session.open_chat(ChatId(13));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":13,"is_outgoing":false,"date":1,"is_channel_post":true,"interaction_info":{"@type":"messageInteractionInfo","view_count":5,"forward_count":0,"reply_info":{"@type":"messageReplyInfo","reply_count":2,"recent_replier_ids":[],"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"last_message_id":503},"reactions":null},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"post","entities":[]}}}}"#,
    );
    let reply_info = |session: &Session| {
        session
            .histories
            .get(&13)
            .and_then(|h| h.messages.get(&101))
            .and_then(|m| m.interaction_info.clone())
            .and_then(|info| info.reply_info)
            .expect("post reply info")
    };
    assert!(reply_info(&session).has_unread());
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
            r#"{{"@type":"messageThreadInfo","@extra":"{}","chat_id":14,"message_thread_id":501,"reply_info":{{"@type":"messageReplyInfo","reply_count":2,"recent_replier_ids":[],"last_read_inbox_message_id":501,"last_read_outbox_message_id":0,"last_message_id":503}},"unread_message_count":2,"messages":[{}]}}"#,
            extra.0,
            msg(501, 14, 501, "root")
        ),
    );
    session.open_chat(ChatId(14));
    let page = session.request(
        RequestPurpose::GetMessageThreadHistory { message_id: 101 },
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":3,"messages":[{},{},{}]}}"#,
            page.0,
            msg(503, 14, 501, "b"),
            msg(502, 14, 501, "a"),
            msg(501, 14, 501, "root")
        ),
    );
    {
        let thread = session.thread_for_chat(ChatId(14)).unwrap();
        assert!(!thread.history.has_newer);
        assert_eq!(thread.unread_count, 2);
    }
    // Both comments on screen: the thread is read, and the post's bar
    // loses its dot at once.
    session.report_visible_messages(ChatId(14), &[MessageId(502), MessageId(503)]);
    {
        let thread = session.thread_for_chat(ChatId(14)).unwrap();
        assert_eq!(thread.unread_count, 0);
        assert_eq!(thread.last_read_inbox_message_id, 503);
    }
    let info = reply_info(&session);
    assert_eq!(info.last_read_inbox_message_id, 503);
    assert!(!info.has_unread());
    // Two live comments arrive, then the server reports them read (another
    // device): the counter follows the read mark.
    for id in [504, 505] {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{}}}"#,
                msg(id, 14, 501, "live")
            ),
        );
    }
    assert_eq!(session.thread_for_chat(ChatId(14)).unwrap().unread_count, 2);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageInteractionInfo","chat_id":13,"message_id":101,"interaction_info":{"@type":"messageInteractionInfo","view_count":9,"forward_count":0,"reply_info":{"@type":"messageReplyInfo","reply_count":4,"recent_replier_ids":[],"last_read_inbox_message_id":505,"last_read_outbox_message_id":0,"last_message_id":505},"reactions":null}}"#,
    );
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    assert_eq!(thread.reply_count, 4);
    assert_eq!(thread.unread_count, 0);
    assert_eq!(thread.last_read_inbox_message_id, 505);
}

#[test]
fn the_replies_menu_entry_follows_tdesktop() {
    assert_eq!(replies_menu_label(0), "View Thread");
    assert_eq!(replies_menu_label(1), "View 1 Reply");
    assert_eq!(replies_menu_label(12), "View 12 Replies");
}

fn reply(id: i64) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{}}}"#,
        msg(id, 14, 40, "live")
    )
}

fn long_thread(last: i64) -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink, seq) = unread_group_thread();
    let ids: Vec<i64> = (40..=last).rev().collect();
    thread_page(&mut session, &seq, &sink, false, &ids);
    let thread = session.threads.thread.as_mut().unwrap();
    assert!(thread.history.loaded_complete, "reached the root");
    thread.unread_anchor = None;
    (session, sink, seq)
}

#[test]
fn a_thread_left_open_at_the_tail_stays_within_the_cap_and_keeps_its_root() {
    let cap = crate::state::HISTORY_WINDOW_CAP as i64;
    let (mut session, sink, seq) = long_thread(40 + cap - 1);
    for id in 40 + cap..=40 + cap + 3 {
        apply_json(&mut session, &seq, &sink, &reply(id));
    }
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    assert!(thread.history.messages.len() <= crate::state::HISTORY_WINDOW_CAP);
    assert!(thread.history.messages.contains_key(&40), "the pinned root");
    assert_eq!(thread.newest_loaded_id(), Some(MessageId(40 + cap + 3)));
    assert!(!thread.history.has_newer);
    assert!(
        !thread.history.loaded_complete,
        "older replies page back in"
    );
    let oldest_reply = thread.history.messages.keys().nth(1).copied().unwrap();
    assert_eq!(thread.history.next_from_message_id, MessageId(oldest_reply));
    assert_eq!(thread.ordered().first().map(|m| m.id.0), Some(oldest_reply));
}

#[test]
fn a_thread_reader_near_the_top_keeps_the_root_and_the_tail_waits() {
    let cap = crate::state::HISTORY_WINDOW_CAP as i64;
    let (mut session, sink, seq) = long_thread(40 + cap - 1);
    let key = (14, None, 40);
    session.note_history_reader(key, ReaderPosition::Away { top: MessageId(45) });
    apply_json(&mut session, &seq, &sink, &reply(40 + cap));
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    assert!(thread.history.loaded_complete, "the rows above stay");
    assert!(thread.history.has_newer, "paged back in below");
    let newest = thread.newest_loaded_id().unwrap();
    assert!(newest.0 < 40 + cap);
    apply_json(&mut session, &seq, &sink, &reply(40 + cap + 1));
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    assert_eq!(thread.newest_loaded_id(), Some(newest));
    assert_eq!(thread.last_message_id, 40 + cap + 1);
}

#[test]
fn a_page_on_the_way_past_a_live_trim_does_not_land_across_a_gap() {
    let cap = crate::state::HISTORY_WINDOW_CAP as i64;
    let (mut session, sink, seq) = long_thread(40 + cap - 1);
    session
        .threads
        .thread
        .as_mut()
        .unwrap()
        .history
        .loaded_complete = false;
    let older = session.request(
        RequestPurpose::GetMessageThreadHistory { message_id: 40 },
        Some(ChatId(14)),
    );
    apply_json(&mut session, &seq, &sink, &reply(40 + cap));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{}]}}"#,
            older.0,
            msg(41, 14, 40, "from before the trim")
        ),
    );
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    assert!(!thread.history.messages.contains_key(&41));
}

#[test]
fn reading_far_back_and_forth_through_a_thread_keeps_its_window_bounded() {
    let cap = crate::state::HISTORY_WINDOW_CAP as i64;
    let (mut session, sink, seq) = unread_group_thread();
    let top = 5_000 + cap - 1;
    let ids: Vec<i64> = (5_000..top).rev().collect();
    thread_page(&mut session, &seq, &sink, false, &ids);
    session.threads.thread.as_mut().unwrap().unread_anchor = None;

    let older: Vec<i64> = (4_800..5_000).rev().chain([40]).collect();
    thread_page(&mut session, &seq, &sink, false, &older);
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    assert!(thread.history.messages.len() < crate::state::HISTORY_WINDOW_CAP);
    assert_eq!(thread.ordered().first().map(|m| m.id.0), Some(40));
    assert!(thread.history.loaded_complete);
    assert_eq!(thread.last_message_id, top - 1, "the end is remembered");
    assert!(thread.history.has_newer, "scrolling down pages it back in");
    let newest = thread.newest_loaded_id().unwrap().0;

    let newer: Vec<i64> = (newest..top).rev().collect();
    thread_page(&mut session, &seq, &sink, true, &newer);
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    assert!(thread.history.messages.len() < crate::state::HISTORY_WINDOW_CAP);
    assert!(thread.history.messages.contains_key(&40), "the pinned root");
    assert!(!thread.history.has_newer);
    assert!(!thread.history.loaded_complete);
    let oldest = thread.history.messages.keys().nth(1).copied().unwrap();
    assert_eq!(thread.history.next_from_message_id, MessageId(oldest));
    assert_eq!(thread.ordered().first().map(|m| m.id.0), Some(oldest));
    assert_eq!(thread.newest_loaded_id(), Some(MessageId(top - 1)));
}

#[test]
fn a_page_on_the_way_past_a_page_trim_does_not_land_across_a_gap() {
    let cap = crate::state::HISTORY_WINDOW_CAP as i64;
    let (mut session, sink, seq) = unread_group_thread();
    let top = 5_000 + cap - 1;
    let ids: Vec<i64> = (5_000..top).rev().collect();
    thread_page(&mut session, &seq, &sink, false, &ids);
    session.threads.thread.as_mut().unwrap().last_message_id = top + 100;
    let newer = session.request(
        RequestPurpose::Threads(ThreadsPurpose::GetMessageThreadHistoryNewer { message_id: 40 }),
        Some(ChatId(14)),
    );
    let older: Vec<i64> = (4_900..5_000).rev().collect();
    thread_page(&mut session, &seq, &sink, false, &older);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{}]}}"#,
            newer.0,
            msg(top, 14, 40, "from before the trim")
        ),
    );
    let thread = session.thread_for_chat(ChatId(14)).unwrap();
    assert!(!thread.history.messages.contains_key(&top));
}
