use super::*;
use serde_json::json;

#[test]
fn star_amount_formats_whole_and_fraction() {
    let a = StarAmount {
        stars: 1234567,
        nanos: 0,
    };
    assert_eq!(a.label(), "1,234,567");
    let b = StarAmount {
        stars: 12,
        nanos: 500_000_000,
    };
    assert_eq!(b.label(), "12.5");
    let out = StarAmount {
        stars: -50,
        nanos: 0,
    };
    assert_eq!(out.signed_label(), "-50");
    assert_eq!(
        StarAmount {
            stars: 100,
            nanos: 0
        }
        .signed_label(),
        "+100"
    );
    assert!(
        StarAmount {
            stars: 0,
            nanos: -1
        }
        .is_negative()
    );
    assert_eq!(
        StarAmount {
            stars: 0,
            nanos: -250_000_000
        }
        .label(),
        "-0.25"
    );
}

#[test]
fn transaction_types_map_to_titles() {
    let page = parse_star_transactions(&json!({
        "@type": "starTransactions",
        "star_amount": {"star_count": 640, "nanostar_count": 0},
        "next_offset": "o2",
        "transactions": [
            {"id": "a", "star_amount": {"star_count": 500}, "is_refund": false, "date": 1700000000,
             "type": {"@type": "starTransactionTypePremiumBotDeposit"}},
            {"id": "b", "star_amount": {"star_count": -100}, "is_refund": false, "date": 1700000100,
             "type": {"@type": "starTransactionTypeGiftPurchase",
                      "owner_id": {"@type": "messageSenderUser", "user_id": 7},
                      "gift": {"@type": "gift", "star_count": 100}}},
            {"id": "c", "star_amount": {"star_count": 25}, "is_refund": true, "date": 1700000200,
             "type": {"@type": "starTransactionTypeBotInvoicePurchase", "user_id": 9,
                      "product_info": {"title": "Pro plan"}}},
            {"id": "d", "star_amount": {"star_count": 1}, "date": 1,
             "type": {"@type": "starTransactionTypeBrandNew"}}
        ]
    }));
    assert_eq!(page.balance.stars, 640);
    assert_eq!(page.next_offset, "o2");
    assert_eq!(page.transactions.len(), 4);
    assert_eq!(page.transactions[0].title, "Stars top-up");
    assert_eq!(page.transactions[0].detail, "Via Premium Bot");
    assert!(page.transactions[0].is_incoming());
    assert_eq!(page.transactions[1].title, "Gift");
    assert_eq!(page.transactions[1].detail, "100 Stars");
    assert_eq!(
        page.transactions[1].peer,
        Some(MessageSender::User { user_id: 7 })
    );
    assert!(!page.transactions[1].is_incoming());
    assert_eq!(page.transactions[2].title, "Pro plan");
    assert!(page.transactions[2].is_refund);
    assert_eq!(page.transactions[3].title, "Transaction");
}

#[test]
fn received_gifts_parse_regular_and_collectible() {
    let page = parse_received_gifts(&json!({
        "@type": "receivedGifts", "total_count": 2, "next_offset": "",
        "gifts": [
          {"received_gift_id": "g1", "sender_id": {"@type": "messageSenderUser", "user_id": 5},
           "text": {"text": "Happy birthday!", "entities": []}, "is_private": false,
           "is_saved": true, "is_pinned": false, "can_be_upgraded": true, "date": 1700000000,
           "sell_star_count": 85,
           "gift": {"@type": "sentGiftRegular", "gift": {"@type": "gift", "id": "1", "star_count": 100,
                    "sticker": {"@type": "sticker", "id": 1, "emoji": "x", "width": 512, "height": 512,
                                "format": {"@type": "stickerFormatWebp"},
                                "sticker": {"@type": "file", "id": 11, "size": 10, "expected_size": 10,
                                            "local": {"path": "", "is_downloading_completed": false, "can_be_downloaded": true},
                                            "remote": {"id": "r", "unique_id": "u"}}}}}},
          {"received_gift_id": "g2", "is_saved": false, "date": 1700000500, "sell_star_count": 0,
           "gift": {"@type": "sentGiftUpgraded", "gift": {"@type": "upgradedGift", "title": "Plush Pepe",
                    "number": 123, "backdrop": {"colors": {"center_color": 16711680, "edge_color": 255}}}}}
        ]
    }));
    assert_eq!(page.total_count, 2);
    assert_eq!(page.gifts.len(), 2);
    let first = &page.gifts[0];
    assert_eq!(first.text, "Happy birthday!");
    assert!(first.is_saved && first.can_convert());
    assert_eq!(first.headline(), "100 Stars");
    assert!(first.gift.sticker.is_some());
    assert_eq!(first.gift.files.len(), 1);
    let second = &page.gifts[1];
    assert!(second.gift.upgraded);
    assert_eq!(second.headline(), "Plush Pepe #123");
    assert_eq!(second.gift.backdrop, Some((0xFF0000, 0x0000FF)));
    assert!(!second.can_convert());
}

