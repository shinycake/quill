//! Reducer tests: composer drafts keyed by (chat, topic) — which chats
//! draft, topic and thread slots, remote draft apply, previews.
use super::common::*;
use super::*;
use crate::composer::DraftTopic;
use crate::telegram::envelope::ChatDraft;

const DRAFT_JSON: &str = r#"{"@type":"draftMessage","reply_to":null,"date":5,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"TEXT","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}"#;

fn draft_json(text: &str) -> String {
    DRAFT_JSON.replace("TEXT", text)
}

fn text_draft(text: &str) -> ChatDraft {
    ChatDraft {
        text: text.into(),
        rich: None,
        item: Default::default(),
        reply_to_message_id: None,
        quote: None,
        suggested_post: None,
    }
}

fn new_chat(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, id: i64, kind: &str) {
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"C{id}","type":{kind},"unread_count":0}}}}"#
        ),
    );
}

fn supergroup_status(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    id: i64,
    is_channel: bool,
    is_forum: bool,
    status: &str,
) {
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{id},"is_channel":{is_channel},"is_forum":{is_forum},"status":{status}}}}}"#
        ),
    );
}

fn topic_json(chat: i64, id: i32, unread: i32, draft: Option<&str>) -> String {
    let draft = draft.map(draft_json).unwrap_or_else(|| "null".into());
    format!(
        r#"{{"info":{{"@type":"forumTopicInfo","chat_id":{chat},"forum_topic_id":{id},"name":"T{id}","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"is_general":false,"is_closed":false,"is_hidden":false}},"last_message":{{"id":{id},"chat_id":{chat},"date":1,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"last in T{id}","entities":[]}}}}}},"order":"{order}","is_pinned":false,"unread_count":{unread},"last_read_inbox_message_id":0,"notification_settings":{{"@type":"chatNotificationSettings"}},"draft_message":{draft}}}"#,
        order = 1000 - i64::from(id)
    )
}

/// Forum 16 with topics 3 (draft "topic three") and 4 (no draft).
fn forum(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>) {
    new_chat(
        session,
        seq,
        sink,
        16,
        r#"{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false}"#,
    );
    supergroup_status(
        session,
        seq,
        sink,
        16,
        false,
        true,
        r#"{"@type":"chatMemberStatusMember"}"#,
    );
    let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(16)));
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"forumTopics","@extra":"{}","total_count":2,"topics":[{},{}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
            extra.0,
            topic_json(16, 3, 0, Some("topic three")),
            topic_json(16, 4, 0, None),
        ),
    );
}

