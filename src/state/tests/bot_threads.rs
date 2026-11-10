//! Reducer tests: a send from the "All" view of a bot whose users create
//! threads, and the "New Thread" card under its history.
use super::common::*;
use super::*;

fn bot_session(has_topics: bool, allows: bool) -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for json in [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":41,"first_name":"Bot","type":{{"@type":"userTypeBot","has_topics":{has_topics},"allows_users_to_create_topics":{allows},"is_inline":false}}}}}}"#
        ),
        r#"{"@type":"updateNewChat","chat":{"id":41,"title":"Bot","type":{"@type":"chatTypePrivate","user_id":41},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":7,"chat_id":41,"is_outgoing":false,"date":1700000007,"topic_id":{"@type":"messageTopicForum","forum_topic_id":3},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"in a thread","entities":[]}}}}"#.to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":8,"chat_id":41,"is_outgoing":false,"date":1700000008,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"off-thread","entities":[]}}}}"#.to_string(),
    ] {
        apply_json(&mut session, &seq, &sink, &json);
    }
    session.open_chat(ChatId(41));
    (session, sink, seq)
}

#[test]
fn a_send_from_all_starts_a_thread_unless_the_reply_is_in_one() {
    let (mut session, _, _) = bot_session(true, true);
    assert!(session.send_starts_bot_thread(ChatId(41), None));
    assert!(session.send_starts_bot_thread(ChatId(41), Some(MessageId(8))));
    // The reply's thread takes the message.
    assert!(!session.send_starts_bot_thread(ChatId(41), Some(MessageId(7))));
    session.open_topic = Some(3);
    assert!(!session.send_starts_bot_thread(ChatId(41), None));
}

#[test]
fn only_bots_whose_users_create_threads_start_one() {
    let (makes_its_own, _, _) = bot_session(true, false);
    assert!(!makes_its_own.send_starts_bot_thread(ChatId(41), None));
    assert!(makes_its_own.new_bot_thread_row(ChatId(41)).is_none());
    let (plain, _, _) = bot_session(false, false);
    assert!(!plain.send_starts_bot_thread(ChatId(41), None));
    assert!(plain.new_bot_thread_row(ChatId(41)).is_none());
}

#[test]
fn the_card_follows_the_newest_message_and_waits_for_a_start_parameter() {
    let (mut session, _, _) = bot_session(true, true);
    let row = session.new_bot_thread_row(ChatId(41)).unwrap();
    assert_eq!(row.id, MessageId(NEW_BOT_THREAD_ROW_ID));
    assert_eq!(row.date, 1700000008);
    session.bots.bot_start_params.insert(41, "ref".to_string());
    assert!(session.new_bot_thread_row(ChatId(41)).is_none());
}

#[test]
fn the_thread_answer_joins_the_tabs_and_waits_for_its_window() {
    let (mut session, sink, seq) = bot_session(true, true);
    session.threads.forum_topics.insert(41, Vec::new());
    let extra = session.request(RequestPurpose::StartBotThread, Some(ChatId(41)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"forumTopicInfo","@extra":"{}","chat_id":41,"forum_topic_id":12,"name":"New Chat","icon":{{"@type":"forumTopicIcon","color":7322096,"custom_emoji_id":"0"}},"is_general":false,"is_outgoing":true,"is_closed":false,"is_hidden":false,"is_name_implicit":true}}"#,
            extra.0
        ),
    );
    let topics = session.threads.forum_topics.get(&41).unwrap();
    assert!(
        topics
            .iter()
            .any(|t| t.forum_topic_id == 12 && t.name == "New Chat")
    );
    assert_eq!(session.take_bot_thread_start(extra), Some(Ok(12)));
    assert_eq!(session.take_bot_thread_start(extra), None);
}

#[test]
fn a_failed_thread_is_answered_with_why() {
    let (mut session, sink, seq) = bot_session(true, true);
    let extra = session.request(RequestPurpose::StartBotThread, Some(ChatId(41)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"TOPICS_TOO_MUCH"}}"#,
            extra.0
        ),
    );
    assert!(matches!(session.take_bot_thread_start(extra), Some(Err(_))));
}
