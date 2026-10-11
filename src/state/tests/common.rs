//! Shared test helpers for state reducer tests.
use super::*;
pub(crate) use crate::diagnostics::MemorySink;
pub(crate) use crate::telegram::client::copy_and_parse;
pub(crate) use crate::telegram::envelope::ChatEventAction;
pub(crate) use crate::telegram::envelope::LocalFileState;
pub(crate) use std::sync::atomic::AtomicU64;

pub(crate) fn session() -> (Session, Arc<MemorySink>) {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    (Session::new(AccountKey::primary(), dyn_sink), sink)
}

pub(crate) fn apply_json(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    json: &str,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let owned = copy_and_parse(json, seq, &dyn_sink).unwrap();
    session.apply(owned);
}

pub(crate) fn scope_settings_json(extra: &str, mute_for: i32, show_preview: bool) -> String {
    format!(
        r#"{{"@type":"scopeNotificationSettings","mute_for":{mute_for},"sound_id":"-1","show_preview":{show_preview},"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"-1","use_default_show_story_poster":true,"show_story_poster":true,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false,"@extra":"{extra}"}}"#,
    )
}

pub(crate) fn media_file_json(id: i32, path: &str, completed: bool) -> String {
    format!(
        r#"{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":{path},"can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}"#,
        path = serde_json::to_string(path).unwrap(),
        completed = completed,
    )
}

pub(crate) fn file_reply_json(extra: u64, id: i32, active: bool) -> String {
    format!(
        r#"{{"@type":"file","@extra":"{extra}","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":{active},"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}"#
    )
}

pub(crate) fn tray_json(
    chat_id: i64,
    list: &str,
    order: i64,
    max_read: i32,
    story_ids: &[i32],
) -> String {
    let stories = story_ids
        .iter()
        .map(|id| {
            format!(
                r#"{{"@type":"storyInfo","story_id":{id},"date":1,"is_for_close_friends":false,"is_live":false}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"@type":"updateChatActiveStories","active_stories":{{"@type":"chatActiveStories","chat_id":{chat_id},"list":{list},"order":"{order}","can_be_archived":false,"max_read_story_id":{max_read},"stories":[{stories}]}}}}"#
    )
}

pub(crate) fn seed_slow_mode_group(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    chat_id: i64,
    status_json: Option<&str>,
    full_info_fields: &str,
    is_channel: bool,
) -> u64 {
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Slow group","type":{{"@type":"chatTypeSupergroup","supergroup_id":{chat_id},"is_channel":{is_channel}}},"unread_count":0}}}}"#
        ),
    );
    if let Some(status) = status_json {
        apply_json(
            session,
            seq,
            sink,
            &format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{chat_id},"is_forum":false,"status":{status}}}}}"#
            ),
        );
    }
    let extra = session.request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, chat_id);
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"supergroupFullInfo","@extra":"{}","description":"d","member_count":10,{}}}"#,
            extra.0, full_info_fields
        ),
    );
    session.groups.supergroup_full_infos[&chat_id].fetched_at_ms
}

pub(crate) const SLOW_MODE_FIELDS: &str = r#""slow_mode_delay":30,"slow_mode_delay_expires_in":25.0,"my_boost_count":0,"unrestrict_boost_count":0"#;
pub(crate) const MEMBER_STATUS: &str = r#"{"@type":"chatMemberStatusMember"}"#;
