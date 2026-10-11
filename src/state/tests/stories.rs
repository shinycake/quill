//! State reducer tests: stories.
use super::common::*;
use super::*;

#[test]
fn edit_scheduled_message_refreshes_scheduled_list_not_history() {
    use crate::telegram::envelope::MessageSchedulingState;

    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.scheduled_messages.push(ParsedMessage {
        sender: None,
        id: MessageId(70),
        chat_id: ChatId(7),
        date: 0,
        is_outgoing: true,
        is_pinned: false,
        topic_id: None,
        thread_id: None,
        ephemeral: None,
        media_album_id: 0,
        author_signature: None,
        scheduling_state: Some(MessageSchedulingState::SendAtDate { send_date: 999 }),
        can_retry: false,
        send_state: Default::default(),
        content: MessageContent::Text("scheduled draft".into()),
        files: Vec::new(),
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
    });
    let extra = session.request(RequestPurpose::EditMessage, Some(ChatId(7)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"message","@extra":"{}","id":70,"chat_id":7,"is_outgoing":true,"scheduling_state":{{"@type":"messageSchedulingStateSendAtDate","send_date":999}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"edited draft","entities":[]}}}}}}"#,
            extra.0,
        ),
    );
    assert_eq!(session.scheduled_messages.len(), 1);
    assert_eq!(
        session.scheduled_messages[0].content.preview(),
        "edited draft"
    );
    assert!(
        session
            .histories
            .get(&7)
            .is_none_or(|h| !h.messages.contains_key(&70)),
        "edited scheduled send must not land in chat history"
    );
}

#[test]
fn edit_regular_message_still_upserts_history() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::EditMessage, Some(ChatId(7)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"message","@extra":"{}","id":71,"chat_id":7,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"edited","entities":[]}}}}}}"#,
            extra.0,
        ),
    );
    let history = session.histories.get(&7).expect("history row present");
    assert_eq!(
        history.messages.get(&71).unwrap().content.preview(),
        "edited"
    );
}

#[test]
fn stale_history_does_not_mix_chats() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    let extra = session.request(RequestPurpose::GetHistory, Some(ChatId(1)));
    session.open_chat(ChatId(2));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":5,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"from-a","entities":[]}}}}}}]}}"#,
            extra.0
        ),
    );
    assert!(
        session
            .histories
            .get(&1)
            .map(|h| h.messages.is_empty())
            .unwrap_or(true)
    );
    assert!(
        session
            .histories
            .get(&2)
            .map(|h| h.messages.is_empty())
            .unwrap_or(true)
    );
}

#[test]
fn photo_and_document_are_stored_and_update_file_completes_path() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    let thumb = media_file_json(1, "", false);
    let full = media_file_json(2, "", false);
    let doc = media_file_json(9, "", false);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":10,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":320,"height":240,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{full},"width":800,"height":600,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"CANARY_PHOTO_body","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":11,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"notes.txt","mime_type":"text/plain","document":{doc}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
        ),
    );
    assert_eq!(
        session
            .histories
            .get(&1)
            .unwrap()
            .messages
            .get(&10)
            .unwrap()
            .content
            .preview(),
        "CANARY_PHOTO_body"
    );
    assert_eq!(
        session
            .histories
            .get(&1)
            .unwrap()
            .messages
            .get(&11)
            .unwrap()
            .content
            .preview(),
        "notes.txt"
    );
    assert!(session.file(FileId(1)).unwrap().needs_download());
    assert_eq!(session.thumb_file_ids_to_download(), vec![FileId(1)]);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateFile","file":{}}}"#,
            media_file_json(1, "/tmp/quill-thumb.jpg", true)
        ),
    );
    assert_eq!(
        session.file(FileId(1)).unwrap().usable_path(),
        Some("/tmp/quill-thumb.jpg")
    );
    assert!(session.thumb_file_ids_to_download().is_empty());
    assert!(!sink.rendered().contains("CANARY_PHOTO"));
    assert!(!sink.rendered().contains("CANARY_REMOTE"));
    assert!(!sink.rendered().contains("/tmp/quill-thumb"));
}

