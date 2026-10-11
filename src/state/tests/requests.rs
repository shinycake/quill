//! State reducer tests: requests.
use super::common::*;
use super::*;

#[test]
fn chat_statistics_fetch_flow_loads_and_caches() {
    // Phase D2 replay: `getChatStatistics` request →
    // `chatStatisticsChannel` response lands `Loaded` under the
    // requested chat id, correlated through the pending request (the
    // response itself carries no chat id).
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetChatStatistics, Some(ChatId(13)));
    let graph = r#"{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""}"#;
    let value = r#"{"@type":"statisticalValue","value":1.0,"previous_value":1.0,"growth_rate_percentage":0.0}"#;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatStatisticsChannel","@extra":"{}","period":{{"@type":"dateRange","start_date":1788000000,"end_date":1788604800}},"member_count":{{"@type":"statisticalValue","value":12345.0,"previous_value":11700.0,"growth_rate_percentage":5.5}},"mean_message_view_count":{v},"mean_message_share_count":{v},"mean_message_reaction_count":{v},"mean_story_view_count":{v},"mean_story_share_count":{v},"mean_story_reaction_count":{v},"enabled_notifications_percentage":61.5,"member_count_graph":{g},"join_graph":{g},"mute_graph":{g},"view_count_by_hour_graph":{g},"view_count_by_source_graph":{g},"join_by_source_graph":{g},"language_graph":{g},"message_interaction_graph":{g},"message_reaction_graph":{g},"story_interaction_graph":{g},"story_reaction_graph":{g},"instant_view_interaction_graph":{g},"recent_interactions":[]}}"#,
            extra.0,
            g = graph,
            v = value,
        ),
    );
    let ChatStatisticsFetch::Loaded(boxed) = session
        .groups
        .chat_statistics
        .get(&13)
        .expect("statistics loaded")
    else {
        panic!("expected loaded channel statistics");
    };
    let ChatStatistics::Channel(stats) = boxed.as_ref() else {
        panic!("expected channel statistics");
    };
    assert_eq!(stats.member_count.value, 12345.0);
    assert_eq!(
        (stats.period_start, stats.period_end),
        (1788000000, 1788604800)
    );
}

#[test]
fn chat_statistics_fetch_flow_error_marks_failed() {
    // Phase D2 replay: a TDLib `error` for `getChatStatistics` lands
    // `Failed` so the panel shows an honest error, not a spinner.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetChatStatistics, Some(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_STATISTICS_NOT_AVAILABLE"}}"#,
            extra.0
        ),
    );
    let ChatStatisticsFetch::Failed(message) = session
        .groups
        .chat_statistics
        .get(&13)
        .expect("statistics failed")
    else {
        panic!("expected failed statistics");
    };
    assert!(message.contains("Could not load statistics"));
}