#[test]
fn every_chat_the_user_can_write_in_accepts_a_draft() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    new_chat(
        &mut session,
        &seq,
        &sink,
        7,
        r#"{"@type":"chatTypePrivate","user_id":7}"#,
    );
    new_chat(
        &mut session,
        &seq,
        &sink,
        8,
        r#"{"@type":"chatTypeBasicGroup","basic_group_id":8}"#,
    );
    new_chat(
        &mut session,
        &seq,
        &sink,
        9,
        r#"{"@type":"chatTypeSupergroup","supergroup_id":9,"is_channel":false}"#,
    );
    // Channel 10: a plain member; channel 11: the creator.
    for (id, status) in [
        (10, r#"{"@type":"chatMemberStatusMember"}"#),
        (
            11,
            r#"{"@type":"chatMemberStatusCreator","is_member":true}"#,
        ),
    ] {
        new_chat(
            &mut session,
            &seq,
            &sink,
            id,
            &format!(r#"{{"@type":"chatTypeSupergroup","supergroup_id":{id},"is_channel":true}}"#),
        );
        supergroup_status(&mut session, &seq, &sink, id, true, false, status);
    }
    // Secret chats: 31 ready, 33 still pending.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":31,"user_id":7,"state":{"@type":"secretChatStateReady"},"is_outbound":true,"key_hash":"","layer":144}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":33,"user_id":7,"state":{"@type":"secretChatStatePending"},"is_outbound":true,"key_hash":"","layer":144}}"#,
    );
    new_chat(
        &mut session,
        &seq,
        &sink,
        31,
        r#"{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7}"#,
    );
    new_chat(
        &mut session,
        &seq,
        &sink,
        33,
        r#"{"@type":"chatTypeSecret","secret_chat_id":33,"user_id":7}"#,
    );
    // Bot 12 is a private chat with a `userTypeBot`.
    new_chat(
        &mut session,
        &seq,
        &sink,
        12,
        r#"{"@type":"chatTypePrivate","user_id":12}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":12,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
    );

    let chat = DraftTopic::Chat;
    assert!(session.accepts_composer_draft(ChatId(7), chat), "user");
    assert!(
        session.accepts_composer_draft(ChatId(8), chat),
        "basic group"
    );
    assert!(
        session.accepts_composer_draft(ChatId(9), chat),
        "supergroup"
    );
    assert!(
        !session.accepts_composer_draft(ChatId(10), chat),
        "channel without posting rights"
    );
    assert!(
        session.accepts_composer_draft(ChatId(11), chat),
        "channel the user posts in"
    );
    assert!(
        session.accepts_composer_draft(ChatId(31), chat),
        "secret ready"
    );
    assert!(
        !session.accepts_composer_draft(ChatId(33), chat),
        "secret pending"
    );
    assert!(session.accepts_composer_draft(ChatId(12), chat), "bot");
    assert!(
        !session.accepts_composer_draft(ChatId(99), chat),
        "unknown chat"
    );
    // Plain chats have thread slots but no topic slots.
    assert!(session.accepts_composer_draft(ChatId(9), DraftTopic::Thread(501)));
    assert!(!session.accepts_composer_draft(ChatId(9), DraftTopic::Thread(0)));
    assert!(!session.accepts_composer_draft(ChatId(9), DraftTopic::Forum(3)));
    assert!(!sink.rendered().contains("CANARY"));
}

#[test]
fn a_forum_drafts_per_topic_and_never_for_the_chat_itself() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    forum(&mut session, &seq, &sink);
    assert!(!session.accepts_composer_draft(ChatId(16), DraftTopic::Chat));
    assert!(session.accepts_composer_draft(ChatId(16), DraftTopic::Forum(3)));
    assert!(session.accepts_composer_draft(ChatId(16), DraftTopic::Forum(4)));
    // Threads inside a forum are keyed by their topic (see
    // `open_draft_topic`), so the thread slot itself is off.
    assert!(!session.accepts_composer_draft(ChatId(16), DraftTopic::Thread(9)));

    // `forumTopic.draft_message` parsed into the topic.
    assert_eq!(
        session.draft_for(ChatId(16), DraftTopic::Forum(3)),
        Some(&text_draft("topic three"))
    );
    assert_eq!(session.draft_for(ChatId(16), DraftTopic::Forum(4)), None);
    // A forum chat row never shows a draft; the topic row does.
    assert_eq!(session.chats.get(&16).unwrap().draft_preview(), None);
    let topics = session.ordered_forum_topics(ChatId(16));
    let three = topics.iter().find(|t| t.forum_topic_id == 3).unwrap();
    let preview = three.draft_preview().expect("topic draft shown");
    assert_eq!(preview.text, "topic three");
    assert!(!preview.reply);

    // The open slot follows the open topic.
    session.open_chat(ChatId(16));
    assert_eq!(session.open_draft_topic(ChatId(16)), DraftTopic::Chat);
    session.select_topic(ChatId(16), 4);
    assert_eq!(session.open_draft_topic(ChatId(16)), DraftTopic::Forum(4));
    // Another chat is never reported as being in this forum's topic.
    assert_eq!(session.open_draft_topic(ChatId(7)), DraftTopic::Chat);
    session.deselect_topic();
    assert_eq!(session.open_draft_topic(ChatId(16)), DraftTopic::Chat);
}

