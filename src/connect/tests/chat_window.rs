//! Connect-driver tests: a chat shown in a window of its own
//! (`parity:platform-new-window`) — it opens and pages beside the main
//! window's chat, stays open in TDLib across the main window's switches,
//! reports its rows as viewed, and closes with its window.
use super::super::*;
use super::*;
use crate::composer::ComposerSnapshot;
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

const ALICE: ChatId = ChatId(7);
const BOB: ChatId = ChatId(8);

impl Harness {
    /// Two private chats, nothing unread: Alice (7) and Bob (8).
    fn two_chats() -> Self {
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
        for (id, title) in [(ALICE.0, "Alice"), (BOB.0, "Bob")] {
            h.ingest(&format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0,"last_message":{}}}}}"#,
                message_json(id, 60, false)
            ));
            h.ingest(&format!(
                r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{id}","is_pinned":false}}}}"#
            ));
        }
        h
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn answer(&mut self, chat: ChatId, extra: RequestId, ids: &[i64]) {
        let messages: Vec<String> = ids
            .iter()
            .rev()
            .map(|id| message_json(chat.0, *id, false))
            .collect();
        self.ingest(&format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":{},"messages":[{}]}}"#,
            extra.0,
            ids.len(),
            messages.join(",")
        ));
    }

    fn sent(&self, kind: &str, chat: ChatId) -> Vec<Value> {
        self.recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains(&format!("\"@type\":\"{kind}\"")))
            .map(|j| serde_json::from_str::<Value>(j).unwrap())
            .filter(|v| v["chat_id"] == chat.0)
            .collect()
    }

    fn history(&self, chat: ChatId) -> &crate::state::HistoryState {
        self.driver.session.histories.get(&chat.0).expect("history")
    }
}

fn message_json(chat: i64, id: i64, outgoing: bool) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":{chat},"is_outgoing":{outgoing},"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"m{id}","entities":[]}}}}}}"#
    )
}

#[test]
fn an_account_going_to_the_background_trims_the_chat_it_showed() {
    let mut h = Harness::two_chats();
    let page = h.driver.select_chat(ALICE).unwrap().unwrap();
    let ids: Vec<i64> = (1..=80).collect();
    h.answer(ALICE, page, &ids);
    assert_eq!(h.history(ALICE).messages.len(), 80);

    h.driver.leave_for_background().unwrap();
    assert_eq!(h.driver.session.open_chat, None);
    assert_eq!(h.sent("closeChat", ALICE).len(), 1);
    assert_eq!(
        h.history(ALICE).messages.len(),
        crate::state::CLOSED_TAIL_KEEP,
        "trimmed without a frame to settle it"
    );
    assert_eq!(h.history(ALICE).newest_id(), Some(MessageId(80)));
}

#[test]
fn a_window_chat_opens_and_pages_beside_the_main_chat() {
    let mut h = Harness::two_chats();
    let main_page = h.driver.select_chat(ALICE).unwrap().unwrap();
    h.answer(ALICE, main_page, &[58, 59, 60]);

    let window_page = h.driver.open_chat_window(BOB).unwrap().expect("first page");
    assert_eq!(
        h.driver.session.open_chat,
        Some(ALICE),
        "the main window keeps its chat"
    );
    assert!(h.driver.session.window_chats.contains(&BOB));
    assert!(h.driver.session.chat_is_shown(BOB));
    assert_eq!(h.sent("openChat", BOB).len(), 1);
    let request = h.sent("getChatHistory", BOB);
    assert_eq!(request.len(), 1);
    assert_eq!(request[0]["from_message_id"], 0);

    // The page lands although the main window shows another chat.
    h.answer(BOB, window_page, &[58, 59, 60]);
    assert!(h.history(BOB).contains(MessageId(60)));
    assert!(h.history(ALICE).contains(MessageId(60)));

    // Paging older goes to the window's chat, not the open one.
    let older = h
        .driver
        .fetch_history_for(BOB)
        .unwrap()
        .expect("older page");
    let request = h.sent("getChatHistory", BOB);
    assert_eq!(request.last().unwrap()["from_message_id"], 58);
    h.answer(BOB, older, &[50, 51, 58]);
    assert!(h.history(BOB).contains(MessageId(50)));
    // In-flight dedupe is per chat.
    assert_eq!(h.sent("getChatHistory", ALICE).len(), 1);
}

