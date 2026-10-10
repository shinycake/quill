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
    assert_eq!(session.saved.topic_count, 2);
    let ordered = session.saved.ordered_topics();
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
    assert_eq!(session.saved.ordered_topics()[0].id, -1001);
    assert_eq!(session.saved.topics.len(), 2);
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
        .saved
        .ordered_topics()
        .into_iter()
        .map(|t| session.saved_topic_title(t))
        .collect();
    assert_eq!(titles, vec!["My Notes", "Author Hidden"]);
}

#[test]
fn sublist_history_pages_and_follows_deletes_and_edits() {
    let (mut session, sink, seq) = saved_session();
    session.open_saved_sublist(77);
    let purpose = RequestPurpose::GetSavedMessagesTopicHistory { topic_id: 77 };
    let extra = session.request(purpose, None);
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
    let view = session.saved.sublist.as_ref().unwrap();
    assert_eq!(view.history.messages.len(), 2);
    assert_eq!(view.history.next_from_message_id, MessageId(20));
    assert!(!view.history.loaded_complete);
    // An empty page ends the history.
    let extra = session.request(purpose, None);
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
            .saved
            .sublist
            .as_ref()
            .unwrap()
            .history
            .loaded_complete
    );
    // A page answered for another sublist is dropped.
    session.open_saved_sublist(5);
    let extra = session.request(purpose, None);
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
    assert!(session.saved.tag_search.is_some());
    session.open_chat(ChatId(5));
    assert!(session.saved.sublist.is_none());
    assert!(session.saved.tag_search.is_none());
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
    assert!(session.saved.tags_loaded);
    assert_eq!(session.saved.tags[0].label, "Love");
    session.open_saved_sublist(77);
    let extra = session.request(RequestPurpose::GetSavedMessagesTags { topic_id: 77 }, None);
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

#[test]
fn tag_search_pages_land_in_the_filter() {
    let (mut session, sink, seq) = saved_session();
    let tag = ReactionType::emoji("\u{2764}");
    session.begin_saved_tag_search(0, tag.clone());
    let extra = session.request(RequestPurpose::SearchSavedMessages { topic_id: 0 }, None);
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
    let search = session.saved.tag_search.as_ref().unwrap();
    assert!(search.loaded);
    assert_eq!(search.tag, tag);
    assert_eq!(search.history.messages.len(), 2);
    assert!(search.history.loaded_complete);
    session.clear_saved_tag_search();
    assert!(session.saved.tag_search.is_none());
}

#[test]
fn load_topics_404_marks_the_list_complete() {
    let (mut session, sink, seq) = saved_session();
    let extra = session.request(RequestPurpose::LoadSavedMessagesTopics, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":404,"message":"Not Found"}}"#,
            extra.0
        ),
    );
    assert!(session.saved.topics_exhausted);
    assert!(session.chat_action_error.is_none());
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
    assert!(session.saved.topics.is_empty());
    assert!(session.saved.sublist.is_none());
}

fn forum_with_pins(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    apply_json(
        session,
        seq,
        sink,
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
    );
    let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(16)));
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
    assert_eq!(session.forum_topic_icons.len(), 1);
    assert_eq!(session.forum_topic_icons[0].custom_emoji_id, Some(5005));
}
