//! Connect-driver tests: the own wallet, profile addresses, wallet links
//! and the first-transfer hint. Every getter waits for
//! `can_use_ton_wallet`, and only read-only methods go out.
use super::ton_wallet::Rig;
use crate::state::DeepLinkState;
use crate::ton_wallet::{
    FIRST_TRANSFER_HINT, MoneyDetail, MoneyTab, ProfileAddress, READ_ONLY_METHODS, WalletSurface,
};
use serde_json::{Value, json};

const ADDRESS: &str = "EQAbcdefghijklmnopqrstuvwxyz0123456789ABCDEFwxyz";

impl Rig {
    fn wallet_on(&mut self, on: bool) {
        self.ingest(&format!(
            r#"{{"@type":"updateOption","name":"can_use_ton_wallet","value":{{"@type":"optionValueBoolean","value":{on}}}}}"#
        ));
    }

    /// The last request of type `ty` that went out.
    fn last(&self, ty: &str) -> Value {
        self.recorder
            .snapshot()
            .into_iter()
            .rev()
            .filter_map(|json| serde_json::from_str::<Value>(&json).ok())
            .find(|json| json["@type"] == ty)
            .unwrap_or(Value::Null)
    }

    /// Answer the last `ty` request with `answer`.
    fn answer(&mut self, ty: &str, mut answer: Value) {
        answer["@extra"] = self.last(ty)["@extra"].clone();
        self.ingest(&answer.to_string());
    }

    /// Methods sent that look like wallet calls.
    fn wallet_methods(&self) -> Vec<String> {
        self.recorder
            .snapshot()
            .into_iter()
            .filter_map(|json| serde_json::from_str::<Value>(&json).ok())
            .filter_map(|json| json["@type"].as_str().map(str::to_string))
            .filter(|ty| ty.contains("Ton") || ty.contains("ExchangeRates"))
            .collect()
    }
}

fn transfer(id: &str, amount: i64) -> Value {
    json!({
        "@type": "tonWalletTransaction",
        "id": id,
        "peer_address": ADDRESS,
        "peer_user_id": 1,
        "peer_domain": "",
        "date": 1_700_000_000,
        "fee_amount": 0,
        "state": {"@type": "tonWalletTransactionStateSucceeded", "tx_hash": "q83v"},
        "type": {
            "@type": "tonWalletTransactionTypeTransfer",
            "amount": amount,
            "is_gasless": false,
            "comment": "",
            "is_comment_encrypted": false,
        },
    })
}

#[test]
fn nothing_is_sent_while_the_wallet_option_is_off() {
    let mut rig = Rig::new();
    assert!(rig.driver.open_money_page().is_err());
    assert!(!rig.driver.fetch_user_ton_wallet_address(1).unwrap());
    assert!(!rig.driver.open_wallet_transaction("t").unwrap());
    assert!(rig.driver.fetch_wallet_history(false).is_err());
    assert!(
        rig.wallet_methods().is_empty(),
        "{:?}",
        rig.wallet_methods()
    );
    assert!(!rig.driver.session.ton_wallet.money_open());
}

#[test]
fn opening_money_loads_state_rates_and_history_once() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    rig.driver.open_money_page().unwrap();
    assert_eq!(rig.sent("loadTonWalletState"), 1);
    assert_eq!(rig.sent("getCurrencyExchangeRates"), 1);
    assert_eq!(rig.sent("getTonWalletTransactions"), 1);
    assert_eq!(rig.last("getTonWalletTransactions")["limit"], 20);

    rig.ingest(&json!({"@type": "updateTonWalletState", "state": {
        "@type": "tonWalletState", "address": ADDRESS, "public_key": "", "gram_amount": 1_000_000_000,
        "is_being_created": false, "is_backup_enabled": true,
    }}).to_string());
    rig.answer(
        "getCurrencyExchangeRates",
        json!({"@type": "currencyExchangeRates", "rates": [{"@type": "currencyExchangeRate", "currency": "EUR", "rate": 0.5}]}),
    );
    rig.answer(
        "getTonWalletTransactions",
        json!({"@type": "tonWalletTransactions", "gram_amount": 3_000_000_000i64,
               "transactions": [transfer("a", 3_000_000_000)], "next_offset": "p2"}),
    );
    let wallet = &rig.driver.session.ton_wallet;
    let own = wallet.own().expect("own");
    assert_eq!(own.gram_amount, 3_000_000_000, "the page's balance wins");
    assert!(own.is_backup_enabled);
    assert_eq!(wallet.rates(), [("EUR".to_string(), 0.5)]);
    assert_eq!(wallet.money_page().history.items().len(), 1);
    assert!(wallet.money_page().history.has_more());

    rig.driver.fetch_wallet_history(true).unwrap();
    assert_eq!(rig.last("getTonWalletTransactions")["offset"], "p2");

    // Reopening asks for the state and rates again only once per session.
    rig.driver.close_money_page();
    rig.driver.open_money_page().unwrap();
    assert_eq!(rig.sent("loadTonWalletState"), 1);
    assert_eq!(rig.sent("getCurrencyExchangeRates"), 1);
    for method in rig.wallet_methods() {
        assert!(READ_ONLY_METHODS.contains(&method.as_str()), "{method}");
    }
}

