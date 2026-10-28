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

    fn history_mut(&mut self) -> &mut crate::state::HistoryState {
        self.driver.session.histories.get_mut(&7).expect("history")
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
    assert!(
        h.driver.session.history_has_newer(ChatId(7)),
        "60 is newer than the window"
    );
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
    assert!(!h.driver.session.history_has_newer(ChatId(7)));
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
    assert!(h.driver.session.history_has_newer(ChatId(7)));
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
    assert!(!h.driver.session.history_has_newer(ChatId(7)));
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
    assert!(!h.driver.session.history_has_newer(ChatId(7)));
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
    assert!(!h.driver.session.history_has_newer(ChatId(7)));
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
    h.driver.session.histories.get_mut(&7).unwrap().latest_seen = 70;
    assert!(h.driver.session.last_editable_message(ChatId(7)).is_none());
}

#[test]
fn mention_search_targets_group_members_and_drops_stale_answers() {
    let mut h = Harness::with_unread_chat();
    // Chat 7 is private: no one to mention.
    h.driver.select_chat(ChatId(7)).unwrap();
    h.driver.search_mentions(Some("al")).unwrap();
    assert!(h.driver.session.messages.mention_search.is_none());

    h.ingest(r#"{"@type":"updateNewChat","chat":{"id":-100,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":100},"unread_count":0}}"#);
    h.ingest(r#"{"@type":"updateChatPosition","chat_id":-100,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"8","is_pinned":false}}"#);
    h.driver.select_chat(ChatId(-100)).unwrap();
    h.driver.search_mentions(Some("a")).unwrap();
    let stale = h
        .driver
        .session
        .messages
        .mention_search
        .as_ref()
        .unwrap()
        .request
        .unwrap();
    h.driver.search_mentions(Some("al")).unwrap();
    let current = h
        .driver
        .session
        .messages
        .mention_search
        .as_ref()
        .unwrap()
        .request
        .unwrap();
    let sent = h
        .recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|j| j.contains("\"@type\":\"searchChatMembers\""))
        .unwrap();
    assert!(sent.contains("\"query\":\"al\""));
    // Same query again: no new request.
    h.driver.search_mentions(Some("al")).unwrap();
    assert_eq!(
        h.driver
            .session
            .messages
            .mention_search
            .as_ref()
            .unwrap()
            .request,
        Some(current)
    );

    let members = |extra: RequestId, ids: &[i64]| {
        let members: Vec<String> = ids
            .iter()
            .map(|id| format!(r#"{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":{id}}},"inviter_user_id":0,"joined_chat_date":0,"status":{{"@type":"chatMemberStatusMember","member_until_date":0}}}}"#))
            .collect();
        format!(
            r#"{{"@type":"chatMembers","@extra":"{}","total_count":{},"members":[{}]}}"#,
            extra.0,
            ids.len(),
            members.join(",")
        )
    };
    h.ingest(&members(stale, &[1, 2]));
    assert!(
        h.driver
            .session
            .messages
            .mention_search
            .as_ref()
            .unwrap()
            .user_ids
            .is_empty()
    );
    h.ingest(&members(current, &[5, 6]));
    assert_eq!(
        h.driver
            .session
            .messages
            .mention_search
            .as_ref()
            .unwrap()
            .user_ids,
        vec![5, 6]
    );
    h.driver.search_mentions(None).unwrap();
    assert!(h.driver.session.messages.mention_search.is_none());
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

#[test]
fn deleting_the_message_under_the_unread_bar_removes_the_bar() {
    let mut h = Harness::with_unread_chat();
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40, 50]);
    assert!(h.history().unread_anchor.is_some());
    let bar = h.history().unread_bar_message().expect("bar message");
    // A later message going away leaves the bar where it is.
    h.ingest(r#"{"@type":"updateDeleteMessages","chat_id":7,"message_ids":[50],"is_permanent":true,"from_cache":false}"#);
    assert_eq!(h.history().unread_bar_message(), Some(bar));
    h.ingest(&format!(r#"{{"@type":"updateDeleteMessages","chat_id":7,"message_ids":[{}],"is_permanent":true,"from_cache":false}}"#, bar.0));
    assert_eq!(h.history().unread_anchor, None);
    assert_eq!(h.history().unread_bar_message(), None);
}

#[test]
fn deleting_the_bar_message_before_the_ui_reports_reads_nothing() {
    let mut h = Harness::with_unread_chat();
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40, 50]);
    let bar = h.history().unread_bar_message().expect("bar message");
    h.ingest(&format!(r#"{{"@type":"updateDeleteMessages","chat_id":7,"message_ids":[{}],"is_permanent":true,"from_cache":false}}"#, bar.0));
    assert_eq!(h.history().unread_anchor, None);
    assert!(h.driver.session.message_ids_to_view(ChatId(7)).is_empty());
}

#[test]
fn trimming_the_bar_message_away_removes_the_bar() {
    let mut h = Harness::with_unread_chat();
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40, 50]);
    let bar = h.history().unread_bar_message().expect("bar message");
    let history = h.driver.session.history_entry(ChatId(7));
    let kept = history.messages.keys().filter(|id| **id > bar.0).count();
    assert!(history.trim_to_tail(kept) > 0);
    assert!(!history.contains(bar));
    assert_eq!(history.unread_bar_message(), None);
    assert_eq!(history.unread_anchor, None);
}

fn sent_json(h: &Harness, ty: &str) -> Option<Value> {
    h.recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|j| j.contains(&format!("\"@type\":\"{ty}\"")))
        .map(|j| serde_json::from_str(&j).unwrap())
}

#[test]
fn mention_button_jumps_to_the_oldest_unread_mention() {
    let mut h = Harness::with_unread_chat();
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40]);

    h.driver
        .jump_to_unread_marker(crate::state::UnreadJumpKind::Mention)
        .unwrap();
    let search = sent_json(&h, "searchChatMessages").expect("search sent");
    assert_eq!(
        search["filter"]["@type"],
        "searchMessagesFilterUnreadMention"
    );
    assert_eq!(search["from_message_id"], 0);
    // Dedupe while in flight.
    let before = h.recorder.snapshot().len();
    h.driver
        .jump_to_unread_marker(crate::state::UnreadJumpKind::Mention)
        .unwrap();
    assert_eq!(h.recorder.snapshot().len(), before);

    // TDLib answers newest first; the oldest (5) is outside the window.
    let extra = search["@extra"].as_str().unwrap();
    let messages: Vec<String> = [55, 52, 5]
        .iter()
        .map(|id| message_json(*id, false))
        .collect();
    h.ingest(&format!(
        r#"{{"@type":"foundChatMessages","@extra":"{extra}","total_count":3,"messages":[{}],"next_from_message_id":0}}"#,
        messages.join(",")
    ));
    let around = h.last_history_request();
    assert_eq!(around["from_message_id"], 5);
    assert_eq!(
        h.driver.session.search.chat_search.jump,
        crate::state::ChatSearchJump::Loading {
            message_id: MessageId(5)
        }
    );
}

