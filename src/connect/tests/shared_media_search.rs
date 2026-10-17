//! Connect-driver tests: search inside the gallery's Files, Music and
//! Links tabs.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::{SharedMediaTab, SharedMediaTabStatus};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn last_search(recorder: &RecordingSender) -> Value {
    recorder
        .snapshot()
        .iter()
        .rev()
        .filter_map(|json| serde_json::from_str::<Value>(json).ok())
        .find(|value| value["@type"] == "searchChatMessages")
        .expect("a searchChatMessages request")
}

#[test]
fn searching_a_tab_sends_the_query_with_its_filter_and_pages_with_it() {
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
    assert!(driver.open_shared_media().unwrap());
    // The Media tab does not search.
    let before = recorder.snapshot().len();
    driver
        .search_shared_media(SharedMediaTab::Media, "cat")
        .unwrap();
    assert_eq!(recorder.snapshot().len(), before);

    driver
        .select_shared_media_tab(SharedMediaTab::Files)
        .unwrap();
    driver
        .search_shared_media(SharedMediaTab::Files, "  report ")
        .unwrap();
    let request = last_search(&recorder);
    assert_eq!(request["query"], "report");
    assert_eq!(request["filter"]["@type"], "searchMessagesFilterDocument");
    assert_eq!(request["from_message_id"], 0);
    let gallery = &driver.session.media.shared_media;
    assert_eq!(gallery.active_query(), "report");
    assert_eq!(
        gallery.tab_at(SharedMediaTab::Files).unwrap().status,
        SharedMediaTabStatus::Loading
    );

    // The same text again asks for nothing.
    let before = recorder.snapshot().len();
    driver
        .search_shared_media(SharedMediaTab::Files, "report")
        .unwrap();
    assert_eq!(recorder.snapshot().len(), before);

    // Older pages keep the query.
    {
        let state = driver
            .session
            .media
            .shared_media
            .tab_state(SharedMediaTab::Files);
        state.status = SharedMediaTabStatus::Ready;
        state.next_from = MessageId(40);
    }
    driver
        .fetch_more_shared_media(SharedMediaTab::Files)
        .unwrap();
    let request = last_search(&recorder);
    assert_eq!(request["query"], "report");
    assert_eq!(request["from_message_id"], 40);

    // Clearing the text asks for the plain list.
    driver
        .search_shared_media(SharedMediaTab::Files, "")
        .unwrap();
    let request = last_search(&recorder);
    assert_eq!(request["query"], "");
    assert_eq!(request["filter"]["@type"], "searchMessagesFilterDocument");

    // Text typed for a tab that is no longer open searches nothing.
    driver
        .select_shared_media_tab(SharedMediaTab::Music)
        .unwrap();
    let before = recorder.snapshot().len();
    driver
        .search_shared_media(SharedMediaTab::Files, "late")
        .unwrap();
    assert_eq!(recorder.snapshot().len(), before);
    driver
        .select_shared_media_tab(SharedMediaTab::Files)
        .unwrap();

    // Leaving the tab drops the query.
    driver
        .search_shared_media(SharedMediaTab::Files, "again")
        .unwrap();
    driver
        .select_shared_media_tab(SharedMediaTab::Links)
        .unwrap();
    assert_eq!(
        driver
            .session
            .media
            .shared_media
            .tab_at(SharedMediaTab::Files)
            .unwrap()
            .query,
        ""
    );
    assert_eq!(last_search(&recorder)["query"], "");
    let _ = std::fs::remove_dir_all(dir);
}
