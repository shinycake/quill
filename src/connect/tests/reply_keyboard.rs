//! Connect-driver tests: bot reply keyboards (`updateChatReplyMarkup`,
//! `chat.reply_markup_message_id`), request buttons and recent inline bots.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Driver = ConnectDriver<Arc<RecordingSender>>;

struct Harness {
    dir: std::path::PathBuf,
    driver: Driver,
    recorder: Arc<RecordingSender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
}

impl Harness {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        Self {
            dir,
            driver,
            recorder,
            sink: dyn_sink,
            seq,
        }
    }

    fn ingest(&mut self, json: &str) {
        let owned = copy_and_parse(json, &self.seq, &self.sink).expect("parse");
        self.driver.ingest(owned).expect("ingest");
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const KEYBOARD: &str = r#"{"@type":"replyMarkupShowKeyboard","rows":[[{"@type":"keyboardButton","text":"Yes","type":{"@type":"keyboardButtonTypeText"}}]],"is_persistent":false,"resize_keyboard":false,"one_time":false,"is_personal":false,"force_reply":false,"input_field_placeholder":""}"#;

fn message_json(id: i64, markup: &str) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":7,"is_outgoing":false,"reply_markup":{markup},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Pick","entities":[]}}}}}}"#
    )
}

#[test]
fn update_chat_reply_markup_shows_a_keyboard_outside_the_loaded_window() {
    let mut h = Harness::new();
    let none = HashSet::new();
    assert!(
        h.driver
            .session
            .custom_keyboard_for_chat(ChatId(7), &none)
            .is_none()
    );
    h.ingest(&format!(
        r#"{{"@type":"updateChatReplyMarkup","chat_id":7,"reply_markup_message":{}}}"#,
        message_json(900, KEYBOARD)
    ));
    let (chat, message, keyboard) = h
        .driver
        .session
        .custom_keyboard_for_chat(ChatId(7), &none)
        .expect("keyboard");
    assert_eq!((chat, message), (ChatId(7), MessageId(900)));
    assert_eq!(keyboard.rows[0][0].text, "Yes");
    // A one-time keyboard the user used is dismissed.
    let dismissed: HashSet<(i64, i64)> = [(7, 900)].into_iter().collect();
    assert!(
        h.driver
            .session
            .custom_keyboard_for_chat(ChatId(7), &dismissed)
            .is_none()
    );
    // `reply_markup_message` null removes the keyboard.
    h.ingest(r#"{"@type":"updateChatReplyMarkup","chat_id":7,"reply_markup_message":null}"#);
    assert!(
        h.driver
            .session
            .custom_keyboard_for_chat(ChatId(7), &none)
            .is_none()
    );
}

#[test]
fn chat_reply_markup_message_id_is_fetched_when_not_loaded() {
    let mut h = Harness::new();
    h.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bot","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0,"reply_markup_message_id":55}}"#,
    );
    assert_eq!(
        h.driver.session.reply_markup_message_to_fetch(ChatId(8)),
        Some(MessageId(55))
    );
    h.driver.session.open_chat(ChatId(8));
    h.driver.maybe_fetch_reply_markup().expect("request");
    let v = sent_request(&h.recorder, "getMessage");
    assert_eq!(v["chat_id"], 8);
    assert_eq!(v["message_id"], 55);
    let answer = format!(
        r#"{{"@extra":"{}","@type":"message","id":55,"chat_id":8,"is_outgoing":false,"reply_markup":{KEYBOARD},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Pick","entities":[]}}}}}}"#,
        v["@extra"].as_str().unwrap()
    );
    h.ingest(&answer);
    let none = HashSet::new();
    let (_, message, _) = h
        .driver
        .session
        .custom_keyboard_for_chat(ChatId(8), &none)
        .expect("keyboard from the fetched message");
    assert_eq!(message, MessageId(55));
    assert_eq!(
        h.driver.session.reply_markup_message_to_fetch(ChatId(8)),
        None,
        "answered, nothing more to fetch"
    );
}

#[test]
fn share_requests_use_the_keyboard_button_source() {
    let mut h = Harness::new();
    h.driver
        .share_users_with_bot(ChatId(7), MessageId(9), 4, &[11, 12])
        .expect("users");
    let v = sent_request(&h.recorder, "shareUsersWithBot");
    assert_eq!(v["source"]["@type"], "keyboardButtonSourceMessage");
    assert_eq!(v["source"]["chat_id"], 7);
    assert_eq!(v["source"]["message_id"], 9);
    assert_eq!(v["button_id"], 4);
    assert_eq!(v["shared_user_ids"], serde_json::json!([11, 12]));
    assert_eq!(v["only_check"], false);
    assert!(
        h.driver
            .share_users_with_bot(ChatId(7), MessageId(9), 4, &[])
            .is_err()
    );
    h.driver
        .share_chat_with_bot(ChatId(7), MessageId(9), 5, ChatId(33))
        .expect("chat");
    let v = sent_request(&h.recorder, "shareChatWithBot");
    assert_eq!(v["button_id"], 5);
    assert_eq!(v["shared_chat_id"], 33);
}

#[test]
fn recent_inline_bots_are_fetched_once_and_kept() {
    let mut h = Harness::new();
    h.driver.maybe_fetch_recent_inline_bots().expect("request");
    let v = sent_request(&h.recorder, "getRecentInlineBots");
    let before = h.recorder.snapshot().len();
    h.driver
        .maybe_fetch_recent_inline_bots()
        .expect("in flight");
    assert_eq!(h.recorder.snapshot().len(), before, "not asked twice");
    let answer = format!(
        r#"{{"@extra":"{}","@type":"users","total_count":2,"user_ids":[21,22]}}"#,
        v["@extra"].as_str().unwrap()
    );
    h.ingest(&answer);
    assert_eq!(h.driver.session.recent_inline_bots(), &[21, 22]);
    h.driver.maybe_fetch_recent_inline_bots().expect("cached");
    assert_eq!(h.recorder.snapshot().len(), before, "kept for the session");
}
