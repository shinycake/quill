//! Revenue and affiliate fixtures for the screenshot demos and the UI
//! tests: TDLib-shaped answers parsed by the real parsers, put where the
//! reducer would put them (no live Telegram).

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::earn::*;
use quill::premium_hub::{TxFilter, parse_star_transactions};
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ChannelMemberStatus;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// The demo channel (the chat-list fixture's broadcast channel).
pub(in crate::ui) const DEMO_CHANNEL: i64 = 13;
/// The demo user's own bot, with Stars revenue.
pub(in crate::ui) const OWN_BOT: i64 = 9101;
/// A bot whose program the user joined.
pub(in crate::ui) const JOINED_BOT: i64 = 9102;
/// Bots with programs to join.
pub(in crate::ui) const FOUND_BOTS: [i64; 2] = [9103, 9104];

const DAY_MS: i64 = 86_400_000;
const START_MS: i64 = 1_788_000_000_000;

fn apply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, jsons: &[String]) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    for json in jsons {
        if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn bot_json(id: i64, name: &str, own: bool) -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{name}","type":{{"@type":"userTypeBot","can_be_edited":{own},"can_join_groups":true,"can_read_all_group_messages":false,"has_main_web_app":true,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":1200}}}}}}"#
    )
}

/// A `statisticalGraphData` with one daily series.
fn graph(
    series: &str,
    color: &str,
    kind: &str,
    currency: Option<&str>,
    values: impl Fn(i64) -> i64,
) -> Value {
    let days = 30;
    let mut x = vec![json!("x")];
    let mut y = vec![json!("y0")];
    for day in 0..days {
        x.push(json!(START_MS + day * DAY_MS));
        y.push(json!(values(day)));
    }
    let mut data = json!({
        "columns": [x, y],
        "types": {"x": "x", "y0": kind},
        "names": {"y0": series},
        "colors": {"y0": color},
    });
    if let (Some(currency), Some(root)) = (currency, data.as_object_mut()) {
        root.insert(
            "yTickFormatter".into(),
            json!(format!("formatValue{currency}")),
        );
    }
    json!({"@type": "statisticalGraphData", "json_data": data.to_string(), "zoom_token": ""})
}

fn wave(day: i64, base: i64, swing: i64) -> i64 {
    let phase = (day as f64 / 4.0).sin();
    base + (phase * swing as f64) as i64
}

/// The bots the affiliate pages and boxes show.
pub(in crate::ui) fn seed_earn_bots(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let jsons = [
        bot_json(OWN_BOT, "Quiz Arena", true),
        bot_json(JOINED_BOT, "Pixel Garden", false),
        bot_json(FOUND_BOTS[0], "Star Trivia", false),
        bot_json(FOUND_BOTS[1], "Cloud Notes", false),
    ];
    apply(session, sink, seq, &jsons);
    if session.my_user_id.is_none() {
        session.my_user_id = Some(777);
    }
    let limits = &mut session.payments.earn.limits;
    limits.star_withdrawal_min = Some(1000);
    limits.star_withdrawal_max = Some(25_000);
    limits.commission_min = Some(1);
    limits.commission_max = Some(800);
    limits.can_withdraw_chat_revenue = true;
    limits.starref_join_allowed = true;
    limits.starref_setup_allowed = true;
    limits.app_config_loaded = true;
}

fn star_stats() -> StarRevenueStats {
    StarRevenueStats::parse(&json!({
        "@type": "starRevenueStatistics",
        "revenue_by_day_graph": graph("Revenue", "#F5BD25", "bar", Some("XTR"), |d| wave(d, 1_400, 600)),
        "status": {"@type": "starRevenueStatus",
            "total_amount": {"star_count": 48_250, "nanostar_count": 0},
            "current_amount": {"star_count": 12_840, "nanostar_count": 0},
            "available_amount": {"star_count": 9_600, "nanostar_count": 0},
            "withdrawal_enabled": true, "next_withdrawal_in": 0},
        "usd_rate": 1.3,
    }))
}

