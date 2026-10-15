use super::{
    GramRevenue, GramsHub, GramsPayload, TonTxKind, TonTxPage, WithdrawalState,
    get_gram_revenue_statistics, get_ton_transactions, get_user, gram_amount_2dp, gram_balance,
    gram_balance_short, parse_kind, parse_payload, parse_ton_transactions, signed_gram_amount,
    usd_line,
};
use crate::ids::RequestId;
use crate::premium_hub::TxFilter;
use crate::telegram::envelope::MessageSender;
use serde_json::{Value, json};

const FIXTURE: &str = include_str!("../../tests/fixtures/grams/ton_transactions.json");

fn page() -> TonTxPage {
    parse_ton_transactions(&serde_json::from_str::<Value>(FIXTURE).unwrap())
}

fn by_id<'a>(page: &'a TonTxPage, id: &str) -> &'a super::TonTx {
    page.transactions.iter().find(|tx| tx.id == id).unwrap()
}

#[test]
fn every_transaction_type_parses() {
    let page = page();
    assert_eq!(page.balance_nano, 12_750_000_000);
    assert_eq!(page.next_offset, "page-2");
    assert_eq!(page.transactions.len(), 11);
    assert_eq!(
        by_id(&page, "tx-topup").kind,
        TonTxKind::FragmentDeposit {
            is_gift: false,
            sticker: None
        }
    );
    assert_eq!(
        by_id(&page, "tx-withdrawal").kind,
        TonTxKind::FragmentWithdrawal {
            state: WithdrawalState::Pending
        }
    );
    assert_eq!(
        by_id(&page, "tx-refund").kind,
        TonTxKind::FragmentWithdrawal {
            state: WithdrawalState::Unknown
        }
    );
    assert_eq!(
        by_id(&page, "tx-suggested").kind,
        TonTxKind::SuggestedPostPayment { chat_id: 13 }
    );
    assert!(matches!(
        by_id(&page, "tx-offer").kind,
        TonTxKind::GiftPurchaseOffer { .. }
    ));
    assert!(matches!(
        by_id(&page, "tx-purchase").kind,
        TonTxKind::UpgradedGiftPurchase { user_id: 11, .. }
    ));
    assert!(matches!(
        by_id(&page, "tx-sale").kind,
        TonTxKind::UpgradedGiftSale {
            user_id: 12,
            via_offer: true,
            ..
        }
    ));
    assert_eq!(by_id(&page, "tx-stake").kind, TonTxKind::StakeDiceStake);
    assert_eq!(by_id(&page, "tx-payout").kind, TonTxKind::StakeDicePayout);
    assert_eq!(by_id(&page, "tx-unsupported").kind, TonTxKind::Unsupported);
    // A type a newer TDLib adds is not lost, just generic.
    assert_eq!(
        parse_kind(&json!({"@type": "tonTransactionTypeSomethingNew"})),
        TonTxKind::Unsupported
    );
    let succeeded = parse_kind(&json!({
        "@type": "tonTransactionTypeFragmentWithdrawal",
        "withdrawal_state": {"@type": "revenueWithdrawalStateSucceeded", "date": 5, "url": "https://x"}
    }));
    assert_eq!(
        succeeded,
        TonTxKind::FragmentWithdrawal {
            state: WithdrawalState::Succeeded {
                date: 5,
                url: "https://x".into()
            }
        }
    );
}

#[test]
fn rows_without_a_type_are_skipped_and_rows_without_an_id_kept() {
    let page = parse_ton_transactions(&json!({
        "gram_amount": 1,
        "transactions": [
            {"id": "", "gram_amount": 1, "date": 1, "type": {"@type": "tonTransactionTypeStakeDiceStake"}},
            {"id": "a", "gram_amount": 1, "date": 1},
            {"id": "b", "gram_amount": 1, "date": 1, "type": {"@type": "tonTransactionTypeStakeDiceStake"}}
        ]
    }));
    assert_eq!(page.transactions.len(), 2);
    assert!(page.next_offset.is_empty());
}

#[test]
fn keys_stay_unique_when_ids_are_empty_or_repeat() {
    let stake = json!({"@type": "tonTransactionTypeStakeDiceStake"});
    let mut hub = GramsHub::default();
    hub.apply_page(
        parse_ton_transactions(&json!({
            "gram_amount": 1,
            "next_offset": "n",
            "transactions": [
                {"id": "", "gram_amount": 1, "date": 10, "type": stake},
                {"id": "a", "gram_amount": 2, "date": 11, "type": stake},
                {"id": "a", "gram_amount": 3, "date": 12, "type": stake}
            ]
        })),
        false,
    );
    hub.apply_page(
        parse_ton_transactions(&json!({
            "gram_amount": 1,
            "transactions": [{"id": "", "gram_amount": 4, "date": 10, "type": stake}]
        })),
        true,
    );
    let keys: Vec<&str> = hub.entries.iter().map(|tx| tx.key.as_str()).collect();
    assert_eq!(keys, ["0-10", "a", "2-12", "3-10"]);
    hub.selected = Some("2-12".into());
    assert_eq!(hub.selected_tx().map(|tx| tx.gram_nano), Some(3));
    assert!(hub.peers_dirty);
}

