use super::*;
use crate::premium_hub::{StarAmount, TxFilter};
use serde_json::json;

#[test]
fn ton_amounts_print_like_tdesktop() {
    // MajorPart drops the fraction, MinorPart cuts it to two digits.
    assert_eq!(ton_major(12_345_678_901), "12");
    assert_eq!(ton_minor(12_345_678_901), ".34");
    assert_eq!(ton_label(5_000_000), "0.00");
    assert_eq!(ton_label(-1_500_000_000), "1.50");
    assert_eq!(ton_signed_label(1_500_000_000, true), "+1.50");
    assert_eq!(ton_signed_label(1_500_000_000, false), "\u{2212}1.50");
}

#[test]
fn dollar_amounts_use_the_tdlib_rates() {
    // TON: cents per nanoton; 2 TON at $3 = 0.0000003 cents per nano.
    assert_eq!(ton_to_usd(2_000_000_000, 3e-7, 0), "\u{2248}$6.00");
    assert_eq!(ton_to_usd(2_000_000_000, 3e-7, 3), "\u{2248}$6.000");
    // Stars: cents per Star; 1,000,000 Stars at 1.3 cents.
    let stars = StarAmount {
        stars: 1_000_000,
        nanos: 0,
    };
    assert_eq!(stars_to_usd(stars, 1.3, 0), "\u{2248}$13,000.00");
}

#[test]
fn commission_and_duration_labels() {
    assert_eq!(format_commission(200), "20%");
    assert_eq!(format_commission(25), "2.5%");
    assert_eq!(format_program_duration(0), "Forever");
    assert_eq!(format_program_duration(1), "1 month");
    assert_eq!(format_program_duration(6), "6 months");
    assert_eq!(format_program_duration(24), "2 years");
    assert_eq!(format_for_program_duration(0), "for lifetime");
    assert_eq!(format_for_program_duration(12), "for 1 year");
    assert_eq!(duration_tick(3), "3m");
    assert_eq!(duration_tick(36), "3y");
    assert_eq!(duration_tick(DURATION_FOREVER), "\u{221E}");
    assert_eq!(format_time_left(65), "1:05");
    assert_eq!(format_time_left(3_725), "1:02:05");
}

#[test]
fn commission_stops_follow_setup_commission() {
    // TDLib defaults 1‰..800‰: 1%..80% plus nothing extra at 20%.
    let stops = commission_stops(1, 800, 200);
    assert_eq!(stops.first(), Some(&10));
    assert_eq!(stops.last(), Some(&800));
    assert_eq!(stops.len(), 80);
    // An odd current value is inserted in order.
    let stops = commission_stops(1, 800, 205);
    let at = stops.iter().position(|v| *v == 205).unwrap();
    assert_eq!(stops.get(at - 1), Some(&200));
    assert_eq!(stops.get(at + 1), Some(&210));
    // Below the first whole percent it leads.
    assert_eq!(commission_stops(5, 800, 5).first(), Some(&5));
}

#[test]
fn duration_stops_and_start_values() {
    assert_eq!(duration_stops(12), vec![1, 3, 6, 12, 24, 36, 999]);
    assert_eq!(duration_stops(0), vec![1, 3, 6, 12, 24, 36, 999]);
    assert_eq!(duration_stops(18), vec![1, 3, 6, 12, 18, 24, 36, 999]);
    let fresh = setup_start(None);
    assert_eq!(fresh.commission_per_mille, COMMISSION_DEFAULT);
    assert_eq!(fresh.month_count, DURATION_DEFAULT);
    let running = ProgramInfo {
        params: ProgramParams {
            commission_per_mille: 150,
            month_count: 0,
        },
        ..ProgramInfo::default()
    };
    assert!(running.is_running());
    let start = setup_start(Some(running));
    assert_eq!(start.commission_per_mille, 150);
    assert_eq!(
        start.month_count, 0,
        "a running forever program stays forever"
    );
}