#[test]
fn reaction_button_uses_the_reaction_filter_and_read_all_sends_the_rpc() {
    let mut h = Harness::with_unread_chat();
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40]);

    h.driver
        .jump_to_unread_marker(crate::state::UnreadJumpKind::Reaction)
        .unwrap();
    let search = sent_json(&h, "searchChatMessages").expect("search sent");
    assert_eq!(
        search["filter"]["@type"],
        "searchMessagesFilterUnreadReaction"
    );

    h.driver
        .read_all_unread_markers(crate::state::UnreadJumpKind::Reaction)
        .unwrap();
    assert_eq!(sent_json(&h, "readAllChatReactions").unwrap()["chat_id"], 7);
    h.driver
        .read_all_unread_markers(crate::state::UnreadJumpKind::Mention)
        .unwrap();
    assert_eq!(sent_json(&h, "readAllChatMentions").unwrap()["chat_id"], 7);
}

fn history_requests(h: &Harness) -> usize {
    h.recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains("\"@type\":\"getChatHistory\""))
        .count()
}

#[test]
fn a_saved_position_opens_around_its_message_without_a_highlight() {
    let mut h = Harness::with_unread_chat();
    let extra = h
        .driver
        .select_chat_open(ChatId(7), crate::state::HistoryOpen::Saved(MessageId(45)))
        .unwrap()
        .expect("first page");
    // The saved position wins over the read boundary (tdesktop reads
    // `scrollTopItem` back for a chat shown at its default position).
    let request = h.last_history_request();
    assert_eq!(request["from_message_id"], 45);
    assert_eq!(request["offset"], HISTORY_AROUND_OFFSET);
    assert!(h.driver.session.saved_open_pending(ChatId(7)));
    assert_eq!(
        h.driver.session.search.chat_search.jump,
        crate::state::ChatSearchJump::None,
        "no highlight"
    );

    h.answer(extra, &[40, 45, 50]);
    assert!(!h.driver.session.saved_open_pending(ChatId(7)));
    assert!(h.history().contains(MessageId(45)));
    assert!(
        h.driver.session.history_has_newer(ChatId(7)),
        "60 is newer than the window"
    );
    assert_eq!(history_requests(&h), 1, "no fallback load");
    assert_eq!(
        h.driver.session.search.chat_search.jump,
        crate::state::ChatSearchJump::None
    );
}