#[test]
fn topic_slots_store_and_dirty_independently() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    forum(&mut session, &seq, &sink);
    session.store_composer_draft(
        ChatId(16),
        DraftTopic::Forum(4),
        Some(text_draft("topic four")),
    );
    assert_eq!(
        session.draft_for(ChatId(16), DraftTopic::Forum(4)),
        Some(&text_draft("topic four"))
    );
    assert_eq!(
        session.draft_for(ChatId(16), DraftTopic::Forum(3)),
        Some(&text_draft("topic three"))
    );
    assert_eq!(session.chats.get(&16).unwrap().draft, None);

    session.mark_draft_dirty(ChatId(16), DraftTopic::Forum(3));
    assert!(session.draft_is_dirty(ChatId(16), DraftTopic::Forum(3)));
    assert!(!session.draft_is_dirty(ChatId(16), DraftTopic::Forum(4)));
    assert!(!session.draft_is_dirty(ChatId(16), DraftTopic::Chat));

    // `updateForumTopic` with a draft: applied to the clean topic, kept
    // out of the dirty one.
    for (id, text) in [(3, "stale three"), (4, "remote four")] {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateForumTopic","chat_id":16,"forum_topic_id":{id},"is_pinned":false,"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"notification_settings":{{"@type":"chatNotificationSettings"}},"draft_message":{}}}"#,
                draft_json(text)
            ),
        );
    }
    assert_eq!(
        session.draft_for(ChatId(16), DraftTopic::Forum(3)),
        Some(&text_draft("topic three"))
    );
    assert_eq!(
        session.draft_for(ChatId(16), DraftTopic::Forum(4)),
        Some(&text_draft("remote four"))
    );
    // A refetched topic list keeps the dirty topic's draft too.
    let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(16)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"forumTopics","@extra":"{}","total_count":2,"topics":[{},{}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
            extra.0,
            topic_json(16, 3, 0, Some("server three")),
            topic_json(16, 4, 0, Some("server four")),
        ),
    );
    assert_eq!(
        session.draft_for(ChatId(16), DraftTopic::Forum(3)),
        Some(&text_draft("topic three"))
    );
    assert_eq!(
        session.draft_for(ChatId(16), DraftTopic::Forum(4)),
        Some(&text_draft("server four"))
    );
    // Storing clears the dirty bit; a later update applies again.
    session.store_composer_draft(ChatId(16), DraftTopic::Forum(3), None);
    assert!(!session.draft_is_dirty(ChatId(16), DraftTopic::Forum(3)));
    assert_eq!(session.draft_for(ChatId(16), DraftTopic::Forum(3)), None);
}

#[test]
fn topic_draft_preview_hides_behind_unread_and_shows_reply_only() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    new_chat(
        &mut session,
        &seq,
        &sink,
        16,
        r#"{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false}"#,
    );
    let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(16)));
    let reply_only = r#"{"@type":"draftMessage","reply_to":{"@type":"inputMessageReplyToMessage","message_id":9,"quote":null,"checklist_task_id":0,"poll_option_id":""},"date":5,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}"#;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"forumTopics","@extra":"{}","total_count":2,"topics":[{},{}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
            extra.0,
            topic_json(16, 1, 2, Some("unread wins")),
            topic_json(16, 2, 0, None).replace(
                r#""draft_message":null"#,
                &format!(r#""draft_message":{reply_only}"#)
            ),
        ),
    );
    let topics = session.ordered_forum_topics(ChatId(16));
    let one = topics.iter().find(|t| t.forum_topic_id == 1).unwrap();
    assert!(one.draft.is_some());
    assert_eq!(one.draft_preview(), None, "unread messages beat the draft");
    let two = topics.iter().find(|t| t.forum_topic_id == 2).unwrap();
    let preview = two.draft_preview().expect("reply-only draft shows");
    assert_eq!(preview.text, "");
    assert!(preview.reply);
}

fn thread_message(id: i64, chat: i64, thread: i64, text: &str) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":{chat},"is_outgoing":false,"date":1700000000,"topic_id":{{"@type":"messageTopicThread","message_thread_id":{thread}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
    )
}

