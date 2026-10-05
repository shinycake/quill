//! Connect-driver tests: the open chat's history window — opening at the
//! first unread message, paging newer, live messages outside the window,
//! jumping to the latest, and dropping pages for a replaced window.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, MessageId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Harness {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    _dir: std::path::PathBuf,
}

impl Harness {
    /// Chat 7: 3 unread after message 30, last message 60.
    fn with_unread_chat() -> Self {
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
            r#"{{"@type":"updateNewChat","chat":{{"id":7,"title":"Alice","type":{{"@type":"chatTypePrivate","user_id":7}},"unread_count":3,"last_read_inbox_message_id":30,"last_message":{}}}}}"#,
            message_json(60, false)
        ));
        h.ingest(r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#);
        h
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn answer(&mut self, extra: RequestId, ids: &[i64]) {
        let messages: Vec<String> = ids
            .iter()
            .rev()
            .map(|id| message_json(*id, false))
            .collect();
        self.ingest(&format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":{},"messages":[{}]}}"#,
            extra.0,
            ids.len(),
            messages.join(",")
        ));
    }

    fn last_history_request(&self) -> Value {
        let json = self
            .recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|j| j.contains("\"@type\":\"getChatHistory\""))
            .expect("getChatHistory sent");
        serde_json::from_str(&json).unwrap()
    }

    fn history(&self) -> &crate::state::HistoryState {
        self.driver.session.histories.get(&7).expect("history")
    }
}

fn message_json(id: i64, outgoing: bool) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":7,"is_outgoing":{outgoing},"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"m{id}","entities":[]}}}}}}"#
    )
}

#[test]
fn unread_chat_opens_around_the_read_boundary_without_marking_read() {
    let mut h = Harness::with_unread_chat();
    let extra = h
        .driver
        .select_chat(ChatId(7))
        .unwrap()
        .expect("first page");
    let request = h.last_history_request();
    assert_eq!(request["from_message_id"], 30);
    assert_eq!(request["offset"], HISTORY_AROUND_OFFSET);
    assert_eq!(h.history().unread_anchor, Some(MessageId(30)));

    h.answer(extra, &[20, 30, 40]);
    let history = h.history();
    assert!(history.contains(MessageId(40)));
    assert!(history.has_newer, "60 is newer than the window");
    // Nothing is viewed until the UI reports on-screen rows.
    assert!(h.driver.session.message_ids_to_view(ChatId(7)).is_empty());
}

#[test]
fn newer_pages_extend_the_window_until_the_latest_message() {
    let mut h = Harness::with_unread_chat();
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40]);

    let newer = h.driver.fetch_history_newer().unwrap().expect("newer page");
    let request = h.last_history_request();
    assert_eq!(request["from_message_id"], 40);
    assert_eq!(request["offset"], -(HISTORY_PAGE_SIZE - 1));
    // In-flight dedupe.
    assert_eq!(h.driver.fetch_history_newer().unwrap(), None);

    h.answer(newer, &[40, 50, 60]);
    assert!(h.history().contains(MessageId(60)));
    assert!(!h.history().has_newer);
    assert_eq!(h.driver.fetch_history_newer().unwrap(), None);
}

#[test]
fn live_messages_stay_out_of_a_window_that_stops_short() {
    let mut h = Harness::with_unread_chat();
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40]);

    h.ingest(&format!(
        r#"{{"@type":"updateNewMessage","message":{}}}"#,
        message_json(70, false)
    ));
    assert!(
        !h.history().contains(MessageId(70)),
        "would fake contiguity"
    );
    assert!(h.history().has_newer);
    assert_eq!(h.history().latest_seen, 70);
}

#[test]
fn sending_from_the_middle_moves_the_window_to_the_latest_run() {
    let mut h = Harness::with_unread_chat();
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40]);
    let epoch = h.history().window_epoch;

    h.ingest(&format!(
        r#"{{"@type":"updateNewMessage","message":{}}}"#,
        message_json(80, true)
    ));
    let history = h.history();
    assert!(history.contains(MessageId(80)));
    assert!(!history.contains(MessageId(40)));
    assert!(!history.has_newer);
    assert_eq!(history.unread_anchor, None);
    assert_ne!(history.window_epoch, epoch);
}

