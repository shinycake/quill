//! State reducer tests: histories of chats nobody is looking at are
//! bounded (`state::history_prune`).
use super::common::{AtomicU64, MemorySink, apply_json, session};
use super::{
    Arc, CLOSED_CHATS_MAX, CLOSED_TAIL_CAP, CLOSED_TAIL_KEEP, CLOSED_TOPICS_MAX,
    ChannelMemberStatus, ChatId, HashMap, InfoPanelTarget, MemberListFilter, MessageContent,
    MessageId, ParsedMessage, ReplyTarget, RequestPurpose, Session, SupergroupMembersFetch,
    TopicHistory, TranslateTarget, Translation, history_message,
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

/// The frame settles the release queue once the UI's claims are in
/// (`Session::set_history_holds`); a test without claims does the same.
fn settle(session: &mut Session) {
    let holds: Vec<ChatId> = session.history_pruning.held_chats().collect();
    session.set_history_holds(holds);
}

fn switch(session: &mut Session, chat: i64) {
    session.open_chat(ChatId(chat));
    settle(session);
}

fn leave(session: &mut Session) {
    session.leave_open_chat();
    settle(session);
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
    switch(&mut session, 7);
    feed(&mut session, 7, 200);
    assert_eq!(len(&session, 7), 200, "an open chat keeps every row");
    switch(&mut session, 8);
    assert_eq!(len(&session, 7), CLOSED_TAIL_KEEP);
    let history = &session.histories[&7];
    assert_eq!(history.newest_id(), Some(MessageId(200)));
    assert_eq!(history.oldest_id(), Some(MessageId(141)));
    assert!(!history.loaded_complete, "older rows page back in");
    assert!(is_closed_tail(&session, 7));
    switch(&mut session, 7);
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
    switch(&mut session, 7);
    feed(&mut session, 7, 10);
    leave(&mut session);
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
    switch(&mut session, -100500);
    feed(&mut session, -100500, 30);
    if let Some(chat) = session.chats.get_mut(&-100500) {
        chat.set_member_status(ChannelMemberStatus::Left, None);
    }
    switch(&mut session, 8);
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
    switch(&mut session, 7);
    feed(&mut session, 7, 200);
    // The chat also shows in a window of its own: leaving the main window
    // keeps every row.
    assert!(session.open_window_chat(ChatId(7)));
    switch(&mut session, 8);
    assert_eq!(len(&session, 7), 200);
    // Closing the window is what releases it.
    assert!(session.close_window_chat(ChatId(7)));
    settle(&mut session);
    assert_eq!(len(&session, 7), CLOSED_TAIL_KEEP);
    // Live rows into a shown chat are never tail-trimmed.
    switch(&mut session, 9);
    feed(&mut session, 9, 300);
    assert_eq!(len(&session, 9), 300);
}

#[test]
fn a_held_chat_is_never_pruned() {
    let (mut session, _sink) = session();
    switch(&mut session, 7);
    feed(&mut session, 7, 200);
    session.set_history_holds([ChatId(7)]);
    switch(&mut session, 8);
    assert_eq!(len(&session, 7), 200);
    // Live rows keep the page cap, not the closed tail.
    feed(&mut session, 7, 300);
    assert_eq!(len(&session, 7), 300);
    // Letting go releases the chat that closed while held, and puts it in
    // the LRU.
    session.set_history_holds([]);
    assert_eq!(len(&session, 7), CLOSED_TAIL_KEEP);
    assert!(is_closed_tail(&session, 7));
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
        switch(&mut session, chat);
        feed(&mut session, chat, 80);
    }
    leave(&mut session);
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

fn fetched_reply(session: &Session, chat: i64, row: i64) -> bool {
    session.messages.reply_targets.contains_key(&(chat, row))
}

#[test]
fn a_kept_tail_keeps_the_replied_messages_of_rows_paged_out() {
    let (mut session, _sink) = session();
    switch(&mut session, 7);
    feed(&mut session, 7, 200);
    // Rows 10 and 190 reply to messages outside the window; 50's original
    // is gone.
    session.accept_replied_message(ChatId(7), MessageId(10), live(7, 3));
    session.accept_replied_message(ChatId(7), MessageId(190), live(7, 4));
    session.reject_replied_message(ChatId(7), MessageId(50));
    switch(&mut session, 8);
    assert_eq!(len(&session, 7), CLOSED_TAIL_KEEP);
    // Coming back scrolled up pages rows 10 and 50 in again: their strips
    // show at once instead of "Loading".
    assert!(fetched_reply(&session, 7, 10));
    assert!(matches!(
        session.messages.reply_targets.get(&(7, 50)),
        Some(ReplyTarget::Missing)
    ));
    assert!(fetched_reply(&session, 7, 190));
}

#[test]
fn a_dropped_chat_forgets_its_pinned_list_and_replied_messages() {
    let (mut session, _sink) = session();
    let total = CLOSED_CHATS_MAX as i64 + 1;
    for chat in 1..=total {
        switch(&mut session, chat);
        feed(&mut session, chat, 80);
        // Row 5 leaves with the tail trim, row 80 stays in the tail.
        session.accept_replied_message(ChatId(chat), MessageId(5), live(chat, 1));
        session.accept_replied_message(ChatId(chat), MessageId(80), live(chat, 2));
        let pinned = session.histories[&chat].messages[&80].clone();
        session.messages.pinned_messages.insert(chat, vec![pinned]);
    }
    leave(&mut session);
    assert!(!session.histories.contains_key(&1), "the oldest went");
    assert!(!session.messages.pinned_messages.contains_key(&1));
    assert!(!fetched_reply(&session, 1, 5));
    assert!(!fetched_reply(&session, 1, 80));
    assert!(session.messages.pinned_messages.contains_key(&2));
    assert!(fetched_reply(&session, 2, 5), "a kept tail keeps them");
    assert!(fetched_reply(&session, 2, 80));
}

fn translate(session: &mut Session, chat: i64, message: i64) {
    let target = TranslateTarget::Message {
        chat_id: chat,
        message_id: message,
    };
    let (job, _) = session.begin_translation(target, "en", Some(ChatId(chat)));
    let done = Translation::Done {
        text: "t".into(),
        entities: Vec::new(),
    };
    session.finish_translation(job, done);
}

fn translated(session: &Session, chat: i64, message: i64) -> bool {
    session
        .message_translation(ChatId(chat), MessageId(message), "en")
        .is_some()
}

#[test]
fn a_dropped_chat_forgets_its_translations() {
    let (mut session, _sink) = session();
    let total = CLOSED_CHATS_MAX as i64 + 1;
    for chat in 1..=total {
        switch(&mut session, chat);
        feed(&mut session, chat, 80);
        session.set_chat_translated_to(ChatId(chat), Some("en"));
        // Row 5 leaves with the tail trim, row 80 stays in the tail.
        translate(&mut session, chat, 5);
        translate(&mut session, chat, 80);
    }
    // Chat 1's answer for row 7 is still on its way when the chat goes.
    let target = TranslateTarget::Message {
        chat_id: 1,
        message_id: 7,
    };
    let (late, _) = session.begin_translation(target, "en", Some(ChatId(1)));
    leave(&mut session);
    assert!(!session.histories.contains_key(&1), "the oldest went");
    assert!(!translated(&session, 1, 5));
    assert!(!translated(&session, 1, 80));
    assert!(translated(&session, 2, 5), "a kept tail keeps them");
    assert!(translated(&session, 2, 80));
    session.finish_translation(late, Translation::Failed("late".into()));
    assert!(!translated(&session, 1, 7), "a late answer finds no job");
    assert_eq!(
        session.chat_translated_to(ChatId(1)),
        Some("en"),
        "the user's choice of language stays"
    );
}

#[test]
fn a_forum_keeps_its_replied_messages_until_its_last_topic_goes() {
    let (mut session, _sink) = session();
    session.topic_history_entry(ChatId(70), 1);
    session.topic_history_entry(ChatId(70), 2);
    session.accept_replied_message(ChatId(70), MessageId(5), live(70, 1));
    // Topics of other chats push chat 70's topics out of the LRU, oldest
    // first.
    session.topic_history_entry(ChatId(71), 1);
    for topic in 2..CLOSED_TOPICS_MAX as i32 {
        session.topic_history_entry(ChatId(71), topic);
    }
    assert!(!session.threads.topic_histories.contains_key(&(70, 1)));
    assert!(session.threads.topic_histories.contains_key(&(70, 2)));
    assert!(fetched_reply(&session, 70, 5), "topic 2 is still kept");
    session.topic_history_entry(ChatId(71), CLOSED_TOPICS_MAX as i32);
    assert!(!session.threads.topic_histories.contains_key(&(70, 2)));
    assert!(!session.histories.contains_key(&70));
    assert!(!fetched_reply(&session, 70, 5));
}

#[test]
fn a_closed_chat_with_an_unsent_message_is_never_dropped_whole() {
    let (mut session, _sink) = session();
    switch(&mut session, 1);
    session.upsert_message(live(1, -5), true);
    for chat in 2..=(CLOSED_CHATS_MAX as i64 + 6) {
        switch(&mut session, chat);
        feed(&mut session, chat, 80);
    }
    leave(&mut session);
    assert!(session.histories.contains_key(&1));
    assert!(session.histories[&1].messages.contains_key(&-5));
}

#[test]
fn recent_live_activity_keeps_a_chat_in_the_lru() {
    let (mut session, _sink) = session();
    for chat in 1..=CLOSED_CHATS_MAX as i64 {
        switch(&mut session, chat);
        feed(&mut session, chat, 80);
    }
    leave(&mut session);
    // Chat 1 is the oldest; a live message makes it the newest.
    session.upsert_message(live(1, 81), false);
    let extra = CLOSED_CHATS_MAX as i64 + 1;
    switch(&mut session, extra);
    feed(&mut session, extra, 80);
    leave(&mut session);
    assert!(session.histories.contains_key(&1));
    assert!(
        !session.histories.contains_key(&2),
        "the oldest idle chat goes"
    );
}

#[test]
fn leaving_a_topic_trims_it_and_the_open_topic_stays_whole() {
    let (mut session, _sink) = session();
    switch(&mut session, 7);
    topic_with(&mut session, 7, 1, 200);
    topic_with(&mut session, 7, 2, 200);
    session.select_topic(ChatId(7), 1);
    assert_eq!(session.threads.topic_histories[&(7, 1)].messages.len(), 200);
    session.select_topic(ChatId(7), 2);
    settle(&mut session);
    let left = &session.threads.topic_histories[&(7, 1)];
    assert_eq!(left.messages.len(), CLOSED_TAIL_KEEP);
    assert!(!left.loaded_complete);
    assert_eq!(left.next_from_message_id, MessageId(141), "paging resumes");
    assert_eq!(session.threads.topic_histories[&(7, 2)].messages.len(), 200);
    session.deselect_topic();
    settle(&mut session);
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
    switch(&mut session, 7);
    let topics = CLOSED_TOPICS_MAX as i32 + 3;
    for topic in 1..=topics {
        topic_with(&mut session, 7, topic, 100);
    }
    switch(&mut session, 8);
    assert_eq!(session.threads.topic_histories.len(), CLOSED_TOPICS_MAX);
    assert!(
        session
            .threads
            .topic_histories
            .values()
            .all(|history| history.messages.len() == CLOSED_TAIL_KEEP)
    );
}

#[test]
fn a_hold_set_in_the_same_event_survives_an_immediate_chat_switch() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    feed(&mut session, 7, 200);
    // Claim and switch before the frame syncs the holds: nothing trims yet.
    session.open_chat(ChatId(8));
    assert_eq!(len(&session, 7), 200, "the trim waits for the claims");
    // A live message meanwhile keeps the page cap, not the closed tail.
    feed(&mut session, 7, 300);
    assert_eq!(len(&session, 7), 300);
    assert!(session.history_pruning.release_pending(ChatId(7)));
    // The frame brings the claim in: the chat stays whole.
    session.set_history_holds([ChatId(7)]);
    assert_eq!(len(&session, 7), 300);
    assert!(!is_closed_tail(&session, 7));
    // The claim lifts: the release goes through.
    session.set_history_holds([]);
    assert_eq!(len(&session, 7), CLOSED_TAIL_KEEP);
    assert!(is_closed_tail(&session, 7));
}