#[test]
fn collectibles_load_on_first_view() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    rig.driver.open_money_page().unwrap();
    rig.driver.set_money_tab(MoneyTab::Collectibles).unwrap();
    assert_eq!(rig.sent("getTonWalletNfts"), 1);
    rig.answer(
        "getTonWalletNfts",
        json!({"@type": "tonNfts", "nfts": [{"@type": "tonNft", "address": "EQnft", "name": "Cat"}], "next_offset": ""}),
    );
    rig.driver.set_money_tab(MoneyTab::Transactions).unwrap();
    rig.driver.set_money_tab(MoneyTab::Collectibles).unwrap();
    assert_eq!(rig.sent("getTonWalletNfts"), 1, "loaded once");
    assert_eq!(
        rig.driver
            .session
            .ton_wallet
            .money_page()
            .nfts
            .items()
            .len(),
        1
    );
}

#[test]
fn profile_address_is_fetched_once_and_never_for_yourself() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    assert!(
        !rig.driver.fetch_user_ton_wallet_address(9).unwrap(),
        "self"
    );
    assert!(rig.driver.fetch_user_ton_wallet_address(1).unwrap());
    assert!(
        !rig.driver.fetch_user_ton_wallet_address(1).unwrap(),
        "cached"
    );
    assert_eq!(rig.sent("getUserTonWalletAddresses"), 1);
    assert_eq!(
        rig.last("getUserTonWalletAddresses")["user_ids"],
        json!([1])
    );
    rig.answer(
        "getUserTonWalletAddresses",
        json!({"@type": "userTonWalletAddresses", "addresses": [{"@type": "userTonWalletAddress", "user_id": 1, "wallet_address": ADDRESS}]}),
    );
    assert_eq!(
        rig.driver.session.ton_wallet.profile_address(1),
        Some(ADDRESS)
    );

    rig.wallet_on(false);
    assert_eq!(rig.driver.session.ton_wallet.profile_address(1), None);
}

#[test]
fn a_user_without_a_wallet_settles_to_none() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    rig.driver.fetch_user_ton_wallet_address(1).unwrap();
    rig.answer(
        "getUserTonWalletAddresses",
        json!({"@type": "userTonWalletAddresses", "addresses": []}),
    );
    assert_eq!(
        rig.driver.session.ton_wallet.profile_addresses.get(&1),
        Some(&ProfileAddress::NoWallet)
    );
}

#[test]
fn a_transfer_card_opens_its_transaction_and_the_first_grams_hint() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    rig.ingest(
        &json!({"@type": "updateSuggestedActions",
        "added_actions": [{"@type": "suggestedActionViewFirstTonWalletTransferHint"}],
        "removed_actions": []})
        .to_string(),
    );
    assert!(rig.driver.open_wallet_transaction("t1").unwrap());
    assert!(matches!(
        rig.driver.session.ton_wallet.money_page().detail,
        MoneyDetail::Lookup { .. }
    ));
    assert_eq!(rig.last("getTonWalletTransaction")["transaction_id"], "t1");
    rig.answer("getTonWalletTransaction", transfer("t1", 2_000_000_000));
    let money = &rig.driver.session.ton_wallet.money_page();
    assert!(rig.driver.session.ton_wallet.money_open());
    assert_eq!(money.selected_tx().map(|tx| tx.id.as_str()), Some("t1"));
    assert!(
        money.hint_open(),
        "first incoming transfer brings up the hint"
    );

    rig.driver.dismiss_first_transfer_hint().unwrap();
    assert!(!rig.driver.session.ton_wallet.money_page().hint_open());
    assert_eq!(rig.sent("hideSuggestedAction"), 1);
    assert_eq!(
        rig.last("hideSuggestedAction")["action"]["@type"],
        FIRST_TRANSFER_HINT
    );
    // Once dismissed it does not come back or send again.
    rig.driver.dismiss_first_transfer_hint().unwrap();
    assert_eq!(rig.sent("hideSuggestedAction"), 1);
}

