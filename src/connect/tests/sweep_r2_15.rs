//! Sweep fixes (r2-15): requests sent from a poll or a refresh, answers
//! matched to the ask that sent them.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Rig {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    _dir: std::path::PathBuf,
}

impl Rig {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let mut rig = Self {
            driver,
            recorder,
            sink,
            seq: AtomicU64::new(0),
            _dir: dir,
        };
        rig.ingest(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        rig
    }

    fn ingest(&mut self, json: &str) {
        if let Some(parsed) = copy_and_parse(json, &self.seq, &self.sink) {
            let _ = self.driver.ingest(parsed);
        }
    }

    /// The requests of this `@type` sent so far.
    fn sent(&self, kind: &str) -> Vec<Value> {
        self.recorder
            .snapshot()
            .iter()
            .filter_map(|json| serde_json::from_str::<Value>(json).ok())
            .filter(|v| v["@type"] == kind)
            .collect()
    }
}

fn extra_of(request: &Value) -> String {
    request["@extra"].as_str().unwrap_or_default().to_owned()
}

fn forum_rig() -> Rig {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":false},"unread_count":0}}"#,
    );
    rig.ingest(
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"is_forum":true}}"#,
    );
    rig
}

fn topics_answer(extra: &str) -> String {
    format!(r#"{{"@type":"forumTopics","@extra":"{extra}","total_count":0,"topics":[]}}"#)
}

#[test]
fn a_forum_refresh_is_sent_even_while_an_older_ask_is_out_and_the_older_answer_is_dropped() {
    let mut rig = forum_rig();
    rig.driver.load_forum_topics(ChatId(13)).unwrap();
    let asks = rig.sent("getForumTopics");
    assert_eq!(asks.len(), 1);
    // A mutation lands while that ask is out: the manage dialog refreshes.
    rig.driver.refresh_forum_topics(ChatId(13)).unwrap();
    let asks = rig.sent("getForumTopics");
    assert_eq!(asks.len(), 2, "the refresh is not a no-op");
    // The older ask predates the change: its answer is not taken.
    rig.ingest(&topics_answer(&extra_of(&asks[0])));
    assert!(!rig.driver.session.threads.forum_topics.contains_key(&13));
    rig.ingest(&topics_answer(&extra_of(&asks[1])));
    assert!(rig.driver.session.threads.forum_topics.contains_key(&13));
}

#[test]
fn a_failed_get_me_is_not_asked_again_by_the_poll() {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":9,"title":"news","type":{"@type":"chatTypeSupergroup","supergroup_id":9,"is_channel":true},"unread_count":0}}"#,
    );
    rig.driver.session.open_chat(ChatId(9));
    rig.driver.maybe_probe_channel_membership().unwrap();
    rig.driver.maybe_probe_channel_membership().unwrap();
    let asks = rig.sent("getMe");
    assert_eq!(asks.len(), 1, "one getMe while it is in flight");
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"boom"}}"#,
        extra_of(&asks[0])
    ));
    rig.driver.maybe_probe_channel_membership().unwrap();
    rig.driver.maybe_probe_channel_membership().unwrap();
    assert_eq!(rig.sent("getMe").len(), 1, "no re-ask after the failure");
}

#[test]
fn a_full_rich_load_ends_only_with_the_answer_of_the_ask_that_waits() {
    let mut rig = Rig::new();
    let key = crate::ids::ChatMessage::new(ChatId(7), crate::ids::MessageId(42));
    let first = rig
        .driver
        .fetch_full_rich_message(key.chat, key.message)
        .unwrap();
    let second = rig
        .driver
        .fetch_full_rich_message(key.chat, key.message)
        .unwrap();
    // The older ask is refused: the newer one still loads.
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"X"}}"#,
        first.0
    ));
    assert!(rig.driver.session.full_rich_loading(key.chat, key.message));
    assert!(rig.driver.session.chats_state.chat_action_error.is_empty());
    rig.ingest(&format!(
        r#"{{"@type":"richMessage","@extra":"{}","is_rtl":false,"is_full":true,"blocks":[]}}"#,
        second.0
    ));
    assert!(!rig.driver.session.full_rich_loading(key.chat, key.message));
}