#[test]
fn a_reply_to_another_chat_holds_its_source_through_the_switch() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(7));
    feed(&mut session, 7, 200);
    // "Reply in Another Chat": the reply bar names chat 7 while chat 8 is
    // the one shown.
    session.open_chat(ChatId(8));
    session.set_history_holds([ChatId(7)]);
    assert_eq!(len(&session, 7), 200);
    assert!(session.reply_target(ChatId(7), MessageId(3)).is_some());
}

#[test]
fn releasing_several_topics_is_deterministic() {
    let order = |seed: &[i32]| {
        let (mut session, _sink) = session();
        switch(&mut session, 7);
        for topic in seed {
            topic_with(&mut session, 7, *topic, 100);
        }
        switch(&mut session, 8);
        session
            .history_pruning
            .closed_topics()
            .collect::<Vec<(ChatId, i32)>>()
    };
    let forward = order(&[1, 2, 3, 4, 5, 6, 7, 8]);
    let backward = order(&[8, 7, 6, 5, 4, 3, 2, 1]);
    assert_eq!(forward, backward, "insertion order does not matter");
    let topics: Vec<i32> = forward.iter().map(|(_, topic)| *topic).collect();
    assert_eq!(topics, vec![8, 7, 6, 5, 4, 3, 2, 1], "touched in id order");
}