#[test]
fn optimistic_mutations_roll_back_on_error() {
    // Slice G1 replay: `setChatPermissions`,
    // `toggleSupergroupJoinByRequest`, and `setSupergroupUsername`
    // apply optimistically at send time. A TDLib error restores the
    // pre-request value carried on `PendingRequest::rollback` so the
    // UI never keeps showing a change the server rejected.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);

    // Permissions: optimistic write, then the error.
    let mut chat = placeholder_chat(ChatId(13));
    chat.permissions = Some(ChatPermissions::all());
    chat.can_send_basic_messages = true;
    session.chats.insert(13, chat);
    let extra = session.request(RequestPurpose::SetChatPermissions, Some(ChatId(13)));
    if let Some(pending) = session.requests.pending_mut(extra) {
        pending.rollback = Some(RequestRollback::ChatPermissions {
            previous: None,
            previous_can_send: false,
        });
    }
    // Simulate the optimistic write the driver performs at send.
    if let Some(chat) = session.chats.get_mut(&13) {
        chat.permissions = Some(ChatPermissions::all());
        chat.can_send_basic_messages = true;
    }
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
            extra.0
        ),
    );
    let chat = session.chats.get(&13).unwrap();
    assert_eq!(chat.permissions, None);
    assert!(!chat.can_send_basic_messages);

    // Join-by-request: previous flag restored.
    session.groups.supergroup_join_by_request.insert(21, true);
    let extra = session.request(
        RequestPurpose::ToggleSupergroupJoinByRequest,
        Some(ChatId(21)),
    );
    if let Some(pending) = session.requests.pending_mut(extra) {
        pending.rollback = Some(RequestRollback::JoinByRequest {
            supergroup_id: 21,
            previous: Some(true),
        });
    }
    session.groups.supergroup_join_by_request.insert(21, false);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.groups.supergroup_join_by_request.get(&21),
        Some(&true)
    );

    // Username: absent stays absent, present is restored.
    session
        .groups
        .supergroup_usernames
        .insert(22, "oldname".into());
    let extra = session.request(RequestPurpose::SetSupergroupUsername, Some(ChatId(22)));
    if let Some(pending) = session.requests.pending_mut(extra) {
        pending.rollback = Some(RequestRollback::SupergroupUsername {
            supergroup_id: 22,
            previous: Some("oldname".into()),
        });
    }
    session
        .groups
        .supergroup_usernames
        .insert(22, "newname".to_string());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"USERNAME_OCCUPIED"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session
            .groups
            .supergroup_usernames
            .get(&22)
            .map(String::as_str),
        Some("oldname")
    );
}

#[test]
fn cache_eviction_is_not_permanent() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":1,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateDeleteMessages","chat_id":1,"message_ids":[9],"is_permanent":false,"from_cache":true}"#,
    );
    let extra = session.request(RequestPurpose::GetHistory, Some(ChatId(1)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":9,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}]}}"#,
            extra.0
        ),
    );
    assert!(session.histories.get(&1).unwrap().messages.contains_key(&9));
}

#[test]
fn cache_eviction_does_not_punch_holes_in_loaded_history() {
    // `from_cache` deletions only drop TDLib's in-memory copy ("can
    // possibly be retrieved again", schema 1.8.67, line 10699); Telegram X
    // ignores them (`Tdlib.updateMessagesDeleted`). Removing the rows would
    // leave a hole that `fetch_history` (which pages from the oldest row)
    // never refills.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    for id in [10, 11, 12] {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"m{id}","entities":[]}}}}}}}}"#
            ),
        );
    }
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateDeleteMessages","chat_id":1,"message_ids":[11],"is_permanent":false,"from_cache":true}"#,
    );
    let ids: Vec<i64> = session
        .histories
        .get(&1)
        .unwrap()
        .messages
        .keys()
        .copied()
        .collect();
    assert_eq!(ids, vec![10, 11, 12]);
    // A real (permanent) deletion still removes and tombstones the row.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateDeleteMessages","chat_id":1,"message_ids":[11],"is_permanent":true,"from_cache":false}"#,
    );
    let history = session.histories.get(&1).unwrap();
    assert!(!history.messages.contains_key(&11));
    assert!(history.is_tombstone(MessageId(11)));
}

#[test]
fn load_chats_404_marks_exhaustion() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::LoadChats, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert!(session.chat_list.chats_exhausted);
    assert!(!sink.rendered().contains("Not Found"));
}

#[test]
fn chat_list_paging_restarts_after_logout_and_login() {
    // Exhaustion belongs to one authorization: after logging out, a fresh
    // Ready (or a new account in the same Session) must page every list
    // again from the start.
    for leaving in ["authorizationStateLoggingOut", "authorizationStateClosed"] {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let auth = |state: &str| {
            format!(
                r#"{{"@type":"updateAuthorizationState","authorization_state":{{"@type":"{state}"}}}}"#
            )
        };
        apply_json(&mut session, &seq, &sink, &auth("authorizationStateReady"));
        for purpose in [RequestPurpose::LoadChats, RequestPurpose::LoadArchiveChats] {
            let extra = session.request(purpose, None);
            apply_json(
                &mut session,
                &seq,
                &sink,
                &format!(
                    r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{}"}}"#,
                    extra.0
                ),
            );
        }
        session.chat_list.folder_chats_exhausted.insert(2);
        assert!(session.chat_list.chats_exhausted && session.chat_list.archive_chats_exhausted);
        apply_json(&mut session, &seq, &sink, &auth(leaving));
        assert!(!session.chat_list.chats_exhausted, "{leaving}");
        assert!(!session.chat_list.archive_chats_exhausted, "{leaving}");
        assert!(
            session.chat_list.folder_chats_exhausted.is_empty(),
            "{leaving}"
        );
    }
}

