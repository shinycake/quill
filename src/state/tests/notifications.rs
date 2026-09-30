//! State reducer tests: notifications.
use super::common::*;
use super::*;

#[test]
fn notification_settings_mute_and_unmute() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"m","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":2147483647,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}}"#,
    );
    let chat = session.chats.get(&7).unwrap();
    assert!(chat.is_muted());
    assert!(chat.notification_settings.is_muted_forever());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatNotificationSettings","chat_id":7,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":0,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}"#,
    );
    assert!(!session.chats.get(&7).unwrap().is_muted());
}

#[test]
fn scope_mute_default_suppresses_toast_and_sound() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // A group chat that keeps the default mute setting.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Demo group","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#,
    );
    assert!(!session.chats.get(&14).unwrap().is_muted());
    // The GroupChats scope is muted "Forever" (as if set through the
    // scope-defaults dialog).
    let extra = session.request_for_scope(
        RequestPurpose::GetScopeNotificationSettings,
        NotificationSettingsScope::GroupChats,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &scope_settings_json(&extra.0.to_string(), 2147483647, true),
    );
    // App in background: the message would normally notify…
    session.app_active = false;
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":42,"chat_id":14,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hey","entities":[]}}}}"#,
    );
    // …but the scope default mute suppresses both the toast and the sound.
    assert!(session.pending_notifications.is_empty());
    assert!(session.pending_sound_plays.is_empty());
    let chat = session.chats.get(&14).unwrap();
    assert!(session.effective_muted(chat));
    assert!(session.notification_sound_for(chat).is_none());
    // Clearing the scope mute restores the toast and a sound decision.
    let extra = session.request_for_scope(
        RequestPurpose::GetScopeNotificationSettings,
        NotificationSettingsScope::GroupChats,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &scope_settings_json(&extra.0.to_string(), 0, true),
    );
    let chat = session.chats.get(&14).unwrap();
    assert!(!session.effective_muted(chat));
    // App background, unmuted, default sound → the app default tone.
    assert_eq!(
        session.notification_sound_for(chat),
        Some(notify::NotificationSoundKind::Default)
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":43,"chat_id":14,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hey again","entities":[]}}}}"#,
    );
    assert_eq!(session.pending_notifications.len(), 1);
    assert_eq!(
        session.pending_notifications[0].sound,
        Some(notify::NotificationSoundKind::Default)
    );
}

#[test]
fn scope_show_preview_default_gates_preview_body() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Demo group","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#,
    );
    // Scope fetched with message previews off.
    let extra = session.request_for_scope(
        RequestPurpose::GetScopeNotificationSettings,
        NotificationSettingsScope::GroupChats,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &scope_settings_json(&extra.0.to_string(), 0, false),
    );
    session.app_active = false;
    // Global previews enabled, but the scope default disables them.
    session.hide_notification_previews = false;
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":42,"chat_id":14,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"secret text","entities":[]}}}}"#,
    );
    assert_eq!(session.pending_notifications.len(), 1);
    assert_eq!(
        session.pending_notifications[0].for_display().body,
        "New message"
    );
    let chat = session.chats.get(&14).unwrap();
    assert!(!session.effective_preview_allowed(chat));
}

#[test]
fn failed_scope_settings_fetch_retries() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_scope(
        RequestPurpose::GetScopeNotificationSettings,
        NotificationSettingsScope::GroupChats,
    );
    // `maybe_fetch_scope_notification_settings` marks the scope in-flight
    // when it sends the request.
    session
        .scope_settings_loading
        .insert(NotificationSettingsScope::GroupChats);
    // TDLib answers with an error.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":500,"message":"CANARY_SCOPE_ERR","@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert!(
        !session
            .scope_settings_loading
            .contains(&NotificationSettingsScope::GroupChats),
        "failed fetch must free the scope for retry"
    );
    assert!(
        !session
            .scope_notification_settings
            .contains_key(&NotificationSettingsScope::GroupChats)
    );
    // A later fetch for the same scope lands normally.
    let extra = session.request_for_scope(
        RequestPurpose::GetScopeNotificationSettings,
        NotificationSettingsScope::GroupChats,
    );
    session
        .scope_settings_loading
        .insert(NotificationSettingsScope::GroupChats);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &scope_settings_json(&extra.0.to_string(), 0, true),
    );
    assert!(
        session
            .scope_notification_settings
            .contains_key(&NotificationSettingsScope::GroupChats)
    );
    assert!(
        !session
            .scope_settings_loading
            .contains(&NotificationSettingsScope::GroupChats)
    );
}

