//! State reducer tests: histories of chats nobody is looking at are
//! bounded (`state::history_prune`).
use super::common::session;
use super::{
    CLOSED_CHATS_MAX, CLOSED_TAIL_CAP, CLOSED_TAIL_KEEP, CLOSED_TOPICS_MAX, ChannelMemberStatus,
    ChatId, MessageContent, MessageId, ParsedMessage, Session, TopicHistory, history_message,
};

fn live(chat: i64, id: i64) -> ParsedMessage {
    ParsedMessage {
        sender: None,
        id: MessageId(id),
        chat_id: ChatId(chat),
        date: 0,
        is_outgoing: false,
        is_pinned: false,
        topic_id: None,
        thread_id: None,
        ephemeral: None,
        media_album_id: 0,
        author_signature: None,
        scheduling_state: None,
        can_retry: false,
        send_state: Default::default(),
        content: MessageContent::Text("m".into()),
        files: Vec::new(),
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
    }
}

fn feed(session: &mut Session, chat: i64, count: i64) {
    for id in 1..=count {
        session.upsert_message(live(chat, id), false);
    }
}

fn topic_with(session: &mut Session, chat: i64, topic: i32, count: i64) {
    let mut history = TopicHistory::default();
    for id in 1..=count {
        history.upsert(history_message(live(chat, id), false));
    }
    history.loaded_complete = true;
    session
        .threads
        .topic_histories
        .insert((chat, topic), history);
}

fn len(session: &Session, chat: i64) -> usize {
    session
        .histories
        .get(&chat)
        .map_or(0, |history| history.messages.len())
}

fn is_closed_tail(session: &Session, chat: i64) -> bool {
    session
        .history_pruning
        .closed_chats()
        .any(|id| id == ChatId(chat))
}

#[test]
fn closing_a_chat_trims_it_to_a_tail_and_reopening_keeps_it() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    feed(&mut session, 7, 200);
    assert_eq!(len(&session, 7), 200, "an open chat keeps every row");
    session.open_chat(ChatId(8));
    assert_eq!(len(&session, 7), CLOSED_TAIL_KEEP);
    let history = &session.histories[&7];
    assert_eq!(history.newest_id(), Some(MessageId(200)));
    assert_eq!(history.oldest_id(), Some(MessageId(141)));
    assert!(!history.loaded_complete, "older rows page back in");
    assert!(is_closed_tail(&session, 7));
    session.open_chat(ChatId(7));
    assert_eq!(len(&session, 7), CLOSED_TAIL_KEEP, "shown from the tail");
    assert!(!session.history_loading(ChatId(7)));
    assert!(
        !is_closed_tail(&session, 7),
        "an open chat is no closed tail"
    );
}

#[test]
fn a_short_history_is_untouched_by_closing() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    feed(&mut session, 7, 10);
    session.leave_open_chat();
    assert_eq!(len(&session, 7), 10);
}

#[test]
fn leaving_a_chat_drops_its_history_once_nothing_shows_it() {
    let (mut session, sink) = session();
    let seq = std::sync::atomic::AtomicU64::new(0);
    super::common::apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":-100500,"title":"Gone","type":{"@type":"chatTypeSupergroup","supergroup_id":500,"is_channel":true},"unread_count":0}}"#,
    );
    session.open_chat(ChatId(-100500));
    feed(&mut session, -100500, 30);
    if let Some(chat) = session.chats.get_mut(&-100500) {
        chat.set_member_status(ChannelMemberStatus::Left, None);
    }
    session.open_chat(ChatId(8));
    assert!(!session.histories.contains_key(&-100500));
    assert!(!is_closed_tail(&session, -100500));
}

#[test]
fn a_deleted_chat_drops_history_and_topics() {
    let (mut session, _sink) = session();
    feed(&mut session, 7, 30);
    topic_with(&mut session, 7, 3, 30);
    session.forget_chat_history(ChatId(7));
    assert!(!session.histories.contains_key(&7));
    assert!(session.threads.topic_histories.is_empty());
}

#[test]
fn views_that_show_a_chat_keep_it_whole() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    feed(&mut session, 7, 200);
    // The chat also shows in a window of its own: leaving the main window
    // keeps every row.
    assert!(session.open_window_chat(ChatId(7)));
    session.open_chat(ChatId(8));
    assert_eq!(len(&session, 7), 200);
    // Closing the window is what releases it.
    assert!(session.close_window_chat(ChatId(7)));
    assert_eq!(len(&session, 7), CLOSED_TAIL_KEEP);
    // Live rows into a shown chat are never tail-trimmed.
    session.open_chat(ChatId(9));
    feed(&mut session, 9, 300);
    assert_eq!(len(&session, 9), 300);
}