#[test]
fn take_purpose_drops_only_matching_request() {
    let (mut session, _) = session();
    let stats_id = session.request(RequestPurpose::GetStorageStatistics, None);
    let sounds_id = session.request(RequestPurpose::GetSavedNotificationSounds, None);
    let dropped = session
        .requests
        .take_purpose(RequestPurpose::GetStorageStatistics);
    assert_eq!(dropped.map(|r| r.id), Some(stats_id));
    assert!(
        !session
            .requests
            .has_purpose(RequestPurpose::GetStorageStatistics)
    );
    assert!(
        session
            .requests
            .has_purpose(RequestPurpose::GetSavedNotificationSounds)
    );
    assert!(session.requests.take(sounds_id).is_some());
}

#[test]
fn storage_statistics_answer_cached_by_purpose() {
    let (mut with_purpose, sink) = session();
    let (mut without_purpose, sink2) = session();
    let seq = AtomicU64::new(0);
    let extra = with_purpose.request(RequestPurpose::GetStorageStatistics, None);
    with_purpose.settings.storage_stats_loading = true;
    apply_json(
        &mut with_purpose,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"storageStatistics","size":6000,"count":2,"by_chat":[{{"chat_id":0,"size":6000,"count":2,"by_file_type":[{{"file_type":{{"@type":"fileTypeSecret"}},"size":4000,"count":1}},{{"file_type":{{"@type":"fileTypePhoto"}},"size":2000,"count":1}}]}}],"@extra":"{}"}}"#,
            extra.0
        ),
    );
    let stats = with_purpose.settings.storage_stats.expect("stats cached");
    assert_eq!(stats.total_size, 6000);
    assert!(
        stats
            .by_file_type
            .iter()
            .any(|t| t.file_type == "fileTypeSecret" && t.size == 4000 && t.count == 1),
        "secret category present"
    );
    assert!(!with_purpose.settings.storage_stats_loading);

    // A stray `storageStatistics` (no matching purpose) is ignored.
    let seq2 = AtomicU64::new(0);
    apply_json(
        &mut without_purpose,
        &seq2,
        &sink2,
        r#"{"@type":"storageStatistics","size":1,"count":1,"by_chat":[]}"#,
    );
    assert!(without_purpose.settings.storage_stats.is_none());
}

#[test]
fn storage_statistics_error_clears_loading() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetStorageStatistics, None);
    session.settings.storage_stats_loading = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"STORAGE_STATS_FAILED"}}"#,
            extra.0
        ),
    );
    assert!(session.settings.storage_stats.is_none());
    assert!(!session.settings.storage_stats_loading);
}

#[test]
fn chat_title_and_unread_updates() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":4,"title":"old","type":{"@type":"chatTypePrivate","user_id":4},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatTitle","chat_id":4,"title":"new title"}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatReadInbox","chat_id":4,"last_read_inbox_message_id":1,"unread_count":7}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPosition","chat_id":4,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"3","is_pinned":false}}"#,
    );
    let chat = session.chats.get(&4).unwrap();
    assert_eq!(chat.title, "new title");
    assert_eq!(chat.unread_count, 7);
    assert_eq!(chat.last_read_inbox_message_id.0, 1);
    assert!(chat.in_main_list);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPosition","chat_id":4,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"0","is_pinned":false}}"#,
    );
    assert!(!session.chats.get(&4).unwrap().in_main_list);
}

