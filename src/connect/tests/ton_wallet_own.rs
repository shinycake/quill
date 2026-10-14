//! Connect-driver tests: the own wallet, profile addresses, wallet links
//! and the first-transfer hint. Every getter waits for
//! `can_use_ton_wallet`, and only read-only methods go out.
use super::ton_wallet::Rig;
use crate::state::DeepLinkState;
use crate::ton_wallet::{FIRST_TRANSFER_HINT, MoneyTab, ProfileAddress, READ_ONLY_METHODS};
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
    assert!(!rig.driver.session.ton_wallet.money.open);
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
    let own = wallet.own.as_ref().expect("own");
    assert_eq!(own.gram_amount, 3_000_000_000, "the page's balance wins");
    assert!(own.is_backup_enabled);
    assert_eq!(wallet.rates, vec![("EUR".to_string(), 0.5)]);
    assert_eq!(wallet.money.history.items.len(), 1);
    assert!(wallet.money.history.has_more());

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
    assert_eq!(rig.driver.session.ton_wallet.money.nfts.items.len(), 1);
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
        Some(&ProfileAddress::None)
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
    assert!(rig.driver.session.ton_wallet.money.tx_loading);
    assert_eq!(rig.last("getTonWalletTransaction")["transaction_id"], "t1");
    rig.answer("getTonWalletTransaction", transfer("t1", 2_000_000_000));
    let money = &rig.driver.session.ton_wallet.money;
    assert!(money.open && !money.tx_loading);
    assert_eq!(
        money.selected_tx.as_ref().map(|tx| tx.id.as_str()),
        Some("t1")
    );
    assert!(
        money.hint_open,
        "first incoming transfer brings up the hint"
    );

    rig.driver.dismiss_first_transfer_hint().unwrap();
    assert!(!rig.driver.session.ton_wallet.money.hint_open);
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
fn a_failed_lookup_says_so() {
    let mut rig = Rig::new();
    rig.wallet_on(true);
    rig.driver.open_wallet_transaction("t9").unwrap();
    rig.answer(
        "getTonWalletTransaction",
        json!({"@type": "error", "code": 400, "message": "TRANSACTION_NOT_FOUND"}),
    );
    let money = &rig.driver.session.ton_wallet.money;
    assert!(!money.tx_loading && money.selected_tx.is_none());
    assert!(money.tx_error.is_some());
}

fn resolve_link(rig: &mut Rig, url: &str, link_type: &str) -> Option<DeepLinkState> {
    rig.driver.request_deep_link_info(url).unwrap();
    assert_eq!(rig.last("getInternalLinkType")["link"], url);
    rig.answer("getInternalLinkType", json!({"@type": link_type}));
    rig.driver.session.chats_state.deep_link.take()
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
    assert_eq!(
        on,
        Some(DeepLinkState::ShowText(
            crate::ton_wallet::links::TRANSFER_NOTICE.into()
        ))
    );
    let connect = resolve_link(&mut rig, "tg://ton?connect=x", "internalLinkTypeTonConnect");
    assert_eq!(
        connect,
        Some(DeepLinkState::ShowText(
            crate::ton_wallet::links::TON_CONNECT_NOTICE.into()
        ))
    );
    // Only the three lookups went out: nothing was prefilled or sent.
    assert_eq!(rig.recorder.snapshot().len(), before + 3);
    assert!(rig.wallet_methods().is_empty());
}