#[test]
fn window_chat_pages_survive_the_main_window_switching_chats() {
    let mut h = Harness::two_chats();
    h.driver.select_chat(ALICE).unwrap();
    let window_page = h.driver.open_chat_window(BOB).unwrap().unwrap();
    // The main window switches away and back: its view generation moves on.
    h.driver.select_chat(ALICE).unwrap();
    let _ = h.driver.deselect_chat();
    h.driver.select_chat(ALICE).unwrap();
    h.answer(BOB, window_page, &[58, 59, 60]);
    assert!(
        h.history(BOB).contains(MessageId(60)),
        "a window chat's page is not stale after the main window moved on"
    );
}

#[test]
fn the_main_window_switching_away_keeps_a_window_chat_open() {
    let mut h = Harness::two_chats();
    h.driver.select_chat(BOB).unwrap();
    // Bob is in the main window and gets a window: no second openChat.
    h.driver.open_chat_window(BOB).unwrap();
    assert_eq!(h.sent("openChat", BOB).len(), 1);

    // The main window moves to Alice: Bob stays open for its window.
    h.driver.select_chat(ALICE).unwrap();
    assert!(h.sent("closeChat", BOB).is_empty());
    assert_eq!(h.sent("openChat", ALICE).len(), 1);

    // Closing the window closes the chat; the main window is elsewhere.
    h.driver.close_chat_window(BOB).unwrap();
    assert_eq!(h.sent("closeChat", BOB).len(), 1);
    assert!(!h.driver.session.window_chats.contains(&BOB));
    // A second close is a no-op.
    h.driver.close_chat_window(BOB).unwrap();
    assert_eq!(h.sent("closeChat", BOB).len(), 1);
}

#[test]
fn closing_the_window_while_the_main_window_shows_the_chat_keeps_it_open() {
    let mut h = Harness::two_chats();
    h.driver.open_chat_window(BOB).unwrap();
    h.driver.select_chat(BOB).unwrap();
    assert_eq!(
        h.sent("openChat", BOB).len(),
        1,
        "already open for the window"
    );
    h.driver.close_chat_window(BOB).unwrap();
    assert!(h.sent("closeChat", BOB).is_empty());
    // The main window's own switch pairs the close now.
    h.driver.select_chat(ALICE).unwrap();
    assert_eq!(h.sent("closeChat", BOB).len(), 1);
}

#[test]
fn moving_the_active_chat_to_a_window_leaves_the_main_window_empty() {
    let mut h = Harness::two_chats();
    h.driver.select_chat(BOB).unwrap();
    h.driver.open_chat_window(BOB).unwrap();
    h.driver.deselect_chat().unwrap();
    assert_eq!(h.driver.session.open_chat, None);
    assert!(
        h.sent("closeChat", BOB).is_empty(),
        "the window still shows it"
    );
    assert!(h.driver.session.chat_is_shown(BOB));
    // Nothing to deselect twice.
    h.driver.deselect_chat().unwrap();
    assert_eq!(h.driver.session.open_chat, None);
}

