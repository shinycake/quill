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
    assert_eq!(
        chat.peer_activity_label(),
        Some("choosing a sticker".into())
    );
    assert_eq!(chat.sidebar_preview(), "choosing a sticker");
    // A typing peer alongside wins over the sticker picker (tdesktop).
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAction","chat_id":7,"sender_id":{"@type":"messageSenderUser","user_id":9},"action":{"@type":"chatActionTyping"}}"#,
    );
    assert_eq!(
        session.chats.get(&7).unwrap().peer_activity_label(),
        Some("typing".into())
    );
    // Cancel clears only the sticker sender; the typer remains.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAction","chat_id":7,"sender_id":{"@type":"messageSenderUser","user_id":7},"action":{"@type":"chatActionCancel"}}"#,
    );
    let chat = session.chats.get(&7).unwrap();
    assert_eq!(chat.peer_activity_label(), Some("typing".into()));
    assert_eq!(chat.sidebar_preview(), "typing");
}

#[test]
fn reset_all_notification_settings_ok_clears_cached_scope_settings() {
    // Parity slice: `resetAllNotificationSettings` (schema 1.8.67, line
    // 13671) confirmed — the cached
    // scope defaults drop so the next fetch (or the authoritative
    // `updateScopeNotificationSettings` answers) shows the server-confirmed
    // defaults instead of the stale pre-reset ones.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.scope_notification_settings.insert(
        NotificationSettingsScope::GroupChats,
        ScopeNotificationSettings {
            mute_for: 2147483647,
            ..ScopeNotificationSettings::default()
        },
    );
    let extra = session.request(RequestPurpose::ResetAllNotificationSettings, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(session.scope_notification_settings.is_empty());
}

#[test]
fn inapp_sounds_toggle_gates_notification_sound() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Demo group","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#,
    );
    // Scope fetched with no mute and the default sound.
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
    session.app_active = false;
    // Toggle on (default): existing behavior — the app default tone.
    let chat = session.chats.get(&14).unwrap();
    assert_eq!(
        session.notification_sound_for(chat),
        Some(notify::NotificationSoundKind::Default)
    );
    // Toggle off: no sound, regardless of mute/focus state.
    session.inapp_sounds_enabled = false;
    let chat = session.chats.get(&14).unwrap();
    assert!(session.notification_sound_for(chat).is_none());
}

/// Parity slice: `chatNotificationSettings` JSON with every
/// `use_default_*` flag set — a "reset to default" payload.
fn default_chat_notification_settings_json() -> &'static str {
    r#"{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}"#
}

/// Parity slice: `getChatNotificationSettingsExceptions` answer lands
/// per scope (correlated via `pending.scope`) and clears the
/// in-flight mark.
#[test]
fn notification_exceptions_answer_lands_per_scope() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_scope(
        RequestPurpose::GetChatNotificationSettingsExceptions,
        NotificationSettingsScope::PrivateChats,
    );
    session
        .notification_exceptions_loading
        .insert(NotificationSettingsScope::PrivateChats);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","total_count":2,"chat_ids":[11,12],"@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.notification_exceptions[&NotificationSettingsScope::PrivateChats],
        vec![11, 12]
    );
    assert!(
        !session
            .notification_exceptions_loading
            .contains(&NotificationSettingsScope::PrivateChats)
    );
}

/// Parity slice: a failed `getChatNotificationSettingsExceptions` must
/// not keep the scope in `notification_exceptions_loading` — otherwise
/// every later dialog open skips the fetch and the list stays
/// unfetchable.
#[test]
fn failed_notification_exceptions_fetch_retries() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_scope(
        RequestPurpose::GetChatNotificationSettingsExceptions,
        NotificationSettingsScope::GroupChats,
    );
    session
        .notification_exceptions_loading
        .insert(NotificationSettingsScope::GroupChats);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":500,"message":"CANARY_EXC_ERR","@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert!(
        !session
            .notification_exceptions_loading
            .contains(&NotificationSettingsScope::GroupChats)
    );
    assert!(
        !session
            .notification_exceptions
            .contains_key(&NotificationSettingsScope::GroupChats)
    );
}

/// Parity slice: `local_notification_exceptions` finds chats with any
/// non-default setting in the right scope (the screenshot demo's
/// answer path).
#[test]
fn local_notification_exceptions_scans_chats() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // Private chat with a custom sound.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"m","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0,"use_default_sound":false,"sound_id":"1","use_default_show_preview":true,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}}"#,
    );
    // Group chat with fully default settings.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#,
    );
    assert_eq!(
        session.local_notification_exceptions(NotificationSettingsScope::PrivateChats),
        vec![7]
    );
    assert!(
        session
            .local_notification_exceptions(NotificationSettingsScope::GroupChats)
            .is_empty()
    );
}