#[test]
fn private_draft_restores_and_dirty_update_is_ignored() {
    let sink = Arc::new(MemorySink::new());
    let seq = AtomicU64::new(0);
    let mut session = Session::new(AccountKey::primary(), sink.clone());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0,"draft_message":{"@type":"draftMessage","reply_to":{"@type":"inputMessageReplyToMessage","message_id":101,"quote":null,"checklist_task_id":0,"poll_option_id":""},"date":1,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"meet at 6","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}}}"#,
    );
    assert!(session.accepts_composer_draft(ChatId(11)));
    let draft = session.chats.get(&11).unwrap().draft.clone().unwrap();
    assert_eq!(draft.text, "meet at 6");
    assert_eq!(draft.reply_to_message_id, Some(MessageId(101)));
    assert!(
        session
            .chats
            .get(&11)
            .unwrap()
            .sidebar_preview()
            .starts_with("Draft:")
    );
    session.mark_draft_dirty(ChatId(11));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatDraftMessage","chat_id":11,"draft_message":{"@type":"draftMessage","reply_to":null,"date":2,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"stale","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null},"positions":[]}"#,
    );
    assert_eq!(
        session.chats.get(&11).unwrap().draft.as_ref().unwrap().text,
        "meet at 6"
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":11,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
    );
    assert!(!session.accepts_composer_draft(ChatId(11)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0,"draft_message":{"@type":"draftMessage","reply_to":null,"date":1,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"nope","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}}}"#,
    );
    assert!(!session.accepts_composer_draft(ChatId(13)));
}

#[test]
fn topic_history_response_is_stored_per_topic() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":40,"messages":[{{"id":50,"chat_id":16,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_TOPIC_page1","entities":[]}}}}}},{{"id":40,"chat_id":16,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older","entities":[]}}}}}}]}}"#,
            extra.0
        ),
    );
    let history = session.threads.topic_histories.get(&(16, 2)).unwrap();
    assert_eq!(history.messages.len(), 2);
    assert_eq!(history.next_from_message_id, MessageId(40));
    assert!(!history.loaded_complete);
    // A response for another topic does not mix in.
    let extra_other =
        session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 3);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
            extra_other.0
        ),
    );
    assert_eq!(
        session
            .threads
            .topic_histories
            .get(&(16, 2))
            .unwrap()
            .messages
            .len(),
        2
    );
    let other = session.threads.topic_histories.get(&(16, 3)).unwrap();
    assert!(other.loaded_complete);
    assert!(other.messages.is_empty());
    // next_from_message_id 0 completes the first topic's history.
    let extra2 = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":30,"chat_id":16,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"oldest","entities":[]}}}}}}]}}"#,
            extra2.0
        ),
    );
    let history = session.threads.topic_histories.get(&(16, 2)).unwrap();
    assert!(history.loaded_complete);
    assert_eq!(history.messages.len(), 3);
}

#[test]
fn deleted_messages_leave_loaded_topic_histories() {
    // The topic view reads `topic_histories` only, so `updateDeleteMessages`
    // must reach it too — not just the chat's main history.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(16));
    let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            "{{\"@type\":\"foundChatMessages\",\"@extra\":\"{}\",{}}}",
            extra.0,
            r#""total_count":2,"next_from_message_id":0,"messages":[{"id":50,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"keep","entities":[]}}},{"id":40,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"gone","entities":[]}}}]"#
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateDeleteMessages","chat_id":16,"message_ids":[40],"is_permanent":true,"from_cache":false}"#,
    );
    let ids: Vec<i64> = session.threads.topic_histories[&(16, 2)]
        .messages
        .keys()
        .copied()
        .collect();
    assert_eq!(ids, vec![50]);
}

#[test]
fn topic_message_update_lands_in_loaded_topic_history() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(16));
    let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            "{{\"@type\":\"foundChatMessages\",\"@extra\":\"{}\",{}}}",
            extra.0,
            r#""total_count":1,"next_from_message_id":0,"messages":[{"id":50,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"seed","entities":[]}}}]"#
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":51,"chat_id":16,"is_outgoing":false,"topic_id":{"@type":"messageTopicForum","forum_topic_id":2},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_TOPIC_live","entities":[]}}}}"#,
    );
    let topic = session.threads.topic_histories.get(&(16, 2)).unwrap();
    assert!(topic.messages.values().any(|m| m.id == MessageId(51)));
    // Still in the main history (unchanged behavior).
    assert!(session.histories.get(&16).unwrap().contains(MessageId(51)));
    // Unloaded topic: no entry is created.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":52,"chat_id":16,"is_outgoing":false,"topic_id":{"@type":"messageTopicForum","forum_topic_id":9},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"unloaded","entities":[]}}}}"#,
    );
    assert!(!session.threads.topic_histories.contains_key(&(16, 9)));
    assert!(session.histories.get(&16).unwrap().contains(MessageId(52)));
    // A message with no topic stays a plain chat message.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":53,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"plain","entities":[]}}}}"#,
    );
    assert!(session.histories.get(&16).unwrap().contains(MessageId(53)));
}