#[test]
fn entries_made_outside_the_lru_join_it_and_stay_bounded() {
    let (mut session, _sink) = session();
    for chat in 1..=(CLOSED_CHATS_MAX as i64 + 10) {
        session.history_entry(ChatId(chat)).loaded_complete = true;
    }
    assert_eq!(session.histories.len(), CLOSED_CHATS_MAX);
    // The shown chat's entry is not part of the LRU.
    session.open_chat(ChatId(900));
    session.history_entry(ChatId(900));
    assert!(!is_closed_tail(&session, 900));
}

#[test]
fn deleted_ids_survive_dropping_the_entry() {
    let (mut session, _sink) = session();
    session.history_entry(ChatId(7)).tombstones.insert(42);
    session.history_entry(ChatId(7)).latest_seen = 99;
    for chat in 100..(100 + CLOSED_CHATS_MAX as i64 + 2) {
        session.history_entry(ChatId(chat));
    }
    assert!(!session.histories.contains_key(&7), "evicted");
    assert!(session.message_deleted(ChatId(7), MessageId(42)));
    assert!(!session.message_deleted(ChatId(7), MessageId(41)));
    // The entry made again picks them up.
    session.open_chat(ChatId(7));
    assert!(session.histories[&7].tombstones.contains(&42));
    assert_eq!(session.histories[&7].latest_seen, 99);
}