/// Parity slice: `updateChatNotificationSettings` keeps the cached
/// exceptions list honest — a reset to the scope default prunes just
/// the chat; any other change drops the scope's list so the next
/// dialog open refetches it.
#[test]
fn chat_notification_settings_update_refreshes_exceptions() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"m","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    );
    session
        .notification_exceptions
        .insert(NotificationSettingsScope::PrivateChats, vec![7, 9]);
    // Reset to default prunes just the chat.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateChatNotificationSettings","chat_id":7,"notification_settings":{}}}"#,
            default_chat_notification_settings_json()
        ),
    );
    assert_eq!(
        session.notification_exceptions[&NotificationSettingsScope::PrivateChats],
        vec![9]
    );
    // A custom change drops the whole cached list.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatNotificationSettings","chat_id":7,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":0,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}"#,
    );
    assert!(
        !session
            .notification_exceptions
            .contains_key(&NotificationSettingsScope::PrivateChats)
    );
}

#[test]
fn story_settings_effective_mute_and_poster_follow_scope_then_chat_exception() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // Private chat keeping all story defaults.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"m","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":true,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}}"#,
    );
    // Scope defaults: stories muted, poster shown — the chat's
    // `use_default_*` flags defer to these.
    session.scope_notification_settings.insert(
        NotificationSettingsScope::PrivateChats,
        ScopeNotificationSettings {
            mute_stories: true,
            show_story_poster: true,
            ..Default::default()
        },
    );
    let chat = session.chats.get(&7).unwrap();
    assert!(session.effective_story_muted(chat));
    assert!(session.effective_story_poster(chat));
    // Per-chat exceptions override the scope defaults.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatNotificationSettings","chat_id":7,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":true,"use_default_mute_stories":false,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":false,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}"#,
    );
    let chat = session.chats.get(&7).unwrap();
    assert!(!session.effective_story_muted(chat));
    assert!(!session.effective_story_poster(chat));
}

#[test]
fn group_chat_actions_name_the_sender_by_first_name() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","type":{"@type":"userTypeRegular"}}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Club","type":{"@type":"chatTypeBasicGroup","basic_group_id":21},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAction","chat_id":21,"sender_id":{"@type":"messageSenderUser","user_id":31},"action":{"@type":"chatActionRecordingVoiceNote"}}"#,
    );
    let chat = session.chats.get(&21).unwrap();
    assert_eq!(
        chat.peer_activity_label().as_deref(),
        Some("Ada is recording a voice message")
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAction","chat_id":21,"sender_id":{"@type":"messageSenderUser","user_id":31},"action":{"@type":"chatActionWatchingAnimations"}}"#,
    );
    assert_eq!(session.chats.get(&21).unwrap().peer_activity_label(), None);
}

const GROUP_CHAT: &str = r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Crew","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#;
const NEW_TEXT: &str = r#"{"@type":"updateNewMessage","message":{"id":42,"chat_id":14,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hey","entities":[]}}}}"#;
const READ_ALL: &str = r#"{"@type":"updateChatReadInbox","chat_id":14,"last_read_inbox_message_id":42,"unread_count":0}"#;

#[test]
fn reading_a_chat_withdraws_its_shown_notification() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, GROUP_CHAT);
    session.app_active = false;
    apply_json(&mut session, &seq, &sink, NEW_TEXT);
    assert_eq!(session.pending_notifications.len(), 1);
    // The UI shows the toast and drains the queue.
    session.pending_notifications.clear();
    apply_json(&mut session, &seq, &sink, READ_ALL);
    assert_eq!(session.pending_notification_clears, vec![ChatId(14)]);
    // Nothing is cleared twice.
    session.pending_notification_clears.clear();
    apply_json(&mut session, &seq, &sink, READ_ALL);
    assert!(session.pending_notification_clears.is_empty());
}

#[test]
fn reading_before_the_toast_is_shown_drops_the_queued_one() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, GROUP_CHAT);
    session.app_active = false;
    apply_json(&mut session, &seq, &sink, NEW_TEXT);
    apply_json(&mut session, &seq, &sink, READ_ALL);
    assert!(session.pending_notifications.is_empty());
}

#[test]
fn a_partial_read_keeps_the_notification() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, GROUP_CHAT);
    session.app_active = false;
    apply_json(&mut session, &seq, &sink, NEW_TEXT);
    session.pending_notifications.clear();
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatReadInbox","chat_id":14,"last_read_inbox_message_id":40,"unread_count":2}"#,
    );
    assert!(session.pending_notification_clears.is_empty());
}

#[test]
fn an_emptied_notification_group_withdraws_the_toast() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, GROUP_CHAT);
    session.app_active = false;
    apply_json(&mut session, &seq, &sink, NEW_TEXT);
    session.pending_notifications.clear();
    // Still has notifications: nothing happens.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNotificationGroup","notification_group_id":1,"type":{"@type":"notificationGroupTypeMessages"},"chat_id":14,"notification_settings_chat_id":14,"notification_sound_id":"0","total_count":1,"added_notifications":[],"removed_notification_ids":[5]}"#,
    );
    assert!(session.pending_notification_clears.is_empty());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNotificationGroup","notification_group_id":1,"type":{"@type":"notificationGroupTypeMessages"},"chat_id":14,"notification_settings_chat_id":14,"notification_sound_id":"0","total_count":0,"added_notifications":[],"removed_notification_ids":[6]}"#,
    );
    assert_eq!(session.pending_notification_clears, vec![ChatId(14)]);
}

