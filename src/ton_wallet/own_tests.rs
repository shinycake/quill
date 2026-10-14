//! TON wallet, the own side: parsing the state, history and NFTs, the
//! fiat value, wallet links and the read-only method allowlist.
use super::links::{TON_CONNECT_NOTICE, TRANSFER_NOTICE, UNAVAILABLE, link_notice};
use super::*;
use crate::deep_link_types::InternalLink;
use serde_json::json;

fn transfer_tx(id: &str, amount: i64, comment: &str, encrypted: bool) -> serde_json::Value {
    json!({
        "@type": "tonWalletTransaction",
        "id": id,
        "peer_address": "EQAbcdefghijklmnopqrstuvwxyz0123456789ABCDEFwxyz",
        "peer_user_id": 0,
        "peer_domain": "",
        "date": 1_700_000_000,
        "fee_amount": 4_000_000,
        "state": {"@type": "tonWalletTransactionStateSucceeded", "tx_hash": "q83vEjRWeJA="},
        "type": {
            "@type": "tonWalletTransactionTypeTransfer",
            "amount": amount,
            "is_gasless": false,
            "comment": comment,
            "is_comment_encrypted": encrypted,
        },
    })
}

#[test]
fn own_state_parses_without_any_key() {
    let Some(TonWalletPayload::State(own)) = parse_payload(
        "updateTonWalletState",
        &json!({"state": {
            "@type": "tonWalletState",
            "address": "EQAbc",
            "public_key": "ignored",
            "gram_amount": 2_500_000_000i64,
            "is_being_created": false,
            "is_backup_enabled": true,
        }}),
    ) else {
        panic!("state");
    };
    assert_eq!(own.address, "EQAbc");
    assert_eq!(own.gram_amount, 2_500_000_000);
    assert!(own.is_backup_enabled && own.exists());
    assert!(!OwnWallet::parse(&json!({"address": ""})).exists());
}

#[test]
fn transfers_word_like_the_ios_list() {
    let incoming = WalletTx::parse(&transfer_tx("a", 1_500_000_000, "hi", false)).expect("tx");
    assert!(incoming.is_incoming() && incoming.is_incoming_gram_transfer());
    assert_eq!(incoming.amount_text(), "+1.5");
    assert_eq!(incoming.subtitle(), "Deposit");
    assert_eq!(incoming.public_comment(), Some("hi"));
    assert!(!incoming.has_encrypted_comment());

    let mut outgoing = WalletTx::parse(&transfer_tx("b", -2_000_000_000, "x", true)).expect("tx");
    assert_eq!(outgoing.amount_text(), "\u{2013}2");
    assert_eq!(outgoing.subtitle(), "Withdrawal");
    assert_eq!(outgoing.public_comment(), None);
    assert!(outgoing.has_encrypted_comment());
    outgoing.peer_user_id = 5;
    assert_eq!(outgoing.subtitle(), "Outgoing transfer");
    outgoing.tx_hash = None;
    assert!(outgoing.failed());
    assert_eq!(outgoing.subtitle(), "Failed");
}

#[test]
fn failed_state_drops_the_hash_and_no_id_is_no_tx() {
    let mut value = transfer_tx("c", 1, "", false);
    value["state"] = json!({"@type": "tonWalletTransactionStateFailed"});
    let tx = WalletTx::parse(&value).expect("tx");
    assert!(tx.failed());
    value["id"] = json!("");
    assert!(WalletTx::parse(&value).is_none());
}