#[test]
fn reopening_the_same_chat_releases_its_open_topic() {
    let (mut session, _sink) = session();
    switch(&mut session, 7);
    topic_with(&mut session, 7, 1, 200);
    session.select_topic(ChatId(7), 1);
    switch(&mut session, 7);
    assert_eq!(
        session.threads.topic_histories[&(7, 1)].messages.len(),
        CLOSED_TAIL_KEEP
    );
}

#[test]
fn a_late_page_for_a_closed_topic_stays_inside_the_topic_bound() {
    let (mut session, _sink) = session();
    for topic in 1..=(CLOSED_TOPICS_MAX as i32 + 6) {
        session
            .topic_history_entry(ChatId(7), topic)
            .loaded_complete = true;
    }
    assert_eq!(session.threads.topic_histories.len(), CLOSED_TOPICS_MAX);
    // The open topic's entry is not part of the LRU.
    session.open_chat(ChatId(7));
    session.open_topic = Some(900);
    session.topic_history_entry(ChatId(7), 900);
    assert_eq!(session.threads.topic_histories.len(), CLOSED_TOPICS_MAX + 1);
}

#[test]
fn a_session_without_a_server_keeps_every_history() {
    // The screenshot demos: nothing could load trimmed rows back.
    let (mut session, _sink) = session();
    session.keep_all_histories();
    switch(&mut session, 7);
    feed(&mut session, 7, 200);
    switch(&mut session, 8);
    assert_eq!(len(&session, 7), 200, "closing trims nothing");
    assert!(!is_closed_tail(&session, 7));
    feed(&mut session, 9, CLOSED_TAIL_CAP as i64 + 20);
    assert_eq!(
        len(&session, 9),
        CLOSED_TAIL_CAP + 20,
        "live rows keep the page cap, not the closed tail"
    );
    for chat in 100..=(100 + CLOSED_CHATS_MAX as i64 + 10) {
        session.history_entry(ChatId(chat));
    }
    assert!(session.histories.contains_key(&100), "no LRU eviction");
    assert!(session.histories.contains_key(&7));
}

