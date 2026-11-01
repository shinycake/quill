//! Connect-driver tests: history loading never stalls. A model of TDLib's
//! `getChatHistory` (short pages, answers out of order) and of the history
//! pane's paging triggers drive the driver through random opens, switches,
//! scrolls, jumps and live messages; whenever everything has answered, the
//! open chat must show messages, and an edge the reader is pressing against
//! must have loaded to the end of the chat.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, MessageId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::{HistoryState, Session};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CHATS: [i64; 3] = [7, 8, 9];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

/// Where the reader holds the history pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    /// Scrolled to the top and still pulling up.
    Top,
    /// At the bottom (where a chat opens).
    Bottom,
}

/// One `getChatHistory` TDLib has not answered yet.
struct Query {
    extra: RequestId,
    chat: i64,
    from: i64,
    offset: i32,
    limit: i32,
}

struct Sim {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    _dir: std::path::PathBuf,
    rng: Rng,
    /// Every chat's server history, oldest first.
    server: BTreeMap<i64, Vec<i64>>,
    /// The newest id each chat ever had: TDLib never reuses a deleted id.
    top: BTreeMap<i64, i64>,
    /// Requests already read from the recorder.
    seen: usize,
    queries: Vec<Query>,
    edge: Edge,
    /// TDLib may answer short (off for the scripted tests).
    short_pages: bool,
    /// What happened, for the failure message.
    log: Vec<String>,
}

fn message_json(chat: i64, id: i64) -> String {
    message_json_from(chat, id, false)
}

fn message_json_from(chat: i64, id: i64, outgoing: bool) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":{chat},"is_outgoing":{outgoing},"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"m{id}","entities":[]}}}}}}"#
    )
}

impl Sim {
    fn new(seed: u64) -> Self {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let lengths: Vec<i64> = CHATS.iter().map(|_| 1 + rng.below(260) as i64).collect();
        Self::with_lengths(rng, &lengths)
    }

