//! Sweep s3-payments-1: a request that never left, or an answer that is not
//! the asked one, leaves the stage of the thing that asked (recorded JSON
//! only).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::earn::*;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

/// Records what it sends and refuses the TDLib types it is told to refuse.
#[derive(Default)]
pub(crate) struct FlakySender {
    sent: Mutex<Vec<String>>,
    refuse: Mutex<HashSet<&'static str>>,
}

impl FlakySender {
    pub(crate) fn refuse(&self, types: &[&'static str]) {
        if let Ok(mut set) = self.refuse.lock() {
            *set = types.iter().copied().collect();
        }
    }

    pub(crate) fn last(&self, type_name: &str) -> Option<Value> {
        let sent = self.sent.lock().ok()?;
        sent.iter()
            .rev()
            .filter_map(|json| serde_json::from_str::<Value>(json).ok())
            .find(|value| value["@type"] == type_name)
    }

    pub(crate) fn count(&self, type_name: &str) -> usize {
        let Ok(sent) = self.sent.lock() else {
            return 0;
        };
        sent.iter()
            .filter_map(|json| serde_json::from_str::<Value>(json).ok())
            .filter(|value| value["@type"] == type_name)
            .count()
    }
}

impl JsonSender for Arc<FlakySender> {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        let refused = self.refuse.lock().is_ok_and(|set| {
            set.iter()
                .any(|name| request.contains(&format!(r#""@type":"{name}""#)))
        });
        if refused {
            return Err(ConnectSendError::Native);
        }
        if let Ok(mut sent) = self.sent.lock() {
            sent.push(request.to_string());
        }
        Ok(())
    }
}

pub(crate) struct Flaky {
    pub(crate) driver: ConnectDriver<Arc<FlakySender>>,
    pub(crate) sender: Arc<FlakySender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
}

impl Flaky {
    pub(crate) fn new() -> Self {
        let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let sender = Arc::new(FlakySender::default());
        let session = crate::state::Session::new(crate::ids::AccountKey::primary(), sink.clone());
        let mut driver = ConnectDriver::new(session, sender.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        let ready = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#;
        if let Some(parsed) = copy_and_parse(ready, &seq, &sink) {
            let _ = driver.ingest(parsed);
        }
        driver.session.my_user_id = Some(ME);
        Self {
            driver,
            sender,
            sink,
            seq,
        }
    }

    pub(crate) fn feed(&mut self, json: Value) {
        let parsed = copy_and_parse(&json.to_string(), &self.seq, &self.sink).unwrap();
        self.driver.ingest(parsed).unwrap();
    }

    /// Answer the latest request of `request`.
    pub(crate) fn answer(&mut self, request: &str, mut body: Value) {
        body["@extra"] = self.sender.last(request).unwrap()["@extra"].clone();
        self.feed(body);
    }

    pub(crate) fn fail(&mut self, request: &str, message: &str) {
        self.answer(
            request,
            json!({"@type": "error", "code": 400, "message": message}),
        );
    }
}

const ME: i64 = 7;
const CHANNEL: i64 = -100_500;
const BOT: i64 = 900;

fn ton_stats(balance: i64) -> Value {
    json!({"@type": "chatRevenueStatistics", "usd_rate": 0.0000004,
        "revenue_amount": {"@type": "chatRevenueAmount", "cryptocurrency": "TON",
            "total_amount": "100", "balance_amount": balance.to_string(),
            "available_amount": balance.to_string(), "withdrawal_enabled": true}})
}

fn found(bots: &[i64]) -> Value {
    let programs: Vec<Value> = bots
        .iter()
        .map(|id| {
            json!({"@type": "foundAffiliateProgram", "bot_user_id": id, "info": {
                "@type": "affiliateProgramInfo",
                "parameters": {"@type": "affiliateProgramParameters",
                    "commission_per_mille": 150, "month_count": 6},
                "end_date": 0,
                "daily_revenue_per_user_amount": {"@type": "starAmount", "star_count": 2, "nanostar_count": 0}}})
        })
        .collect();
    json!({"@type": "foundAffiliatePrograms", "total_count": programs.len(),
        "programs": programs, "next_offset": ""})
}

#[test]
fn a_refused_balance_send_or_error_is_asked_again_at_the_next_open() {
    let mut t = Flaky::new();
    let peer = EarnPeer::Chat(CHANNEL);
    t.sender.refuse(&["getChatRevenueStatistics"]);
    assert!(t.driver.fetch_earn_balances(peer, true, true).is_err());
    let earn = &mut t.driver.session.payments.earn;
    assert_eq!(
        earn.balance_asks.get(&peer),
        Some(&crate::state::fetch::BatchAsk::Failed)
    );
    // Nothing asks again by itself while the profile stays open.
    t.sender.refuse(&[]);
    t.driver.fetch_earn_balances(peer, true, true).unwrap();
    assert_eq!(t.sender.count("getChatRevenueStatistics"), 0);
    // The next open forgets the failure and asks.
    t.driver.session.payments.earn.forget_failed_balances();
    t.driver.fetch_earn_balances(peer, true, false).unwrap();
    assert_eq!(t.sender.count("getChatRevenueStatistics"), 1);
    t.fail("getChatRevenueStatistics", "FLOOD_WAIT_3");
    assert_eq!(
        t.driver.session.payments.earn.balance_asks.get(&peer),
        Some(&crate::state::fetch::BatchAsk::Failed)
    );
    t.driver.session.payments.earn.forget_failed_balances();
    t.driver.fetch_earn_balances(peer, true, false).unwrap();
    assert_eq!(t.sender.count("getChatRevenueStatistics"), 2);
    t.answer("getChatRevenueStatistics", ton_stats(5));
    assert_eq!(
        t.driver.session.payments.earn.balance_asks.get(&peer),
        Some(&crate::state::fetch::BatchAsk::Answered)
    );
}

#[test]
fn a_page_whose_later_send_is_refused_shows_its_error_not_a_spinner() {
    let mut t = Flaky::new();
    let peer = EarnPeer::Chat(CHANNEL);
    t.sender.refuse(&["getStarRevenueStatistics"]);
    assert!(
        t.driver
            .open_earn_page(peer, EarnScreen::Monetization, false)
            .is_err()
    );
    let page = t.driver.session.payments.earn.page.clone().unwrap();
    assert!(matches!(page.stars, Load::Failed(_)), "{:?}", page.stars);
    // The TON side did go out, and its answer still lands.
    assert!(page.ton.is_loading());
    assert!(page.ton_history.load.is_loading());
    t.answer("getChatRevenueStatistics", ton_stats(9));
    let page = t.driver.session.payments.earn.page.clone().unwrap();
    assert!(page.ton.ready().is_some());
}

#[test]
fn a_refused_recipient_send_is_not_a_cached_partial_list() {
    let mut t = Flaky::new();
    t.driver.open_starref_join(EarnPeer::User(ME)).unwrap();
    t.answer("searchAffiliatePrograms", found(&[BOT]));
    t.sender.refuse(&["getOwnedBots"]);
    assert!(t.driver.open_affiliate_program(BOT, false).is_err());
    assert_eq!(
        t.driver.session.payments.earn.recipients,
        Recipients::Failed
    );
    // The chats list that did go out answers late: nothing is cached.
    t.answer(
        "getCreatedPublicChats",
        json!({"@type": "chats", "total_count": 0, "chat_ids": []}),
    );
    assert_eq!(
        t.driver.session.payments.earn.recipients,
        Recipients::Failed
    );
    // A retry asks both again and settles when both answer.
    t.sender.refuse(&[]);
    t.driver.open_affiliate_program(BOT, false).unwrap();
    assert_eq!(t.sender.count("getCreatedPublicChats"), 2);
    t.answer(
        "getCreatedPublicChats",
        json!({"@type": "chats", "total_count": 0, "chat_ids": []}),
    );
    t.answer(
        "getOwnedBots",
        json!({"@type": "users", "total_count": 1, "user_ids": [BOT + 5]}),
    );
    assert_eq!(
        t.driver.session.payments.earn.recipients.list().unwrap(),
        &[EarnPeer::User(ME), EarnPeer::User(BOT + 5)]
    );
}

#[test]
fn an_older_program_page_does_not_land_on_the_list_a_sort_reset() {
    let mut t = Flaky::new();
    t.driver.open_starref_join(EarnPeer::Chat(CHANNEL)).unwrap();
    let first = t.sender.last("searchAffiliatePrograms").unwrap();
    t.driver.set_starref_sort(ProgramSort::Revenue).unwrap();
    // The answer of the first (default sort) request is late.
    let mut late = found(&[BOT]);
    late["@extra"] = first["@extra"].clone();
    t.feed(late);
    let list = &t.driver.session.payments.earn.join.as_ref().unwrap().found;
    assert!(list.programs.is_empty(), "the old order is not shown");
    assert!(list.load.is_loading());
    t.answer("searchAffiliatePrograms", found(&[BOT + 1]));
    let list = &t.driver.session.payments.earn.join.as_ref().unwrap().found;
    assert_eq!(list.programs.len(), 1);
    assert_eq!(list.programs[0].bot_user_id, BOT + 1);
    assert_eq!(list.load, PageLoad::Ready);
}