#[test]
fn opening_a_chat_clears_viewed_ids_for_that_generation() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    session.mark_viewed(ChatId(1), &[MessageId(5)]);
    assert!(session.histories.get(&1).unwrap().viewed.contains(&5));
    session.open_chat(ChatId(1));
    assert!(session.histories.get(&1).unwrap().viewed.is_empty());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":5,"chat_id":1,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
    );
    assert_eq!(session.message_ids_to_view(ChatId(1)), vec![MessageId(5)]);
    session.mark_viewed(ChatId(1), &[MessageId(5)]);
    assert!(session.message_ids_to_view(ChatId(1)).is_empty());
}

#[test]
fn update_chat_photo_swaps_and_clears() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let photo = |id: i32| {
        format!(
            r#""photo":{{"@type":"chatPhotoInfo","small":{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}}}},"big":null,"minithumbnail":null,"has_animation":false,"is_personal":false}}"#
        )
    };
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            "{{\"@type\":\"updateChatPhoto\",\"chat_id\":11,{}}}",
            photo(91)
        ),
    );
    assert_eq!(session.chats.get(&11).unwrap().photo_file_id, Some(91));
    assert!(session.files.contains_key(&91));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            "{{\"@type\":\"updateChatPhoto\",\"chat_id\":11,{}}}",
            photo(95)
        ),
    );
    assert_eq!(session.chats.get(&11).unwrap().photo_file_id, Some(95));
    // Photo removed → fallback avatar.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPhoto","chat_id":11,"photo":null}"#,
    );
    assert_eq!(session.chats.get(&11).unwrap().photo_file_id, None);
}

#[test]
fn topic_selection_state() {
    let (mut session, _sink) = session();
    session.open_chat(ChatId(16));
    assert_eq!(session.open_topic, None);
    session.select_topic(ChatId(16), 2);
    assert_eq!(session.open_topic, Some(2));
    session.deselect_topic();
    assert_eq!(session.open_topic, None);
    session.select_topic(ChatId(16), 2);
    session.open_chat(ChatId(17));
    assert_eq!(session.open_topic, None);
}

#[test]
fn chat_permissions_gate_topic_composer() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let permissions = r#""permissions":{"@type":"chatPermissions","can_send_basic_messages":false,"can_send_audios":true,"can_send_documents":true,"can_send_photos":true,"can_send_videos":true,"can_send_video_notes":true,"can_send_voice_notes":true,"can_send_polls":true,"can_send_other_messages":true,"can_add_link_previews":true,"can_react_to_messages":true,"can_edit_tag":false,"can_change_info":false,"can_invite_users":true,"can_pin_messages":false,"can_create_topics":false}"#;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            "{{\"@type\":\"updateNewChat\",\"chat\":{{\"id\":16,\"title\":\"Demo forum\",\"type\":{{\"@type\":\"chatTypeSupergroup\",\"supergroup_id\":16,\"is_channel\":false}},{permissions},\"unread_count\":0}}}}",
            permissions = permissions
        ),
    );
    assert!(!session.chats.get(&16).unwrap().can_send_basic_messages);
    let permissions_on = permissions.replace(
        "\"can_send_basic_messages\":false",
        "\"can_send_basic_messages\":true",
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            "{{\"@type\":\"updateChatPermissions\",\"chat_id\":16,{}}}",
            permissions_on
        ),
    );
    assert!(session.chats.get(&16).unwrap().can_send_basic_messages);
}

#[test]
fn info_panel_open_close() {
    let (mut session, _sink) = session();
    assert!(session.open_info_panel.is_none());
    session.open_info_panel = Some(InfoPanelTarget::User(31));
    assert_eq!(session.open_info_panel, Some(InfoPanelTarget::User(31)));
    session.open_info_panel = Some(InfoPanelTarget::Supergroup(77));
    assert_eq!(
        session.open_info_panel,
        Some(InfoPanelTarget::Supergroup(77))
    );
    session.open_info_panel = None;
    assert!(session.open_info_panel.is_none());
}

