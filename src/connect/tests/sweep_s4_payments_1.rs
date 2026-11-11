//! Sweep s4-payments-1: answers act on the box, list or request that asked
//! (recorded JSON only).
use super::sweep_s3_payments_1::Flaky;
use crate::earn::*;
use serde_json::{Value, json};

const CHANNEL: i64 = -100_500;
const BOT: i64 = 900;

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

fn ton_rows(offset: &str) -> Value {
    let row = json!({"@type": "chatRevenueTransaction", "cryptocurrency": "TON",
        "cryptocurrency_amount": "500", "type": {
            "@type": "chatRevenueTransactionTypeSponsoredMessageEarnings",
            "start_date": 1, "end_date": 2}});
    json!({"@type": "chatRevenueTransactions", "ton_amount": 0,
        "transactions": [row], "next_offset": offset})
}

/// A join page for the channel with two programs listed.
fn join_page(t: &mut Flaky) {
    t.driver.open_starref_join(EarnPeer::Chat(CHANNEL)).unwrap();
    t.answer("searchAffiliatePrograms", found(&[BOT, BOT + 1]));
    t.answer(
        "getConnectedAffiliatePrograms",
        json!({"@type": "connectedAffiliatePrograms", "total_count": 0, "programs": [], "next_offset": ""}),
    );
}

fn not_joined() -> Value {
    json!({"@type": "error", "code": 404, "message": "NOT_FOUND"})
}

/// 02-051: a refused "End program" send keeps the confirmation box.
#[test]
fn a_refused_end_keeps_the_confirmation_box() {
    let mut t = Flaky::new();
    t.driver.open_starref_setup(BOT).unwrap();
    let setup = t.driver.session.payments.earn.setup.as_mut().unwrap();
    setup.confirm = Some(SetupConfirm::End);
    t.sender.refuse(&["setChatAffiliateProgram"]);
    assert!(t.driver.end_starref_program().is_err());
    let setup = t.driver.session.payments.earn.setup.as_ref().unwrap();
    assert_eq!(setup.confirm, Some(SetupConfirm::End));
    assert!(!setup.is_saving());
    t.sender.refuse(&[]);
    t.driver.end_starref_program().unwrap();
    let setup = t.driver.session.payments.earn.setup.as_ref().unwrap();
    assert_eq!(setup.confirm, None);
    assert!(setup.is_saving());
}

/// 05-123: a page of a history that was reloaded is not the new one's.
#[test]
fn an_old_ton_history_page_is_not_applied_to_the_reloaded_page() {
    let mut t = Flaky::new();
    let peer = EarnPeer::Chat(CHANNEL);
    t.driver
        .open_earn_page(peer, EarnScreen::Monetization, false)
        .unwrap();
    t.driver
        .open_earn_page(peer, EarnScreen::Monetization, false)
        .unwrap();
    t.answer_nth("getChatRevenueTransactions", 0, ton_rows("old"));
    let page = t.driver.session.payments.earn.page.as_ref().unwrap();
    assert!(page.ton_history.load.is_loading());
    assert!(page.ton_history.rows.is_empty());
    t.answer_nth("getChatRevenueTransactions", 1, ton_rows(""));
    let page = t.driver.session.payments.earn.page.as_ref().unwrap();
    assert_eq!(page.ton_history.load, PageLoad::Ready);
    assert_eq!(page.ton_history.rows.len(), 1);
    // An old error does not fail the list either.
    t.driver
        .open_earn_page(peer, EarnScreen::Monetization, false)
        .unwrap();
    t.answer_nth("getChatRevenueTransactions", 1, not_joined());
    let page = t.driver.session.payments.earn.page.as_ref().unwrap();
    assert!(page.ton_history.load.is_loading());
}