#[test]
fn row_titles_details_and_peers_per_type() {
    let page = page();
    let row = |id: &str| {
        let tx = by_id(&page, id);
        (tx.title(), tx.detail(), tx.peer())
    };
    assert_eq!(
        row("tx-topup"),
        (Some("Gram Top-Up".into()), "Fragment".into(), None)
    );
    assert_eq!(
        row("tx-gift-deposit"),
        (Some("Received Gift".into()), "Fragment".into(), None)
    );
    assert_eq!(
        row("tx-withdrawal"),
        (Some("Fragment".into()), "Pending".into(), None)
    );
    assert_eq!(
        row("tx-refund"),
        (Some("Fragment".into()), "Refund".into(), None)
    );
    assert_eq!(
        row("tx-suggested"),
        (
            None,
            String::new(),
            Some(MessageSender::Chat { chat_id: 13 })
        )
    );
    assert_eq!(
        row("tx-offer"),
        (Some("Plush Pepe #1,204".into()), "Gift Offer".into(), None)
    );
    assert_eq!(
        row("tx-purchase"),
        (
            Some("Homemade Cake #77".into()),
            "Gift Purchase".into(),
            Some(MessageSender::User { user_id: 11 })
        )
    );
    assert_eq!(
        row("tx-sale"),
        (
            Some("Signet Ring #19".into()),
            "Gift Sale".into(),
            Some(MessageSender::User { user_id: 12 })
        )
    );
    assert_eq!(row("tx-stake"), (Some("Stake".into()), String::new(), None));
    assert_eq!(
        row("tx-payout"),
        (Some("Payout".into()), String::new(), None)
    );
    assert_eq!(
        row("tx-unsupported"),
        (Some("Transaction".into()), String::new(), None)
    );
    let failed = super::TonTx {
        id: "f".into(),
        key: "f".into(),
        gram_nano: -1,
        is_refund: false,
        date: 0,
        kind: TonTxKind::FragmentWithdrawal {
            state: WithdrawalState::Failed,
        },
    };
    assert_eq!(failed.detail(), "Failed");
    assert!(!failed.is_incoming());
}

#[test]
fn amounts_format_like_tdesktop() {
    assert_eq!(gram_amount_2dp(1_259_000_000), "1.25");
    assert_eq!(gram_amount_2dp(1_234_500_000_000), "1,234.50");
    assert_eq!(gram_amount_2dp(5_000_000), "0.00");
    assert_eq!(signed_gram_amount(-2_500_000_000), "\u{2212}2.50");
    assert_eq!(signed_gram_amount(25_000_000_000), "+25.00");
    assert_eq!(gram_balance(12_750_000_000), "12.75");
    assert_eq!(gram_balance(1_000_000_000), "1");
    assert_eq!(gram_balance_short(12_750_000_000), "12.75");
    assert_eq!(gram_balance_short(12_345_000_000_000), "12.3K");
    assert_eq!(gram_balance_short(20_000_000_000_000), "20K");
    assert_eq!(gram_balance_short(1_500_000_000_000_000), "1.5M");
    assert_eq!(
        usd_line(2_000_000_000, Some(3.5), 3).as_deref(),
        Some("\u{2248}$7.000")
    );
    assert_eq!(usd_line(0, Some(3.5), 3), None);
    assert_eq!(usd_line(1, None, 3), None);
    assert_eq!(usd_line(1, Some(f64::NAN), 3), None);
}

#[test]
fn paging_appends_and_a_fresh_page_replaces() {
    let mut hub = GramsHub::default();
    assert!(!hub.shows_settings_row());
    hub.apply_page(page(), false);
    assert!(hub.shows_settings_row());
    assert_eq!(hub.entries.len(), 11);
    assert!(hub.has_more());
    assert!(hub.has_in && hub.has_out && hub.shows_tabs());
    hub.apply_page(
        parse_ton_transactions(&json!({
            "gram_amount": 1,
            "next_offset": "",
            "transactions": [{"id": "z", "gram_amount": 1, "date": 1,
                              "type": {"@type": "tonTransactionTypeStakeDiceStake"}}]
        })),
        true,
    );
    assert_eq!(hub.entries.len(), 12);
    assert!(!hub.has_more());
    assert_eq!(hub.balance_nano, Some(1));
    // A new tab starts over and keeps the tabs.
    hub.restart(TxFilter::Outgoing);
    assert!(hub.entries.is_empty() && !hub.loaded);
    assert!(hub.shows_tabs());
    // Only incoming rows: one "All Transactions" list, no tabs.
    let mut one_way = GramsHub::default();
    one_way.apply_page(
        parse_ton_transactions(&json!({
            "gram_amount": 0,
            "transactions": [{"id": "a", "gram_amount": 5, "date": 1,
                              "type": {"@type": "tonTransactionTypeStakeDiceStake"}}]
        })),
        false,
    );
    assert!(!one_way.shows_tabs());
    assert!(!one_way.shows_settings_row());
    hub.selected = Some("z".into());
    assert!(hub.selected_tx().is_none());
}

