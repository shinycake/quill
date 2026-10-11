//! Connect-driver tests: the gallery's profile tabs and tab-bar counts.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::state::SharedMediaTab;
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
    for method in [
        "getChatPostedToChatPageStories",
        "getReceivedGifts",
        "getUserProfileAudios",
    ] {
        assert!(sent.contains(&method.to_string()), "{method}");
    }
    let gallery = &driver.session.media.shared_media;
    assert_eq!(
        gallery.tabs[SharedMediaTab::Gifts.index()].status,
        crate::state::SharedMediaTabStatus::Loading
    );
    // Reopening keeps what was fetched and asks for nothing new.
    let before = recorder.snapshot().len();
    assert!(driver.open_shared_media().unwrap());
    assert_eq!(recorder.snapshot().len(), before);

    // A group has no stories, gifts or playlist.
    driver.close_shared_media();
    driver.session.open_chat = Some(ChatId(-5));
    let before = recorder.snapshot().len();
    assert!(driver.open_shared_media().unwrap());
    let sent = types_sent(&recorder)[before..].to_vec();
    for method in [
        "getChatPostedToChatPageStories",
        "getReceivedGifts",
        "getUserProfileAudios",
    ] {
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