#[test]
fn open_ready_secret_chat_for_user_gates_on_ready() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // Deterministic 36-byte key_hash (same fixture as the
    // key-verification screenshot demo).
    let hash_b64 = "GUYMUT5VLuA6j7l7taiDAR9tM+Y30on50Cklur/t+/w57sWo";
    let secret_ready = format!(
        r#"{{"@type":"updateSecretChat","secret_chat":{{"@type":"secretChat","id":7,"user_id":41,"state":{{"@type":"secretChatStateReady"}},"is_outbound":true,"key_hash":"{hash_b64}","layer":144}}}}"#
    );
    apply_json(&mut session, &seq, &sink, &secret_ready);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":41,"title":"Zed","type":{"@type":"chatTypeSecret","secret_chat_id":7,"user_id":41},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":8,"user_id":43,"state":{"@type":"secretChatStatePending"},"is_outbound":false,"key_hash":"","layer":144}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":42,"title":"Wendy","type":{"@type":"chatTypeSecret","secret_chat_id":8,"user_id":43},"unread_count":0}}"#,
    );

    session.open_chat(ChatId(41));
    let record = session
        .open_ready_secret_chat_for_user(41)
        .expect("Ready secret chat record");
    assert_eq!(record.id, 7);
    assert_eq!(record.key_hash.len(), 36);
    let pixels = crate::key_fingerprint::key_hash_pixels(&record.key_hash)
        .expect("36-byte hash is renderable");
    assert_eq!(pixels.len(), 144);
    // Wrong partner → None.
    assert!(session.open_ready_secret_chat_for_user(999).is_none());
    // The header opens the partner's panel for secret chats too.
    assert_eq!(
        session.info_panel_target_for_chat(ChatId(41)),
        Some(InfoPanelTarget::User(41))
    );

    // Pending chat → no record for the key UI.
    session.open_chat(ChatId(42));
    assert!(session.open_ready_secret_chat_for_user(43).is_none());
}

#[test]
fn chat_can_invite_users_gate() {
    // Phase D3a: Invite permissions follow channel and supergroup membership rights.
    let (mut session, _) = session();

    assert!(!session.chat_can_invite_users(ChatId(999)));

    let mut channel = placeholder_chat(ChatId(13));
    channel.kind = ChatKind::Supergroup {
        supergroup_id: 13,
        is_channel: true,
    };
    session.chats.insert(13, channel);
    assert!(!session.chat_can_invite_users(ChatId(13)));

    session
        .chats
        .get_mut(&13)
        .unwrap()
        .set_member_status(ChannelMemberStatus::Creator, None);
    assert!(session.chat_can_invite_users(ChatId(13)));

    {
        let channel = session.chats.get_mut(&13).unwrap();
        channel.set_member_status(ChannelMemberStatus::Administrator, None);
        channel.set_admin_can_invite_users(Some(true));
    }
    assert!(session.chat_can_invite_users(ChatId(13)));

    session
        .chats
        .get_mut(&13)
        .unwrap()
        .set_admin_can_invite_users(Some(false));
    assert!(!session.chat_can_invite_users(ChatId(13)));

    session
        .chats
        .get_mut(&13)
        .unwrap()
        .set_admin_can_invite_users(None);
    assert!(!session.chat_can_invite_users(ChatId(13)));

    let mut creator_group = placeholder_chat(ChatId(14));
    creator_group.kind = ChatKind::Supergroup {
        supergroup_id: 14,
        is_channel: false,
    };
    session.chats.insert(14, creator_group);
    session
        .groups
        .supergroup_member_status
        .insert(14, ChannelMemberStatus::Creator);
    assert!(session.chat_can_invite_users(ChatId(14)));

    let mut admin_group = placeholder_chat(ChatId(15));
    admin_group.kind = ChatKind::Supergroup {
        supergroup_id: 15,
        is_channel: false,
    };
    session.chats.insert(15, admin_group);
    session
        .groups
        .supergroup_member_status
        .insert(15, ChannelMemberStatus::Administrator);
    session.groups.supergroup_invite_right.insert(15, true);
    assert!(session.chat_can_invite_users(ChatId(15)));

    session.groups.supergroup_invite_right.insert(15, false);
    assert!(!session.chat_can_invite_users(ChatId(15)));

    let mut member_group = placeholder_chat(ChatId(16));
    member_group.kind = ChatKind::Supergroup {
        supergroup_id: 16,
        is_channel: false,
    };
    session.chats.insert(16, member_group);
    session
        .groups
        .supergroup_member_status
        .insert(16, ChannelMemberStatus::Member);
    assert!(!session.chat_can_invite_users(ChatId(16)));
}