#[test]
fn topic_send_succeeded_replaces_pending_row_in_topic_history() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(16));
    let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            "{{\"@type\":\"foundChatMessages\",\"@extra\":\"{}\",{}}}",
            extra.0, r#""total_count":0,"next_from_message_id":0,"messages":[]"#
        ),
    );
    // The `sendMessage` response: pending outgoing message with a
    // temporary (negative) id and the forum topic attached.
    let send_extra = session.request(RequestPurpose::SendMessage, Some(ChatId(16)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            "{{\"@type\":\"message\",\"@extra\":\"{}\",{}}}",
            send_extra.0,
            r#""id":-1,"chat_id":16,"is_outgoing":true,"topic_id":{"@type":"messageTopicForum","forum_topic_id":2},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_TOPIC_send","entities":[]}}"#
        ),
    );
    assert!(
        session
            .threads
            .topic_histories
            .get(&(16, 2))
            .unwrap()
            .messages
            .contains_key(&-1)
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageSendSucceeded","message":{"id":60,"chat_id":16,"is_outgoing":true,"topic_id":{"@type":"messageTopicForum","forum_topic_id":2},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_TOPIC_send","entities":[]}}},"old_message_id":-1}"#,
    );
    let topic = session.threads.topic_histories.get(&(16, 2)).unwrap();
    assert!(!topic.messages.contains_key(&-1));
    assert!(topic.messages.contains_key(&60));
}

#[test]
fn update_profile_accent_colors_stores_palette_and_ids() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateProfileAccentColors","colors":[{"@type":"profileAccentColor","id":3,"light_theme_colors":{"@type":"profileAccentColors","palette_colors":[43776,65280],"background_colors":[],"story_colors":[]},"dark_theme_colors":{"@type":"profileAccentColors","palette_colors":[262144],"background_colors":[],"story_colors":[]}}],"available_accent_color_ids":[1,3,5]}"#,
    );
    assert_eq!(
        session.chats_state.available_accent_color_ids,
        vec![1, 3, 5]
    );
    assert_eq!(session.chats_state.profile_accent_colors.len(), 1);
    assert_eq!(session.chats_state.profile_accent_colors[0].id, 3);
    assert_eq!(
        session.chats_state.profile_accent_colors[0].swatch_rgb(),
        0xAB00
    );
    // Replacement, not merge.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateProfileAccentColors","colors":[],"available_accent_color_ids":[7]}"#,
    );
    assert_eq!(session.chats_state.available_accent_color_ids, vec![7]);
    assert!(session.chats_state.profile_accent_colors.is_empty());
}

#[test]
fn update_accent_colors_stores_name_palette() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateAccentColors","colors":[{"@type":"accentColor","id":4242,"built_in_accent_color_id":3,"light_theme_colors":[1122867],"dark_theme_colors":[11189196],"min_channel_chat_boost_level":0}],"available_accent_color_ids":[4242]}"#,
    );
    assert_eq!(session.chats_state.name_accent_colors.len(), 1);
    assert_eq!(session.chats_state.name_accent_colors[0].id, 4242);
    // The renderer's lookup sees the server colors for a 7+ id.
    assert_eq!(
        crate::telegram::name_accent::name_color_rgb(4242, false),
        0x112233
    );
    assert_eq!(
        crate::telegram::name_accent::name_color_rgb(4242, true),
        0xAABBCC
    );
}

#[test]
fn story_tray_keeps_main_entries_sorted_by_order() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &tray_json(11, r#"{"@type":"storyListMain"}"#, 10, 0, &[5]),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &tray_json(12, r#"{"@type":"storyListMain"}"#, 30, 5, &[6]),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &tray_json(13, r#"{"@type":"storyListArchive"}"#, 50, 0, &[7]),
    );
    let tray = session.ordered_story_tray();
    // Archived entries drop out of the tray.
    assert_eq!(tray.len(), 2);
    // Sorted by (order, chat_id) descending (schema line 6781).
    assert_eq!(tray[0].chat_id, 12);
    assert_eq!(tray[1].chat_id, 11);
    assert!(tray[0].has_unread());
    assert!(tray[1].has_unread());
}

#[test]
fn story_tray_update_replaces_and_hides_entries() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &tray_json(11, r#"{"@type":"storyListMain"}"#, 10, 0, &[5]),
    );
    assert_eq!(session.ordered_story_tray().len(), 1);
    // Later update moves the chat to the archive list → tray hides it.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &tray_json(11, r#"{"@type":"storyListArchive"}"#, 10, 5, &[]),
    );
    assert!(session.ordered_story_tray().is_empty());
    assert!(!session.stories.tray.contains_key(&11));
}

