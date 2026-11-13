//! State reducer tests: forum view mode, topic extras, Saved Messages
//! sublists and tags (batch B16).
use super::common::*;
use super::*;
use crate::telegram::envelope::{ReactionType, SavedTopicKind};

const ME: i64 = 13;

fn saved_session() -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateOption","name":"my_id","value":{{"@type":"optionValueInteger","value":"{ME}"}}}}"#
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{ME},"title":"Saved","type":{{"@type":"chatTypePrivate","user_id":{ME}}},"unread_count":0}}}}"#
        ),
    );
    (session, sink, seq)
}

fn topic_json(id: i64, chat: i64, pinned: bool, order: i64, text: &str) -> String {
    format!(
        r#"{{"@type":"updateSavedMessagesTopic","topic":{{"@type":"savedMessagesTopic","id":"{id}","type":{{"@type":"savedMessagesTopicTypeSavedFromChat","chat_id":{chat}}},"is_pinned":{pinned},"order":"{order}","last_message":{{"id":{mid},"chat_id":{ME},"date":1700000000,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}},"draft_message":null}}}}"#,
        mid = 1000 + id.abs()
    )
}

fn message_json(id: i64, text: &str) -> String {
    format!(
        r#"{{"id":{id},"chat_id":{ME},"date":1700000000,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
    )
}

#[test]
fn view_as_topics_defaults_follow_the_chat_kind() {
    let (mut session, sink, seq) = saved_session();
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
    );
    // A forum without an answer shows topics; Saved Messages shows messages.
    assert!(session.chat_views_as_topics(ChatId(16)));
    assert!(!session.chat_views_as_topics(ChatId(ME)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatViewAsTopics","chat_id":16,"view_as_topics":false}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"updateChatViewAsTopics","chat_id":{ME},"view_as_topics":true}}"#),
    );
    assert!(!session.chat_views_as_topics(ChatId(16)));
    assert!(session.chat_views_as_topics(ChatId(ME)));
}

#[test]
fn chat_object_carries_view_as_topics() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"view_as_topics":false,"unread_count":0}}"#,
    );
    assert!(!session.chat_views_as_topics(ChatId(16)));
}

#[test]
fn saved_sublists_sort_by_order_and_name_themselves() {
    let (mut session, sink, seq) = saved_session();
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":-1001,"title":"Rust News","type":{"@type":"chatTypeSupergroup","supergroup_id":1001,"is_channel":true},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &topic_json(-1001, -1001, false, 10, "post"),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &topic_json(77, 77, true, 50, "pinned"),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSavedMessagesTopicCount","topic_count":2}"#,
    );
    assert_eq!(session.threads.saved.topic_count, 2);
    let ordered = session.threads.saved.ordered_topics();
    assert_eq!(ordered[0].id, 77);
    assert!(ordered[0].is_pinned);
    assert_eq!(ordered[1].kind, SavedTopicKind::FromChat(-1001));
    assert_eq!(session.saved_topic_title(ordered[1]), "Rust News");
    // An update replaces the row in place (unpinned, lower order).
    apply_json(
        &mut session,
        &seq,
        &sink,
        &topic_json(77, 77, false, 5, "later"),
    );
    assert_eq!(session.threads.saved.ordered_topics()[0].id, -1001);
    assert_eq!(session.threads.saved.topics.len(), 2);
}

#[test]
fn my_notes_and_hidden_author_use_tdesktop_names() {
    let (mut session, sink, seq) = saved_session();
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSavedMessagesTopic","topic":{"@type":"savedMessagesTopic","id":"13","type":{"@type":"savedMessagesTopicTypeMyNotes"},"is_pinned":false,"order":"3"}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSavedMessagesTopic","topic":{"@type":"savedMessagesTopic","id":"0","type":{"@type":"savedMessagesTopicTypeAuthorHidden"},"is_pinned":false,"order":"2"}}"#,
    );
    let titles: Vec<String> = session
        .threads
        .saved
        .ordered_topics()
        .into_iter()
        .map(|t| session.saved_topic_title(t))
        .collect();
    assert_eq!(titles, vec!["My Notes", "Author Hidden"]);
}

/// The open sublist waits for `extra`, as the driver records it on send.
fn ask_sublist(session: &mut Session, extra: RequestId) {
    if let Some(view) = session.threads.saved.sublist.as_mut() {
        view.history.ask_older(extra);
    }
}

