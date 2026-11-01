//! Connect-driver tests: a forum topic's window pages its newest end back
//! in once it stopped short of the latest message, and jumping to the
//! latest replaces it.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Fixture {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    dyn_sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    dir: std::path::PathBuf,
}

impl Fixture {
    /// Forum 16 open on topic 3, its first page (messages 101..=150) in.
    fn topic_open() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        let mut f = Self {
            driver,
            recorder,
            dyn_sink,
            seq,
            dir,
        };
        f.ingest(r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#);
        f.ingest(r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true,"status":{"@type":"chatMemberStatusMember"}}}"#);
        f.driver.select_chat(ChatId(16)).unwrap();
        f.driver.select_topic(3).unwrap();
        f.answer_page(&(101..=150).rev().collect::<Vec<_>>(), 101);
        f
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.dyn_sink).unwrap())
            .unwrap();
    }

    fn last_search(&self) -> Value {
        self.recorder
            .snapshot()
            .into_iter()
            .map(|j| serde_json::from_str::<Value>(&j).unwrap())
            .rfind(|v| v["@type"] == "searchChatMessages")
            .expect("searchChatMessages sent")
    }

    /// Answer the latest `searchChatMessages` with `ids`, newest first.
    fn answer_page(&mut self, ids: &[i64], next_from: i64) {
        let extra = self.last_search()["@extra"].as_str().unwrap().to_owned();
        let messages: Vec<String> = ids
            .iter()
            .map(|id| format!(r#"{{"id":{id},"chat_id":16,"is_outgoing":false,"topic_id":{{"@type":"messageTopicForum","forum_topic_id":3}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"m","entities":[]}}}}}}"#))
            .collect();
        self.ingest(&format!(
            r#"{{"@type":"foundChatMessages","@extra":"{extra}","total_count":1000,"next_from_message_id":{next_from},"messages":[{}]}}"#,
            messages.join(",")
        ));
    }

    fn topic(&mut self) -> &mut crate::state::TopicHistory {
        self.driver
            .session
            .threads
            .topic_histories
            .get_mut(&(16, 3))
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_topic_at_its_latest_asks_for_no_newer_page() {
    let mut f = Fixture::topic_open();
    assert_eq!(f.driver.fetch_topic_history_newer().unwrap(), None);
}

#[test]
fn a_topic_short_of_its_latest_pages_newer_from_its_newest_row() {
    let mut f = Fixture::topic_open();
    f.topic().has_newer = true;
    f.topic().latest_seen = 300;
    assert!(f.driver.fetch_topic_history_newer().unwrap().is_some());
    let request = f.last_search();
    assert_eq!(request["from_message_id"], 150);
    assert_eq!(request["offset"], -(TOPIC_HISTORY_PAGE_SIZE - 1));
    assert_eq!(request["limit"], TOPIC_HISTORY_PAGE_SIZE);
    assert_eq!(request["topic_id"]["forum_topic_id"], 3);
    assert_eq!(
        f.driver.fetch_topic_history_newer().unwrap(),
        None,
        "one at a time"
    );
    f.answer_page(&(150..=199).rev().collect::<Vec<_>>(), 0);
    assert_eq!(f.topic().newest_loaded_id(), Some(MessageId(199)));
    assert!(f.topic().has_newer, "300 is still ahead");
    assert!(f.driver.fetch_topic_history_newer().unwrap().is_some());
    f.answer_page(&[199], 0);
    assert!(!f.topic().has_newer, "nothing newer: at the latest");
    assert_eq!(
        f.topic().messages.keys().next(),
        Some(&101),
        "older rows stay"
    );
}

#[test]
fn jumping_to_the_latest_replaces_a_short_window_with_the_latest_page() {
    let mut f = Fixture::topic_open();
    assert_eq!(
        f.driver.topic_jump_to_latest().unwrap(),
        None,
        "already there"
    );
    f.topic().has_newer = true;
    assert!(f.driver.topic_jump_to_latest().unwrap().is_some());
    assert_eq!(f.last_search()["from_message_id"], 0);
    assert!(f.topic().messages.is_empty());
    f.answer_page(&(951..=1000).rev().collect::<Vec<_>>(), 951);
    assert_eq!(f.topic().newest_loaded_id(), Some(MessageId(1000)));
    assert!(!f.topic().has_newer);
}