#[test]
fn window_rows_are_viewed_and_sends_go_to_the_window_chat() {
    let mut h = Harness::two_chats();
    h.driver.select_chat(ALICE).unwrap();
    let page = h.driver.open_chat_window(BOB).unwrap().unwrap();
    h.answer(BOB, page, &[58, 59, 60]);
    // Opening views the newest message once the page is there.
    h.driver
        .view_messages(BOB, &[MessageId(59), MessageId(60)])
        .unwrap();
    let viewed = h.sent("viewMessages", BOB);
    assert!(!viewed.is_empty(), "rows of a window chat are reported");
    assert_eq!(
        viewed.last().unwrap()["source"]["@type"],
        "messageSourceChatHistory"
    );

    let generation = h.driver.session.view_generation;
    h.driver
        .send_snapshot(&ComposerSnapshot::capture(
            BOB,
            generation,
            "from the window",
        ))
        .unwrap();
    let sent = h.sent("sendMessage", BOB);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0]["input_message_content"]["text"]["text"],
        "from the window"
    );
    assert!(h.sent("sendMessage", ALICE).is_empty());
}

#[test]
fn a_chat_that_is_not_shown_cannot_page() {
    let mut h = Harness::two_chats();
    h.driver.select_chat(ALICE).unwrap();
    assert!(h.driver.fetch_history_for(BOB).is_err());
    assert!(h.driver.fetch_history_newer_for(BOB).is_err());
    assert!(h.sent("getChatHistory", BOB).is_empty());
}

#[test]
fn a_window_chat_runs_the_open_chat_follow_ups_without_moving_the_main_view() {
    let mut h = Harness::two_chats();
    h.driver.select_chat(ALICE).unwrap();
    h.driver.session.open_topic = Some(5);
    h.driver.open_chat_window(BOB).unwrap();
    // The follow-ups ran for Bob (its pinned message was asked for) with
    // the main window's view untouched.
    assert_eq!(
        h.sent("searchChatMessages", BOB).len(),
        1,
        "pinned messages asked for"
    );
    assert_eq!(h.driver.session.open_chat, Some(ALICE));
    assert_eq!(h.driver.session.open_topic, Some(5));
    // Inside `with_window_chat` the window's chat is the open one, with a
    // plain view (no topic); outside, the main window's view is back.
    let seen = h.driver.with_window_chat(BOB, |driver| {
        (driver.session.open_chat, driver.session.open_topic)
    });
    assert_eq!(seen, (Some(BOB), None));
    assert_eq!(h.driver.session.open_chat, Some(ALICE));
    assert_eq!(h.driver.session.open_topic, Some(5));
}

#[test]
fn a_search_typed_in_a_chat_window_searches_its_chat_and_answers_there() {
    let mut h = Harness::two_chats();
    h.driver.select_chat(ALICE).unwrap();
    h.driver.open_chat_window(BOB).unwrap();
    // Typed in Bob's window; the debounce ends later, as the window's work
    // (the UI runs it with the window's state in place).
    let typed = h
        .driver
        .with_window_chat(BOB, |driver| driver.set_chat_search_query("m6"))
        .unwrap();
    let ChatSearchQueryOutcome::Debounced { token } = typed else {
        panic!("a typed query waits for the debounce: {typed:?}");
    };
    h.driver
        .with_window_chat(BOB, |driver| driver.commit_debounced_chat_search(token))
        .unwrap();
    let search = h.sent("searchChatMessages", BOB);
    let search = search.last().expect("Bob's chat was searched");
    assert_eq!(search["query"], "m6");
    let extra = search["@extra"].as_str().unwrap().to_string();

    // The answer arrives with the main window's view in place.
    h.ingest(&format!(
        r#"{{"@type":"foundChatMessages","@extra":"{extra}","total_count":1,"next_from_message_id":0,"messages":[{}]}}"#,
        message_json(BOB.0, 60, false)
    ));
    assert!(!h.driver.session.search.chat_search.open);
    let hits = h.driver.with_window_chat(BOB, |driver| {
        let state = &driver.session.search.chat_search;
        (state.chat_id, state.hits.len())
    });
    assert_eq!(hits, (Some(BOB), 1), "the hits landed in Bob's window");
}