#[test]
fn sublist_history_pages_and_follows_deletes_and_edits() {
    let (mut session, sink, seq) = saved_session();
    session.open_saved_sublist(77);
    let purpose = RequestPurpose::GetSavedMessagesTopicHistory { topic_id: 77 };
    let extra = session.request(purpose, None);
    ask_sublist(&mut session, extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{},{}]}}"#,
            extra.0,
            message_json(30, "newer"),
            message_json(20, "older")
        ),
    );
    let view = session.threads.saved.sublist.as_ref().unwrap();
    assert_eq!(view.history.messages.len(), 2);
    assert_eq!(view.history.next_from_message_id, MessageId(20));
    assert!(!view.history.loaded_complete());
    // An empty page ends the history.
    let extra = session.request(purpose, None);
    ask_sublist(&mut session, extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":0,"messages":[]}}"#,
            extra.0
        ),
    );
    assert!(
        session
            .threads
            .saved
            .sublist
            .as_ref()
            .unwrap()
            .history
            .loaded_complete()
    );
    // A page answered for another sublist is dropped.
    session.open_saved_sublist(5);
    let extra = session.request(purpose, None);
    ask_sublist(&mut session, extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{}]}}"#,
            extra.0,
            message_json(40, "stale")
        ),
    );
    assert!(
        session
            .threads
            .saved
            .sublist
            .as_ref()
            .unwrap()
            .history
            .messages
            .is_empty()
    );
    // Deleting a message reaches the open sublist.
    session.open_saved_sublist(77);
    let extra = session.request(
        RequestPurpose::GetSavedMessagesTopicHistory { topic_id: 77 },
        None,
    );
    ask_sublist(&mut session, extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{}]}}"#,
            extra.0,
            message_json(30, "x")
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateDeleteMessages","chat_id":{ME},"message_ids":[30],"is_permanent":true,"from_cache":false}}"#
        ),
    );
    assert!(
        session
            .threads
            .saved
            .sublist
            .as_ref()
            .unwrap()
            .history
            .messages
            .is_empty()
    );
}

#[test]
fn leaving_the_saved_chat_closes_sublist_and_tag_filter() {
    let (mut session, _sink, _seq) = saved_session();
    session.open_saved_sublist(77);
    session.begin_saved_tag_search(77, ReactionType::emoji("\u{2764}"));
    assert!(session.threads.saved.tag_search.is_some());
    session.open_chat(ChatId(5));
    assert!(session.threads.saved.sublist.is_none());
    assert!(session.threads.saved.tag_search.is_none());
}

#[test]
fn tag_updates_and_sublist_tag_names() {
    let (mut session, sink, seq) = saved_session();
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSavedMessagesTags","saved_messages_topic_id":"0","tags":{"@type":"savedMessagesTags","tags":[{"tag":{"@type":"reactionTypeEmoji","emoji":"❤"},"label":"Love","count":4},{"tag":{"@type":"reactionTypeEmoji","emoji":"🔥"},"label":"","count":1}]}}"#,
    );
    assert!(session.threads.saved.tags.data().is_some());
    assert_eq!(session.threads.saved.tag_list()[0].label, "Love");
    session.open_saved_sublist(77);
    let extra = session.request(RequestPurpose::GetSavedMessagesTags { topic_id: 77 }, None);
    let fetch = session.threads.saved.topic_tags.entry(77).or_default();
    fetch.begin_with(extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"savedMessagesTags","@extra":"{}","tags":[{{"tag":{{"@type":"reactionTypeEmoji","emoji":"❤"}},"label":"","count":2}}]}}"#,
            extra.0
        ),
    );
    // The sublist's own list borrows the global names.
    let choices = session.saved_tag_choices();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].label, "Love");
    assert_eq!(choices[0].count, 2);
}