#[test]
fn a_thread_restores_its_draft_from_message_thread_info() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    new_chat(
        &mut session,
        &seq,
        &sink,
        13,
        r#"{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true}"#,
    );
    new_chat(
        &mut session,
        &seq,
        &sink,
        14,
        r#"{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false}"#,
    );
    session.open_chat(ChatId(13));
    session.begin_thread(ChatId(13), MessageId(101));
    // Resolving: the thread is not a slot yet.
    assert_eq!(session.open_draft_topic(ChatId(13)), DraftTopic::Chat);
    let extra = session.request(
        RequestPurpose::GetMessageThread { message_id: 101 },
        Some(ChatId(13)),
    );
    let reply_draft = r#"{"@type":"draftMessage","reply_to":{"@type":"inputMessageReplyToMessage","message_id":502,"quote":null,"checklist_task_id":0,"poll_option_id":""},"date":5,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"in the thread","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}"#;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messageThreadInfo","@extra":"{}","chat_id":14,"message_thread_id":501,"reply_info":{{"@type":"messageReplyInfo","reply_count":2,"recent_replier_ids":[],"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"last_message_id":503}},"unread_message_count":0,"messages":[{}],"draft_message":{}}}"#,
            extra.0,
            thread_message(501, 14, 501, "root"),
            reply_draft
        ),
    );
    session.open_chat(ChatId(14));
    assert_eq!(
        session.open_draft_topic(ChatId(14)),
        DraftTopic::Thread(501)
    );
    let draft = session
        .draft_for(ChatId(14), DraftTopic::Thread(501))
        .expect("thread draft restored");
    assert_eq!(draft.text, "in the thread");
    assert_eq!(draft.reply_to_message_id, Some(MessageId(502)));
    // The chat's own slot stays empty: the thread draft is not a chat draft.
    assert_eq!(session.draft_for(ChatId(14), DraftTopic::Chat), None);
    assert_eq!(session.chats.get(&14).unwrap().draft_preview(), None);

    // The replied-to message lives in the thread's history, not the chat's.
    let page = session.request(
        RequestPurpose::GetMessageThreadHistory { message_id: 101 },
        Some(ChatId(13)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{}]}}"#,
            page.0,
            thread_message(502, 14, 501, "a reply")
        ),
    );
    assert_eq!(
        session
            .reply_target(ChatId(14), MessageId(502))
            .map(effective_preview),
        Some("a reply".into())
    );
    assert!(session.reply_target(ChatId(14), MessageId(999)).is_none());

    // Storing into the thread slot and closing the thread.
    session.store_composer_draft(
        ChatId(14),
        DraftTopic::Thread(501),
        Some(text_draft("typed")),
    );
    assert_eq!(
        session.draft_for(ChatId(14), DraftTopic::Thread(501)),
        Some(&text_draft("typed"))
    );
    session.close_thread();
    assert_eq!(session.open_draft_topic(ChatId(14)), DraftTopic::Chat);
    assert_eq!(session.draft_for(ChatId(14), DraftTopic::Thread(501)), None);
}

#[test]
fn a_thread_inside_a_forum_topic_uses_the_topic_slot() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    forum(&mut session, &seq, &sink);
    session.open_chat(ChatId(16));
    session.select_topic(ChatId(16), 3);
    session.begin_thread(ChatId(16), MessageId(77));
    let extra = session.request(
        RequestPurpose::GetMessageThread { message_id: 77 },
        Some(ChatId(16)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messageThreadInfo","@extra":"{}","chat_id":16,"message_thread_id":77,"reply_info":{{"@type":"messageReplyInfo","reply_count":1,"recent_replier_ids":[],"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"last_message_id":78}},"unread_message_count":0,"messages":[],"draft_message":null}}"#,
            extra.0,
        ),
    );
    assert!(session.thread_for_chat(ChatId(16)).is_some());
    assert_eq!(session.open_draft_topic(ChatId(16)), DraftTopic::Forum(3));
}

