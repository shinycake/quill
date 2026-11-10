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

#[test]
fn a_commit_that_cannot_be_sent_keeps_the_stars_and_tells_once() {
    use crate::ids::{ChatId, MessageId};
    let mut t = Flaky::new();
    t.driver
        .add_paid_reaction(ChatId(-100), MessageId(7), 3, None, 0)
        .unwrap();
    t.sender.refuse(&["commitPendingPaidMessageReactions"]);
    assert!(
        t.driver
            .paid_reaction_tick(crate::paid_reaction::ACCUMULATE_MS)
    );
    let paid = &t.driver.session.payments.paid;
    assert_eq!(paid.pending_stars(-100, 7), 3, "TDLib still holds them");
    assert!(paid.has_pending(), "the toast and its Undo are back");
    assert_eq!(t.driver.session.payments.notices.len(), 1);
    // The window restarted; the next try is one window later, and the
    // user is not told again.
    assert!(
        !t.driver
            .paid_reaction_tick(crate::paid_reaction::ACCUMULATE_MS + 1)
    );
    let later = 2 * crate::paid_reaction::ACCUMULATE_MS;
    assert!(t.driver.paid_reaction_tick(later));
    assert_eq!(t.driver.session.payments.notices.len(), 1);
    t.sender.refuse(&[]);
    assert!(
        t.driver
            .paid_reaction_tick(3 * crate::paid_reaction::ACCUMULATE_MS)
    );
    assert_eq!(t.sender.count("commitPendingPaidMessageReactions"), 1);
    t.answer("commitPendingPaidMessageReactions", json!({"@type": "ok"}));
    assert_eq!(t.driver.session.payments.paid.total_pending(), 0);
}

#[test]
fn a_refused_commit_answer_tells_the_user_and_ends_the_entry() {
    use crate::ids::{ChatId, MessageId};
    let mut t = Flaky::new();
    t.driver
        .add_paid_reaction(ChatId(-100), MessageId(7), 3, None, 0)
        .unwrap();
    t.driver
        .paid_reaction_tick(crate::paid_reaction::ACCUMULATE_MS);
    t.fail("commitPendingPaidMessageReactions", "BALANCE_TOO_LOW");
    assert_eq!(t.driver.session.payments.paid.total_pending(), 0);
    assert!(
        t.driver
            .session
            .payments
            .notices
            .iter()
            .any(|n| n.text.starts_with("Stars were not sent"))
    );
}

#[test]
fn a_late_refusal_of_an_earlier_tap_leaves_the_newer_tap_alone() {
    use crate::ids::{ChatId, MessageId};
    let mut t = Flaky::new();
    t.driver
        .add_paid_reaction(ChatId(-100), MessageId(7), 1, None, 0)
        .unwrap();
    let first = t.sender.last("addPendingPaidMessageReaction").unwrap();
    t.driver
        .paid_reaction_tick(crate::paid_reaction::ACCUMULATE_MS);
    t.driver
        .add_paid_reaction(
            ChatId(-100),
            MessageId(7),
            4,
            None,
            crate::paid_reaction::ACCUMULATE_MS + 1,
        )
        .unwrap();
    let mut refused = json!({"@type": "error", "code": 400, "message": "REACTION_INVALID"});
    refused["@extra"] = first["@extra"].clone();
    t.feed(refused);
    let paid = &t.driver.session.payments.paid;
    assert_eq!(paid.toast().map(|p| p.stars), Some(4));
    assert_eq!(paid.total_pending(), 4);
}