#[test]
fn active_notifications_from_a_previous_launch_can_be_cleared() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, GROUP_CHAT);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateActiveNotifications","groups":[{"@type":"notificationGroup","id":1,"type":{"@type":"notificationGroupTypeMessages"},"chat_id":14,"total_count":2,"notifications":[]},{"@type":"notificationGroup","id":2,"type":{"@type":"notificationGroupTypeMessages"},"chat_id":15,"total_count":0,"notifications":[]}]}"#,
    );
    assert!(session.shown_notification_chats.contains(&ChatId(14)));
    assert!(!session.shown_notification_chats.contains(&ChatId(15)));
    apply_json(&mut session, &seq, &sink, READ_ALL);
    assert_eq!(session.pending_notification_clears, vec![ChatId(14)]);
}

const REACTION_SETTINGS_ALL: &str = r#"{"@type":"updateReactionNotificationSettings","notification_settings":{"@type":"reactionNotificationSettings","message_reaction_source":{"@type":"reactionNotificationSourceAll"},"story_reaction_source":{"@type":"reactionNotificationSourceNone"},"poll_vote_source":{"@type":"reactionNotificationSourceNone"},"sound_id":"-1","show_preview":true}}"#;
const REACTION_UPDATE: &str = r#"{"@type":"updateMessageUnreadReactions","chat_id":14,"message_id":7,"unread_reactions":[{"@type":"unreadReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"sender_id":{"@type":"messageSenderUser","user_id":31},"is_big":false}],"unread_reaction_count":COUNT}"#;
const REACTOR: &str = r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","is_contact":false,"type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusOffline","was_online":1}}}"#;

fn reaction_update(count: u32) -> String {
    REACTION_UPDATE.replace("COUNT", &count.to_string())
}

#[test]
fn a_new_reaction_notifies_when_the_setting_allows_it() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, GROUP_CHAT);
    apply_json(&mut session, &seq, &sink, REACTOR);
    session.app_active = false;
    // The default source is "none": the reaction badge updates silently.
    apply_json(&mut session, &seq, &sink, &reaction_update(1));
    assert!(session.pending_notifications.is_empty());
    assert_eq!(session.chats[&14].unread_reaction_count, 1);

    apply_json(&mut session, &seq, &sink, REACTION_SETTINGS_ALL);
    session.hide_notification_previews = false;
    apply_json(&mut session, &seq, &sink, &reaction_update(2));
    let queued = &session.pending_notifications;
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].title, "Crew");
    assert_eq!(
        queued[0].body,
        "Ada Lovelace reacted \u{2764} to your message"
    );
}

#[test]
fn reading_a_reaction_does_not_notify() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, GROUP_CHAT);
    session.app_active = false;
    apply_json(&mut session, &seq, &sink, REACTION_SETTINGS_ALL);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageUnreadReactions","chat_id":14,"message_id":7,"unread_reactions":[],"unread_reaction_count":0}"#,
    );
    assert!(session.pending_notifications.is_empty());
}

#[test]
fn the_contacts_source_ignores_strangers() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, GROUP_CHAT);
    apply_json(&mut session, &seq, &sink, REACTOR);
    session.app_active = false;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &REACTION_SETTINGS_ALL.replace(
            "reactionNotificationSourceAll",
            "reactionNotificationSourceContacts",
        ),
    );
    apply_json(&mut session, &seq, &sink, &reaction_update(1));
    assert!(session.pending_notifications.is_empty());
}

#[test]
fn default_auto_delete_roundtrip_and_failure() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetDefaultAutoDelete, None);
    session.default_auto_delete_busy = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messageAutoDeleteTime","time":604800,"@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert_eq!(session.default_auto_delete_secs, Some(604_800));
    assert!(!session.default_auto_delete_busy);

    let extra = session.request(
        RequestPurpose::Settings(SettingsPurpose::SetDefaultAutoDelete { seconds: 86_400 }),
        None,
    );
    session.default_auto_delete_busy = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(session.default_auto_delete_secs, Some(86_400));

    let extra = session.request(
        RequestPurpose::Settings(SettingsPurpose::SetDefaultAutoDelete { seconds: 0 }),
        None,
    );
    session.default_auto_delete_busy = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"nope","@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.default_auto_delete_secs,
        Some(86_400),
        "a refusal keeps the old value"
    );
    assert!(session.default_auto_delete_error.is_some());
    assert!(!session.default_auto_delete_busy);
}