#[test]
fn failed_state_rates_and_explorer_are_asked_again_on_the_next_open() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    rig.driver.open_money_page().unwrap();
    assert_eq!(rig.sent("getApplicationConfig"), 1);
    let error = json!({"@type": "error", "code": 500, "message": "TIMEOUT"});
    rig.answer("loadTonWalletState", error.clone());
    rig.answer("getCurrencyExchangeRates", error.clone());
    rig.answer("getApplicationConfig", error);
    // Still in flight, so nothing is asked twice meanwhile.
    rig.driver.open_money_page().unwrap();
    assert_eq!(rig.sent("loadTonWalletState"), 2);
    assert_eq!(rig.sent("getCurrencyExchangeRates"), 2);
    assert_eq!(rig.sent("getApplicationConfig"), 2);
    rig.driver.open_money_page().unwrap();
    assert_eq!(rig.sent("loadTonWalletState"), 2, "in flight: not again");
    assert_eq!(rig.sent("getApplicationConfig"), 2, "in flight: not again");
}

#[test]
fn a_failed_lookup_says_so() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    rig.driver.open_wallet_transaction("t9").unwrap();
    rig.answer(
        "getTonWalletTransaction",
        json!({"@type": "error", "code": 400, "message": "TRANSACTION_NOT_FOUND"}),
    );
    let money = &rig.driver.session.ton_wallet.money_page();
    assert!(money.selected_tx().is_none());
    assert!(money.tx_error().is_some());
}

#[test]
fn a_late_answer_for_an_earlier_card_is_dropped() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    rig.driver.open_wallet_transaction("t1").unwrap();
    let first_extra = rig.last("getTonWalletTransaction")["@extra"].clone();
    rig.driver.open_wallet_transaction("t2").unwrap();
    let second = rig.driver.session.ton_wallet.money_page().detail.clone();
    // The first card answers after the second was clicked: dropped, for an
    // answer and for an error alike.
    let mut late = transfer("t1", 1);
    late["@extra"] = first_extra.clone();
    rig.ingest(&late.to_string());
    let money = &rig.driver.session.ton_wallet.money_page();
    assert!(money.selected_tx().is_none());
    assert_eq!(money.detail, second);
    rig.ingest(
        &json!({"@type": "error", "@extra": first_extra, "code": 400, "message": "X"}).to_string(),
    );
    assert_eq!(rig.driver.session.ton_wallet.money_page().detail, second);
    rig.answer("getTonWalletTransaction", transfer("t2", 2_000_000_000));
    let money = &rig.driver.session.ton_wallet.money_page();
    assert_eq!(money.selected_tx().map(|tx| tx.id.as_str()), Some("t2"));
}

fn resolve_link(rig: &mut Rig, url: &str, link_type: &str) -> Option<DeepLinkState> {
    rig.driver.request_deep_link_info(url).unwrap();
    assert_eq!(rig.last("getInternalLinkType")["link"], url);
    rig.answer("getInternalLinkType", json!({"@type": link_type}));
    rig.driver
        .session
        .chats_state
        .deep_link
        .take()
        .map(|flow| flow.stage)
}

#[test]
fn wallet_links_show_a_notice_instead_of_acting() {
    let mut rig = Rig::new();
    let before = rig.recorder.snapshot().len();
    let off = resolve_link(
        &mut rig,
        "https://t.me/sendgrams?to=EQabc",
        "internalLinkTypeTonWalletTransfer",
    );
    assert_eq!(
        off,
        Some(DeepLinkState::ShowText(
            "Wallet is currently unavailable.".into()
        ))
    );
    rig.wallet_on(true);
    let on = resolve_link(
        &mut rig,
        "tg://sendgrams?to=EQabc&amount=5",
        "internalLinkTypeTonWalletTransfer",
    );
    assert_eq!(on, None, "the notice replaces the link banner");
    assert_eq!(
        rig.driver.session.ton_wallet.notice,
        Some(WalletSurface::TransferLink)
    );
    rig.driver.session.ton_wallet.dismiss_notice();
    let connect = resolve_link(&mut rig, "tg://ton?connect=x", "internalLinkTypeTonConnect");
    assert_eq!(connect, None);
    assert_eq!(
        rig.driver.session.ton_wallet.notice,
        Some(WalletSurface::ConnectLink)
    );
    // Only the three lookups went out: nothing was prefilled or sent.
    assert_eq!(rig.recorder.snapshot().len(), before + 3);
    assert!(rig.wallet_methods().is_empty());
}

