//! Connect-driver tests: story statistics, public forwards, public search.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::state::{Session, StorySearchQuery};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn sent_types(recorder: &RecordingSender) -> Vec<Value> {
    recorder
        .snapshot()
        .into_iter()
        .map(|json| serde_json::from_str::<Value>(&json).unwrap())
        .collect()
}

#[test]
fn story_insights_send_both_requests_only_when_allowed() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    // Unknown story, then one without `can_get_statistics`: refused.
    assert_eq!(
        driver.fetch_story_insights(ChatId(7), 5, false),
        Err(ConnectSendError::InvalidRequest)
    );
    seed_story(&mut driver, &seq, &dyn_sink, 7, 5, "storyContentPhoto", "");
    assert_eq!(
        driver.fetch_story_insights(ChatId(7), 5, false),
        Err(ConnectSendError::InvalidRequest)
    );

    seed_story(
        &mut driver,
        &seq,
        &dyn_sink,
        7,
        6,
        "storyContentPhoto",
        r#""can_get_statistics":true,"#,
    );
    driver
        .fetch_story_insights(ChatId(7), 6, true)
        .unwrap()
        .expect("sends");
    let sent = sent_types(&recorder);
    let stats = sent
        .iter()
        .find(|v| v["@type"] == "getStoryStatistics")
        .expect("statistics request");
    assert_eq!(stats["chat_id"], 7);
    assert_eq!(stats["story_id"], 6);
    assert_eq!(stats["is_dark"], true);
    let forwards = sent
        .iter()
        .find(|v| v["@type"] == "getStoryPublicForwards")
        .expect("forwards request");
    assert_eq!(forwards["story_poster_chat_id"], 7);
    assert_eq!(forwards["offset"], "");
    // A second tap while both are in flight sends nothing new.
    assert!(
        driver
            .fetch_story_insights(ChatId(7), 6, true)
            .unwrap()
            .is_none()
    );
    assert_eq!(sent_types(&recorder).len(), sent.len());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn public_story_search_pages_by_next_offset() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    assert_eq!(
        driver.search_public_stories(StorySearchQuery::Tag("#".into())),
        Err(ConnectSendError::InvalidRequest)
    );
    let extra = driver
        .search_public_stories(StorySearchQuery::Tag("#sunset".into()))
        .unwrap()
        .expect("sends");
    let first = sent_types(&recorder).pop().unwrap();
    assert_eq!(first["@type"], "searchPublicStoriesByTag");
    assert_eq!(first["tag"], "sunset");
    assert_eq!(first["offset"], "");
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"foundStories","@extra":"{}","total_count":2,"next_offset":"p2","stories":[{{"@type":"story","id":1,"poster_chat_id":21,"date":1,"content":{{"@type":"storyContentUnsupported"}}}}]}}"#,
                    extra.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver
        .load_more_found_stories()
        .unwrap()
        .expect("second page");
    let second = sent_types(&recorder).pop().unwrap();
    assert_eq!(second["offset"], "p2");
    // Nothing more to load while the page is in flight.
    assert!(driver.load_more_found_stories().unwrap().is_none());
    let _ = std::fs::remove_dir_all(dir);
}