/// What opening `chat` and its info panel leaves beside its rows: its ads,
/// a poll row in the poll index, the panel's media counts and member lists.
fn fill_side_tables(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, chat: i64) {
    session.accept_sponsored_messages(ChatId(chat), Vec::new(), 0, &[]);
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":90,"chat_id":{chat},"is_outgoing":false,"content":{{"@type":"messagePoll","poll":{{"@type":"poll","id":{chat},"question":{{"@type":"formattedText","text":"Q","entities":[]}},"options":[],"total_voter_count":0,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{{"@type":"pollTypeRegular"}}}},"description":{{"@type":"formattedText","text":"","entities":[]}},"can_add_option":false}}}}}}"#
        ),
    );
    session
        .media
        .chat_media_counts
        .insert(chat, HashMap::from([(0, 3)]));
    let loaded = || SupergroupMembersFetch::Loaded {
        members: Vec::new(),
        total_count: 200,
    };
    for filter in [MemberListFilter::Recent, MemberListFilter::Administrators] {
        session
            .groups
            .supergroup_members
            .insert((chat, filter), loaded());
    }
    session.groups.basic_group_members.insert(chat, loaded());
}

/// The chats each side table holds something for, sorted.
fn side_table_chats(session: &Session) -> [Vec<i64>; 5] {
    let sorted = |mut chats: Vec<i64>| {
        chats.sort_unstable();
        chats.dedup();
        chats
    };
    [
        sorted(session.messages.sponsored.keys().copied().collect()),
        sorted(
            session
                .messages
                .poll_messages
                .values()
                .flatten()
                .map(|(chat, _)| *chat)
                .collect(),
        ),
        sorted(session.media.chat_media_counts.keys().copied().collect()),
        sorted(
            session
                .groups
                .supergroup_members
                .keys()
                .map(|(chat, _)| *chat)
                .collect(),
        ),
        sorted(session.groups.basic_group_members.keys().copied().collect()),
    ]
}

#[test]
fn side_tables_of_dropped_chats_go_back_to_empty() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let visited = 4 * CLOSED_CHATS_MAX as i64;
    for chat in 1..=visited {
        switch(&mut session, chat);
        feed(&mut session, chat, 80);
        fill_side_tables(&mut session, &seq, &sink, chat);
    }
    leave(&mut session);
    let mut kept: Vec<i64> = session.histories.keys().copied().collect();
    kept.sort_unstable();
    assert_eq!(kept.len(), CLOSED_CHATS_MAX, "only the newest tails stay");
    for (table, chats) in side_table_chats(&session).iter().enumerate() {
        assert_eq!(chats, &kept, "table {table} holds only the kept chats");
    }
    // As many chats again that leave nothing beside their rows push every
    // kept tail out: the tables are back where they started.
    for chat in (visited + 1)..=(visited + 1 + CLOSED_CHATS_MAX as i64) {
        switch(&mut session, chat);
        feed(&mut session, chat, 1);
    }
    leave(&mut session);
    for (table, chats) in side_table_chats(&session).iter().enumerate() {
        assert!(chats.is_empty(), "table {table} still holds {chats:?}");
    }
    let stale = &session.groups.member_list_stale;
    assert!(
        (1..=visited).all(|chat| stale.contains(&chat)),
        "an open member dialog of a dropped chat asks again"
    );
}

