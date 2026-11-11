//! Connect-driver tests: the gallery's profile tabs and tab-bar counts.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::state::SharedMediaTab;
use crate::telegram::AuthorizationState;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn types_sent(recorder: &RecordingSender) -> Vec<String> {
    recorder
        .snapshot()
        .iter()
        .filter_map(|json| {
            let value: serde_json::Value = serde_json::from_str(json).ok()?;
            let kind = value["@type"].as_str()?.to_string();
            match value.get("filter").and_then(|f| f["@type"].as_str()) {
                Some(filter) => Some(format!("{kind}:{filter}")),
                None => Some(kind),
            }
        })
        .collect()
}

#[test]
fn opening_the_gallery_asks_for_counts_and_the_profile_tabs_of_a_user() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    for json in [
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":31},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":-5,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":5},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &sink).unwrap())
            .unwrap();
    }
    driver.session.open_chat = Some(ChatId(7));
    let before = recorder.snapshot().len();
    assert!(driver.open_shared_media().unwrap());
    let sent = types_sent(&recorder)[before..].to_vec();
    // The active tab, the info-panel counts, the two extra counts, and the
    // three profile tabs.
    assert!(sent.contains(&"searchChatMessages:searchMessagesFilterPhotoAndVideo".to_string()));
    for filter in [
        "searchMessagesFilterPhoto",
        "searchMessagesFilterVideo",
        "searchMessagesFilterPoll",
        "searchMessagesFilterVoiceAndVideoNote",
    ] {
        assert!(
            sent.contains(&format!("getChatMessageCount:{filter}")),
            "{filter}"
        );
    }
    for method in ["getChatPostedToChatPageStories", "getReceivedGifts"] {
        assert!(sent.contains(&method.to_string()), "{method}");
    }
    // Saved music is not a tab: it is asked for with the profile.
    assert!(!sent.contains(&"getUserProfileAudios".to_string()));
    let gallery = &driver.session.media.shared_media;
    assert_eq!(
        gallery.tabs[SharedMediaTab::Gifts.index()].status,
        crate::state::SharedMediaTabStatus::Loading
    );
    // Reopening keeps what was fetched and asks for nothing new.
    let before = recorder.snapshot().len();
    assert!(driver.open_shared_media().unwrap());
    assert_eq!(recorder.snapshot().len(), before);

    // A group has no stories or gifts.
    driver.close_shared_media();
    driver.session.open_chat = Some(ChatId(-5));
    let before = recorder.snapshot().len();
    assert!(driver.open_shared_media().unwrap());
    let sent = types_sent(&recorder)[before..].to_vec();
    for method in ["getChatPostedToChatPageStories", "getReceivedGifts"] {
        assert!(!sent.contains(&method.to_string()), "{method}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn splitting_media_fetches_the_tab_it_moves_to() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":31},"unread_count":0}}"#,
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver.session.open_chat = Some(ChatId(7));
    driver.open_shared_media().unwrap();
    let before = recorder.snapshot().len();
    driver.set_shared_media_split(true).unwrap();
    let sent = types_sent(&recorder)[before..].to_vec();
    assert_eq!(
        driver.session.media.shared_media.active_tab,
        SharedMediaTab::Photos
    );
    assert_eq!(
        sent,
        vec!["searchChatMessages:searchMessagesFilterPhoto".to_string()]
    );
    driver.set_shared_media_split(false).unwrap();
    assert_eq!(
        driver.session.media.shared_media.active_tab,
        SharedMediaTab::Media
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_playlist_is_asked_for_once_per_user_and_pages_on_demand() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let before = recorder.snapshot().len();
    driver.fetch_playlist(31, false).unwrap();
    driver.fetch_playlist(31, false).unwrap();
    let sent = recorder.snapshot()[before..].to_vec();
    assert_eq!(sent.len(), 1);
    let request: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
    assert_eq!(request["@type"], "getUserProfileAudios");
    assert_eq!(request["user_id"], 31);
    assert_eq!(request["offset"], 0);
    // Answer with one of two tracks, then ask for the next page.
    let answer = format!(
        r#"{{"@type":"audios","@extra":"{}","total_count":2,"audios":[{{"@type":"audio","duration":5,"title":"A","performer":"B","file_name":"a.mp3","mime_type":"audio/mpeg"}}]}}"#,
        request["@extra"].as_str().unwrap()
    );
    driver
        .ingest(copy_and_parse(&answer, &seq, &sink).unwrap())
        .unwrap();
    // Reloading the first page is a no-op once loaded.
    driver.fetch_playlist(31, false).unwrap();
    let before = recorder.snapshot().len();
    driver.fetch_playlist(31, true).unwrap();
    let sent = recorder.snapshot()[before..].to_vec();
    let request: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
    assert_eq!(request["offset"], 1);
    let _ = std::fs::remove_dir_all(dir);
}

/// Offline, opening the Playlist page shows a failure with Retry, and a
/// Retry that cannot send keeps it (13-087).
#[test]
fn an_offline_playlist_page_fails_and_keeps_its_retry() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.auth = AuthorizationState::WaitPhoneNumber;
    driver.open_playlist(31);
    let failed = |driver: &ConnectDriver<Arc<RecordingSender>>| {
        driver
            .session
            .media
            .playlists
            .get(&31)
            .is_some_and(|p| p.error.is_some() && !p.loading)
    };
    assert!(failed(&driver), "the page shows the failure, not Loading");
    driver.retry_playlist(31);
    assert!(failed(&driver), "Retry stays when nothing went out");
    // Back online: Retry asks again.
    driver.session.auth = AuthorizationState::Ready;
    let before = recorder.snapshot().len();
    driver.retry_playlist(31);
    assert_eq!(recorder.snapshot().len(), before + 1);
    assert!(driver.session.media.playlists[&31].loading);
    let _ = std::fs::remove_dir_all(dir);
}

/// A failed Stories tab says so (with Retry) instead of loading forever.
#[test]
fn a_failed_stories_tab_ends_its_loading() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let chat = r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":31},"unread_count":0}}"#;
    driver
        .ingest(copy_and_parse(chat, &seq, &sink).unwrap())
        .unwrap();
    driver.session.open_chat = Some(ChatId(7));
    driver.open_shared_media().unwrap();
    let request = sent_request(&recorder, "getChatPostedToChatPageStories");
    let failed = format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"INTERNAL"}}"#,
        request["@extra"].as_str().unwrap()
    );
    driver
        .ingest(copy_and_parse(&failed, &seq, &sink).unwrap())
        .unwrap();
    let tab = &driver.session.media.shared_media.tabs[SharedMediaTab::Stories.index()];
    assert_eq!(tab.status, crate::state::SharedMediaTabStatus::Failed);
    assert!(!tab.loading_more);
    let _ = std::fs::remove_dir_all(dir);
}