#[test]
fn parses_chat_revenue_statistics_and_transactions() {
    let stats = json!({
        "@type": "chatRevenueStatistics",
        "revenue_by_hour_graph": {"@type": "statisticalGraphData", "json_data": "{}", "zoom_token": ""},
        "revenue_graph": {"@type": "statisticalGraphAsync", "token": "t"},
        "revenue_amount": {"@type": "chatRevenueAmount", "cryptocurrency": "TON",
            "total_amount": "9000000000", "balance_amount": "4000000000",
            "available_amount": "3000000000", "withdrawal_enabled": true},
        "usd_rate": 3e-7,
    });
    let Some(EarnPayload::TonStats(parsed)) = parse_payload("chatRevenueStatistics", &stats) else {
        panic!("not parsed");
    };
    assert_eq!(parsed.amounts.total, 9_000_000_000);
    assert_eq!(parsed.amounts.balance, 4_000_000_000);
    assert_eq!(parsed.amounts.available, 3_000_000_000);
    assert!(parsed.amounts.withdrawal_enabled);
    assert!(parsed.by_hour.is_some() && parsed.revenue.is_some());

    let page = json!({"@type": "chatRevenueTransactions", "next_offset": "n", "transactions": [
        {"cryptocurrency": "TON", "cryptocurrency_amount": "2000000000",
         "type": {"@type": "chatRevenueTransactionTypeSponsoredMessageEarnings",
                  "start_date": 100, "end_date": 200}},
        {"cryptocurrency": "TON", "cryptocurrency_amount": "-1000000000",
         "type": {"@type": "chatRevenueTransactionTypeFragmentWithdrawal", "withdrawal_date": 300,
                  "state": {"@type": "revenueWithdrawalStateSucceeded", "date": 301,
                            "url": "https://tonviewer.com/x"}}},
        {"cryptocurrency": "TON", "cryptocurrency_amount": "-5",
         "type": {"@type": "chatRevenueTransactionTypeFragmentWithdrawal", "withdrawal_date": 400,
                  "state": {"@type": "revenueWithdrawalStatePending"}}},
        {"cryptocurrency": "TON", "cryptocurrency_amount": "5",
         "type": {"@type": "chatRevenueTransactionTypeFragmentRefund", "refund_date": 500}},
    ]});
    let Some(EarnPayload::TonTransactions(page)) = parse_payload("chatRevenueTransactions", &page)
    else {
        panic!("not parsed");
    };
    assert_eq!(page.next_offset, "n");
    let rows = &page.transactions;
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].title(), "Rewards from ads");
    assert!(rows[0].is_incoming());
    assert_eq!(rows[1].title(), "Collection");
    assert_eq!(rows[1].explorer_url(), Some("https://tonviewer.com/x"));
    assert!(!rows[1].is_incoming());
    assert!(rows[2].pending());
    assert_eq!(rows[2].date_line(), "Pending");
    assert_eq!(rows[3].title(), "Refund");
    assert!(rows[3].is_incoming());
}

#[test]
fn parses_star_revenue_and_updates() {
    let stats = json!({"@type": "starRevenueStatistics",
        "revenue_by_day_graph": {"@type": "statisticalGraphError", "error_message": "x"},
        "status": {"@type": "starRevenueStatus",
            "total_amount": {"star_count": 900, "nanostar_count": 0},
            "current_amount": {"star_count": 400, "nanostar_count": 0},
            "available_amount": {"star_count": 300, "nanostar_count": 5},
            "withdrawal_enabled": true, "next_withdrawal_in": 30},
        "usd_rate": 1.3});
    let Some(EarnPayload::StarStats(parsed)) = parse_payload("starRevenueStatistics", &stats)
    else {
        panic!("not parsed");
    };
    assert_eq!(parsed.status.available.stars, 300);
    assert_eq!(parsed.status.next_withdrawal_in, 30);
    assert!(parsed.status.has_any());
    assert_eq!(parsed.usd_rate, 1.3);

    let update = json!({"@type": "updateStarRevenueStatus",
        "owner_id": {"@type": "messageSenderChat", "chat_id": -100},
        "status": {"current_amount": {"star_count": 7}}});
    assert_eq!(
        parse_payload("updateStarRevenueStatus", &update),
        Some(EarnPayload::UpdateStarStatus {
            owner: EarnPeer::Chat(-100),
            status: StarRevenueStatus {
                current: StarAmount { stars: 7, nanos: 0 },
                ..StarRevenueStatus::default()
            },
        })
    );
    let update = json!({"@type": "updateChatRevenueAmount", "chat_id": -100,
        "revenue_amount": {"balance_amount": "11"}});
    let Some(EarnPayload::UpdateTonAmounts { chat_id, amounts }) =
        parse_payload("updateChatRevenueAmount", &update)
    else {
        panic!("not parsed");
    };
    assert_eq!((chat_id, amounts.balance), (-100, 11));
}

#[test]
fn parses_affiliate_programs() {
    let found = json!({"@type": "foundAffiliatePrograms", "total_count": 2, "next_offset": "o",
        "programs": [{"bot_user_id": 5, "info": {"parameters":
            {"commission_per_mille": 100, "month_count": 6}, "end_date": 0,
            "daily_revenue_per_user_amount": {"star_count": 3, "nanostar_count": 0}}}]});
    let Some(EarnPayload::FoundPrograms(found)) = parse_payload("foundAffiliatePrograms", &found)
    else {
        panic!("not parsed");
    };
    assert_eq!(found.total_count, 2);
    assert_eq!(found.programs[0].bot_user_id, 5);
    assert_eq!(found.programs[0].info.params.month_count, 6);
    assert_eq!(found.programs[0].info.daily_revenue_per_user.stars, 3);

    let connected = json!({"@type": "connectedAffiliateProgram", "url": "https://t.me/b?start=_tgr_x",
        "bot_user_id": 5, "parameters": {"commission_per_mille": 100, "month_count": 0},
        "connection_date": 9, "is_disconnected": false, "user_count": "4",
        "revenue_star_count": 12});
    let Some(EarnPayload::Connected(program)) =
        parse_payload("connectedAffiliateProgram", &connected)
    else {
        panic!("not parsed");
    };
    assert_eq!(program.user_count, 4);
    assert_eq!(program.url, "https://t.me/b?start=_tgr_x");
}