    /// Chats 7, 8 and 9 with `lengths` messages (ids 10, 20, ...).
    fn with_lengths(rng: Rng, lengths: &[i64]) -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let mut sim = Sim {
            driver,
            recorder,
            seq: AtomicU64::new(0),
            sink,
            _dir: dir,
            rng,
            server: BTreeMap::new(),
            top: BTreeMap::new(),
            seen: 0,
            queries: Vec::new(),
            edge: Edge::Bottom,
            short_pages: true,
            log: Vec::new(),
        };
        sim.ingest(r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#);
        for (chat, len) in CHATS.iter().copied().zip(lengths.iter().copied()) {
            let ids: Vec<i64> = (1..=len).map(|n| n * 10).collect();
            let last = *ids.last().unwrap_or(&10);
            sim.server.insert(chat, ids);
            sim.ingest(&format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{chat},"title":"c{chat}","type":{{"@type":"chatTypePrivate","user_id":{chat}}},"unread_count":0,"last_message":{}}}}}"#,
                message_json(chat, last)
            ));
            sim.ingest(&format!(
                r#"{{"@type":"updateChatPosition","chat_id":{chat},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{chat}","is_pinned":false}}}}"#
            ));
        }
        sim
    }

    /// Add `n` new messages to `chat` on the server; returns the last id.
    fn fresh_ids(&mut self, chat: i64, n: i64) -> i64 {
        let top = self.top.entry(chat).or_default();
        let ids = self.server.entry(chat).or_default();
        for _ in 0..n {
            *top = (*top).max(ids.last().copied().unwrap_or(0)) + 10;
            ids.push(*top);
        }
        *top
    }

    fn ingest(&mut self, json: &str) {
        let envelope = copy_and_parse(json, &self.seq, &self.sink).expect("valid json");
        let _ = self.driver.ingest(envelope);
    }

    /// Pick up the `getChatHistory` requests the driver sent since last time.
    fn collect_queries(&mut self) {
        let sent = self.recorder.snapshot();
        for json in &sent[self.seen..] {
            if !json.contains("\"@type\":\"getChatHistory\"") {
                continue;
            }
            let v: Value = serde_json::from_str(json).expect("request json");
            let extra = v["@extra"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .or_else(|| v["@extra"].as_u64())
                .expect("@extra");
            self.queries.push(Query {
                extra: RequestId(extra),
                chat: v["chat_id"].as_i64().unwrap_or_default(),
                from: v["from_message_id"].as_i64().unwrap_or_default(),
                offset: v["offset"].as_i64().unwrap_or_default() as i32,
                limit: v["limit"].as_i64().unwrap_or_default() as i32,
            });
        }
        self.seen = sent.len();
    }

    /// TDLib's answer (`OrderedMessages::get_history`), newest first: with
    /// offset 0 the messages older than `from` (0 = from the latest); with
    /// a negative offset `from` itself, `-offset - 1` newer ones and older
    /// ones up to `limit`. TDLib picks how many it returns: it walks down
    /// from the newest message the offset reached and stops where its
    /// in-memory run ends, so a page may come back short, down to one
    /// message, but never cuts between `from` and the newer messages.
    fn answer(&mut self, query: Query) {
        let ids = &self.server[&query.chat];
        let end = if query.from == 0 {
            ids.len()
        } else if query.offset == 0 {
            ids.partition_point(|id| *id < query.from)
        } else {
            let past_from = ids.partition_point(|id| *id <= query.from);
            (past_from as i64 - query.offset as i64 - 1).clamp(0, ids.len() as i64) as usize
        };
        let begin = end.saturating_sub(query.limit.max(0) as usize);
        let mut page: Vec<i64> = ids[begin..end].to_vec();
        let floor = if query.offset < 0 {
            query.from
        } else {
            i64::MAX
        };
        let cuttable = page.iter().filter(|id| **id < floor).count();
        if self.short_pages && cuttable > 1 && self.rng.chance(35) {
            // A page from `from` may end right at it: nothing older is in
            // memory yet.
            let reach = cuttable + usize::from(query.offset < 0);
            let cut = self.rng.below(reach as u64) as usize;
            page.drain(..cut.min(page.len().saturating_sub(1)));
        }
        self.log.push(format!(
            "answer {} chat {} from {} off {} -> {:?}..{:?} ({})",
            query.extra.0,
            query.chat,
            query.from,
            query.offset,
            page.first(),
            page.last(),
            page.len()
        ));
        let messages: Vec<String> = page
            .iter()
            .rev()
            .map(|id| message_json(query.chat, *id))
            .collect();
        self.ingest(&format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":{},"messages":[{}]}}"#,
            query.extra.0,
            page.len(),
            messages.join(",")
        ));
    }

    /// The history pane redraws: a visible first row pages older, a visible
    /// row near the end pages newer while the window stops short of the
    /// latest message (`history_message_list`). A window shorter than the
    /// viewport shows both ends.
    fn render(&mut self) {
        let Some(chat) = self.driver.session.open_chat else {
            return;
        };
        let rows = self
            .driver
            .session
            .histories
            .get(&chat.0)
            .map_or(0, |h| h.messages.len());
        let has_newer = self.driver.session.history_has_newer(chat);
        let top = self
            .driver
            .session
            .histories
            .get(&chat.0)
            .and_then(HistoryState::oldest_id)
            .filter(|_| self.edge == Edge::Top);
        self.driver
            .session
            .note_history_reader((chat.0, None, 0), top.map(|id| id.0));
        if rows == 0 {
            // The empty pane asks for its first page (`conversation_part1`).
            if !self.driver.session.history_loading(chat) {
                let _ = self.driver.fetch_history();
            }
            self.collect_queries();
            return;
        }
        let short = rows < 15;
        if self.edge == Edge::Top || short {
            let _ = self.driver.fetch_history();
        }
        if has_newer && (self.edge == Edge::Bottom || short) {
            let _ = self.driver.fetch_history_newer();
        }
        self.collect_queries();
    }

    /// Deliver one pending answer (any of them: TDLib answers requests in
    /// whatever order they complete). Returns whether one was delivered.
    fn step(&mut self) -> bool {
        self.collect_queries();
        if self.queries.is_empty() {
            return false;
        }
        let ix = self.rng.below(self.queries.len() as u64) as usize;
        let query = self.queries.swap_remove(ix);
        self.answer(query);
        self.render();
        true
    }

    fn settle(&mut self) {
        for _ in 0..2000 {
            self.render();
            if !self.step() {
                return;
            }
        }
        panic!("history paging never settled:\n{}", self.log.join("\n"));
    }

    fn random_action(&mut self) {
        match self.rng.below(15) {
            0..=3 => {
                let chat = CHATS[self.rng.below(CHATS.len() as u64) as usize];
                self.log.push(format!("open {chat}"));
                self.edge = Edge::Bottom;
                let _ = self.driver.select_chat(ChatId(chat));
            }
            4 | 5 => {
                self.log.push("scroll to top".into());
                self.edge = Edge::Top;
            }
            6 => {
                self.log.push("jump to latest".into());
                self.edge = Edge::Bottom;
                let has_newer = self
                    .driver
                    .session
                    .open_chat
                    .is_some_and(|c| self.driver.session.history_has_newer(c));
                if has_newer {
                    let _ = self.driver.jump_to_latest();
                }
            }
            7 | 11 => {
                // 11: sent from another device (or this one).
                let outgoing = self.rng.chance(50);
                let chat = CHATS[self.rng.below(CHATS.len() as u64) as usize];
                let id = self.fresh_ids(chat, 1);
                self.log
                    .push(format!("live message {id} in {chat}, outgoing {outgoing}"));
                let message = message_json_from(chat, id, outgoing);
                self.ingest(&format!(
                    r#"{{"@type":"updateNewMessage","message":{message}}}"#
                ));
                self.ingest(&format!(
                    r#"{{"@type":"updateChatLastMessage","chat_id":{chat},"last_message":{message},"positions":[]}}"#
                ));
            }
            8 if std::env::var_os("QUILL_SIM_NO_GAPS").is_none() => {
                // Missed while offline or after a long gap: TDLib moves the
                // chat's last message without an `updateNewMessage` for each
                // message in between.
                let chat = CHATS[self.rng.below(CHATS.len() as u64) as usize];
                let n = 1 + self.rng.below(120) as i64;
                let last = self.fresh_ids(chat, n);
                self.log
                    .push(format!("{n} messages missed in {chat}, last {last}"));
                let message = message_json(chat, last);
                self.ingest(&format!(
                    r#"{{"@type":"updateChatLastMessage","chat_id":{chat},"last_message":{message},"positions":[]}}"#
                ));
            }
            12 => {
                // The latest message, or one in the middle, is deleted:
                // TDLib sends the deletion, then the chat's new last message.
                let chat = CHATS[self.rng.below(CHATS.len() as u64) as usize];
                let latest = self.rng.chance(60);
                let ids = self.server.get_mut(&chat).expect("chat");
                if ids.len() > 1 {
                    let ix = if latest {
                        ids.len() - 1
                    } else {
                        self.rng.below(ids.len() as u64) as usize
                    };
                    let id = ids.remove(ix);
                    let top = self.top.entry(chat).or_default();
                    *top = (*top).max(id);
                    let last = ids.last().copied().unwrap_or(0);
                    self.log.push(format!("deleted {id} in {chat}"));
                    self.ingest(&format!(
                        r#"{{"@type":"updateDeleteMessages","chat_id":{chat},"message_ids":[{id}],"is_permanent":true,"from_cache":false}}"#
                    ));
                    let message = message_json(chat, last);
                    self.ingest(&format!(
                        r#"{{"@type":"updateChatLastMessage","chat_id":{chat},"last_message":{message},"positions":[]}}"#
                    ));
                }
            }
            13 => {
                // Unread messages arrived while the chat was elsewhere: the
                // next open loads around the read boundary.
                let chat = CHATS[self.rng.below(CHATS.len() as u64) as usize];
                let ids = &self.server[&chat];
                let ix = self.rng.below(ids.len() as u64) as usize;
                let read = ids[ix];
                let unread = (ids.len() - 1 - ix) as i64;
                self.log
                    .push(format!("chat {chat} read up to {read}, {unread} unread"));
                self.ingest(&format!(
                    r#"{{"@type":"updateChatReadInbox","chat_id":{chat},"last_read_inbox_message_id":{read},"unread_count":{unread}}}"#
                ));
            }
            _ => {
                // Some answers arrive before the next thing the reader does.
                let n = self.rng.below(3);
                for _ in 0..n {
                    self.step();
                }
            }
        }
        self.render();
    }

    fn burst(&mut self, chat: i64, n: i64) {
        self.log.push(format!("{n} live messages in {chat}"));
        for _ in 0..n {
            let id = self.fresh_ids(chat, 1);
            let message = message_json(chat, id);
            self.ingest(&format!(
                r#"{{"@type":"updateNewMessage","message":{message}}}"#
            ));
            self.ingest(&format!(
                r#"{{"@type":"updateChatLastMessage","chat_id":{chat},"last_message":{message},"positions":[]}}"#
            ));
        }
        self.render();
    }

    /// Everything has answered: the open chat shows messages, and the edge
    /// the reader holds has loaded to the end of the chat.
    fn check(&self, round: usize) {
        let Some(chat) = self.driver.session.open_chat else {
            return;
        };
        let ids = &self.server[&chat.0];
        let history = self.driver.session.histories.get(&chat.0);
        let fail = |what: &str| {
            panic!(
                "round {round}, chat {}: {what}\nwindow {:?}..{:?} ({} rows), complete {}, has_newer {}, edge {:?}\n{}",
                chat.0,
                history.and_then(|h| h.oldest_id()),
                history.and_then(|h| h.newest_id()),
                history.map_or(0, |h| h.messages.len()),
                history.is_some_and(|h| h.loaded_complete),
                self.driver.session.history_has_newer(chat),
                self.edge,
                self.log[self.log.len().saturating_sub(40)..].join("\n")
            )
        };
        let Some(history) = history.filter(|h| !h.messages.is_empty()) else {
            fail("the open chat shows no messages");
            return;
        };
        let loaded: Vec<i64> = history.messages.keys().copied().collect();
        let lo = loaded.first().copied().unwrap_or_default();
        let hi = loaded.last().copied().unwrap_or_default();
        let expected: Vec<i64> = ids
            .iter()
            .copied()
            .filter(|id| (lo..=hi).contains(id))
            .collect();
        if loaded != expected {
            fail("the window has a gap: messages between its ends are missing");
        }
        match self.edge {
            Edge::Top => {
                if history.oldest_id() != ids.first().copied().map(MessageId) {
                    fail("scrolled to the top but the oldest messages never loaded");
                }
            }
            Edge::Bottom => {
                if history.newest_id() != ids.last().copied().map(MessageId) {
                    fail("at the bottom but the latest messages never loaded");
                }
            }
        }
    }
}