/// A sublist's tags are taken from the ask that is out for it: an older ask
/// answering after a newer one began, and one nobody asked, change nothing.
#[test]
fn a_sublist_takes_only_the_tags_it_asked_for() {
    let (mut session, sink, seq) = saved_session();
    let tags = |count: i32| {
        format!(
            r#""tags":[{{"tag":{{"@type":"reactionTypeEmoji","emoji":"❤"}},"label":"","count":{count}}}]"#
        )
    };
    let ask = |session: &mut Session| {
        let extra = session.request(RequestPurpose::GetSavedMessagesTags { topic_id: 77 }, None);
        let fetch = session.threads.saved.topic_tags.entry(77).or_default();
        fetch.begin_with(extra);
        extra
    };
    let older = ask(&mut session);
    let newer = ask(&mut session);
    for (request, count) in [(older, 1), (newer, 2)] {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"savedMessagesTags","@extra":"{}",{}}}"#,
                request.0,
                tags(count)
            ),
        );
        let held = session
            .threads
            .saved
            .topic_tags
            .get(&77)
            .and_then(|f| f.data());
        assert_eq!(held.map(|t| t[0].count), (count == 2).then_some(2));
    }
    // An update is the server's current list, whatever the fetch.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateSavedMessagesTags","saved_messages_topic_id":"77","tags":{{"@type":"savedMessagesTags",{}}}}}"#,
            tags(5)
        ),
    );
    let held = session
        .threads
        .saved
        .topic_tags
        .get(&77)
        .and_then(|f| f.data());
    assert_eq!(held.map(|t| t[0].count), Some(5));
}

#[test]
fn tag_search_pages_land_in_the_filter() {
    let (mut session, sink, seq) = saved_session();
    let tag = ReactionType::emoji("\u{2764}");
    session.begin_saved_tag_search(0, tag.clone());
    let extra = session.request(RequestPurpose::SearchSavedMessages { topic_id: 0 }, None);
    if let Some(search) = session.threads.saved.tag_search.as_mut() {
        search.history.ask_older(extra);
    }
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"messages":[{},{}],"next_from_message_id":0}}"#,
            extra.0,
            message_json(9, "a"),
            message_json(8, "b")
        ),
    );
    let search = session.threads.saved.tag_search.as_ref().unwrap();
    assert_eq!(search.tag, tag);
    assert_eq!(search.history.messages.len(), 2);
    assert!(search.history.loaded_complete());
    session.clear_saved_tag_search();
    assert!(session.threads.saved.tag_search.is_none());
}

#[test]
fn load_topics_404_marks_the_list_complete() {
    let (mut session, sink, seq) = saved_session();
    let extra = session.request(RequestPurpose::LoadSavedMessagesTopics, None);
    session.threads.saved.page_sent(extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":404,"message":"Not Found"}}"#,
            extra.0
        ),
    );
    assert!(session.threads.saved.topics_exhausted());
    assert!(session.chats_state.chat_action_error.is_empty());
}

/// Round 7, 2d: a failed first page is not "sent" any more, so the pane
/// does not wait on it for the rest of the session. The pump holds off
/// until the connection is back, and failed tags wait the same way
/// instead of being sent again on every poll pass.
#[test]
fn a_failed_first_page_is_asked_again_once_back_online() {
    let (mut session, sink, seq) = saved_session();
    session.connection = ConnectionState::Ready;
    let error = |extra: RequestId| {
        format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"Timeout"}}"#,
            extra.0
        )
    };
    let page = session.request(RequestPurpose::LoadSavedMessagesTopics, None);
    session.threads.saved.page_sent(page);
    apply_json(&mut session, &seq, &sink, &error(page));
    assert_eq!(session.threads.saved.topics_load, SavedTopicsLoad::Retry);
    assert!(session.threads.saved.waiting_to_retry());
    assert!(
        !session.threads.saved.topics_exhausted(),
        "the pane is not 'no chats'"
    );

    let state = |name: &str| {
        format!(
            r#"{{"@type":"updateConnectionState","state":{{"@type":"connectionState{name}"}}}}"#
        )
    };
    apply_json(&mut session, &seq, &sink, &state("Connecting"));
    assert!(session.threads.saved.waiting_to_retry());
    apply_json(&mut session, &seq, &sink, &state("Ready"));
    assert!(!session.threads.saved.waiting_to_retry());

    let tags = session.request(RequestPurpose::GetSavedMessagesTags { topic_id: 0 }, None);
    session.threads.saved.tags.begin_with(tags);
    apply_json(&mut session, &seq, &sink, &error(tags));
    assert!(session.threads.saved.waiting_to_retry());
    assert!(session.threads.saved.tags.error().is_some());
    // Back online, the tags are asked again (the pump sees them idle).
    apply_json(&mut session, &seq, &sink, &state("Connecting"));
    apply_json(&mut session, &seq, &sink, &state("Ready"));
    assert!(session.threads.saved.tags.is_idle());
}