#[test]
fn cl_chat_preview_cached_for_unopened_chat() {
    // Slice CL: a `getChatHistory` answer for `GetChatPreview` is
    // retained under the requested chat even when that chat is not
    // open (the normal history branch drops non-open answers);
    // nothing leaks into the chat's history. A failure marks the
    // preview failed so the panel shows an error, not a spinner.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetChatPreview, Some(ChatId(12)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","messages":[{{"@type":"message","id":7,"chat_id":12,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}],"total_count":1}}"#,
            extra.0
        ),
    );
    let fetch = session
        .chat_list
        .chat_preview_fetch
        .as_ref()
        .expect("preview fetch");
    assert_eq!(fetch.chat_id, ChatId(12));
    assert_eq!(fetch.messages.len(), 1);
    assert_eq!(fetch.failed, None);
    assert!(
        session
            .histories
            .get(&12)
            .is_none_or(|history| history.ordered().is_empty()),
        "preview must not merge into the chat's history"
    );
    let extra = session.request(RequestPurpose::GetChatPreview, Some(ChatId(12)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"SOMETHING"}}"#,
            extra.0
        ),
    );
    let fetch = session
        .chat_list
        .chat_preview_fetch
        .as_ref()
        .expect("preview fetch");
    assert!(
        fetch
            .failed
            .as_ref()
            .unwrap()
            .contains("Could not load preview")
    );
}

#[test]
fn avatar_click_routes_to_the_senders_profile() {
    // tdesktop `Element::fromLink`: user -> user profile (bots too), a
    // chat sender (anonymous admin / channel) -> that chat's profile.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for chat in [
        r#"{"id":-1001,"title":"Group","type":{"@type":"chatTypeSupergroup","supergroup_id":1001,"is_channel":false},"unread_count":0}"#,
        r#"{"id":-1002,"title":"Channel","type":{"@type":"chatTypeSupergroup","supergroup_id":1002,"is_channel":true},"unread_count":0}"#,
        r#"{"id":-5,"title":"Basic","type":{"@type":"chatTypeBasicGroup","basic_group_id":5},"unread_count":0}"#,
    ] {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"updateNewChat","chat":{chat}}}"#),
        );
    }
    assert_eq!(
        session.avatar_profile_target(MessageSender::User { user_id: 77 }),
        Some(InfoPanelTarget::User(77))
    );
    // Anonymous admin: the group itself is the sender.
    assert_eq!(
        session.avatar_profile_target(MessageSender::Chat { chat_id: -1001 }),
        Some(InfoPanelTarget::Supergroup(1001))
    );
    // A channel posting into its discussion group.
    assert_eq!(
        session.avatar_profile_target(MessageSender::Chat { chat_id: -1002 }),
        Some(InfoPanelTarget::Supergroup(1002))
    );
    assert_eq!(
        session.avatar_profile_target(MessageSender::Chat { chat_id: -5 }),
        Some(InfoPanelTarget::BasicGroup(5))
    );
    // Unknown chat: nothing to open.
    assert_eq!(
        session.avatar_profile_target(MessageSender::Chat { chat_id: -999 }),
        None
    );
}