#[test]
fn nft_transfer_and_key_change_rows() {
    let tx = WalletTx::parse(&json!({
        "id": "n",
        "peer_address": "",
        "state": {"@type": "tonWalletTransactionStateSucceeded", "tx_hash": ""},
        "type": {
            "@type": "tonWalletTransactionTypeNftTransfer",
            "nft": {"address": "EQnft", "name": "", "index": "7"},
            "is_outgoing": false,
        },
    }))
    .expect("tx");
    assert_eq!(tx.amount_text(), "+1 item");
    assert_eq!(tx.subtitle(), "Incoming collectible");
    let own::TxKind::NftTransfer { nft, .. } = &tx.kind else {
        panic!("nft");
    };
    assert_eq!(nft.title(), "Collectible");

    let key = WalletTx::parse(&json!({
        "id": "k",
        "fee_amount": 12_345,
        "state": {"@type": "tonWalletTransactionStateSucceeded", "tx_hash": ""},
        "type": {"@type": "tonWalletTransactionTypeKeyChange"},
    }))
    .expect("tx");
    assert_eq!(key.subtitle(), "Key Update");
    assert_eq!(key.amount_text(), "\u{2013}0.00001");
}

#[test]
fn pages_and_profile_addresses_parse() {
    let Some(TonWalletPayload::Transactions(page)) = parse_payload(
        "tonWalletTransactions",
        &json!({"gram_amount": 9, "transactions": [transfer_tx("a", 1, "", false)], "next_offset": "n2"}),
    ) else {
        panic!("page");
    };
    assert_eq!((page.gram_amount, page.transactions.len()), (9, 1));
    assert_eq!(page.next_offset, "n2");

    let Some(TonWalletPayload::Nfts(nfts)) = parse_payload(
        "tonNfts",
        &json!({"nfts": [{"address": "EQ1", "name": "Cat", "attributes": [{"trait_type": "Fur", "value": "Red"}]}], "next_offset": ""}),
    ) else {
        panic!("nfts");
    };
    assert_eq!(
        nfts.nfts.first().map(|n| n.attributes.clone()),
        Some(vec![("Fur".into(), "Red".into())])
    );

    let Some(TonWalletPayload::UserAddresses(found)) = parse_payload(
        "userTonWalletAddresses",
        &json!({"addresses": [
            {"user_id": 5, "wallet_address": "EQfive"},
            {"user_id": "6", "wallet_address": " "},
            {"user_id": 0, "wallet_address": "EQzero"},
        ]}),
    ) else {
        panic!("addresses");
    };
    assert_eq!(found, vec![(5, "EQfive".to_string())]);
    assert!(parse_payload("somethingElse", &json!({})).is_none());
}

#[test]
fn fiat_value_follows_the_ios_formula() {
    let rates = fiat::parse_exchange_rates(&json!({"rates": [
        {"currency": "EUR", "rate": 0.5},
        {"currency": "XXX", "rate": -1.0},
    ]}));
    assert_eq!(rates, vec![("EUR".to_string(), 0.5)]);
    let config = json!({"members": [{"key": "ton_usd_rate", "value": {"@type": "jsonValueNumber", "value": 3.0}}]});
    let usd = fiat::ton_usd_rate_from_config(&config);
    assert_eq!(usd, Some(3.0));
    // 1,000 Grams at $3 = $3,000.00; in EUR at 0.5 per USD = €1,500.00.
    assert_eq!(
        fiat::fiat_balance(
            1_000_000_000_000,
            fiat::units_per_gram(usd, &rates, "USD"),
            "USD"
        ),
        "$3,000.00"
    );
    assert_eq!(
        fiat::fiat_balance(
            1_000_000_000_000,
            fiat::units_per_gram(usd, &rates, "EUR"),
            "EUR"
        ),
        "\u{20AC}1,500.00"
    );
    assert_eq!(fiat::fiat_balance(0, Some(3.0), "USD"), "$0");
    assert_eq!(fiat::fiat_balance(5, None, "USD"), "\u{2014}");
    assert_eq!(fiat::units_per_gram(usd, &rates, "GBP"), None);
    assert_eq!(fiat::available_currencies(&rates), vec!["EUR"]);
}