/// 03-027, 05-124: only the click being resolved opens a box.
#[test]
fn a_late_resolve_answer_opens_nothing_for_a_program_tapped_past() {
    let mut t = Flaky::new();
    join_page(&mut t);
    t.driver.open_affiliate_program(BOT, false).unwrap();
    t.driver.open_affiliate_program(BOT + 1, false).unwrap();
    t.answer_nth("getConnectedAffiliateProgram", 0, not_joined());
    assert!(t.driver.session.payments.earn.join_box.is_none());
    t.answer_nth("getConnectedAffiliateProgram", 1, not_joined());
    let earn = &t.driver.session.payments.earn;
    assert_eq!(earn.join_box.as_ref().unwrap().bot_user_id, BOT + 1);
    assert_eq!(earn.resolve, Resolve::Idle);
}

#[test]
fn a_resolve_answer_after_leaving_the_page_opens_nothing() {
    let mut t = Flaky::new();
    join_page(&mut t);
    t.driver.open_affiliate_program(BOT, false).unwrap();
    t.driver
        .session
        .set_open_info_panel(Some(crate::state::InfoPanelTarget::User(BOT)));
    t.answer(
        "getConnectedAffiliateProgram",
        connected(BOT, "https://t.me/bot?start=_tgr_x"),
    );
    let earn = &t.driver.session.payments.earn;
    assert!(earn.link_box.is_none() && earn.join_box.is_none());
}

#[test]
fn a_refused_resolve_send_leaves_nothing_waiting() {
    let mut t = Flaky::new();
    join_page(&mut t);
    t.sender.refuse(&["getConnectedAffiliateProgram"]);
    assert!(t.driver.open_affiliate_program(BOT, false).is_err());
    assert_eq!(t.driver.session.payments.earn.resolve, Resolve::Idle);
}

/// 03-027: the answer of a join the user left does not close the box
/// opened since, nor put a link box over it.
#[test]
fn a_late_connect_answer_leaves_a_newer_join_box_alone() {
    let mut t = Flaky::new();
    join_page(&mut t);
    t.driver.open_affiliate_program(BOT, false).unwrap();
    t.answer("getConnectedAffiliateProgram", not_joined());
    t.driver.join_affiliate_program().unwrap();
    assert!(
        t.driver
            .session
            .payments
            .earn
            .join_box
            .as_ref()
            .unwrap()
            .stage
            .is_joining()
    );
    // The user closes the box and opens another program's.
    t.driver.session.payments.earn.join_box = None;
    t.driver.open_affiliate_program(BOT + 1, false).unwrap();
    t.answer("getConnectedAffiliateProgram", not_joined());
    t.answer(
        "connectAffiliateProgram",
        connected(BOT, "https://t.me/bot?start=_tgr_a"),
    );
    let earn = &t.driver.session.payments.earn;
    let open = earn.join_box.as_ref().unwrap();
    assert_eq!(open.bot_user_id, BOT + 1);
    assert_eq!(open.stage, JoinStage::Open);
    assert!(earn.link_box.is_none());
    // The join did happen: the program is in My Programs.
    assert_eq!(earn.join.as_ref().unwrap().mine.programs.len(), 1);
}

#[test]
fn a_connect_error_returns_only_its_own_box_to_open() {
    let mut t = Flaky::new();
    join_page(&mut t);
    t.driver.open_affiliate_program(BOT, false).unwrap();
    t.answer("getConnectedAffiliateProgram", not_joined());
    t.driver.join_affiliate_program().unwrap();
    t.driver.session.payments.earn.join_box = None;
    t.driver.open_affiliate_program(BOT + 1, false).unwrap();
    t.answer("getConnectedAffiliateProgram", not_joined());
    t.driver.join_affiliate_program().unwrap();
    t.answer_nth("connectAffiliateProgram", 0, not_joined());
    let join = t.driver.session.payments.earn.join_box.as_ref().unwrap();
    assert!(join.stage.is_joining(), "the second join is still out");
    t.answer_nth("connectAffiliateProgram", 1, not_joined());
    let join = t.driver.session.payments.earn.join_box.as_ref().unwrap();
    assert_eq!(join.stage, JoinStage::Open);
}
