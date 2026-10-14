//! Connect-driver tests: revenue pages, withdrawals and affiliate
//! programs (recorded JSON only; nothing is withdrawn or joined live).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::earn::*;
use crate::platform::MemorySecretStore;
use crate::premium_hub::TxFilter;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::ChannelMemberStatus;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;
type Parts = (
    ConnectDriver<Recorder>,
    Recorder,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

const ME: i64 = 7;
const CHANNEL: i64 = -100_500;
const BOT: i64 = 900;

fn parts() -> Parts {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.my_user_id = Some(ME);
    (driver, recorder, sink, seq)
}

fn feed(p: &mut Parts, json: Value) {
    let (driver, _, sink, seq) = p;
    driver
        .ingest(copy_and_parse(&json.to_string(), seq, sink).unwrap())
        .unwrap();
}

fn count(recorder: &RecordingSender, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| serde_json::from_str::<Value>(json).is_ok_and(|v| v["@type"] == type_name))
        .count()
}

fn answer(p: &mut Parts, request: &str, mut body: Value) {
    let sent = sent_request(&p.1, request);
    body["@extra"] = sent["@extra"].clone();
    feed(p, body);
}

fn error(p: &mut Parts, request: &str, message: &str) {
    answer(
        p,
        request,
        json!({"@type": "error", "code": 400, "message": message}),
    );
}

fn channel(p: &mut Parts, id: i64, status: ChannelMemberStatus) {
    feed(
        p,
        json!({"@type": "updateNewChat", "chat": {"id": id, "title": "News",
            "type": {"@type": "chatTypeSupergroup", "supergroup_id": 5, "is_channel": true},
            "unread_count": 0}}),
    );
    if let Some(chat) = p.0.session.chats.get_mut(&id) {
        chat.set_member_status(status, None);
    }
}

fn ton_stats(balance: i64) -> Value {
    json!({"@type": "chatRevenueStatistics", "usd_rate": 0.0000004,
        "revenue_amount": {"@type": "chatRevenueAmount", "cryptocurrency": "TON",
            "total_amount": "9000000000", "balance_amount": balance.to_string(),
            "available_amount": "1000000000", "withdrawal_enabled": true}})
}

