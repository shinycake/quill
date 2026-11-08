//! Connect-driver tests: a reply thread or Saved Messages view open in a
//! chat window gets the live updates the main window's would (round 11,
//! area A): new replies, the own reply's server id, edits, deletions and
//! poll votes.
use super::chat_window::{BOB, Harness};
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::ThreadView;
use crate::telegram::envelope::MessageContent;

const GROUP: ChatId = ChatId(14);

fn reply_json(id: i64, text: &str, extra: &str) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":14,"is_outgoing":false,"date":1700000000,"topic_id":{{"@type":"messageTopicThread","message_thread_id":40}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}{extra}}}"#
    )
}

const POLL: &str = r#"{"@type":"messagePoll","poll":{"@type":"poll","id":9001,"question":{"@type":"formattedText","text":"Lunch?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Sushi","entities":[]},"voter_count":1,"vote_percentage":50,"is_chosen":false},{"@type":"pollOption","id":"b","text":{"@type":"formattedText","text":"Pizza","entities":[]},"voter_count":1,"vote_percentage":50,"is_chosen":false}],"total_voter_count":2,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{"@type":"pollTypeRegular"}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}"#;

fn poll_reply_json(id: i64) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":14,"is_outgoing":false,"date":1700000000,"topic_id":{{"@type":"messageTopicThread","message_thread_id":40}},"content":{POLL}}}"#
    )
}

/// Bob in the main window; the group in its own window with the reply
/// thread of message 40 open there, its first page holding replies 41
/// and the poll 42.
fn thread_in_group_window() -> Harness {
    let mut h = Harness::two_chats();
    h.ingest(r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Group","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#);
    h.driver.select_chat(BOB).unwrap();
    h.driver.open_chat_window(GROUP).unwrap();
    let info = h.driver.with_window_chat(GROUP, |driver| {
        driver
            .open_thread(GROUP, MessageId(40))
            .unwrap()
            .expect("getMessageThread")
    });
    h.ingest(&format!(
        r#"{{"@type":"messageThreadInfo","@extra":"{}","chat_id":14,"message_thread_id":40,"reply_info":{{"@type":"messageReplyInfo","reply_count":2,"recent_replier_ids":[],"last_read_inbox_message_id":42,"last_read_outbox_message_id":0,"last_message_id":42}},"unread_message_count":0,"messages":[{}]}}"#,
        info.0,
        reply_json(40, "root", "")
    ));
    let page: RequestId = h.driver.with_window_chat(GROUP, |driver| {
        driver
            .start_thread_in_open_chat()
            .unwrap()
            .expect("first page")
    });
    h.ingest(&format!(
        r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{},{}]}}"#,
        page.0,
        poll_reply_json(42),
        reply_json(41, "first", ""),
    ));
    assert!(h.driver.session.threads.thread.is_none(), "main shows Bob");
    assert!(window_thread(&mut h, |thread| thread
        .history
        .messages
        .contains_key(&41)));
    h
}

/// Read the group window's thread.
fn window_thread<R>(h: &mut Harness, f: impl FnOnce(&ThreadView) -> R) -> R {
    h.driver
        .session
        .with_view(Some(GROUP), |session| {
            f(session.threads.thread.as_ref().expect("window thread"))
        })
        .expect("window open")
}

fn row_text(h: &mut Harness, id: i64) -> Option<String> {
    window_thread(h, |thread| {
        thread
            .history
            .messages
            .get(&id)
            .map(|message| match &message.content {
                MessageContent::Text(text) => text.text.clone(),
                _ => String::new(),
            })
    })
}

#[test]
fn a_new_reply_lands_in_the_chat_windows_thread() {
    let mut h = thread_in_group_window();
    h.ingest(&format!(
        r#"{{"@type":"updateNewMessage","message":{}}}"#,
        reply_json(43, "live", "")
    ));
    assert_eq!(row_text(&mut h, 43).as_deref(), Some("live"));
}