#[test]
fn requests_have_the_schema_shape() {
    let all: Value =
        serde_json::from_str(&get_ton_transactions(RequestId(7), TxFilter::All, "", 50)).unwrap();
    assert_eq!(all["@type"], "getTonTransactions");
    assert_eq!(all["limit"], 50);
    assert_eq!(all["offset"], "");
    assert!(all.get("direction").is_none());
    let incoming: Value = serde_json::from_str(&get_ton_transactions(
        RequestId(8),
        TxFilter::Incoming,
        "o",
        50,
    ))
    .unwrap();
    assert_eq!(
        incoming["direction"]["@type"],
        "transactionDirectionIncoming"
    );
    assert_eq!(incoming["offset"], "o");
    let stats: Value =
        serde_json::from_str(&get_gram_revenue_statistics(RequestId(9), true)).unwrap();
    assert_eq!(stats["@type"], "getGramRevenueStatistics");
    assert_eq!(stats["is_dark"], true);
    let user: Value = serde_json::from_str(&get_user(RequestId(10), 42)).unwrap();
    assert_eq!(
        (user["@type"].as_str(), user["user_id"].as_i64()),
        (Some("getUser"), Some(42))
    );
    let schema = include_str!("../../schema/td_api.tl");
    assert!(schema.contains("getTonTransactions direction:TransactionDirection offset:string limit:int32 = TonTransactions;"));
    assert!(schema.contains("getGramRevenueStatistics is_dark:Bool = GramRevenueStatistics;"));
}

#[test]
fn revenue_and_its_updates_parse() {
    let Some(GramsPayload::Revenue(revenue)) = parse_payload(
        "gramRevenueStatistics",
        &json!({
            "status": {"total_amount": "30000000000", "balance_amount": "20000000000",
                       "available_amount": "15000000000", "withdrawal_enabled": true},
            "usd_rate": 0.00000035
        }),
    ) else {
        panic!("revenue");
    };
    assert_eq!(revenue.available_nano, 15_000_000_000);
    assert!(revenue.withdrawal_enabled);
    let per_gram = revenue.usd_per_gram().unwrap();
    assert!((per_gram - 3.5).abs() < 1e-9);
    assert_eq!(
        parse_payload(
            "updateGramRevenueStatus",
            &json!({"status": {"available_amount": 0, "balance_amount": 0, "withdrawal_enabled": false}})
        ),
        Some(GramsPayload::RevenueStatus(GramRevenue::default()))
    );
    let mut hub = GramsHub::default();
    assert!(hub.withdrawable().is_none());
    hub.revenue = Some(revenue);
    assert!(hub.withdrawable().is_some());
}

#[test]
fn collect_routes_on_the_password_state() {
    use super::GramWithdraw;
    // Unknown or set: the password box; known unset: the security check.
    assert_eq!(GramWithdraw::open(None), GramWithdraw::idle(None));
    assert_eq!(GramWithdraw::open(Some(true)), GramWithdraw::idle(None));
    assert_eq!(
        GramWithdraw::open(Some(false)),
        GramWithdraw::SecurityCheck { later: false }
    );
    // A late "no password" turns only the idle box into the check.
    let mut idle = GramWithdraw::idle(None);
    idle.password_state_known(false);
    assert_eq!(idle, GramWithdraw::SecurityCheck { later: false });
    let mut sending = GramWithdraw::Password {
        request: Some(RequestId(9)),
        error: None,
    };
    sending.password_state_known(false);
    assert!(sending.sending() && !sending.can_submit());
    assert!(sending.waits_for(Some(RequestId(9))));
    assert!(!sending.waits_for(Some(RequestId(8))) && !sending.waits_for(None));
    let mut set = GramWithdraw::idle(None);
    set.password_state_known(true);
    assert!(set.can_submit());
}

#[test]
fn the_withdrawal_request_has_the_schema_shape() {
    let request: Value =
        serde_json::from_str(&super::get_gram_withdrawal_url(RequestId(5), "pw")).unwrap();
    assert_eq!(request["@type"], "getGramWithdrawalUrl");
    assert_eq!(request["password"], "pw");
    assert_eq!(request.as_object().map(|o| o.len()), Some(3));
    let schema = include_str!("../../schema/td_api.tl");
    assert!(schema.contains("getGramWithdrawalUrl password:string = HttpUrl;"));
}