#[test]
fn phase81_desktop_notification_replay() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // Unmuted private chat, nothing read yet.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    );
    // App in background: incoming unread message queues a notification.
    // `hide_notification_previews` defaults to true → generic body.
    session.app_active = false;
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":42,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hey you","entities":[]}}}}"#,
    );
    assert_eq!(session.pending_notifications.len(), 1);
    let queued = &session.pending_notifications[0];
    assert_eq!(queued.chat_id, ChatId(7));
    assert_eq!(queued.title, "Ada");
    assert_eq!(queued.count, 1);
    assert_eq!(queued.for_display().body, "New message");
    session.pending_notifications.clear();

    // Previews enabled → body is the message preview.
    session.hide_notification_previews = false;
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":43,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hey you","entities":[]}}}}"#,
    );
    assert_eq!(session.pending_notifications.len(), 1);
    assert_eq!(
        session.pending_notifications[0].for_display().body,
        "hey you"
    );

    // Second message for the same chat coalesces into a burst summary.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":44,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"and more","entities":[]}}}}"#,
    );
    assert_eq!(session.pending_notifications.len(), 1);
    assert_eq!(session.pending_notifications[0].count, 2);
    assert_eq!(
        session.pending_notifications[0].for_display().body,
        "2 new messages"
    );
    session.pending_notifications.clear();
}

#[test]
fn phase81_desktop_notification_suppressed_cases() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // Muted chat.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":2147483647,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":true,"use_default_mute_stories":true,"mute_stories":false,"use_default_show_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}}"#,
    );
    // Unmuted chat.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Noor","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0}}"#,
    );
    session.app_active = false;
    session.hide_notification_previews = false;
    let incoming = |id: i64, chat_id: i64| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}}}"#
        )
    };
    // Muted → nothing.
    apply_json(&mut session, &seq, &sink, &incoming(1, 7));
    assert!(session.pending_notifications.is_empty());
    // Outgoing → nothing.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":2,"chat_id":8,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
    );
    assert!(session.pending_notifications.is_empty());
    // Unknown chat → nothing.
    apply_json(&mut session, &seq, &sink, &incoming(3, 99));
    assert!(session.pending_notifications.is_empty());
    // Currently open chat while the app is active → nothing.
    session.app_active = true;
    session.open_chat(ChatId(8));
    apply_json(&mut session, &seq, &sink, &incoming(4, 8));
    assert!(session.pending_notifications.is_empty());
    // Same chat, app in background → notifies.
    session.app_active = false;
    apply_json(&mut session, &seq, &sink, &incoming(5, 8));
    assert_eq!(session.pending_notifications.len(), 1);
    session.pending_notifications.clear();
    // Already-read message (at/below the inbox read marker) → nothing.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatReadInbox","chat_id":8,"last_read_inbox_message_id":6,"unread_count":0}"#,
    );
    apply_json(&mut session, &seq, &sink, &incoming(6, 8));
    assert!(session.pending_notifications.is_empty());
}

#[test]
fn chat_action_choosing_sticker_label() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAction","chat_id":7,"sender_id":{"@type":"messageSenderUser","user_id":7},"action":{"@type":"chatActionChoosingSticker"}}"#,
    );
    let chat = session.chats.get(&7).unwrap();
    assert!(!chat.is_peer_typing());
    assert_eq!(chat.peer_activity_label(), Some("choosing a sticker…"));
    assert_eq!(chat.sidebar_preview(), "choosing a sticker…");
    // A typing peer alongside keeps the sticker label (more specific wins).
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAction","chat_id":7,"sender_id":{"@type":"messageSenderUser","user_id":9},"action":{"@type":"chatActionTyping"}}"#,
    );
    assert_eq!(
        session.chats.get(&7).unwrap().peer_activity_label(),
        Some("choosing a sticker…")
    );
    // Cancel clears only the sticker sender; the typer remains.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAction","chat_id":7,"sender_id":{"@type":"messageSenderUser","user_id":7},"action":{"@type":"chatActionCancel"}}"#,
    );
    let chat = session.chats.get(&7).unwrap();
    assert_eq!(chat.peer_activity_label(), Some("typing…"));
    assert_eq!(chat.sidebar_preview(), "typing…");
}