#[test]
fn gifts_without_an_id_are_dropped() {
    let page = parse_received_gifts(&json!({"gifts": [{"gift": {"gift": {}}}]}));
    assert!(page.gifts.is_empty());
}

#[test]
fn premium_features_keep_known_rows_and_limits() {
    let info = parse_premium_features(&json!({
        "features": [
            {"@type": "premiumFeatureIncreasedLimits"},
            {"@type": "premiumFeatureFromTheFuture"},
            {"@type": "premiumFeatureVoiceRecognition"}
        ],
        "limits": [
            {"type": {"@type": "premiumLimitTypeChatFolderCount"}, "default_value": 10, "premium_value": 20},
            {"type": {"@type": "premiumLimitTypeUnknown"}, "default_value": 1, "premium_value": 2}
        ]
    }));
    assert_eq!(info.features.len(), 2);
    assert_eq!(info.features[0].title, "Doubled limits");
    assert_eq!(info.limits.len(), 1);
    assert_eq!(info.limits[0].title, "Folders");
    assert_eq!(info.limits[0].premium_value, 20);
}

#[test]
fn premium_state_reads_current_plan() {
    let s = parse_premium_state(&json!({
        "state": {"text": "Thanks for subscribing", "entities": []},
        "payment_options": [{"is_current": false}, {"is_current": true}]
    }));
    assert!(s.is_subscribed);
    assert_eq!(s.text, "Thanks for subscribing");
    assert!(!parse_premium_state(&json!({})).is_subscribed);
}

#[test]
fn hub_pages_append_and_replace() {
    let mut hub = PremiumHub::default();
    let tx = |id: &str| StarTx {
        id: id.into(),
        amount: StarAmount { stars: 1, nanos: 0 },
        is_refund: false,
        date: 0,
        title: "t".into(),
        detail: String::new(),
        peer: None,
        kind: String::new(),
    };
    hub.apply_transactions(
        StarTxPage {
            balance: StarAmount { stars: 9, nanos: 0 },
            transactions: vec![tx("a")],
            next_offset: "n".into(),
        },
        false,
    );
    hub.apply_transactions(
        StarTxPage {
            balance: StarAmount { stars: 9, nanos: 0 },
            transactions: vec![tx("b")],
            next_offset: String::new(),
        },
        true,
    );
    assert_eq!(hub.transactions.len(), 2);
    assert!(hub.tx_offset.is_empty());
    hub.tx_selected = Some("b".into());
    assert_eq!(hub.selected_tx().map(|t| t.id.as_str()), Some("b"));
    hub.apply_transactions(StarTxPage::default(), false);
    assert!(hub.transactions.is_empty());
}

#[test]
fn only_own_gifts_are_mine() {
    let mut hub = PremiumHub {
        gifts_owner: Some(MessageSender::User { user_id: 4 }),
        ..Default::default()
    };
    assert!(hub.gifts_are_mine(Some(4)));
    assert!(!hub.gifts_are_mine(Some(5)));
    assert!(!hub.gifts_are_mine(None));
    hub.gifts_owner = Some(MessageSender::Chat { chat_id: 4 });
    assert!(!hub.gifts_are_mine(Some(4)));
}

#[test]
fn filters_map_to_td_directions() {
    assert_eq!(TxFilter::All.direction_type(), None);
    assert_eq!(
        TxFilter::Incoming.direction_type(),
        Some("transactionDirectionIncoming")
    );
    assert_eq!(
        TxFilter::Outgoing.direction_type(),
        Some("transactionDirectionOutgoing")
    );
}