/// The UI holds the group the info panel shows (`ui::history_holds`): a
/// frame's claim, made here after each chat switch.
fn switch_with_panel(session: &mut Session, chat: i64) {
    session.open_chat(ChatId(chat));
    let holds: Vec<ChatId> = session.info_panel_chat().into_iter().collect();
    session.set_history_holds(holds);
    feed(session, chat, 1);
}

/// The panel shows chat 1 while 17 other chats are used, then closes.
fn panel_cycle(chat_type: &str, target: InfoPanelTarget) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":1,"title":"G","type":{chat_type},"unread_count":0}}}}"#
        ),
    );
    switch(&mut session, 1);
    fill_side_tables(&mut session, &seq, &sink, 1);
    session.set_open_info_panel(Some(target));
    assert_eq!(session.info_panel_chat(), Some(ChatId(1)));
    for chat in 2..=(CLOSED_CHATS_MAX as i64 + 2) {
        switch_with_panel(&mut session, chat);
    }
    assert_eq!(session.info_panel_chat(), Some(ChatId(1)), "not open");
    assert!(session.histories.contains_key(&1), "the panel holds it");
    let shown = side_table_chats(&session);
    assert!(shown.iter().all(|chats| chats.contains(&1)), "{shown:?}");
    // The panel closes: nothing of the chat stays once it is dropped.
    session.set_open_info_panel(None);
    for chat in 100..=(100 + CLOSED_CHATS_MAX as i64) {
        switch_with_panel(&mut session, chat);
    }
    let dropped = side_table_chats(&session);
    assert!(
        dropped.iter().all(|chats| !chats.contains(&1)),
        "{dropped:?}"
    );
}

#[test]
fn an_open_info_panel_keeps_what_it_shows() {
    panel_cycle(
        r#"{"@type":"chatTypeSupergroup","supergroup_id":101,"is_channel":false}"#,
        InfoPanelTarget::Supergroup(101),
    );
}

#[test]
fn a_basic_group_panel_keeps_what_it_shows() {
    panel_cycle(
        r#"{"@type":"chatTypeBasicGroup","basic_group_id":202}"#,
        InfoPanelTarget::BasicGroup(202),
    );
}

/// A box that reads the member lists of chat 1 (the Edit box's Manage
/// rows, the member dialog) stays open while the main chat moves on.
#[test]
fn a_chat_an_open_box_holds_keeps_its_member_lists() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    switch(&mut session, 1);
    fill_side_tables(&mut session, &seq, &sink, 1);
    session.set_history_holds([ChatId(1)]);
    let churn = |session: &mut Session, from: i64| {
        for chat in from..=(from + CLOSED_CHATS_MAX as i64) {
            switch(session, chat);
            feed(session, chat, 1);
        }
    };
    churn(&mut session, 2);
    let held = side_table_chats(&session);
    assert!(
        held.iter().all(|chats| chats.contains(&1)),
        "the box's lists stay: {held:?}"
    );
    assert!(!session.groups.member_list_stale.contains(&1));
    // The box closes: the chat is an ordinary closed tail from then on.
    session.set_history_holds([]);
    churn(&mut session, 100);
    let dropped = side_table_chats(&session);
    assert!(
        dropped.iter().all(|chats| !chats.contains(&1)),
        "{dropped:?}"
    );
}

#[test]
fn a_forum_forgets_its_side_tables_with_its_last_topic() {
    let (mut session, _sink) = session();
    session.topic_history_entry(ChatId(70), 1);
    session.accept_sponsored_messages(ChatId(70), Vec::new(), 0, &[]);
    session
        .media
        .chat_media_counts
        .insert(70, HashMap::from([(0, 3)]));
    session.groups.supergroup_members.insert(
        (70, MemberListFilter::Recent),
        SupergroupMembersFetch::Loaded {
            members: Vec::new(),
            total_count: 200,
        },
    );
    assert!(side_table_chats(&session)[0].contains(&70));
    for topic in 1..=CLOSED_TOPICS_MAX as i32 {
        session.topic_history_entry(ChatId(71), topic);
    }
    assert!(!session.threads.topic_histories.contains_key(&(70, 1)));
    let tables = side_table_chats(&session);
    assert!(
        tables.iter().all(|chats| !chats.contains(&70)),
        "{tables:?}"
    );
}

