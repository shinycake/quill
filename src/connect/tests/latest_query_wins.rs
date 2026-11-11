//! Connect-driver tests: a list fed by pages shows the answer to the query
//! it last asked (round 11, area A). Recent actions match each page and
//! poll to the query and filters that asked; Recent calls match each page
//! to the request the list waits for. A newer refresh is sent at once and
//! replaces the older one.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::ChatEventLogFetch;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::ChannelMemberStatus;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CHANNEL: ChatId = ChatId(13);

struct Fixture {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    dir: std::path::PathBuf,
}

impl Fixture {
    /// A channel we administer, so its recent actions can be read.
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let driver = ready_driver(&recorder, prepared, &sink, &seq);
        let mut f = Fixture {
            driver,
            recorder,
            seq,
            sink,
            dir,
        };
        f.ingest(r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#);
        if let Some(chat) = f.driver.session.chats.get_mut(&13) {
            chat.set_member_status(ChannelMemberStatus::Administrator, None);
        }
        f
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn events(&mut self, extra: RequestId, ids: &[i64]) {
        let events: Vec<String> = ids
            .iter()
            .map(|id| {
                format!(
                    r#"{{"@type":"chatEvent","id":{id},"date":1700000000,"member_id":{{"@type":"messageSenderUser","user_id":777}},"action":{{"@type":"chatEventMemberJoined"}}}}"#
                )
            })
            .collect();
        self.ingest(&format!(
            r#"{{"@type":"chatEvents","@extra":"{}","events":[{}]}}"#,
            extra.0,
            events.join(",")
        ));
    }

    fn shown_events(&self) -> Option<Vec<i64>> {
        match self.driver.session.groups.event_logs.get(&CHANNEL.0)? {
            ChatEventLogFetch::Loaded(page) => Some(page.events.iter().map(|e| e.id).collect()),
            _ => None,
        }
    }

    fn sent_queries(&self) -> Vec<String> {
        self.recorder
            .snapshot()
            .into_iter()
            .filter_map(|json| serde_json::from_str::<Value>(&json).ok())
            .filter(|value| value["@type"] == "getChatEventLog")
            .map(|value| value["query"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    fn calls_page(&mut self, extra: RequestId, ids: &[i64], next: &str) {
        let messages: Vec<String> = ids
            .iter()
            .map(|id| {
                format!(
                    r#"{{"id":{id},"chat_id":7,"is_outgoing":false,"date":1790000000,"content":{{"@type":"messageCall","unique_id":{id},"is_video":false,"discard_reason":{{"@type":"callDiscardReasonMissed"}},"duration":0}}}}"#
                )
            })
            .collect();
        self.ingest(&format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":{},"messages":[{}],"next_offset":"{next}"}}"#,
            extra.0,
            ids.len(),
            messages.join(",")
        ));
    }

    fn shown_calls(&self) -> Vec<i64> {
        self.driver
            .session
            .calls
            .recent_calls
            .iter()
            .map(|message| message.id.0)
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_search_typed_while_the_last_one_loads_is_sent_and_shown() {
    let mut f = Fixture::new();
    f.driver.set_chat_event_log_query(CHANNEL, "a");
    let first = f
        .driver
        .refresh_chat_event_log(CHANNEL)
        .unwrap()
        .expect("a");
    // "ab" before the answer for "a": sent at once, not dropped.
    f.driver.set_chat_event_log_query(CHANNEL, "ab");
    let second = f
        .driver
        .refresh_chat_event_log(CHANNEL)
        .unwrap()
        .expect("ab");
    assert_eq!(f.sent_queries(), vec!["a", "ab"]);
    // The answer for "a" lands first and is not shown under "ab".
    f.events(first, &[9, 8, 7]);
    assert_eq!(f.shown_events(), None);
    f.events(second, &[8]);
    assert_eq!(f.shown_events(), Some(vec![8]));
}

#[test]
fn an_older_page_asked_before_new_filters_is_dropped() {
    let mut f = Fixture::new();
    let first = f
        .driver
        .fetch_chat_event_log(CHANNEL)
        .unwrap()
        .expect("first");
    let full: Vec<i64> = (101..=200).rev().collect();
    f.events(first, &full);
    let older = f
        .driver
        .fetch_chat_event_log_more(CHANNEL)
        .unwrap()
        .expect("older page");
    // New filters saved while the older page is out: a refresh goes out.
    f.driver.set_chat_event_log_filter(CHANNEL, None, vec![7]);
    let filtered = f
        .driver
        .refresh_chat_event_log(CHANNEL)
        .unwrap()
        .expect("refresh");
    f.events(older, &[100, 99]);
    assert_eq!(f.shown_events(), None, "the old page is not the list");
    f.events(filtered, &[150]);
    assert_eq!(f.shown_events(), Some(vec![150]));
    // A failure of a request the log no longer waits for changes nothing.
    f.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"INTERNAL"}}"#,
        older.0
    ));
    assert_eq!(f.shown_events(), Some(vec![150]));
}

#[test]
fn a_poll_sent_under_old_filters_does_not_merge_into_the_new_list() {
    let mut f = Fixture::new();
    let first = f
        .driver
        .fetch_chat_event_log(CHANNEL)
        .unwrap()
        .expect("first");
    f.events(first, &[10]);
    let poll = f
        .driver
        .poll_chat_event_log(CHANNEL)
        .unwrap()
        .expect("poll");
    assert_eq!(
        f.driver.poll_chat_event_log(CHANNEL).unwrap(),
        None,
        "one poll at a time"
    );
    f.driver.set_chat_event_log_query(CHANNEL, "x");
    let refreshed = f
        .driver
        .refresh_chat_event_log(CHANNEL)
        .unwrap()
        .expect("refresh");
    f.events(refreshed, &[5]);
    f.events(poll, &[11, 10]);
    assert_eq!(f.shown_events(), Some(vec![5]));
    // The next poll goes out (the dropped one no longer blocks it).
    assert!(f.driver.poll_chat_event_log(CHANNEL).unwrap().is_some());
}

#[test]
fn reopening_recent_calls_during_a_more_page_starts_from_the_newest() {
    let mut f = Fixture::new();
    let first = f.driver.fetch_call_history().expect("first page");
    f.calls_page(first, &[300, 299], "next");
    let more = f.driver.fetch_more_call_history().expect("more");
    // Close and reopen the tab within one round trip.
    let reopened = f.driver.fetch_call_history().expect("first page again");
    f.calls_page(more, &[260, 259], "later");
    assert!(f.shown_calls().is_empty(), "the old page is dropped");
    f.calls_page(reopened, &[301, 300, 299], "next");
    assert_eq!(f.shown_calls(), vec![301, 300, 299]);
    assert!(!f.driver.session.calls.recent_calls_loading());
}

#[test]
fn a_failed_later_page_keeps_the_loaded_calls_and_retries_that_page() {
    let mut f = Fixture::new();
    let first = f.driver.fetch_call_history().expect("first page");
    f.calls_page(first, &[300, 299], "next");
    let more = f.driver.fetch_more_call_history().expect("more");
    f.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"X"}}"#,
        more.0
    ));
    assert!(f.driver.session.calls.recent_calls_error);
    assert_eq!(f.shown_calls(), vec![300, 299], "the loaded calls stay");
    let retry = f.driver.fetch_more_call_history().expect("retry the page");
    assert!(!f.driver.session.calls.recent_calls_error, "loading again");
    f.calls_page(retry, &[260, 259], "later");
    assert_eq!(f.shown_calls(), vec![300, 299, 260, 259]);
}

#[test]
fn clear_all_during_a_page_keeps_the_deleted_calls_away() {
    let mut f = Fixture::new();
    let first = f.driver.fetch_call_history().expect("first page");
    f.calls_page(first, &[300, 299], "next");
    let more = f.driver.fetch_more_call_history().expect("more");
    let clear = f
        .driver
        .clear_call_history(false)
        .expect("send")
        .expect("clear");
    f.ingest(&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, clear.0));
    assert!(f.shown_calls().is_empty());
    f.calls_page(more, &[260, 259], "later");
    assert!(f.shown_calls().is_empty(), "the cleared calls stay away");
}