fn star_history(peer_chat: Option<i64>) -> quill::premium_hub::StarTxPage {
    let source = match peer_chat {
        Some(chat_id) => {
            json!({"@type": "starTransactionTypeChannelPaidReactionReceive", "user_id": 777, "chat_id": chat_id})
        }
        None => {
            json!({"@type": "starTransactionTypeBotInvoiceSale", "user_id": 777, "product_info": {"title": "Season pass"}})
        }
    };
    parse_star_transactions(&json!({
        "@type": "starTransactions",
        "star_amount": {"star_count": 12_840},
        "next_offset": "",
        "transactions": [
            {"id": "t1", "star_amount": {"star_count": 250}, "date": 1_790_600_000, "type": source},
            {"id": "t2", "star_amount": {"star_count": 1_200}, "date": 1_790_500_000,
             "type": {"@type": "starTransactionTypeAffiliateProgramCommission", "chat_id": JOINED_BOT, "commission_per_mille": 200}},
            {"id": "t3", "star_amount": {"star_count": -5_000}, "date": 1_790_400_000,
             "type": {"@type": "starTransactionTypeFragmentWithdrawal",
                      "withdrawal_state": {"@type": "revenueWithdrawalStateSucceeded", "date": 1_790_400_100, "url": "https://fragment.com"}}},
        ],
    }))
}

/// The demo channel's Monetization page, loaded, with the user as its
/// creator.
pub(in crate::ui) fn seed_channel_monetization(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    seed_earn_bots(session, sink, seq);
    let full_info = format!(
        r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{DEMO_CHANNEL},"supergroup_full_info":{{"description":"Daily news","member_count":12345,"can_get_statistics":true,"can_get_revenue_statistics":true,"can_get_star_revenue_statistics":true,"can_have_sponsored_messages":true}}}}"#
    );
    apply(session, sink, seq, &[full_info]);
    if let Some(chat) = session.chats.get_mut(&DEMO_CHANNEL) {
        chat.my_member_status = Some(ChannelMemberStatus::Creator);
    }
    let peer = EarnPeer::Chat(DEMO_CHANNEL);
    let mut page = EarnPage::new(peer, EarnScreen::Monetization);
    page.ton = Load::Ready(TonRevenueStats::parse(&json!({
        "@type": "chatRevenueStatistics",
        "revenue_by_hour_graph": graph("Ad impressions", "#3497ED", "step", None, |d| wave(d, 42_000, 15_000)),
        "revenue_graph": graph("Ad rewards", "#3CC23F", "bar", Some("TON"), |d| wave(d, 3_000_000_000, 1_200_000_000)),
        "revenue_amount": {"@type": "chatRevenueAmount", "cryptocurrency": "TON",
            "total_amount": "182450000000", "balance_amount": "24870000000",
            "available_amount": "21340000000", "withdrawal_enabled": true},
        "usd_rate": 3.1e-7,
    })));
    page.stars = Load::Ready(star_stats());
    let ton = TonTxPage::parse(
        &json!({"@type": "chatRevenueTransactions", "next_offset": "", "transactions": [
            {"cryptocurrency": "TON", "cryptocurrency_amount": "4120000000",
             "type": {"@type": "chatRevenueTransactionTypeSponsoredMessageEarnings",
                      "start_date": 1_790_300_000, "end_date": 1_790_900_000}},
            {"cryptocurrency": "TON", "cryptocurrency_amount": "-15000000000",
             "type": {"@type": "chatRevenueTransactionTypeFragmentWithdrawal", "withdrawal_date": 1_790_200_000,
                      "state": {"@type": "revenueWithdrawalStateSucceeded", "date": 1_790_200_100,
                                "url": "https://tonviewer.com"}}},
            {"cryptocurrency": "TON", "cryptocurrency_amount": "3870000000",
             "type": {"@type": "chatRevenueTransactionTypeSponsoredMessageEarnings",
                      "start_date": 1_789_700_000, "end_date": 1_790_300_000}},
        ]}),
    );
    page.ton_history.rows = ton.transactions;
    page.ton_history.loaded = true;
    page.history_mut(TxFilter::All)
        .apply(star_history(Some(DEMO_CHANNEL)), false);
    let earn = &mut session.payments.earn;
    earn.ton_balances.insert(
        peer,
        TonAmounts {
            total: 182_450_000_000,
            balance: 24_870_000_000,
            available: 21_340_000_000,
            withdrawal_enabled: true,
        },
    );
    earn.page = Some(page);
}