fn star_stats(current: i64) -> Value {
    json!({"@type": "starRevenueStatistics", "usd_rate": 1.3,
        "status": {"@type": "starRevenueStatus",
            "total_amount": {"@type": "starAmount", "star_count": 5000, "nanostar_count": 0},
            "current_amount": {"@type": "starAmount", "star_count": current, "nanostar_count": 0},
            "available_amount": {"@type": "starAmount", "star_count": current, "nanostar_count": 0},
            "withdrawal_enabled": true, "next_withdrawal_in": 0}})
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

fn connected(bot: i64, url: &str) -> Value {
    json!({"@type": "connectedAffiliateProgram", "url": url, "bot_user_id": bot,
        "parameters": {"@type": "affiliateProgramParameters", "commission_per_mille": 150, "month_count": 6},
        "connection_date": 10, "is_disconnected": false, "user_count": "3",
        "revenue_star_count": "40"})
}

#[test]
fn channel_monetization_loads_ton_stars_and_ads_link() {
    let mut p = parts();
    p.0.open_earn_page(EarnPeer::Chat(CHANNEL), EarnScreen::Monetization, true)
        .unwrap();
    let stats = sent_request(&p.1, "getChatRevenueStatistics");
    assert_eq!(stats["chat_id"], CHANNEL);
    assert_eq!(stats["is_dark"], true);
    let history = sent_request(&p.1, "getChatRevenueTransactions");
    assert_eq!(history["limit"], TON_FIRST_SLICE);
    assert_eq!(count(&p.1, "getStarRevenueStatistics"), 1);
    // One Stars list on a channel page.
    assert_eq!(count(&p.1, "getStarTransactions"), 1);
    assert_eq!(
        sent_request(&p.1, "getStarTransactions")["limit"],
        STAR_TX_LIMIT
    );
    assert_eq!(count(&p.1, "getStarAdAccountUrl"), 1);

    answer(&mut p, "getChatRevenueStatistics", ton_stats(3_000_000_000));
    answer(&mut p, "getStarRevenueStatistics", star_stats(250));
    answer(
        &mut p,
        "getStarAdAccountUrl",
        json!({"@type": "httpUrl", "url": "https://ads.example/acc"}),
    );
    let earn = &p.0.session.payments.earn;
    let page = earn.page.as_ref().unwrap();
    assert!(page.loaded());
    assert_eq!(page.ton.ready().unwrap().amounts.balance, 3_000_000_000);
    assert_eq!(page.stars.ready().unwrap().status.current.stars, 250);
    assert_eq!(
        page.ad_account_url.as_deref(),
        Some("https://ads.example/acc")
    );
    assert_eq!(
        earn.ton_balance(EarnPeer::Chat(CHANNEL)),
        Some(3_000_000_000)
    );
}

#[test]
fn megagroup_without_ton_ads_hides_the_ton_part() {
    let mut p = parts();
    p.0.open_earn_page(EarnPeer::Chat(CHANNEL), EarnScreen::Monetization, false)
        .unwrap();
    error(&mut p, "getChatRevenueStatistics", "BROADCAST_REQUIRED");
    let page = p.0.session.payments.earn.page.as_ref().unwrap();
    assert!(page.ton_unavailable);
    assert_eq!(page.ton, Load::Idle);
}

#[test]
fn ton_history_pages_by_ten_after_the_first_five() {
    let mut p = parts();
    p.0.open_earn_page(EarnPeer::Chat(CHANNEL), EarnScreen::Monetization, false)
        .unwrap();
    let row = json!({"@type": "chatRevenueTransaction", "cryptocurrency": "TON",
        "cryptocurrency_amount": "500", "type": {
            "@type": "chatRevenueTransactionTypeSponsoredMessageEarnings",
            "start_date": 1, "end_date": 2}});
    answer(
        &mut p,
        "getChatRevenueTransactions",
        json!({"@type": "chatRevenueTransactions", "ton_amount": 0,
            "transactions": [row.clone()], "next_offset": "n1"}),
    );
    p.0.more_earn_ton_history().unwrap();
    let more = sent_request(&p.1, "getChatRevenueTransactions");
    assert_eq!(more["offset"], "n1");
    assert_eq!(more["limit"], TON_MORE_SLICE);
    // A second tap while loading sends nothing.
    p.0.more_earn_ton_history().unwrap();
    assert_eq!(count(&p.1, "getChatRevenueTransactions"), 2);
    answer(
        &mut p,
        "getChatRevenueTransactions",
        json!({"@type": "chatRevenueTransactions", "ton_amount": 0,
            "transactions": [row], "next_offset": ""}),
    );
    let history = &p.0.session.payments.earn.page.as_ref().unwrap().ton_history;
    assert_eq!(history.rows.len(), 2);
    // The end: no more requests.
    p.0.more_earn_ton_history().unwrap();
    assert_eq!(count(&p.1, "getChatRevenueTransactions"), 2);
}

#[test]
fn bot_stars_page_loads_three_lists_and_own_page_none() {
    let mut p = parts();
    p.0.open_earn_page(EarnPeer::User(BOT), EarnScreen::Stars, false)
        .unwrap();
    assert_eq!(count(&p.1, "getChatRevenueStatistics"), 0);
    assert_eq!(count(&p.1, "getStarTransactions"), 3);
    let p2 = &mut parts();
    p2.0.open_earn_page(EarnPeer::User(ME), EarnScreen::Stars, false)
        .unwrap();
    assert_eq!(count(&p2.1, "getStarTransactions"), 0);
    assert_eq!(count(&p2.1, "getStarAdAccountUrl"), 0);
    let _ = TxFilter::All;
}

#[test]
fn withdrawal_asks_without_password_then_with_it() {
    let mut p = parts();
    let target = WithdrawTarget::Stars {
        owner: EarnPeer::User(BOT),
        count: 1500,
    };
    p.0.start_withdrawal(target).unwrap();
    let first = sent_request(&p.1, "getStarWithdrawalUrl");
    assert_eq!(first["password"], "");
    assert_eq!(first["star_count"], 1500);
    // Any other error → the password box.
    error(&mut p, "getStarWithdrawalUrl", "PASSWORD_HASH_INVALID");
    let withdraw = p.0.session.payments.earn.withdraw.clone().unwrap();
    assert_eq!(
        withdraw.step,
        WithdrawStep::Password {
            sending: false,
            error: None
        }
    );
    p.0.submit_withdrawal_password("hunter2").unwrap();
    assert_eq!(
        sent_request(&p.1, "getStarWithdrawalUrl")["password"],
        "hunter2"
    );
    // A wrong password stays in the box with an error.
    error(&mut p, "getStarWithdrawalUrl", "PASSWORD_HASH_INVALID");
    assert!(matches!(
        p.0.session.payments.earn.withdraw.as_ref().unwrap().step,
        WithdrawStep::Password { error: Some(_), .. }
    ));
    p.0.submit_withdrawal_password("right").unwrap();
    answer(
        &mut p,
        "getStarWithdrawalUrl",
        json!({"@type": "httpUrl", "url": "https://fragment.com/w"}),
    );
    let earn = &p.0.session.payments.earn;
    assert!(earn.withdraw.is_none());
    assert_eq!(earn.open_url.as_deref(), Some("https://fragment.com/w"));
}

#[test]
fn withdrawal_errors_follow_tdesktop() {
    let mut p = parts();
    let target = WithdrawTarget::Ton { chat_id: CHANNEL };
    p.0.start_withdrawal(target).unwrap();
    error(&mut p, "getChatRevenueWithdrawalUrl", "PASSWORD_MISSING");
    assert_eq!(
        p.0.session.payments.earn.withdraw.as_ref().unwrap().step,
        WithdrawStep::SecurityCheck { later: false }
    );
    p.0.cancel_withdrawal();
    p.0.start_withdrawal(target).unwrap();
    error(
        &mut p,
        "getChatRevenueWithdrawalUrl",
        "SESSION_TOO_FRESH_86400",
    );
    assert_eq!(
        p.0.session.payments.earn.withdraw.as_ref().unwrap().step,
        WithdrawStep::SecurityCheck { later: true }
    );
    p.0.cancel_withdrawal();
    p.0.start_withdrawal(target).unwrap();
    error(&mut p, "getChatRevenueWithdrawalUrl", "AMOUNT_TOO_SMALL");
    let earn = &p.0.session.payments.earn;
    assert!(earn.withdraw.is_none());
    assert_eq!(
        earn.notes,
        vec!["The amount is too small to withdraw.".to_string()]
    );
    // No Stars, no request.
    assert!(
        p.0.start_withdrawal(WithdrawTarget::Stars {
            owner: EarnPeer::User(BOT),
            count: 0
        })
        .is_err()
    );
}

#[test]
fn balances_are_fetched_once_and_follow_updates() {
    let mut p = parts();
    let peer = EarnPeer::Chat(CHANNEL);
    p.0.fetch_earn_balances(peer, true, true).unwrap();
    p.0.fetch_earn_balances(peer, true, true).unwrap();
    assert_eq!(count(&p.1, "getChatRevenueStatistics"), 1);
    assert_eq!(count(&p.1, "getStarTransactions"), 1);
    answer(&mut p, "getChatRevenueStatistics", ton_stats(42));
    assert_eq!(p.0.session.payments.earn.ton_balance(peer), Some(42));
    feed(
        &mut p,
        json!({"@type": "updateChatRevenueAmount", "chat_id": CHANNEL, "revenue_amount": {
            "@type": "chatRevenueAmount", "total_amount": "1", "balance_amount": "77",
            "available_amount": "77", "withdrawal_enabled": true}}),
    );
    assert_eq!(p.0.session.payments.earn.ton_balance(peer), Some(77));
    feed(
        &mut p,
        json!({"@type": "updateStarRevenueStatus",
            "owner_id": {"@type": "messageSenderChat", "chat_id": CHANNEL},
            "status": {"@type": "starRevenueStatus",
                "current_amount": {"@type": "starAmount", "star_count": 9, "nanostar_count": 0}}}),
    );
    assert_eq!(
        p.0.session.payments.earn.star_balance(peer).unwrap().stars,
        9
    );
}

#[test]
fn options_set_the_limits() {
    let mut p = parts();
    feed(
        &mut p,
        json!({"@type": "updateOption", "name": "star_withdrawal_count_min",
            "value": {"@type": "optionValueInteger", "value": "500"}}),
    );
    feed(
        &mut p,
        json!({"@type": "updateOption", "name": "affiliate_program_commission_per_mille_max",
            "value": {"@type": "optionValueInteger", "value": "900"}}),
    );
    let limits = p.0.session.payments.earn.limits;
    assert_eq!(limits.withdrawal_min(), 500);
    assert_eq!(limits.commission_range(), (1, 900));
}

#[test]
fn setup_starts_updates_and_ends_a_program() {
    let mut p = parts();
    p.0.open_starref_setup(BOT).unwrap();
    assert_eq!(count(&p.1, "getApplicationConfig"), 1);
    let setup = p.0.session.payments.earn.setup.as_mut().unwrap();
    assert!(!setup.exists());
    setup.params.commission_per_mille = 300;
    p.0.save_starref_program().unwrap();
    // Double tap: still one request.
    p.0.save_starref_program().unwrap();
    assert_eq!(count(&p.1, "setChatAffiliateProgram"), 1);
    let sent = sent_request(&p.1, "setChatAffiliateProgram");
    assert_eq!(sent["chat_id"], BOT);
    assert_eq!(sent["parameters"]["commission_per_mille"], 300);
    answer(&mut p, "setChatAffiliateProgram", json!({"@type": "ok"}));
    let earn = &p.0.session.payments.earn;
    assert!(earn.setup.is_none());
    assert!(earn.setup_closed);
    assert!(earn.notes[0].starts_with("Affiliate program started."));

    p.0.open_starref_setup(BOT).unwrap();
    p.0.end_starref_program().unwrap();
    let sent = sent_request(&p.1, "setChatAffiliateProgram");
    assert!(sent["parameters"].is_null());
    error(&mut p, "setChatAffiliateProgram", "STARREF_HASH_REVOKED");
    let earn = &p.0.session.payments.earn;
    assert!(!earn.setup.as_ref().unwrap().saving);
    assert!(earn.notes[1].starts_with("Couldn't save the affiliate program"));
}

#[test]
fn join_page_lists_sorts_and_joins_for_a_channel() {
    let mut p = parts();
    let page = EarnPeer::Chat(CHANNEL);
    p.0.open_starref_join(page).unwrap();
    let search = sent_request(&p.1, "searchAffiliatePrograms");
    assert_eq!(search["affiliate"]["@type"], "affiliateTypeChannel");
    assert_eq!(search["limit"], PROGRAMS_PER_PAGE);
    answer(&mut p, "searchAffiliatePrograms", found(&[BOT, BOT + 1]));
    answer(
        &mut p,
        "getConnectedAffiliatePrograms",
        json!({"@type": "connectedAffiliatePrograms", "total_count": 0, "programs": [], "next_offset": ""}),
    );
    p.0.set_starref_sort(ProgramSort::Revenue).unwrap();
    assert_eq!(count(&p.1, "searchAffiliatePrograms"), 2);
    answer(&mut p, "searchAffiliatePrograms", found(&[BOT, BOT + 1]));

    // Not joined yet: the join box for the page's channel, no recipients.
    p.0.open_affiliate_program(BOT, false).unwrap();
    assert_eq!(count(&p.1, "getCreatedPublicChats"), 0);
    error(&mut p, "getConnectedAffiliateProgram", "NOT_FOUND");
    let join = p.0.session.payments.earn.join_box.clone().unwrap();
    assert_eq!(join.recipient, page);
    assert!(!join.choose_recipient);

    p.0.join_affiliate_program().unwrap();
    p.0.join_affiliate_program().unwrap();
    assert_eq!(count(&p.1, "connectAffiliateProgram"), 1);
    answer(
        &mut p,
        "connectAffiliateProgram",
        connected(BOT, "https://t.me/bot?start=_tgr_x"),
    );
    let earn = &p.0.session.payments.earn;
    assert!(earn.join_box.is_none());
    assert_eq!(earn.link_box.as_ref().unwrap().program.user_count, 3);
    let join = earn.join.as_ref().unwrap();
    assert_eq!(join.mine.programs.len(), 1);
    assert!(join.found.programs.iter().all(|f| f.bot_user_id != BOT));

    // Revoke moves it back.
    let program = join.mine.programs[0].clone();
    p.0.session.payments.earn.revoke = Some(RevokeBox {
        program,
        recipient: page,
        sending: false,
    });
    p.0.revoke_affiliate_link().unwrap();
    let sent = sent_request(&p.1, "disconnectAffiliateProgram");
    assert_eq!(sent["url"], "https://t.me/bot?start=_tgr_x");
    let mut gone = connected(BOT, "https://t.me/bot?start=_tgr_x");
    gone["is_disconnected"] = json!(true);
    answer(&mut p, "disconnectAffiliateProgram", gone);
    let earn = &p.0.session.payments.earn;
    assert!(earn.revoke.is_none());
    let join = earn.join.as_ref().unwrap();
    assert!(join.mine.programs.is_empty());
    assert_eq!(join.found.programs[0].bot_user_id, BOT);
    assert_eq!(
        earn.notes.last().unwrap(),
        "Link removed. It will no longer work."
    );
}

#[test]
fn own_page_waits_for_recipients_and_keeps_postable_channels() {
    let mut p = parts();
    channel(&mut p, CHANNEL, ChannelMemberStatus::Creator);
    channel(&mut p, CHANNEL - 1, ChannelMemberStatus::Member);
    p.0.open_starref_join(EarnPeer::User(ME)).unwrap();
    assert_eq!(
        sent_request(&p.1, "searchAffiliatePrograms")["affiliate"]["@type"],
        "affiliateTypeCurrentUser"
    );
    answer(&mut p, "searchAffiliatePrograms", found(&[BOT]));
    p.0.open_affiliate_program(BOT, false).unwrap();
    assert_eq!(count(&p.1, "getCreatedPublicChats"), 1);
    assert_eq!(count(&p.1, "getOwnedBots"), 1);
    error(&mut p, "getConnectedAffiliateProgram", "NOT_FOUND");
    // Parked until both recipient lists are in.
    assert!(p.0.session.payments.earn.join_box.is_none());
    answer(
        &mut p,
        "getCreatedPublicChats",
        json!({"@type": "chats", "total_count": 2, "chat_ids": [CHANNEL, CHANNEL - 1]}),
    );
    assert!(p.0.session.payments.earn.join_box.is_none());
    answer(
        &mut p,
        "getOwnedBots",
        json!({"@type": "users", "total_count": 1, "user_ids": [BOT + 5]}),
    );
    let earn = &p.0.session.payments.earn;
    let join = earn.join_box.as_ref().unwrap();
    assert!(join.choose_recipient);
    assert_eq!(
        earn.recipients.as_deref().unwrap(),
        &[
            EarnPeer::User(ME),
            EarnPeer::Chat(CHANNEL),
            EarnPeer::User(BOT + 5)
        ]
    );
    // A live link opens the link box instead.
    p.0.open_affiliate_program(BOT, false).unwrap();
    assert_eq!(count(&p.1, "getCreatedPublicChats"), 1);
    answer(
        &mut p,
        "getConnectedAffiliateProgram",
        connected(BOT, "https://t.me/bot?start=_tgr_y"),
    );
    assert!(p.0.session.payments.earn.link_box.is_some());
}

#[test]
fn nothing_is_sent_before_sign_in() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let session = crate::state::Session::new(crate::ids::AccountKey::primary(), sink);
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    assert!(
        driver
            .open_earn_page(EarnPeer::Chat(CHANNEL), EarnScreen::Monetization, false)
            .is_err()
    );
    assert!(
        driver
            .start_withdrawal(WithdrawTarget::Ton { chat_id: CHANNEL })
            .is_err()
    );
    assert_eq!(recorder.snapshot().len(), 0);
}
