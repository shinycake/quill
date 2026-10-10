//! Screenshot-demo fixtures for folder badges and notification sounds.

use super::*;

/// Parity slice: notification-sounds screenshot fixture — saved sounds
/// (`getSavedNotificationSounds` answer), all three scope defaults
/// (`getScopeNotificationSettings` answers, correlated via
/// `request_for_scope`), and chat 11 on a custom saved sound.
/// `ReadyNotifyOs` / `ReadyFolderBadges` fixture: unread-chat totals for the
/// main list and the two demo folders. "Work" has only muted unread chats,
/// "News" a mix, so the tabs show both badge colors.
pub(in crate::ui) fn apply_ready_folder_badges(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    for (list, all, unmuted) in [
        (r#"{"@type":"chatListMain"}"#, 12, 4),
        (r#"{"@type":"chatListFolder","chat_folder_id":1}"#, 5, 0),
        (r#"{"@type":"chatListFolder","chat_folder_id":2}"#, 3, 2),
    ] {
        let json = format!(
            r#"{{"@type":"updateUnreadChatCount","chat_list":{list},"total_count":30,"unread_count":{all},"unread_unmuted_count":{unmuted},"marked_as_unread_count":0,"marked_as_unread_unmuted_count":0}}"#
        );
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

pub(in crate::ui) fn apply_ready_notification_sound(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let sound_path = demo_media_allowlist()
        .join("demo-voice.ogg")
        .to_string_lossy()
        .into_owned();
    let sound_json = |id: i64, file_id: i32, title: &str, duration: i32| {
        format!(
            r#"{{"@type":"notificationSound","id":{id},"duration":{duration},"date":0,"title":{title},"data":"","sound":{file}}}"#,
            file = demo_file_json(file_id, &sound_path, true),
            title = serde_json::to_string(title).unwrap(),
        )
    };
    let sounds_extra = session.request(RequestPurpose::GetSavedNotificationSounds, None);
    let mut jsons = vec![
        format!(
            r#"{{"@type":"notificationSounds","notification_sounds":[{a},{b}],"@extra":"{extra}"}}"#,
            a = sound_json(1, 91, "Ding", 2),
            b = sound_json(2, 92, "Chime", 3),
            extra = sounds_extra.0,
        ),
        {
            let chat_settings = ChatNotificationSettings {
                use_default_mute_for: true,
                mute_for: 0,
                use_default_sound: false,
                sound_id: 1,
                use_default_show_preview: false,
                show_preview: true,
                ..Default::default()
            };
            format!(
                r#"{{"@type":"updateChatNotificationSettings","chat_id":11,"notification_settings":{}}}"#,
                notification_settings_json(&chat_settings)
            )
        },
    ];
    for scope in NotificationSettingsScope::ALL {
        let extra = session.request_for_scope(RequestPurpose::GetScopeNotificationSettings, scope);
        jsons.push(format!(
            r#"{{"@type":"scopeNotificationSettings","mute_for":0,"sound_id":"-1","show_preview":true,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"-1","use_default_show_story_poster":true,"show_story_poster":true,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false,"@extra":"{extra}"}}"#,
            extra = extra.0,
        ));
    }
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    // Parity slice: answer `getChatNotificationSettingsExceptions` per
    // scope from the demo chats' own settings, through the real
    // request/response correlation — chat 11's custom settings above make
    // it a PrivateChats exception.
    for scope in NotificationSettingsScope::ALL {
        let extra =
            session.request_for_scope(RequestPurpose::GetChatNotificationSettingsExceptions, scope);
        let ids = session.local_notification_exceptions(scope);
        let json = format!(
            r#"{{"@type":"chats","total_count":{n},"chat_ids":[{ids}],"@extra":"{extra}"}}"#,
            n = ids.len(),
            ids = ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(","),
            extra = extra.0,
        );
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}