/// Round 8, 2e: a server error arrives while already online, so no
/// reconnect follows it. Opening Saved Messages again asks again; opening
/// another chat does not.
#[test]
fn a_failure_while_online_is_asked_again_on_the_next_open() {
    let (mut session, sink, seq) = saved_session();
    session.connection = ConnectionState::Ready;
    session.open_chat(ChatId(ME));
    let page = session.request(RequestPurpose::LoadSavedMessagesTopics, None);
    session.threads.saved.page_sent(page);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":429,"message":"FLOOD_WAIT_3"}}"#,
            page.0
        ),
    );
    assert!(session.threads.saved.waiting_to_retry());
    session.open_chat(ChatId(ME + 1));
    assert!(session.threads.saved.waiting_to_retry(), "another chat");
    session.open_chat(ChatId(ME));
    assert!(
        !session.threads.saved.waiting_to_retry(),
        "Saved Messages again"
    );
    assert_eq!(
        session.threads.saved.topics_load,
        SavedTopicsLoad::Idle,
        "the pump sends the page"
    );
}

#[test]
fn delete_sublist_history_removes_the_row_and_leaves_the_sublist() {
    let (mut session, sink, seq) = saved_session();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &topic_json(77, 77, false, 5, "x"),
    );
    session.open_saved_sublist(77);
    let extra = session.request(
        RequestPurpose::Threads(ThreadsPurpose::DeleteSavedMessagesTopicHistory { topic_id: 77 }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(session.threads.saved.topics.is_empty());
    assert!(session.threads.saved.sublist.is_none());
}

fn forum_with_pins(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    apply_json(
        session,
        seq,
        sink,
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
    );
    let extra = session.ask_forum_topics(ChatId(16));
    let topic = |id: i32, pinned: bool, order: i64, mentions: i32| {
        format!(
            r#"{{"info":{{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":{id},"name":"T{id}","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"is_general":false,"is_closed":false,"is_hidden":false}},"last_message":null,"order":"{order}","is_pinned":{pinned},"unread_count":0,"last_read_inbox_message_id":0,"unread_mention_count":{mentions},"unread_reaction_count":2,"notification_settings":{{"@type":"chatNotificationSettings"}}}}"#
        )
    };
    let json = format!(
        r#"{{"@type":"forumTopics","@extra":"{}","total_count":3,"topics":[{},{},{}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
        extra.0,
        topic(1, true, 900, 3),
        topic(2, true, 800, 0),
        topic(3, false, 100, 0),
    );
    apply_json(session, seq, sink, &json);
}

#[test]
fn pinned_topic_order_moves_one_step() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    forum_with_pins(&mut session, &sink, &seq);
    assert_eq!(session.pinned_forum_topic_ids(ChatId(16)), vec![1, 2]);
    assert_eq!(
        session.moved_pinned_order(ChatId(16), 2, true),
        Some(vec![2, 1])
    );
    // The first cannot move up, the last cannot move down, unpinned never.
    assert_eq!(session.moved_pinned_order(ChatId(16), 1, true), None);
    assert_eq!(session.moved_pinned_order(ChatId(16), 2, false), None);
    assert_eq!(session.moved_pinned_order(ChatId(16), 3, true), None);
    session.apply_pinned_forum_order(ChatId(16), &[2, 1]);
    assert_eq!(session.pinned_forum_topic_ids(ChatId(16)), vec![2, 1]);
}