fn run(seed: u64, rounds: usize) {
    let mut sim = Sim::new(seed);
    sim.log.push(format!("seed {seed}"));
    for round in 0..rounds {
        sim.random_action();
        if sim.rng.chance(40) {
            sim.settle();
            sim.check(round);
        }
    }
    sim.settle();
    sim.check(rounds);
}

fn run_long(seed: u64, rounds: usize) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let lengths: Vec<i64> = CHATS.iter().map(|_| 900 + rng.below(800) as i64).collect();
    let mut sim = Sim::with_lengths(rng, &lengths);
    sim.log.push(format!("seed {seed}"));
    for round in 0..rounds {
        if sim.rng.chance(35) {
            let n = 1 + sim.rng.below(400) as i64;
            let open = sim.driver.session.open_chat.map_or(CHATS[0], |chat| chat.0);
            sim.burst(open, n);
        } else {
            sim.random_action();
        }
        if sim.rng.chance(40) {
            sim.settle();
            sim.check(round);
        }
    }
    sim.settle();
    sim.check(rounds);
}

#[test]
fn history_paging_never_stalls() {
    let seeds: u64 = std::env::var("QUILL_SIM_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);
    every_seed(seeds, |seed| run(seed, 60));
}

#[test]
fn paging_past_the_window_cap_under_live_messages_never_stalls() {
    let seeds: u64 = std::env::var("QUILL_SIM_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .map_or(4, |seeds: u64| (seeds / 40).max(1));
    every_seed(seeds, |seed| run_long(seed, 30));
}

fn every_seed(seeds: u64, run: impl Fn(u64) + std::panic::RefUnwindSafe) {
    let mut failures = BTreeMap::<String, Vec<u64>>::new();
    let mut first = BTreeMap::<String, String>::new();
    for seed in 1..=seeds {
        let outcome = std::panic::catch_unwind(|| run(seed));
        if let Err(panic) = outcome {
            let text = panic.downcast_ref::<String>().cloned().unwrap_or_default();
            let kind = text
                .lines()
                .next()
                .and_then(|line| line.split(": ").nth(1))
                .unwrap_or(&text)
                .to_string();
            first.entry(kind.clone()).or_insert(text);
            failures.entry(kind).or_default().push(seed);
        }
    }
    if !failures.is_empty() {
        let report: Vec<String> = failures
            .iter()
            .map(|(kind, seeds)| {
                format!(
                    "{} seeds: {kind} (e.g. {:?})\n{}",
                    seeds.len(),
                    &seeds[..seeds.len().min(5)],
                    first.get(kind).map_or("", String::as_str)
                )
            })
            .collect();
        panic!("{}", report.join("\n\n"));
    }
}

impl Sim {
    /// Every page is TDLib's full answer.
    fn scripted() -> Self {
        let mut sim = Self::with_lengths(Rng(1), &[200, 200, 200]);
        sim.short_pages = false;
        sim
    }

    fn pending(&mut self, chat: i64) -> Vec<(RequestId, i64, i32)> {
        self.collect_queries();
        self.queries
            .iter()
            .filter(|q| q.chat == chat)
            .map(|q| (q.extra, q.from, q.offset))
            .collect()
    }

    /// Answer the queued request `extra` with TDLib's full page.
    fn answer_full(&mut self, extra: RequestId) {
        self.collect_queries();
        let ix = self
            .queries
            .iter()
            .position(|q| q.extra == extra)
            .expect("queued request");
        let query = self.queries.swap_remove(ix);
        self.answer(query);
    }

    fn missed(&mut self, chat: i64, n: i64) {
        let last = self.fresh_ids(chat, n);
        let message = message_json(chat, last);
        self.ingest(&format!(
            r#"{{"@type":"updateChatLastMessage","chat_id":{chat},"last_message":{message},"positions":[]}}"#
        ));
    }
}

#[test]
fn reopening_a_chat_before_its_first_page_answers_still_fills_it() {
    let mut sim = Sim::scripted();
    sim.driver.select_chat(ChatId(7)).unwrap();
    let [(first, 0, 0)] = sim.pending(7)[..] else {
        panic!("one first page for chat 7");
    };
    sim.driver.select_chat(ChatId(8)).unwrap();
    sim.driver.select_chat(ChatId(7)).unwrap();
    assert_eq!(sim.pending(7).len(), 1, "the page on the way is reused");

    sim.answer_full(first);
    let history = sim.driver.session.histories.get(&7).expect("history");
    assert_eq!(history.newest_id(), Some(MessageId(2000)));
    assert_eq!(history.messages.len(), 50);
}

#[test]
fn a_chat_that_moved_on_without_new_message_updates_pages_to_its_latest() {
    let mut sim = Sim::scripted();
    sim.driver.select_chat(ChatId(7)).unwrap();
    let (first, ..) = sim.pending(7)[0];
    sim.answer_full(first);
    assert!(!sim.driver.session.history_has_newer(ChatId(7)));

    // After sleep TDLib moves the last message without an update for each
    // message in between.
    sim.missed(7, 30);
    assert!(sim.driver.session.history_has_newer(ChatId(7)));
    // A live message now would not be next to the window.
    sim.ingest(&format!(
        r#"{{"@type":"updateNewMessage","message":{}}}"#,
        message_json(7, 2310)
    ));
    assert!(!sim.driver.session.histories[&7].contains(MessageId(2310)));

    let newer = sim
        .driver
        .fetch_history_newer()
        .unwrap()
        .expect("newer page");
    sim.answer_full(newer);
    let history = &sim.driver.session.histories[&7];
    assert_eq!(history.newest_id(), Some(MessageId(2300)));
    assert_eq!(history.messages.len(), 80, "no gap between 2000 and 2300");
}

#[test]
fn reopening_a_chat_whose_kept_tail_fell_behind_opens_at_its_latest() {
    let mut sim = Sim::scripted();
    sim.driver.select_chat(ChatId(7)).unwrap();
    let (first, ..) = sim.pending(7)[0];
    sim.answer_full(first);
    sim.driver.select_chat(ChatId(8)).unwrap();
    sim.missed(7, 70);

    sim.driver.select_chat(ChatId(7)).unwrap();
    let pending = sim.pending(7);
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].1, 0,
        "the latest page, not one below the old tail"
    );
    sim.answer_full(pending[0].0);
    assert_eq!(
        sim.driver.session.histories[&7].newest_id(),
        Some(MessageId(2700))
    );
}

