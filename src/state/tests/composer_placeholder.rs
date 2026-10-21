//! Reducer tests: the topic and the bot state the compose field's
//! placeholder is derived from.
use super::common::*;
use super::*;
use crate::composer::TopicPlaceholder;
use crate::state::session_chat_search::history_message;

const FORUM: i64 = 16;

fn topic_json(chat: i64, id: i32, name: &str, general: bool) -> String {
    format!(
        r#"{{"info":{{"@type":"forumTopicInfo","chat_id":{chat},"forum_topic_id":{id},"name":"{name}","icon":{{"@type":"forumTopicIcon","color":16766590,"custom_emoji_id":"0"}},"is_general":{general},"is_closed":false,"is_hidden":false}},"order":"{id}","is_pinned":false,"unread_count":0,"last_read_inbox_message_id":0,"notification_settings":{{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0}}}}"#
    )
}

fn forum_session(topics: &[(i32, &str, bool)]) -> Session {
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
    let list: Vec<String> = topics
        .iter()
        .map(|(id, name, general)| topic_json(FORUM, *id, name, *general))
        .collect();
    let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(FORUM)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"forumTopics","@extra":"{}","total_count":{},"topics":[{}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
            extra.0,
            topics.len(),
            list.join(",")
        ),
    );
    session
}

fn placeholder(title: &str, reply: bool) -> Option<TopicPlaceholder> {
    Some(TopicPlaceholder {
        title: title.into(),
        reply,
    })
}

fn bot_session(has_topics: bool, allows: bool) -> Session {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateUser","user":{{"id":41,"first_name":"Bot","type":{{"@type":"userTypeBot","has_topics":{has_topics},"allows_users_to_create_topics":{allows},"is_inline":false}}}}}}"#
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":41,"title":"Bot","type":{"@type":"chatTypePrivate","user_id":41},"unread_count":0}}"#,
    );
    session
}

#[test]
fn an_open_topic_keeps_the_plain_placeholder() {
    let mut session = forum_session(&[(1, "General", true), (5, "Releases", false)]);
    session.open_chat(ChatId(FORUM));
    session.open_topic = Some(5);
    assert_eq!(session.composer_topic(ChatId(FORUM), None), None);
    assert_eq!(
        session.composer_topic(ChatId(FORUM), Some(MessageId(90))),
        None
    );
}

#[test]
fn a_forum_without_a_topic_open_names_general() {
    let mut session = forum_session(&[(1, "General", true), (5, "Releases", false)]);
    session.open_chat(ChatId(FORUM));
    assert_eq!(
        session.composer_topic(ChatId(FORUM), None),
        placeholder("General", false)
    );
    // Before the topics arrive, the name is the fallback.
    let mut empty = forum_session(&[]);
    empty.open_chat(ChatId(FORUM));
    assert_eq!(
        empty.composer_topic(ChatId(FORUM), None),
        placeholder("General", false)
    );
}

fn parsed_message(id: i64, topic: Option<i32>) -> crate::telegram::envelope::ParsedMessage {
    let topic_id = topic.map_or(String::new(), |topic| {
        format!(r#""topic_id":{{"@type":"messageTopicForum","forum_topic_id":{topic}}},"#)
    });
    let json = format!(
        r#"{{"id":{id},"chat_id":{FORUM},{topic_id}"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}}"#
    );
    crate::telegram::envelope::parse_message(&serde_json::from_str(&json).unwrap()).unwrap()
}

/// Puts a message of the forum's main history in the session, in `topic` when given.
fn main_history_message(session: &mut Session, id: i64, topic: Option<i32>) -> MessageId {
    let history = session.histories.entry(FORUM).or_default();
    history
        .messages
        .insert(id, history_message(parsed_message(id, topic), false));
    MessageId(id)
}

#[test]
fn an_unnamed_topic_is_called_topic() {
    let mut session = forum_session(&[(7, "  ", false)]);
    session.open_chat(ChatId(FORUM));
    let named_blank = main_history_message(&mut session, 91, Some(7));
    assert_eq!(
        session.composer_topic(ChatId(FORUM), Some(named_blank)),
        placeholder("Topic", true)
    );
    let unknown = main_history_message(&mut session, 92, Some(9));
    assert_eq!(
        session.composer_topic(ChatId(FORUM), Some(unknown)),
        placeholder("Topic", true)
    );
}

#[test]
fn a_reply_names_the_topic_of_the_message_itself() {
    let mut session = forum_session(&[(1, "General", true), (5, "Releases", false)]);
    session.open_chat(ChatId(FORUM));
    let in_topic = main_history_message(&mut session, 90, Some(5));
    assert_eq!(
        session.composer_topic(ChatId(FORUM), Some(in_topic)),
        placeholder("Releases", true)
    );
    // A message with no topic of its own is in General.
    let plain = main_history_message(&mut session, 93, None);
    assert_eq!(
        session.composer_topic(ChatId(FORUM), Some(plain)),
        placeholder("General", true)
    );
}

#[test]
fn a_loaded_topic_history_names_a_reply_target_without_a_topic() {
    let mut session = forum_session(&[(1, "General", true), (5, "Releases", false)]);
    session.open_chat(ChatId(FORUM));
    let mut topic = TopicHistory::default();
    topic
        .messages
        .insert(94, history_message(parsed_message(94, None), false));
    session.threads.topic_histories.insert((FORUM, 5), topic);
    assert_eq!(
        session.composer_topic(ChatId(FORUM), Some(MessageId(94))),
        placeholder("Releases", true)
    );
    // A message found nowhere is sent to General.
    assert_eq!(
        session.composer_topic(ChatId(FORUM), Some(MessageId(95))),
        placeholder("General", true)
    );
}

#[test]
fn other_chats_and_threads_have_no_topic() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":20,"title":"Group","type":{"@type":"chatTypeSupergroup","supergroup_id":20,"is_channel":false},"unread_count":0}}"#,
    );
    assert_eq!(session.composer_topic(ChatId(20), None), None);
    assert_eq!(session.composer_topic(ChatId(99), None), None);

    let mut forum = forum_session(&[(1, "General", true)]);
    forum.open_chat(ChatId(FORUM));
    let mut thread = ThreadView::resolving(ChatId(FORUM), MessageId(3));
    thread.status = ThreadStatus::Ready;
    thread.chat_id = ChatId(FORUM);
    thread.thread_id = 3;
    forum.threads.thread = Some(thread);
    assert_eq!(forum.composer_topic(ChatId(FORUM), None), None);
}

#[test]
fn a_bot_that_makes_its_own_topics_is_off_thread_outside_them() {
    let mut session = bot_session(true, false);
    session.open_chat(ChatId(41));
    assert!(session.bot_chat_off_thread(ChatId(41)));
    session.open_topic = Some(2);
    assert!(!session.bot_chat_off_thread(ChatId(41)));
}

#[test]
fn other_bots_are_never_off_thread() {
    let mut plain = bot_session(false, false);
    plain.open_chat(ChatId(41));
    assert!(!plain.bot_chat_off_thread(ChatId(41)));
    let mut users_make_topics = bot_session(true, true);
    users_make_topics.open_chat(ChatId(41));
    assert!(!users_make_topics.bot_chat_off_thread(ChatId(41)));
    assert!(!users_make_topics.bot_chat_off_thread(ChatId(77)));
}