#[test]
fn a_held_chat_is_never_pruned() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    feed(&mut session, 7, 200);
    session.set_history_holds([ChatId(7)]);
    session.open_chat(ChatId(8));
    assert_eq!(len(&session, 7), 200);
    // Live rows keep the page cap, not the closed tail.
    feed(&mut session, 7, 300);
    assert_eq!(len(&session, 7), 300);
    // Letting go makes it prunable at the next close, never before.
    session.set_history_holds([]);
    session.open_chat(ChatId(7));
    session.open_chat(ChatId(8));
    assert_eq!(len(&session, 7), CLOSED_TAIL_KEEP);
}

#[test]
fn live_rows_in_a_closed_chat_stay_bounded() {
    let (mut session, _sink) = session();
    let total = CLOSED_TAIL_CAP as i64 * 3;
    feed(&mut session, 7, total);
    assert!(len(&session, 7) <= CLOSED_TAIL_CAP);
    assert_eq!(session.histories[&7].newest_id(), Some(MessageId(total)));
}

#[test]
fn the_least_recently_used_closed_chat_is_dropped_past_the_bound() {
    let (mut session, _sink) = session();
    let total = CLOSED_CHATS_MAX as i64 + 4;
    for chat in 1..=total {
        session.open_chat(ChatId(chat));
        feed(&mut session, chat, 80);
    }
    session.leave_open_chat();
    let kept: Vec<ChatId> = session.history_pruning.closed_chats().collect();
    assert_eq!(kept.len(), CLOSED_CHATS_MAX);
    assert_eq!(kept.first(), Some(&ChatId(total)), "most recent first");
    for chat in 1..=4 {
        assert!(
            !session.histories.contains_key(&chat),
            "chat {chat} dropped"
        );
    }
    for chat in 5..=total {
        assert_eq!(len(&session, chat), CLOSED_TAIL_KEEP, "chat {chat} kept");
    }
    assert_eq!(session.histories.len(), CLOSED_CHATS_MAX);
}

#[test]
fn a_closed_chat_with_an_unsent_message_is_never_dropped_whole() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(1));
    session.upsert_message(live(1, -5), true);
    for chat in 2..=(CLOSED_CHATS_MAX as i64 + 6) {
        session.open_chat(ChatId(chat));
        feed(&mut session, chat, 80);
    }
    session.leave_open_chat();
    assert!(session.histories.contains_key(&1));
    assert!(session.histories[&1].messages.contains_key(&-5));
}

#[test]
fn recent_live_activity_keeps_a_chat_in_the_lru() {
    let (mut session, _sink) = session();
    for chat in 1..=CLOSED_CHATS_MAX as i64 {
        session.open_chat(ChatId(chat));
        feed(&mut session, chat, 80);
    }
    session.leave_open_chat();
    // Chat 1 is the oldest; a live message makes it the newest.
    session.upsert_message(live(1, 81), false);
    let extra = CLOSED_CHATS_MAX as i64 + 1;
    session.open_chat(ChatId(extra));
    feed(&mut session, extra, 80);
    session.leave_open_chat();
    assert!(session.histories.contains_key(&1));
    assert!(
        !session.histories.contains_key(&2),
        "the oldest idle chat goes"
    );
}

#[test]
fn leaving_a_topic_trims_it_and_the_open_topic_stays_whole() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    topic_with(&mut session, 7, 1, 200);
    topic_with(&mut session, 7, 2, 200);
    session.select_topic(ChatId(7), 1);
    assert_eq!(session.threads.topic_histories[&(7, 1)].messages.len(), 200);
    session.select_topic(ChatId(7), 2);
    let left = &session.threads.topic_histories[&(7, 1)];
    assert_eq!(left.messages.len(), CLOSED_TAIL_KEEP);
    assert!(!left.loaded_complete);
    assert_eq!(left.next_from_message_id, MessageId(141), "paging resumes");
    assert_eq!(session.threads.topic_histories[&(7, 2)].messages.len(), 200);
    session.deselect_topic();
    assert_eq!(
        session.threads.topic_histories[&(7, 2)].messages.len(),
        CLOSED_TAIL_KEEP
    );
    // Reselecting reclaims it.
    session.select_topic(ChatId(7), 1);
    assert!(
        !session
            .history_pruning
            .closed_topics()
            .any(|key| key == (ChatId(7), 1))
    );
}

#[test]
fn closing_a_chat_shrinks_its_topics_and_the_topic_lru_is_bounded() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    let topics = CLOSED_TOPICS_MAX as i32 + 3;
    for topic in 1..=topics {
        topic_with(&mut session, 7, topic, 100);
    }
    session.open_chat(ChatId(8));
    assert_eq!(session.threads.topic_histories.len(), CLOSED_TOPICS_MAX);
    assert!(
        session
            .threads
            .topic_histories
            .values()
            .all(|history| history.messages.len() == CLOSED_TAIL_KEEP)
    );
}