fn open_send_gift_dialog(t: &mut Flaky) {
    t.feed(
        json!({"@type":"updateNewChat","chat":{"id":21,"title":"Ann",
        "type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}),
    );
    t.driver.open_send_gift(crate::ids::ChatId(21)).unwrap();
    t.answer(
        "getAvailableGifts",
        json!({"@type": "availableGifts", "gifts": [
            {"@type": "availableGift", "resale_count": 0, "gift": {
                "@type": "gift", "id": "55", "star_count": 50, "upgrade_star_count": 0}}]}),
    );
}

#[test]
fn a_gift_sent_after_its_box_closed_is_reported_either_way() {
    let mut t = Flaky::new();
    open_send_gift_dialog(&mut t);
    t.driver.send_star_gift(55, "", false, false).unwrap();
    t.driver.close_send_gift();
    t.answer("sendGift", json!({"@type": "ok"}));
    let notices = &t.driver.session.payments.notices;
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].text, "Gift sent.");
    assert!(!t.driver.session.payments.gifts_manage.send.sending());

    // A refusal after the box closed is told the same way.
    t.driver.session.payments.notices.clear();
    open_send_gift_dialog(&mut t);
    t.driver.send_star_gift(55, "", false, false).unwrap();
    t.driver.close_send_gift();
    t.fail("sendGift", "STARGIFT_USAGE_LIMITED");
    let notices = &t.driver.session.payments.notices;
    assert_eq!(notices.len(), 1);
    assert!(notices[0].text.starts_with("Couldn't send the gift"));
    assert_eq!(notices[0].kind, crate::state::NoticeKind::Error);
}

#[test]
fn gift_limits_that_failed_are_asked_again_at_the_next_gifts_open() {
    use crate::telegram::envelope::MessageSender;
    let mut t = Flaky::new();
    let owner = MessageSender::User { user_id: ME };
    t.sender.refuse(&["getOption"]);
    t.driver.begin_gift_manage(owner);
    assert_eq!(
        t.driver.session.payments.gifts_manage.limits_ask,
        crate::state::fetch::BatchAsk::Failed
    );
    t.sender.refuse(&[]);
    t.driver.begin_gift_manage(owner);
    assert!(matches!(
        t.driver.session.payments.gifts_manage.limits_ask,
        crate::state::fetch::BatchAsk::Asking { .. }
    ));
    let asked = t.sender.count("getOption");
    // Nothing asks again while they are out.
    t.driver.begin_gift_manage(owner);
    assert_eq!(t.sender.count("getOption"), asked);
}

#[test]
fn a_receipt_the_user_dismissed_does_not_reopen() {
    use crate::ids::{ChatId, MessageId};
    let mut t = Flaky::new();
    t.driver
        .fetch_payment_receipt(ChatId(51), MessageId(3))
        .unwrap();
    assert!(matches!(
        t.driver.session.payments.receipt,
        crate::state::ReceiptStage::Loading(_)
    ));
    t.driver.session.payments.receipt = crate::state::ReceiptStage::Closed;
    t.answer(
        "getPaymentReceipt",
        json!({"@type": "paymentReceipt", "date": 1, "seller_bot_user_id": 51,
            "product_info": {"@type": "productInfo", "title": "T",
                "description": {"@type": "formattedText", "text": "", "entities": []}, "photo": null},
            "type": {"@type": "paymentReceiptTypeStars", "star_count": 5,
                "transaction_id": "x", "is_refunded": false}}),
    );
    assert!(!t.driver.session.payments.receipt.is_open());
}

#[test]
fn a_refused_payment_form_send_leaves_no_checkout_behind() {
    use crate::payment_invoice::InputInvoice;
    let mut t = Flaky::new();
    t.sender.refuse(&["getPaymentForm"]);
    let invoice = InputInvoice::Name("slug".into());
    assert!(t.driver.send_payment_form_request(invoice).is_err());
    let payments = &t.driver.session.payments;
    assert!(payments.request.is_none());
    assert!(payments.form_request.is_none());
}

#[test]
fn a_buy_in_flight_is_not_replaced_by_another_resale_box() {
    let mut t = Flaky::new();
    let market = &mut t.driver.session.payments.gift_market;
    let gift = crate::premium_hub::upgraded::parse_collectible(&json!({
        "@type": "upgradedGift", "id": "9001", "regular_gift_id": "500",
        "title": "Plush Pepe", "name": "PlushPepe-1", "number": 1,
        "model": {"@type": "upgradedGiftModel", "name": "Gold", "rarity": {"@type": "upgradedGiftAttributeRarityRare"}},
        "symbol": {"@type": "upgradedGiftSymbol", "name": "Star", "rarity": {"@type": "upgradedGiftAttributeRarityRare"}},
        "backdrop": {"@type": "upgradedGiftBackdrop", "id": 3, "name": "Onyx",
            "colors": {"center_color": 1, "edge_color": 2, "symbol_color": 3, "text_color": 4},
            "rarity": {"@type": "upgradedGiftAttributeRarityRare"}},
        "owner_id": {"@type": "messageSenderUser", "user_id": 99}})).unwrap();
    let listing = crate::gift_market::ResaleListing {
        gift,
        received_gift_id: String::new(),
    };
    let recipient = crate::gift_manage::GiftRecipient {
        sender: crate::telegram::envelope::MessageSender::User { user_id: ME },
        name: String::new(),
        is_self: true,
        is_channel: false,
    };
    let mut buy = crate::gift_market::ResaleBuy::new(listing, recipient, false);
    buy.sending = true;
    market.buy = Some(buy);
    assert!(t.driver.open_gift_resale(5, "Pepe").is_err());
    assert!(t.driver.start_gift_resale_buy("x", None, false).is_err());
    assert!(
        t.driver
            .session
            .payments
            .gift_market
            .buy
            .as_ref()
            .is_some_and(|b| b.sending),
        "the purchase keeps its box"
    );
}

#[test]
fn a_refused_auction_follow_is_not_asked_again_every_frame() {
    use crate::gift_market::AuctionFollow;
    let mut t = Flaky::new();
    t.sender.refuse(&["openGiftAuction"]);
    assert!(t.driver.follow_gift_auction(Some(800)).is_err());
    assert_eq!(
        t.driver.session.payments.gift_market.follow,
        AuctionFollow::Failed(800)
    );
    t.sender.refuse(&[]);
    t.driver.follow_gift_auction(Some(800)).unwrap();
    assert_eq!(t.sender.count("openGiftAuction"), 0, "same box: no retry");
    // Closing the box and opening it again follows afresh.
    t.driver.follow_gift_auction(None).unwrap();
    t.driver.follow_gift_auction(Some(800)).unwrap();
    assert_eq!(t.sender.count("openGiftAuction"), 1);
    // An error answer ends the follow the same way.
    t.fail("openGiftAuction", "AUCTION_INVALID");
    assert_eq!(
        t.driver.session.payments.gift_market.follow,
        AuctionFollow::Failed(800)
    );
}

fn admin_channel(t: &mut Flaky) {
    t.feed(json!({"@type":"updateNewChat","chat":{"id":13,"title":"C",
        "type":{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":false},
        "unread_count":0}}));
    t.feed(
        json!({"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":113,
        "status":{"@type":"chatMemberStatusCreator","is_member":true}}}),
    );
}

fn boosts(ids: &[i64], next: &str) -> Value {
    let rows: Vec<Value> = ids
        .iter()
        .map(|id| {
            json!({"@type":"chatBoost","id":format!("b{id}"),"count":1,
                "source":{"@type":"chatBoostSourcePremium","user_id":id},
                "start_date":100,"expiration_date":900})
        })
        .collect();
    json!({"@type":"foundChatBoosts","total_count":5,"boosts":rows,"next_offset":next})
}

#[test]
fn appending_boosts_shows_loading_once_and_a_refused_page_keeps_the_rows() {
    use crate::ids::ChatId;
    let mut t = Flaky::new();
    admin_channel(&mut t);
    t.driver.open_chat_boosts(ChatId(13), false).unwrap();
    t.answer("getChatBoosts", boosts(&[1, 2], "n1"));
    assert!(
        t.driver
            .load_more_chat_boosts(ChatId(13))
            .unwrap()
            .is_some()
    );
    let list = t.driver.session.groups.chat_boost_lists.get(&13).unwrap();
    assert!(list.loading(), "the Show more spinner");
    assert!(
        t.driver
            .load_more_chat_boosts(ChatId(13))
            .unwrap()
            .is_none(),
        "no second request for the same offset"
    );
    assert_eq!(t.sender.count("getChatBoosts"), 2);
    t.answer("getChatBoosts", boosts(&[3], "n2"));

    // The next page is refused at the send: the rows stay, with an error.
    t.sender.refuse(&["getChatBoosts"]);
    assert!(t.driver.load_more_chat_boosts(ChatId(13)).is_err());
    let list = t.driver.session.groups.chat_boost_lists.get(&13).unwrap();
    assert_eq!(list.boosts.len(), 3);
    assert!(!list.loading());
    assert!(list.error().is_some());

    // A first page that cannot be sent shows its error, not "Loading".
    assert!(t.driver.open_chat_boosts(ChatId(13), true).is_err());
    let list = t.driver.session.groups.chat_boost_lists.get(&13).unwrap();
    assert!(list.boosts.is_empty() && list.error().is_some());
}

#[test]
fn a_star_history_or_gifts_first_page_that_cannot_be_sent_shows_its_error() {
    let mut t = Flaky::new();
    t.sender
        .refuse(&["getStarTransactions", "getReceivedGifts"]);
    assert!(
        t.driver
            .set_star_transactions_filter(crate::premium_hub::TxFilter::Outgoing)
            .is_err()
    );
    assert!(t.driver.session.payments.hub.tx_error.is_some());
    assert!(
        t.driver
            .open_received_gifts(crate::telegram::envelope::MessageSender::User { user_id: ME })
            .is_err()
    );
    let hub = &t.driver.session.payments.hub;
    assert!(hub.gifts_error.is_some() && !hub.gifts_loading);
}

#[test]
fn a_stale_gift_refetch_waits_for_a_page_in_flight_and_is_cleared_by_its_answer() {
    let mut t = Flaky::new();
    let owner = crate::telegram::envelope::MessageSender::User { user_id: ME };
    t.driver.open_received_gifts(owner).unwrap();
    t.driver.session.payments.hub.gifts_stale = true;
    // The first page is still out: no second first-page request.
    assert!(
        t.driver
            .refresh_received_gifts_if_stale()
            .unwrap()
            .is_none()
    );
    assert_eq!(t.sender.count("getReceivedGifts"), 1);
    assert!(t.driver.session.payments.hub.gifts_stale);
    t.answer(
        "getReceivedGifts",
        json!({"@type": "receivedGifts", "total_count": 0, "gifts": [], "next_offset": ""}),
    );
    assert!(!t.driver.session.payments.hub.gifts_stale);
    // A refetch whose send is refused is not retried every frame.
    t.driver.session.payments.hub.gifts_stale = true;
    t.sender.refuse(&["getReceivedGifts"]);
    assert!(t.driver.refresh_received_gifts_if_stale().is_err());
    assert!(!t.driver.session.payments.hub.gifts_stale);
    assert!(t.driver.session.payments.hub.gifts_error.is_some());
}

#[test]
fn reopening_the_gifts_box_keeps_the_lock_of_a_mutation_in_flight() {
    let mut t = Flaky::new();
    let owner = crate::telegram::envelope::MessageSender::User { user_id: ME };
    t.driver.session.payments.hub.gift_mutating = true;
    t.driver.open_received_gifts(owner).unwrap();
    assert!(t.driver.session.payments.hub.gift_mutating);
}