#[test]
fn a_saved_message_that_is_gone_falls_back_to_the_default_window() {
    let mut h = Harness::with_unread_chat();
    let extra = h
        .driver
        .select_chat_open(ChatId(7), crate::state::HistoryOpen::Saved(MessageId(45)))
        .unwrap()
        .unwrap();
    let epoch = h.history().window_epoch;
    // 45 was deleted: the page around it comes back without it.
    h.answer(extra, &[40, 50]);
    assert!(!h.driver.session.saved_open_pending(ChatId(7)));
    assert!(h.history().messages.is_empty(), "the window is not kept");
    assert_ne!(h.history().window_epoch, epoch);
    // The default open follows: around the read boundary.
    assert_eq!(history_requests(&h), 2);
    let request = h.last_history_request();
    assert_eq!(request["from_message_id"], 30);
    let fallback = RequestId(request["@extra"].as_str().unwrap().parse().unwrap());
    h.answer(fallback, &[20, 30, 40]);
    assert!(h.history().contains(MessageId(30)));
    assert_eq!(history_requests(&h), 2);
}

#[test]
fn a_failed_saved_open_falls_back_to_the_default_window() {
    let mut h = Harness::with_unread_chat();
    let extra = h
        .driver
        .select_chat_open(ChatId(7), crate::state::HistoryOpen::Saved(MessageId(45)))
        .unwrap()
        .unwrap();
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"MESSAGE_ID_INVALID"}}"#,
        extra.0
    ));
    assert!(!h.driver.session.saved_open_pending(ChatId(7)));
    assert!(!h.history().load_failed, "no retry row: the default loads");
    assert_eq!(history_requests(&h), 2);
    assert_eq!(h.last_history_request()["from_message_id"], 30);
}

#[test]
fn a_window_that_holds_the_saved_message_is_kept() {
    let mut h = Harness::with_unread_chat();
    h.ingest(r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0}}"#);
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40]);
    h.driver.select_chat(ChatId(8)).unwrap();
    let epoch = h.history().window_epoch;
    let before = h.recorder.snapshot().len();

    h.driver
        .select_chat_open(ChatId(7), crate::state::HistoryOpen::Saved(MessageId(30)))
        .unwrap();
    assert!(!h.driver.session.saved_open_pending(ChatId(7)));
    assert_eq!(h.history().window_epoch, epoch, "restored in place");
    assert!(h.history().contains(MessageId(30)));
    // No load around the saved message: its rows are already there.
    let around = h
        .recorder
        .snapshot()
        .iter()
        .skip(before)
        .filter(|j| j.contains("\"@type\":\"getChatHistory\"") && j.contains("\"chat_id\":7"))
        .any(|j| j.contains("\"from_message_id\":30"));
    assert!(!around);
}