#[test]
fn wallet_state_hides_everything_when_the_option_is_off() {
    let mut wallet = TonWalletState {
        can_use: true,
        ..Default::default()
    };
    wallet
        .profile_addresses
        .insert(5, ProfileAddress::Found("EQfive".into()));
    wallet.own = Some(OwnWallet::default());
    wallet.money.open = true;
    assert_eq!(wallet.profile_address(5), Some("EQfive"));
    wallet.set_can_use(false);
    assert_eq!(wallet.profile_address(5), None);
    assert!(wallet.own.is_none() && !wallet.money.open && !wallet.state_requested);
}

#[test]
fn paged_lists_replace_then_append() {
    let mut list: Paged<u8> = Paged {
        loading: true,
        ..Paged::default()
    };
    list.apply(vec![1, 2], "next".into(), false);
    assert!(list.has_more() && !list.loading);
    list.apply(vec![3], String::new(), true);
    assert_eq!(list.items, vec![1, 2, 3]);
    assert!(!list.has_more());
    list.fail("boom".into());
    assert_eq!(list.items.len(), 3);
    list.apply(vec![9], String::new(), false);
    assert_eq!(list.items, vec![9]);
}

#[test]
fn transaction_explorer_link_uses_the_hash_in_hex() {
    assert_eq!(
        transaction_explorer_url(None, "q83vEjRWeJA=").as_deref(),
        Some("https://tonviewer.com/transaction/abcdef1234567890")
    );
    assert_eq!(
        transaction_explorer_url(Some("https://tonscan.org"), "q83v").as_deref(),
        Some("https://tonscan.org/transaction/abcdef")
    );
    assert_eq!(transaction_explorer_url(None, "not base64!"), None);
    assert_eq!(transaction_explorer_url(None, ""), None);
}

#[test]
fn wallet_links_show_a_notice_and_never_act() {
    let transfer = InternalLink::Other("TonWalletTransfer".into());
    let connect = InternalLink::Other("TonConnect".into());
    assert_eq!(link_notice(&transfer, true), Some(TRANSFER_NOTICE));
    assert_eq!(link_notice(&connect, true), Some(TON_CONNECT_NOTICE));
    assert_eq!(link_notice(&transfer, false), Some(UNAVAILABLE));
    assert_eq!(link_notice(&connect, false), Some(UNAVAILABLE));
    assert_eq!(
        link_notice(&InternalLink::Other("QrCodeAuthentication".into()), true),
        None
    );
}

#[test]
fn read_only_methods_never_overlap_the_forbidden_ones() {
    for method in READ_ONLY_METHODS {
        assert!(!FORBIDDEN_METHODS.contains(method), "{method}");
    }
}

/// Every TDLib method named in the wallet request builders is one of the
/// read-only getters: a new wallet method has to be added to
/// [`READ_ONLY_METHODS`] (and justified) before it can be sent.
#[test]
fn wallet_request_builders_only_build_read_only_methods() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/telegram/requests/ton_wallet.rs");
    let source = std::fs::read_to_string(&path).unwrap_or_default();
    let built: Vec<&str> = source
        .split("\"@type\": \"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .collect();
    assert_eq!(built.len(), READ_ONLY_METHODS.len(), "{built:?}");
    for method in built {
        assert!(
            READ_ONLY_METHODS.contains(&method),
            "{method} is not read-only"
        );
    }
}

#[test]
fn wallet_prefs_round_trip() {
    let dir = std::env::temp_dir().join(format!("quill-ton-prefs-{}", std::process::id()));
    let paths = crate::settings::AccountPaths::for_root(&dir, &crate::ids::AccountKey::primary());
    assert_eq!(load_prefs(&paths), WalletPrefs::default());
    let prefs = WalletPrefs {
        currency: Some("EUR".into()),
    };
    if std::fs::create_dir_all(&paths.root).is_ok() && save_prefs(&paths, &prefs).is_ok() {
        assert_eq!(load_prefs(&paths), prefs);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