/// The user's own bot's Stars Balance page.
pub(in crate::ui) fn seed_bot_stars(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    seed_earn_bots(session, sink, seq);
    let mut page = EarnPage::new(EarnPeer::User(OWN_BOT), EarnScreen::Stars);
    page.stars = Load::Ready(star_stats());
    for filter in [TxFilter::All, TxFilter::Incoming, TxFilter::Outgoing] {
        let mut history = star_history(None);
        history.transactions.retain(|tx| match filter {
            TxFilter::Incoming => tx.amount.stars > 0,
            TxFilter::Outgoing => tx.amount.stars < 0,
            _ => true,
        });
        page.history_mut(filter).apply(history, false);
    }
    page.ad_account_url = Some("https://ads.telegram.org".into());
    session.payments.earn.page = Some(page);
}

fn found(bot_user_id: i64, commission: i32, months: i32, daily: i64) -> FoundProgram {
    FoundProgram {
        bot_user_id,
        info: ProgramInfo {
            params: ProgramParams {
                commission_per_mille: commission,
                month_count: months,
            },
            end_date: 0,
            daily_revenue_per_user: quill::premium_hub::StarAmount {
                stars: daily,
                nanos: 0,
            },
        },
    }
}

/// A joined program of `peer`.
pub(in crate::ui) fn joined_program() -> ConnectedProgram {
    ConnectedProgram::parse(&json!({
        "@type": "connectedAffiliateProgram",
        "url": "https://t.me/pixelgardenbot?start=_tgr_demo42",
        "bot_user_id": JOINED_BOT,
        "parameters": {"commission_per_mille": 150, "month_count": 12},
        "connection_date": 1_790_000_000, "is_disconnected": false,
        "user_count": "37", "revenue_star_count": 2_480,
    }))
}

/// The affiliate programs page of `peer`: one joined program and two to
/// join.
pub(in crate::ui) fn seed_affiliate_join(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    peer: EarnPeer,
) {
    seed_earn_bots(session, sink, seq);
    let mut join = JoinPage::new(peer);
    join.mine.programs = vec![joined_program()];
    join.mine.total_count = 1;
    join.mine.loaded = true;
    join.found.programs = vec![
        found(FOUND_BOTS[0], 300, 0, 4),
        found(FOUND_BOTS[1], 120, 6, 1),
    ];
    join.found.total_count = 2;
    join.found.loaded = true;
    let earn = &mut session.payments.earn;
    earn.join = Some(join);
    earn.recipients = Some(vec![EarnPeer::User(777), EarnPeer::Chat(DEMO_CHANNEL)]);
}

/// The own bot's setup page with no program yet.
pub(in crate::ui) fn seed_affiliate_setup(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    seed_earn_bots(session, sink, seq);
    session.payments.earn.setup = Some(SetupPage::new(OWN_BOT, None));
}

/// The join box for the first program to join.
pub(in crate::ui) fn open_join_box(session: &mut Session) {
    let program = found(FOUND_BOTS[0], 300, 0, 4);
    session.payments.earn.join_box = Some(JoinBox {
        bot_user_id: program.bot_user_id,
        info: program.info,
        recipient: EarnPeer::User(777),
        choose_recipient: true,
        joining: false,
    });
}

/// The link box of the joined program.
pub(in crate::ui) fn open_link_box(session: &mut Session, recipient: EarnPeer) {
    session.payments.earn.link_box = Some(LinkBox {
        program: joined_program(),
        recipient,
    });
}