#[test]
fn topic_mark_counts_parse_and_clear() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    forum_with_pins(&mut session, &sink, &seq);
    let topic = &session.ordered_forum_topics(ChatId(16))[0];
    assert_eq!(topic.unread_mention_count, 3);
    assert_eq!(topic.unread_reaction_count, 2);
    let extra = session.request(
        RequestPurpose::Threads(ThreadsPurpose::ReadAllForumTopicMentions { forum_topic_id: 1 }),
        Some(ChatId(16)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    let topic = &session.ordered_forum_topics(ChatId(16))[0];
    assert_eq!(topic.unread_mention_count, 0);
    assert_eq!(topic.unread_reaction_count, 2);
    let extra = session.request(
        RequestPurpose::Threads(ThreadsPurpose::ReadAllForumTopicReactions { forum_topic_id: 1 }),
        Some(ChatId(16)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(
        session.ordered_forum_topics(ChatId(16))[0].unread_reaction_count,
        0
    );
}

#[test]
fn default_icons_keep_only_custom_emoji_stickers() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetForumTopicDefaultIcons, None);
    session.threads.forum_topic_icons.begin_with(extra);
    let sticker = |id: i64, emoji_id: i64| {
        format!(
            r#"{{"@type":"sticker","id":"{id}","set_id":"1","width":100,"height":100,"emoji":"A","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeCustomEmoji","custom_emoji_id":"{emoji_id}","needs_repainting":false}},"thumbnail":null,"sticker":{{"@type":"file","id":{id},"size":1,"expected_size":1,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":1}}}}}}"#
        )
    };
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"stickers","@extra":"{}","stickers":[{},{}]}}"#,
            extra.0,
            sticker(5, 5005),
            sticker(6, 0)
        ),
    );
    assert_eq!(session.threads.forum_topic_icons.items().len(), 1);
    assert_eq!(
        session.threads.forum_topic_icons.items()[0].custom_emoji_id,
        Some(5005)
    );
}

#[test]
fn a_topic_action_answer_waits_for_the_request_that_asked() {
    let (mut session, _) = session();
    session.topic_action_answered(RequestId(7), AskingView::Main, true);
    session.topic_action_answered(RequestId(8), AskingView::Main, false);
    assert_eq!(session.take_topic_action_answer(RequestId(9)), None);
    assert_eq!(session.take_topic_action_answer(RequestId(8)), Some(false));
    assert_eq!(session.take_topic_action_answer(RequestId(7)), Some(true));
    assert_eq!(session.take_topic_action_answer(RequestId(7)), None, "once");
}

/// r2-18 05-071 / 06-013: the older page of a Saved Messages view is asked
/// through its own older end: a view left and entered again drops the answer
/// of the page the first one asked, and a failed page settles the end.
#[test]
fn a_saved_page_lands_only_in_the_view_that_asked_it() {
    let (mut session, sink, seq) = saved_session();
    let purpose = RequestPurpose::GetSavedMessagesTopicHistory { topic_id: 77 };
    session.open_saved_sublist(77);
    let old = session.request(purpose, None);
    ask_sublist(&mut session, old);
    // The sublist is left and entered again: a new view, its own page.
    session.open_saved_sublist(77);
    let new = session.request(purpose, None);
    ask_sublist(&mut session, new);
    let page = |extra: RequestId, id: i64| {
        format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{}]}}"#,
            extra.0,
            message_json(id, "m")
        )
    };
    apply_json(&mut session, &seq, &sink, &page(old, 30));
    let view = session.threads.saved.sublist.as_ref().unwrap();
    assert!(view.history.messages.is_empty());
    assert_eq!(view.history.older, OlderEnd::Asking(new));
    apply_json(&mut session, &seq, &sink, &page(new, 20));
    let view = session.threads.saved.sublist.as_ref().unwrap();
    assert_eq!(view.history.messages.len(), 1);
    assert_eq!(view.history.older, OlderEnd::Open);
    // A failed page ends in `Failed`, which waits for the user's Retry.
    let again = session.request(purpose, None);
    ask_sublist(&mut session, again);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"INTERNAL"}}"#,
            again.0
        ),
    );
    let view = session.threads.saved.sublist.as_ref().unwrap();
    assert_eq!(view.history.older, OlderEnd::Failed);
}

#[test]
fn an_older_page_cannot_settle_the_page_now_out() {
    let (mut session, sink, seq) = saved_session();
    session.connection = ConnectionState::Ready;
    let first = session.request(RequestPurpose::LoadSavedMessagesTopics, None);
    session.threads.saved.page_sent(first);
    // The first page failed and was asked again before its late answer.
    session.threads.saved.topics_load = SavedTopicsLoad::Idle;
    let second = session.request(RequestPurpose::LoadSavedMessagesTopics, None);
    session.threads.saved.page_sent(second);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":404,"message":"Not Found"}}"#,
            first.0
        ),
    );
    assert_eq!(
        session.threads.saved.topics_load,
        SavedTopicsLoad::Loading { request: second },
        "a late 404 of the first page is not the second's answer"
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, second.0),
    );
    assert_eq!(session.threads.saved.topics_load, SavedTopicsLoad::Loaded);
}