#[test]
fn update_story_deleted_removes_cache_and_tray() {
    // Phase 9.2: `updateStoryDeleted` (schema 1.8.67 line 10898).
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &tray_json(11, r#"{"@type":"storyListMain"}"#, 10, 0, &[5]),
    );
    session.stories.stories.insert(
        (11, 5),
        crate::telegram::envelope::ParsedStory {
            id: 5,
            poster_chat_id: 11,
            date: 1,
            content: crate::telegram::envelope::StoryContentView::Unsupported,
            caption: String::new(),
            caption_entities: Vec::new(),
            chosen_reaction_emoji: None,
            chosen_reaction_extra: None,
            interaction_info: None,
            can_be_deleted: false,
            can_be_replied: false,
            can_get_interactions: false,
            can_be_edited: false,
            can_set_privacy_settings: false,
            can_be_forwarded: false,
            is_edited: false,
            repost_info: None,
            privacy_settings: None,
            area_link_url: None,
            area_reaction_emojis: Vec::new(),
            can_be_added_to_album: false,
            is_posted_to_chat_page: false,
            can_toggle_is_posted_to_chat_page: false,
            can_get_statistics: false,
            areas: Vec::new(),
        },
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateStoryDeleted","story_poster_chat_id":11,"story_id":5}"#,
    );
    assert!(!session.stories.stories.contains_key(&(11, 5)));
    // Tray no longer references the deleted story; without an unread
    // story left, the entry is dropped.
    assert!(session.ordered_story_tray().is_empty());
}

#[test]
fn story_viewers_accumulate_pages_and_drop_stale() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.begin_story_viewers(11, 5);
    let page = |extra: u64, offset: &str| {
        format!(
            r#"{{"@type":"storyInteractions","@extra":"{extra}","total_count":3,"interactions":[{{"actor_id":{{"@type":"messageSenderUser","user_id":777}},"interaction_date":1700000100,"block_list":null,"type":{{"@type":"storyInteractionTypeView","chosen_reaction_type":null}}}}],"next_offset":"{offset}"}}"#
        )
    };
    let extra1 = session.request_for_story(RequestPurpose::GetStoryInteractions, ChatId(11), 5);
    apply_json(&mut session, &seq, &sink, &page(extra1.0, "1"));
    let state = session.stories.viewers.as_ref().unwrap();
    assert_eq!(state.rows.len(), 1);
    assert_eq!(state.next_offset, "1");
    assert!(!state.loading);
    // Second page appends.
    let extra2 = session.request_for_story(RequestPurpose::GetStoryInteractions, ChatId(11), 5);
    apply_json(&mut session, &seq, &sink, &page(extra2.0, ""));
    assert_eq!(session.stories.viewers.as_ref().unwrap().rows.len(), 2);
    assert!(
        session
            .stories
            .viewers
            .as_ref()
            .unwrap()
            .next_offset
            .is_empty()
    );
    // Stale page for another story is dropped.
    session.begin_story_viewers(11, 6);
    let extra3 = session.request_for_story(RequestPurpose::GetStoryInteractions, ChatId(11), 5);
    apply_json(&mut session, &seq, &sink, &page(extra3.0, "9"));
    assert!(session.stories.viewers.as_ref().unwrap().rows.is_empty());
    // Error lands on the current panel.
    let extra4 = session.request_for_story(RequestPurpose::GetStoryInteractions, ChatId(11), 6);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"STORY_NOT_FOUND"}}"#,
            extra4.0
        ),
    );
    let state = session.stories.viewers.as_ref().unwrap();
    assert!(!state.loading);
    // The server message is classified by `error_reason` (native
    // text is never surfaced); the panel shows the failure.
    assert!(
        state
            .error
            .as_deref()
            .unwrap()
            .starts_with("Could not load viewers")
    );
}

#[test]
fn story_viewers_reopen_resets_rows() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let page = |extra: u64| {
        format!(
            r#"{{"@type":"storyInteractions","@extra":"{extra}","total_count":1,"interactions":[{{"actor_id":{{"@type":"messageSenderUser","user_id":777}},"interaction_date":1700000100,"block_list":null,"type":{{"@type":"storyInteractionTypeView","chosen_reaction_type":null}}}}],"next_offset":""}}"#
        )
    };
    // Open the panel: begin + first page.
    session.begin_story_viewers(11, 5);
    let extra1 = session.request_for_story(RequestPurpose::GetStoryInteractions, ChatId(11), 5);
    apply_json(&mut session, &seq, &sink, &page(extra1.0));
    assert_eq!(session.stories.viewers.as_ref().unwrap().rows.len(), 1);
    // `begin_story_viewers` alone keeps the same story's rows —
    // which is why the UI clears on re-open.
    session.begin_story_viewers(11, 5);
    assert_eq!(session.stories.viewers.as_ref().unwrap().rows.len(), 1);
    // Re-open (clear, then begin + fresh page-1) → no duplication.
    session.clear_story_viewers();
    session.begin_story_viewers(11, 5);
    let extra2 = session.request_for_story(RequestPurpose::GetStoryInteractions, ChatId(11), 5);
    apply_json(&mut session, &seq, &sink, &page(extra2.0));
    assert_eq!(session.stories.viewers.as_ref().unwrap().rows.len(), 1);
}