#[test]
fn an_own_reply_from_the_chat_windows_thread_gets_its_server_id() {
    let mut h = thread_in_group_window();
    let pending = r#","sending_state":{"@type":"messageSendingStatePending","sending_id":0}"#;
    let sending =
        reply_json(-5, "mine", pending).replace(r#""is_outgoing":false"#, r#""is_outgoing":true"#);
    h.ingest(&format!(
        r#"{{"@type":"updateNewMessage","message":{sending}}}"#
    ));
    assert!(window_thread(&mut h, |thread| thread
        .history
        .messages
        .get(&-5)
        .is_some_and(|message| message.pending())));
    let sent =
        reply_json(44, "mine", "").replace(r#""is_outgoing":false"#, r#""is_outgoing":true"#);
    h.ingest(&format!(
        r#"{{"@type":"updateMessageSendSucceeded","message":{sent},"old_message_id":-5}}"#
    ));
    window_thread(&mut h, |thread| {
        assert!(
            !thread.history.messages.contains_key(&-5),
            "temporary row gone"
        );
        assert!(
            thread
                .history
                .messages
                .get(&44)
                .is_some_and(|message| !message.pending()),
            "the sent reply shows as sent"
        );
    });
}

#[test]
fn edits_and_deletes_reach_the_chat_windows_thread() {
    let mut h = thread_in_group_window();
    h.ingest(r#"{"@type":"updateMessageContent","chat_id":14,"message_id":41,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"edited","entities":[]}}}"#);
    assert_eq!(row_text(&mut h, 41).as_deref(), Some("edited"));
    h.ingest(r#"{"@type":"updateDeleteMessages","chat_id":14,"message_ids":[41],"is_permanent":true,"from_cache":false}"#);
    assert_eq!(row_text(&mut h, 41), None);
}

fn chosen(h: &mut Harness) -> Vec<bool> {
    window_thread(h, |thread| match &thread.history.messages[&42].content {
        MessageContent::Poll(poll) => poll.poll.options.iter().map(|o| o.is_chosen).collect(),
        _ => Vec::new(),
    })
}

#[test]
fn a_poll_in_a_chat_windows_thread_can_be_voted_on() {
    let mut h = thread_in_group_window();
    assert!(
        !h.driver
            .session
            .histories
            .get(&GROUP.0)
            .is_some_and(|history| history.messages.contains_key(&42)),
        "the poll lives only in the thread"
    );
    let vote = h.driver.with_window_chat(GROUP, |driver| {
        driver.send_poll_answer(GROUP, MessageId(42), 1)
    });
    assert!(vote.is_ok(), "the vote goes out: {vote:?}");
    assert_eq!(h.sent("setPollAnswer", GROUP).len(), 1);
    assert_eq!(chosen(&mut h), vec![false, true], "the mark flips at once");
    // The server's count lands in the thread's copy too.
    h.ingest(&format!(
        r#"{{"@type":"updatePoll","poll":{}}}"#,
        POLL.split_once(r#""poll":"#)
            .and_then(|(_, rest)| rest.split_once(r#","description""#))
            .map(|(poll, _)| poll.replace(r#""total_voter_count":2"#, r#""total_voter_count":3"#))
            .expect("poll json")
    ));
    let total = window_thread(&mut h, |thread| {
        match &thread.history.messages[&42].content {
            MessageContent::Poll(poll) => poll.poll.total_voter_count,
            _ => 0,
        }
    });
    assert_eq!(total, 3);
}

#[test]
fn a_refused_vote_in_a_chat_windows_thread_rolls_back_there() {
    let mut h = thread_in_group_window();
    let vote = h
        .driver
        .with_window_chat(GROUP, |driver| {
            driver.send_poll_answer(GROUP, MessageId(42), 0)
        })
        .expect("vote sent");
    assert_eq!(chosen(&mut h), vec![true, false]);
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"POLL_CLOSED"}}"#,
        vote.0
    ));
    assert_eq!(chosen(&mut h), vec![false, false]);
}

/// A poll row held only by a loaded forum topic (older than the chat's
/// loaded window) is found, voted on and updated there.
#[test]
fn a_poll_in_a_forum_topic_can_be_voted_on() {
    let mut h = Harness::two_chats();
    h.ingest(r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#);
    h.driver
        .session
        .threads
        .topic_histories
        .insert((16, 4), crate::state::TopicHistory::default());
    h.ingest(&format!(
        r#"{{"@type":"updateNewMessage","message":{{"@type":"message","id":70,"chat_id":16,"is_outgoing":false,"date":1700000000,"topic_id":{{"@type":"messageTopicForum","forum_topic_id":4}},"content":{POLL}}}}}"#
    ));
    // Only the topic keeps the row.
    if let Some(history) = h.driver.session.histories.get_mut(&16) {
        history.messages.remove(&70);
    }
    let topic_marks = |h: &Harness| -> Vec<bool> {
        match &h.driver.session.threads.topic_histories[&(16, 4)].messages[&70].content {
            MessageContent::Poll(poll) => poll.poll.options.iter().map(|o| o.is_chosen).collect(),
            _ => Vec::new(),
        }
    };
    let vote = h.driver.send_poll_answer(ChatId(16), MessageId(70), 0);
    assert!(vote.is_ok(), "the vote goes out: {vote:?}");
    assert_eq!(topic_marks(&h), vec![true, false]);
}
