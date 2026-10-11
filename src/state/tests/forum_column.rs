//! Reducer tests: the forum topic column and threads opened inside topics.
use super::common::*;
use super::*;
use crate::state::session_chat_search::history_message;
use crate::telegram::envelope::ParsedMessage;

fn forum_session() -> Session {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":20,"title":"Group","type":{"@type":"chatTypeSupergroup","supergroup_id":20,"is_channel":false},"unread_count":0}}"#,
    );
    session
}

fn topic_message(id: i64, topic: i32, reply_to: i64) -> ParsedMessage {
    let json = format!(
        r#"{{"id":{id},"chat_id":16,"is_outgoing":false,"topic_id":{{"@type":"messageTopicForum","forum_topic_id":{topic}}},"reply_to":{{"@type":"messageReplyToMessage","chat_id":16,"message_id":{reply_to}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}}"#
    );
    crate::telegram::envelope::parse_message(&serde_json::from_str(&json).unwrap()).unwrap()
}

#[test]
fn column_sits_beside_the_list_on_wide_windows_and_replaces_it_on_narrow_ones() {
    let mut session = forum_session();
    assert_eq!(session.forum_column(1400., false), ForumColumn::Hidden);
    session.open_chat(ChatId(16));
    assert_eq!(session.forum_column(1400., false), ForumColumn::Beside);
    assert_eq!(
        session.forum_column(FORUM_COLUMN_COLLAPSE_BELOW, false),
        ForumColumn::Beside
    );
    assert_eq!(session.forum_column(900., false), ForumColumn::Replacing);
    // "Chats" on a narrow window brings the list back, topics fill the pane.
    assert_eq!(session.forum_column(900., true), ForumColumn::Hidden);
    assert_eq!(session.forum_column(1400., true), ForumColumn::Beside);
}

#[test]
fn plain_groups_have_no_column() {
    let mut session = forum_session();
    session.open_chat(ChatId(20));
    assert_eq!(session.forum_column(1400., false), ForumColumn::Hidden);
}

#[test]
fn column_follows_the_view_as_messages_choice() {
    let mut session = forum_session();
    session.open_chat(ChatId(16));
    session.set_chat_view_as_topics(16, false);
    assert_eq!(session.forum_column(1400., false), ForumColumn::Hidden);
    session.set_chat_view_as_topics(16, true);
    assert_eq!(session.forum_column(1400., false), ForumColumn::Beside);
}

#[test]
fn selecting_a_topic_keeps_the_column() {
    let mut session = forum_session();
    session.open_chat(ChatId(16));
    session.select_topic(ChatId(16), 7);
    assert_eq!(session.forum_column(1400., false), ForumColumn::Beside);
    session.deselect_topic();
    assert_eq!(session.forum_column(1400., false), ForumColumn::Beside);
}

#[test]
fn a_thread_opened_in_a_topic_remembers_the_topic() {
    let mut session = forum_session();
    session.open_chat(ChatId(16));
    session.select_topic(ChatId(16), 7);
    session.begin_thread(ChatId(16), MessageId(120));
    assert_eq!(
        session.threads.thread.as_ref().unwrap().forum_topic_id,
        Some(7)
    );
    // Outside a forum there is no topic.
    session.open_chat(ChatId(20));
    session.begin_thread(ChatId(20), MessageId(5));
    assert_eq!(
        session.threads.thread.as_ref().unwrap().forum_topic_id,
        None
    );
}

#[test]
fn topic_thread_accepts_replies_to_replies_but_not_other_topics() {
    let mut session = forum_session();
    session.open_chat(ChatId(16));
    session.select_topic(ChatId(16), 7);
    session.begin_thread(ChatId(16), MessageId(120));
    {
        let thread = session.threads.thread.as_mut().unwrap();
        thread.thread_id = 120;
        thread
            .history
            .upsert(history_message(topic_message(130, 7, 120), false));
    }
    assert!(session.thread_accepts(&topic_message(131, 7, 130)));
    assert!(session.thread_accepts(&topic_message(132, 7, 120)));
    assert!(!session.thread_accepts(&topic_message(133, 9, 130)));
}

#[test]
fn leaving_or_switching_the_topic_closes_its_thread() {
    let mut session = forum_session();
    session.open_chat(ChatId(16));
    session.select_topic(ChatId(16), 7);
    session.begin_thread(ChatId(16), MessageId(120));
    session.select_topic(ChatId(16), 7);
    assert!(session.threads.thread.is_some(), "same topic keeps it");
    session.select_topic(ChatId(16), 9);
    assert!(session.threads.thread.is_none());
    session.begin_thread(ChatId(16), MessageId(121));
    session.deselect_topic();
    assert!(session.threads.thread.is_none());
}