#[test]
fn jumping_to_the_latest_while_an_older_page_is_on_the_way_still_loads_it() {
    let mut sim = Sim::scripted();
    sim.driver.select_chat(ChatId(7)).unwrap();
    let (first, ..) = sim.pending(7)[0];
    sim.answer_full(first);
    sim.missed(7, 70);
    let older = sim.driver.fetch_history().unwrap().expect("older page");

    let latest = sim.driver.jump_to_latest().unwrap().expect("latest page");
    sim.answer_full(older);
    assert!(sim.driver.session.histories[&7].messages.is_empty());
    sim.answer_full(latest);
    assert_eq!(
        sim.driver.session.histories[&7].newest_id(),
        Some(MessageId(2700))
    );
}

#[test]
fn deleting_the_latest_message_leaves_the_window_at_the_latest() {
    let mut sim = Sim::scripted();
    sim.driver.select_chat(ChatId(7)).unwrap();
    let (first, ..) = sim.pending(7)[0];
    sim.answer_full(first);
    sim.ingest(r#"{"@type":"updateDeleteMessages","chat_id":7,"message_ids":[2000],"is_permanent":true,"from_cache":false}"#);
    sim.ingest(&format!(
        r#"{{"@type":"updateChatLastMessage","chat_id":7,"last_message":{},"positions":[]}}"#,
        message_json(7, 1990)
    ));
    assert!(!sim.driver.session.history_has_newer(ChatId(7)));
}

#[test]
fn a_reader_at_the_top_of_a_chat_left_open_still_reaches_both_ends() {
    let mut sim = Sim::with_lengths(Rng(3), &[1490, 10, 10]);
    sim.short_pages = false;
    let cap = crate::state::HISTORY_WINDOW_CAP as i64;
    sim.driver.select_chat(ChatId(7)).unwrap();
    sim.settle();
    sim.edge = Edge::Top;
    sim.settle();
    sim.check(0);
    assert_eq!(sim.driver.session.histories[&7].messages.len(), 1490);

    sim.burst(7, cap - 1490 + 200);
    let history = &sim.driver.session.histories[&7];
    assert_eq!(history.oldest_id(), Some(MessageId(10)));
    assert!(history.messages.len() < crate::state::HISTORY_WINDOW_CAP);
    assert!(sim.driver.session.history_has_newer(ChatId(7)));

    sim.edge = Edge::Bottom;
    sim.settle();
    sim.check(1);
    assert!(!sim.driver.session.history_has_newer(ChatId(7)));
    assert!(sim.driver.session.histories[&7].messages.len() <= crate::state::HISTORY_WINDOW_CAP);

    sim.edge = Edge::Top;
    sim.settle();
    sim.check(2);
}

#[test]
fn a_chat_left_open_at_the_bottom_pages_back_after_its_trim() {
    let mut sim = Sim::with_lengths(Rng(3), &[100, 10, 10]);
    sim.short_pages = false;
    let cap = crate::state::HISTORY_WINDOW_CAP as i64;
    sim.driver.select_chat(ChatId(7)).unwrap();
    sim.settle();
    sim.burst(7, cap + 400);
    sim.settle();
    sim.check(0);
    let history = &sim.driver.session.histories[&7];
    assert!(history.messages.len() <= crate::state::HISTORY_WINDOW_CAP);
    assert!(!history.loaded_complete);

    sim.edge = Edge::Top;
    sim.settle();
    sim.check(1);
}