/// The gallery shows its view's open chat, whose history entry stays while
/// it is shown, so closed topics of that chat going leave its counts.
#[test]
fn the_open_chat_keeps_its_gallery_counts_while_its_topics_go() {
    let (mut session, _sink) = session();
    switch(&mut session, 1);
    session.media.shared_media.open_for(ChatId(1));
    session
        .media
        .chat_media_counts
        .insert(1, HashMap::from([(0, 3)]));
    for topic in 1..=(CLOSED_TOPICS_MAX as i32 + 4) {
        session.topic_history_entry(ChatId(1), topic);
    }
    for chat in 2..=(CLOSED_CHATS_MAX as i64 + 4) {
        session.history_entry(ChatId(chat));
    }
    assert!(!session.threads.topic_histories.contains_key(&(1, 1)));
    assert!(session.media.shared_media.open);
    assert!(session.media.chat_media_counts.contains_key(&1));
}

fn sponsored_answer(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, chat: i64) {
    let extra = session.request(RequestPurpose::GetChatSponsoredMessages, Some(ChatId(chat)));
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"sponsoredMessages","@extra":"{}","messages_between":0,"messages":[{{"@type":"sponsoredMessage","message_id":9001,"is_recommended":false,"can_be_reported":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"ad","entities":[]}}}},"sponsor":{{"@type":"advertisementSponsor","url":"https://example.com","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}},"info":""}},"title":"T","button_text":"Go","accent_color_id":0,"background_custom_emoji_id":"0","additional_info":""}}]}}"#,
            extra.0
        ),
    );
}

#[test]
fn a_reported_ad_stays_hidden_after_its_chat_is_dropped() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    switch(&mut session, 1);
    sponsored_answer(&mut session, &seq, &sink, 1);
    let shown = |session: &Session| session.open_sponsored_tail().map(|ad| ad.message_id);
    assert_eq!(shown(&session), Some(9001));
    assert!(session.begin_sponsored_report(ChatId(1), 9001).is_some());
    let report = session.request(RequestPurpose::ReportChatSponsoredMessage, Some(ChatId(1)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"reportSponsoredResultOk","@extra":"{}"}}"#,
            report.0
        ),
    );
    assert_eq!(shown(&session), None, "the reported ad left the chat");
    for chat in 2..=(CLOSED_CHATS_MAX as i64 + 2) {
        switch(&mut session, chat);
        feed(&mut session, chat, 1);
    }
    assert!(!session.histories.contains_key(&1), "its rows went");
    assert!(
        session.sponsored_message(ChatId(1), 9001).is_none(),
        "and its ads"
    );
    assert!(session.sponsored_fetch_due(ChatId(1), std::time::Instant::now()));
    // TDLib's list still has the ad when the chat opens again.
    switch(&mut session, 1);
    sponsored_answer(&mut session, &seq, &sink, 1);
    assert!(session.sponsored_message(ChatId(1), 9001).is_some());
    assert_eq!(shown(&session), None, "the reported ad stays hidden");
}

/// A report still on its way when the chat goes (the UI holds the chat of
/// an ad being reported, so this is the reducer's own fallback).
#[test]
fn a_report_answered_after_the_drop_still_hides_its_ad() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    switch(&mut session, 1);
    sponsored_answer(&mut session, &seq, &sink, 1);
    assert!(session.begin_sponsored_report(ChatId(1), 9001).is_some());
    let report = session.request(RequestPurpose::ReportChatSponsoredMessage, Some(ChatId(1)));
    for chat in 2..=(CLOSED_CHATS_MAX as i64 + 2) {
        switch(&mut session, chat);
        feed(&mut session, chat, 1);
    }
    assert!(
        session.sponsored_message(ChatId(1), 9001).is_none(),
        "dropped"
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"reportSponsoredResultOk","@extra":"{}"}}"#,
            report.0
        ),
    );
    switch(&mut session, 1);
    sponsored_answer(&mut session, &seq, &sink, 1);
    assert!(session.sponsored_message(ChatId(1), 9001).is_some());
    assert_eq!(
        session.open_sponsored_tail().map(|ad| ad.message_id),
        None,
        "the reported ad stays hidden"
    );
}
