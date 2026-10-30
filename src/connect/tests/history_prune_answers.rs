//! Connect-driver tests: answers still on their way for a chat that is no
//! longer kept (`state::history_prune`) bring none of its info panel data
//! back, and opening the chat again asks for it again.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::{CLOSED_CHATS_MAX, MemberListFilter, Session};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const GROUP: i64 = 13;
const BASIC: i64 = 14;

struct Harness {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    _dir: std::path::PathBuf,
}

impl Harness {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let mut h = Harness {
            driver,
            recorder,
            seq: AtomicU64::new(0),
            sink,
            _dir: dir,
        };
        h.ingest(r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#);
        h.ingest(&format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{GROUP},"title":"Group","type":{{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":false}},"unread_count":0}}}}"#
        ));
        h.ingest(&format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{BASIC},"title":"Basic","type":{{"@type":"chatTypeBasicGroup","basic_group_id":114}},"unread_count":0}}}}"#
        ));
        h
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn open(&mut self, chat: i64) {
        self.driver.session.open_chat(ChatId(chat));
        self.driver.session.set_history_holds([]);
    }

    /// The `@extra` of every `method` request sent so far.
    fn sent(&self, method: &str) -> Vec<RequestId> {
        self.recorder
            .snapshot()
            .iter()
            .map(|json| serde_json::from_str::<Value>(json).unwrap())
            .filter(|json| json["@type"] == method)
            .map(|json| RequestId(json["@extra"].as_str().unwrap().parse().unwrap()))
            .collect()
    }
}

#[test]
fn answers_for_a_dropped_chat_bring_nothing_back() {
    let mut h = Harness::new();
    h.open(GROUP);
    h.driver.fetch_chat_media_counts(ChatId(GROUP)).unwrap();
    let members = h
        .driver
        .fetch_supergroup_members(ChatId(GROUP), MemberListFilter::Recent, "")
        .unwrap()
        .expect("first page asked for");
    h.open(BASIC);
    let basic = h
        .driver
        .fetch_basic_group_members(ChatId(BASIC))
        .unwrap()
        .expect("member list asked for");
    let counts = h.sent("getChatMessageCount");
    assert!(!counts.is_empty());
    // Enough other chats go by that neither group keeps its rows.
    for chat in 100..=(100 + CLOSED_CHATS_MAX as i64) {
        h.open(chat);
    }
    assert!(!h.driver.session.histories.contains_key(&GROUP));
    assert!(!h.driver.session.histories.contains_key(&BASIC));
    // The answers arrive now.
    for extra in &counts {
        h.ingest(&format!(
            r#"{{"@type":"count","@extra":"{}","count":3}}"#,
            extra.0
        ));
    }
    h.ingest(&format!(
        r#"{{"@type":"chatMembers","@extra":"{}","total_count":0,"members":[]}}"#,
        members.0
    ));
    h.ingest(&format!(
        r#"{{"@type":"basicGroupFullInfo","@extra":"{}","description":"","members":[]}}"#,
        basic.0
    ));
    let groups = &h.driver.session.groups;
    assert!(
        !h.driver
            .session
            .media
            .chat_media_counts
            .contains_key(&GROUP)
    );
    assert!(
        groups
            .supergroup_members
            .keys()
            .all(|(chat, _)| *chat != GROUP)
    );
    assert!(!groups.basic_group_members.contains_key(&BASIC));
    // Opening the chats and their panels again asks again, in full.
    h.open(GROUP);
    h.driver.fetch_chat_media_counts(ChatId(GROUP)).unwrap();
    assert_eq!(h.sent("getChatMessageCount").len(), 2 * counts.len());
    assert!(
        h.driver
            .fetch_supergroup_members(ChatId(GROUP), MemberListFilter::Recent, "")
            .unwrap()
            .is_some()
    );
    h.open(BASIC);
    assert!(
        h.driver
            .fetch_basic_group_members(ChatId(BASIC))
            .unwrap()
            .is_some()
    );
}