#[test]
fn jump_to_latest_replaces_the_window_and_drops_its_stale_pages() {
    let mut h = Harness::with_unread_chat();
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40]);
    let newer = h.driver.fetch_history_newer().unwrap().unwrap();

    let latest = h.driver.jump_to_latest().unwrap().expect("latest page");
    let request = h.last_history_request();
    assert_eq!(request["from_message_id"], 0);
    assert!(h.history().messages.is_empty());
    assert_eq!(h.history().unread_anchor, None);

    // The newer page for the old window answers late: dropped.
    h.answer(newer, &[40, 50]);
    assert!(h.history().messages.is_empty());

    h.answer(latest, &[50, 60]);
    assert!(h.history().contains(MessageId(60)));
    assert!(!h.history().has_newer);
}

#[test]
fn chat_without_unread_opens_at_the_latest_page() {
    let mut h = Harness::with_unread_chat();
    h.ingest(r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":60,"unread_count":0}"#);
    h.driver.select_chat(ChatId(7)).unwrap();
    assert_eq!(h.last_history_request()["from_message_id"], 0);
    assert_eq!(h.history().unread_anchor, None);
}

#[test]
fn a_failed_send_does_not_make_the_window_look_short() {
    let mut h = Harness::with_unread_chat();
    h.ingest(r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":60,"unread_count":0}"#);
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[50, 60]);
    // A send gets a temporary id, then fails under a different one.
    h.ingest(&format!(
        r#"{{"@type":"updateNewMessage","message":{}}}"#,
        message_json(61, true)
    ));
    h.ingest(&format!(
        r#"{{"@type":"updateMessageSendFailed","old_message_id":61,"error":{{"@type":"error","code":400,"message":"FAIL"}},"message":{}}}"#,
        message_json(62, true)
    ));
    assert!(!h.history().has_newer);
    // A later incoming message still joins the window.
    h.ingest(&format!(
        r#"{{"@type":"updateNewMessage","message":{}}}"#,
        message_json(70, false)
    ));
    assert!(h.history().contains(MessageId(70)));
}

#[test]
fn last_editable_message_is_the_newest_own_message_of_a_tail_window() {
    let mut h = Harness::with_unread_chat();
    h.ingest(r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":60,"unread_count":0}"#);
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    let page = format!(
        r#"{{"@type":"messages","@extra":"{}","total_count":3,"messages":[{},{},{}]}}"#,
        first.0,
        message_json(60, false),
        message_json(55, true),
        message_json(50, true)
    );
    h.ingest(&page);
    let edit = h
        .driver
        .session
        .last_editable_message(ChatId(7))
        .expect("own text message");
    assert_eq!(edit.message_id, MessageId(55));
    // A window that stops short of the latest has no reliable "last".
    h.driver.session.histories.get_mut(&7).unwrap().has_newer = true;
    assert!(h.driver.session.last_editable_message(ChatId(7)).is_none());
}

#[test]
fn chat_peer_online_follows_the_private_user_status() {
    fn h_chat(h: &Harness) -> crate::state::ChatSummary {
        h.driver.session.chats.get(&7).unwrap().clone()
    }
    let mut h = Harness::with_unread_chat();
    assert!(!h.driver.session.chat_peer_online(&h_chat(&h)));
    h.ingest(r#"{"@type":"updateUser","user":{"@type":"user","id":7,"first_name":"Alice","type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusOnline","expires":1900000000}}}"#);
    assert!(h.driver.session.chat_peer_online(&h_chat(&h)));
    h.ingest(r#"{"@type":"updateUserStatus","user_id":7,"status":{"@type":"userStatusOffline","was_online":1700000000}}"#);
    assert!(!h.driver.session.chat_peer_online(&h_chat(&h)));
}