/// Chat 7 opened with 80 rows, then left for chat 8; the frame settles the
/// release queue, so 7 keeps only its newest rows (`history_prune.rs`).
fn left_and_trimmed() -> Harness {
    let mut h = Harness::with_unread_chat();
    h.ingest(&chat_json(8));
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    let page: Vec<i64> = (1..=80).collect();
    h.answer(first, &page);
    h.driver.select_chat(ChatId(8)).unwrap();
    h.driver.session.set_history_holds([]);
    assert_eq!(h.history().messages.len(), crate::state::CLOSED_TAIL_KEEP);
    assert_eq!(h.history().oldest_id(), Some(MessageId(21)));
    h
}

#[test]
fn a_chat_trimmed_below_its_saved_row_reopens_around_it() {
    let mut h = left_and_trimmed();
    let epoch = h.history().window_epoch;
    let extra = h
        .driver
        .select_chat_open(ChatId(7), crate::state::HistoryOpen::Saved(MessageId(10)))
        .unwrap()
        .expect("loads around the saved row");
    // The tail no longer holds the row: it is replaced, not shown first.
    assert!(h.history().messages.is_empty());
    assert_ne!(h.history().window_epoch, epoch);
    assert!(h.driver.session.saved_open_pending(ChatId(7)));
    let request = h.last_history_request();
    assert_eq!(request["from_message_id"], 10);
    assert_eq!(request["offset"], HISTORY_AROUND_OFFSET);

    let before = history_requests(&h);
    h.answer(extra, &[5, 10, 15]);
    assert!(!h.driver.session.saved_open_pending(ChatId(7)));
    assert!(h.history().contains(MessageId(10)));
    assert!(
        h.driver.session.history_has_newer(ChatId(7)),
        "the latest rows are newer"
    );
    assert_eq!(history_requests(&h), before, "no fallback load");
}

#[test]
fn a_trimmed_tail_that_still_holds_the_saved_row_is_kept() {
    let mut h = left_and_trimmed();
    let epoch = h.history().window_epoch;
    let before = h.recorder.snapshot().len();
    h.driver
        .select_chat_open(ChatId(7), crate::state::HistoryOpen::Saved(MessageId(50)))
        .unwrap();
    assert!(!h.driver.session.saved_open_pending(ChatId(7)));
    assert_eq!(h.history().window_epoch, epoch, "restored in place");
    assert!(h.history().contains(MessageId(50)));
    let around = h.recorder.snapshot().iter().skip(before).any(|j| {
        j.contains("\"@type\":\"getChatHistory\"") && j.contains("\"from_message_id\":50")
    });
    assert!(!around);
}

#[test]
fn an_explicit_target_opens_highlighted_whatever_was_saved() {
    let mut h = Harness::with_unread_chat();
    h.driver
        .select_chat_open(ChatId(7), crate::state::HistoryOpen::Message(MessageId(45)))
        .unwrap();
    assert!(!h.driver.session.saved_open_pending(ChatId(7)));
    assert_eq!(
        h.driver.session.search.chat_search.jump,
        crate::state::ChatSearchJump::Loading {
            message_id: MessageId(45)
        }
    );
}

fn chat_json(id: i64) -> String {
    format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"Bob","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
    )
}

#[test]
fn switching_chats_during_the_saved_open_leaves_the_other_chat_alone() {
    let mut h = Harness::with_unread_chat();
    h.ingest(&chat_json(8));
    let around = h
        .driver
        .select_chat_open(ChatId(7), crate::state::HistoryOpen::Saved(MessageId(45)))
        .unwrap()
        .unwrap();
    h.driver.select_chat(ChatId(8)).unwrap();
    let before = history_requests(&h);

    // The old chat's page comes back without its message.
    h.answer(around, &[40, 50]);
    assert_eq!(history_requests(&h), before, "no fallback for any chat");
    assert_eq!(h.driver.session.open_chat, Some(ChatId(8)));
    assert!(!h.driver.session.saved_open_pending(ChatId(7)));
    assert!(
        h.driver
            .session
            .histories
            .get(&8)
            .is_none_or(|b| b.messages.is_empty() && !b.load_failed)
    );
}