impl Rig {
    /// The `@extra` of the last `ty` request.
    fn extra_of(&self, ty: &str) -> Value {
        self.last(ty)["@extra"].clone()
    }

    fn fail_request(&mut self, extra: Value) {
        self.ingest(
            &json!({"@type": "error", "@extra": extra, "code": 500, "message": "later"})
                .to_string(),
        );
    }
}

#[test]
fn a_failed_profile_lookup_is_asked_again_when_the_profile_opens_again() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    assert!(rig.driver.fetch_user_ton_wallet_address(1).unwrap());
    let extra = rig.extra_of("getUserTonWalletAddresses");
    rig.fail_request(extra);
    assert_eq!(
        rig.driver.session.ton_wallet.profile_addresses.get(&1),
        Some(&ProfileAddress::Failed)
    );
    assert!(rig.driver.fetch_user_ton_wallet_address(1).unwrap());
    assert_eq!(rig.sent("getUserTonWalletAddresses"), 2);
    // A lookup in flight is not asked twice.
    assert!(!rig.driver.fetch_user_ton_wallet_address(1).unwrap());
}

#[test]
fn a_late_lookup_error_after_the_wallet_toggled_blocks_nothing() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    rig.driver.fetch_user_ton_wallet_address(1).unwrap();
    let late = rig.extra_of("getUserTonWalletAddresses");
    rig.wallet_on(false);
    rig.wallet_on(true);
    rig.fail_request(late);
    assert!(rig.driver.session.ton_wallet.profile_addresses.is_empty());
    assert!(rig.driver.fetch_user_ton_wallet_address(1).unwrap());
}

#[test]
fn an_older_state_request_failing_late_leaves_the_newer_one_loading() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    rig.driver.open_money_page().unwrap();
    let older = rig.extra_of("loadTonWalletState");
    rig.wallet_on(false);
    rig.wallet_on(true);
    rig.driver.open_money_page().unwrap();
    assert_eq!(rig.sent("loadTonWalletState"), 2);
    // The older request fails after the newer one went out.
    rig.fail_request(older);
    assert!(rig.driver.session.ton_wallet.own.is_loading());
    // The newer one failing is its own error.
    let newer = rig.extra_of("loadTonWalletState");
    rig.fail_request(newer);
    assert!(rig.driver.session.ton_wallet.own.error().is_some());
}

#[test]
fn the_wallet_turning_off_closes_its_notice_with_the_money_page() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    rig.driver.open_money_page().unwrap();
    rig.driver.session.ton_wallet.notice = Some(WalletSurface::Send);
    rig.wallet_on(false);
    let wallet = &rig.driver.session.ton_wallet;
    assert!(wallet.notice.is_none() && !wallet.money_open());
    assert!(wallet.rates.is_idle());
}

fn push_state(rig: &mut Rig, gram_amount: i64) {
    rig.ingest(&json!({"@type": "updateTonWalletState", "state": {
        "@type": "tonWalletState", "address": ADDRESS, "public_key": "", "gram_amount": gram_amount,
        "is_being_created": false, "is_backup_enabled": true,
    }}).to_string());
}

#[test]
fn the_update_then_the_ok_settles_the_load() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    rig.driver.open_money_page().unwrap();
    push_state(&mut rig, 5);
    assert!(rig.driver.session.ton_wallet.own.data().is_some());
    assert!(
        !rig.driver.session.ton_wallet.own.is_loading(),
        "the update settles the load it belongs to"
    );
    rig.answer("loadTonWalletState", json!({"@type": "ok"}));
    let own = &rig.driver.session.ton_wallet.own;
    assert_eq!(own.data().map(|s| s.gram_amount), Some(5));
    assert!(own.error().is_none() && !own.is_loading());
}

#[test]
fn an_ok_with_no_update_for_this_load_fails_it_despite_an_earlier_state() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    rig.driver.open_money_page().unwrap();
    push_state(&mut rig, 5);
    // The earlier load left a state; the next open asks again.
    rig.driver.session.ton_wallet.own.fail("offline");
    rig.driver.open_money_page().unwrap();
    assert_eq!(rig.sent("loadTonWalletState"), 2);
    assert!(rig.driver.session.ton_wallet.own.is_loading());
    rig.answer("loadTonWalletState", json!({"@type": "ok"}));
    let own = &rig.driver.session.ton_wallet.own;
    assert!(own.error().is_some(), "not settled from the old state");
    assert!(own.can_ask());
    rig.driver.open_money_page().unwrap();
    assert_eq!(rig.sent("loadTonWalletState"), 3, "asked again");
}