#[test]
fn a_sent_message_clears_the_slot_it_went_to() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    new_chat(
        &mut session,
        &seq,
        &sink,
        9,
        r#"{"@type":"chatTypeSupergroup","supergroup_id":9,"is_channel":false}"#,
    );
    let sent = |topic: &str| {
        format!(
            r#"{{"@type":"updateMessageSendSucceeded","message":{{"id":9,"chat_id":9,"is_outgoing":true,{topic}"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}},"old_message_id":-1}}"#
        )
    };
    apply_json(&mut session, &seq, &sink, &sent(""));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &sent(r#""topic_id":{"@type":"messageTopicForum","forum_topic_id":3},"#),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &sent(r#""topic_id":{"@type":"messageTopicThread","message_thread_id":501},"#),
    );
    assert_eq!(
        session.messages.draft_clears,
        vec![
            (ChatId(9), DraftTopic::Chat),
            (ChatId(9), DraftTopic::Forum(3)),
            (ChatId(9), DraftTopic::Thread(501)),
        ]
    );
}

#[test]
fn chat_level_drafts_still_apply_and_respect_the_dirty_slot() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    new_chat(
        &mut session,
        &seq,
        &sink,
        8,
        r#"{"@type":"chatTypeBasicGroup","basic_group_id":8}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateChatDraftMessage","chat_id":8,"draft_message":{},"positions":[]}}"#,
            draft_json("group draft")
        ),
    );
    assert_eq!(
        session.draft_for(ChatId(8), DraftTopic::Chat),
        Some(&text_draft("group draft"))
    );
    assert_eq!(
        session.chats.get(&8).unwrap().sidebar_preview(),
        "Draft: group draft"
    );
    // Dirty only on a thread slot of the same chat: the chat update still applies.
    session.mark_draft_dirty(ChatId(8), DraftTopic::Thread(5));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatDraftMessage","chat_id":8,"draft_message":null,"positions":[]}"#,
    );
    assert_eq!(session.draft_for(ChatId(8), DraftTopic::Chat), None);
    // Dirty on the chat slot: the update is ignored.
    session.store_composer_draft(ChatId(8), DraftTopic::Chat, Some(text_draft("typing")));
    session.mark_draft_dirty(ChatId(8), DraftTopic::Chat);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateChatDraftMessage","chat_id":8,"draft_message":{},"positions":[]}}"#,
            draft_json("stale")
        ),
    );
    assert_eq!(
        session.draft_for(ChatId(8), DraftTopic::Chat),
        Some(&text_draft("typing"))
    );
    // A slot nobody holds is a no-op write.
    session.store_composer_draft(ChatId(8), DraftTopic::Forum(1), Some(text_draft("lost")));
    assert_eq!(session.draft_for(ChatId(8), DraftTopic::Forum(1)), None);
    assert!(!sink.rendered().contains("CANARY"));
}

#[test]
fn a_remote_draft_change_moves_the_revision_and_an_echo_does_not() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    new_chat(
        &mut session,
        &seq,
        &sink,
        8,
        r#"{"@type":"chatTypeBasicGroup","basic_group_id":8}"#,
    );
    let update = |text: &str| {
        format!(
            r#"{{"@type":"updateChatDraftMessage","chat_id":8,"draft_message":{},"positions":[]}}"#,
            draft_json(text)
        )
    };
    let start = session.remote_draft_revision();
    // Typed on another client: the composer showing this chat re-reads it.
    apply_json(&mut session, &seq, &sink, &update("from the phone"));
    let after_remote = session.remote_draft_revision();
    assert_ne!(after_remote, start);
    // Quill's own save echoed back by TDLib: nothing new to show.
    session.store_composer_draft(ChatId(8), DraftTopic::Chat, Some(text_draft("typed here")));
    apply_json(&mut session, &seq, &sink, &update("typed here"));
    assert_eq!(session.remote_draft_revision(), after_remote);
    // Skipped while the composer has unsaved changes.
    session.mark_draft_dirty(ChatId(8), DraftTopic::Chat);
    apply_json(&mut session, &seq, &sink, &update("stale"));
    assert_eq!(session.remote_draft_revision(), after_remote);
    // Cleared elsewhere.
    session.store_composer_draft(ChatId(8), DraftTopic::Chat, Some(text_draft("typed here")));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatDraftMessage","chat_id":8,"draft_message":null,"positions":[]}"#,
    );
    assert_ne!(session.remote_draft_revision(), after_remote);
    assert_eq!(session.draft_for(ChatId(8), DraftTopic::Chat), None);
}