#[test]
fn a_swept_saved_open_falls_back_to_the_default_window_once() {
    let mut h = Harness::with_unread_chat();
    h.driver
        .select_chat_open(ChatId(7), crate::state::HistoryOpen::Saved(MessageId(45)))
        .unwrap()
        .unwrap();
    assert_eq!(history_requests(&h), 1);
    h.ingest(r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#);

    let late = std::time::Instant::now() + crate::state::PENDING_REQUEST_TTL * 2;
    assert!(h.driver.request_tick(late));
    assert!(!h.driver.session.saved_open_pending(ChatId(7)));
    assert_eq!(history_requests(&h), 2);
    assert_eq!(h.last_history_request()["from_message_id"], 30);

    h.driver
        .request_tick(late + std::time::Duration::from_secs(60));
    assert_eq!(history_requests(&h), 2, "one fallback only");
}

#[test]
fn a_live_message_in_the_empty_window_does_not_end_the_saved_open() {
    let mut h = Harness::with_unread_chat();
    let around = h
        .driver
        .select_chat_open(ChatId(7), crate::state::HistoryOpen::Saved(MessageId(45)))
        .unwrap()
        .unwrap();
    h.ingest(&format!(
        r#"{{"@type":"updateNewMessage","message":{}}}"#,
        message_json(61, false)
    ));
    assert!(
        h.driver.session.saved_open_pending(ChatId(7)),
        "the anchoring still waits for the page"
    );
    assert_eq!(history_requests(&h), 1);

    h.answer(around, &[40, 45, 50]);
    assert!(!h.driver.session.saved_open_pending(ChatId(7)));
    assert!(h.history().contains(MessageId(45)));
    // 61 is not adjacent to the page: it stays out, and newer pages lead
    // to it.
    assert!(!h.history().contains(MessageId(61)));
    assert!(h.driver.session.history_has_newer(ChatId(7)));
    assert_eq!(history_requests(&h), 1, "no fallback");
}

#[test]
fn a_saved_open_of_a_chat_without_a_history_entry_loads_around_the_message() {
    let mut h = Harness::with_unread_chat();
    h.ingest(&chat_json(8));
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40]);
    h.driver.select_chat(ChatId(8)).unwrap();
    // A closed chat's history is dropped from memory.
    h.driver.session.histories.remove(&7);

    let extra = h
        .driver
        .select_chat_open(ChatId(7), crate::state::HistoryOpen::Saved(MessageId(30)))
        .unwrap()
        .expect("loads around the saved message");
    assert_eq!(h.last_history_request()["from_message_id"], 30);
    assert!(h.driver.session.saved_open_pending(ChatId(7)));
    h.answer(extra, &[20, 30, 40]);
    assert!(!h.driver.session.saved_open_pending(ChatId(7)));
    assert!(h.history().contains(MessageId(30)));

    // A missing message still falls back with no entry to start from.
    h.driver.select_chat(ChatId(8)).unwrap();
    h.driver.session.histories.remove(&7);
    let extra = h
        .driver
        .select_chat_open(ChatId(7), crate::state::HistoryOpen::Saved(MessageId(33)))
        .unwrap()
        .unwrap();
    h.answer(extra, &[20, 40]);
    assert_eq!(h.last_history_request()["from_message_id"], 30);
}

/// A sender that refuses requests containing `needle` and accepts the rest.
struct RefusesContaining {
    needle: &'static str,
}

impl JsonSender for RefusesContaining {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        if request.contains(self.needle) {
            Err(ConnectSendError::Native)
        } else {
            Ok(())
        }
    }
}