#[test]
fn reads_the_affiliate_switches_from_the_app_config() {
    let config = json!({"@type": "jsonValueObject", "members": [
        {"key": "starref_connect_allowed", "value": {"@type": "jsonValueBoolean", "value": true}},
        {"key": "starref_program_allowed", "value": {"@type": "jsonValueBoolean", "value": false}}]});
    assert_eq!(starref_flags(&config), (true, false));
    assert_eq!(starref_flags(&json!({"members": []})), (false, false));
}

#[test]
fn affiliate_types_and_senders() {
    assert_eq!(
        EarnPeer::User(7).affiliate_json(Some(7)),
        json!({"@type": "affiliateTypeCurrentUser"})
    );
    assert_eq!(
        EarnPeer::User(9).affiliate_json(Some(7)),
        json!({"@type": "affiliateTypeBot", "user_id": 9})
    );
    assert_eq!(
        EarnPeer::Chat(-100).affiliate_json(Some(7)),
        json!({"@type": "affiliateTypeChannel", "chat_id": -100})
    );
    assert_eq!(EarnPeer::Chat(-100).chat_id(), -100);
}

#[test]
fn classifies_withdrawal_errors() {
    assert_eq!(
        error_hint("AMOUNT_TOO_SMALL"),
        Some(EarnErrorHint::AmountTooSmall)
    );
    assert_eq!(
        error_hint("PASSWORD_MISSING"),
        Some(EarnErrorHint::PasswordMissing)
    );
    assert_eq!(
        error_hint("PASSWORD_TOO_FRESH_3600"),
        Some(EarnErrorHint::TooFresh)
    );
    assert_eq!(
        error_hint("SESSION_TOO_FRESH_100"),
        Some(EarnErrorHint::TooFresh)
    );
    assert_eq!(
        error_hint("PASSWORD_HASH_INVALID"),
        Some(EarnErrorHint::PasswordInvalid)
    );
    assert_eq!(error_hint("SOMETHING_ELSE"), None);
}

#[test]
fn limits_come_from_options() {
    let mut limits = EarnLimits::default();
    assert_eq!(limits.withdrawal_min(), 1000);
    assert!(limits.apply_option("star_withdrawal_count_min", Some(10), None));
    assert!(limits.apply_option(
        "affiliate_program_commission_per_mille_max",
        Some(500),
        None
    ));
    assert!(limits.apply_option("can_withdraw_chat_revenue", None, Some(true)));
    assert!(!limits.apply_option("other", Some(1), None));
    assert_eq!(limits.withdrawal_min(), 10);
    assert_eq!(limits.commission_range(), (1, 500));
    assert!(limits.can_withdraw_chat_revenue);
}

#[test]
fn requests_carry_the_right_fields() {
    let parse = |s: String| serde_json::from_str::<serde_json::Value>(&s).unwrap();
    let id = crate::ids::RequestId(3);
    let v = parse(get_star_transactions(
        id,
        EarnPeer::User(5),
        TxFilter::Incoming,
        "",
        STAR_TX_LIMIT,
    ));
    assert_eq!(v["@type"], "getStarTransactions");
    assert_eq!(v["owner_id"]["user_id"], 5);
    assert_eq!(v["direction"]["@type"], "transactionDirectionIncoming");
    assert_eq!(v["limit"], 100);
    let v = parse(get_star_withdrawal_url(id, EarnPeer::Chat(-1), 1500, ""));
    assert_eq!(v["star_count"], 1500);
    assert_eq!(v["password"], "");
    let v = parse(set_chat_affiliate_program(id, 9, None));
    assert!(v["parameters"].is_null());
    let v = parse(set_chat_affiliate_program(
        id,
        9,
        Some(ProgramParams {
            commission_per_mille: 200,
            month_count: 12,
        }),
    ));
    assert_eq!(v["parameters"]["commission_per_mille"], 200);
    let v = parse(search_affiliate_programs(
        id,
        EarnPeer::User(5).affiliate_json(Some(5)),
        ProgramSort::Date,
        "",
        PROGRAMS_PER_PAGE,
    ));
    assert_eq!(
        v["sort_order"]["@type"],
        "affiliateProgramSortOrderCreationDate"
    );
    assert_eq!(v["limit"], 50);
}