struct RefusingHarness {
    driver: ConnectDriver<RefusesContaining>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    _dir: std::path::PathBuf,
}

impl RefusingHarness {
    fn new(needle: &'static str) -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(
            session,
            RefusesContaining { needle },
            test_credentials(),
            prepared,
        );
        let mut h = RefusingHarness {
            driver,
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
}

#[test]
fn a_refused_around_request_opens_through_the_default_window() {
    let mut h = RefusingHarness::new(r#""from_message_id":45"#);
    let opened = h
        .driver
        .select_chat_open(ChatId(7), crate::state::HistoryOpen::Saved(MessageId(45)));
    assert!(
        matches!(opened, Ok(Some(_))),
        "the fallback's request is the open's result"
    );
    assert!(!h.driver.session.saved_open_pending(ChatId(7)));
    assert!(!h.driver.session.history_load_failed(ChatId(7)));
    assert!(
        h.driver
            .session
            .requests
            .has_purpose_for_chat(crate::state::RequestPurpose::GetHistoryAround, ChatId(7)),
        "the default window loads around the read boundary"
    );
}

#[test]
fn a_refused_default_window_after_a_missing_saved_message_shows_retry() {
    let mut h = RefusingHarness::new(r#""from_message_id":30"#);
    let around = h
        .driver
        .select_chat_open(ChatId(7), crate::state::HistoryOpen::Saved(MessageId(45)))
        .unwrap()
        .unwrap();
    let messages = [50, 40].map(|id| message_json(id, false)).join(",");
    h.ingest(&format!(
        r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{messages}]}}"#,
        around.0
    ));
    assert!(!h.driver.session.saved_open_pending(ChatId(7)));
    assert!(
        h.driver.session.history_load_failed(ChatId(7)),
        "the usual Retry row, not a skeleton"
    );
}

#[test]
fn jump_down_shows_the_first_unread_message_in_the_window() {
    let mut h = Harness::with_unread_chat();
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40]);
    h.history_mut().end_unread_open();
    let epoch = h.history().window_epoch;
    let sent = h.recorder.snapshot().len();

    assert_eq!(h.driver.jump_down().unwrap(), None);
    assert_eq!(h.history().unread_anchor, Some(MessageId(30)));
    assert_eq!(h.history().unread_bar_message(), Some(MessageId(40)));
    // The UI re-anchors at the bar; nothing is loaded.
    assert_eq!(h.history().window_epoch, epoch.wrapping_add(1));
    assert_eq!(h.recorder.snapshot().len(), sent);
}

#[test]
fn jump_down_loads_around_a_first_unread_message_past_the_window() {
    let mut h = Harness::with_unread_chat();
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40]);
    h.ingest(r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":40,"unread_count":1}"#);

    let around = h
        .driver
        .jump_down()
        .unwrap()
        .expect("page around the boundary");
    let request = h.last_history_request();
    assert_eq!(request["from_message_id"], 40);
    assert!(h.history().messages.is_empty());
    h.answer(around, &[40, 50, 60]);
    assert_eq!(h.history().unread_bar_message(), Some(MessageId(50)));
}

#[test]
fn jump_down_without_unread_messages_jumps_to_the_latest() {
    let mut h = Harness::with_unread_chat();
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40]);
    h.ingest(r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":60,"unread_count":0}"#);

    h.driver.jump_down().unwrap().expect("latest page");
    assert_eq!(h.last_history_request()["from_message_id"], 0);
    assert_eq!(h.history().unread_anchor, None);
}

#[test]
fn hiding_the_bar_keeps_reads_held_back() {
    let mut h = Harness::with_unread_chat();
    let first = h.driver.select_chat(ChatId(7)).unwrap().unwrap();
    h.answer(first, &[20, 30, 40]);
    h.driver.session.hide_unread_bar(ChatId(7));
    assert_eq!(h.history().unread_anchor, None);
    assert!(h.history().opened_with_unread);
    assert!(h.driver.session.message_ids_to_view(ChatId(7)).is_empty());
}
