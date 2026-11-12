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

/// TDLib 1.8.68+ documents `product_info` of the bot subscription transaction
/// types as nullable; a JSON null (or a missing key) falls back to the label.
#[test]
fn null_product_info_falls_back_to_generic_titles() {
    let page = parse_star_transactions(&json!({
        "@type": "starTransactions",
        "star_amount": {"star_count": 0, "nanostar_count": 0},
        "next_offset": "",
        "transactions": [
            {"id": "a", "star_amount": {"star_count": -5}, "date": 1,
             "type": {"@type": "starTransactionTypeBotSubscriptionPurchase", "user_id": 9,
                      "subscription_period": 2592000, "product_info": null}},
            {"id": "b", "star_amount": {"star_count": 5}, "date": 2,
             "type": {"@type": "starTransactionTypeBotSubscriptionSale", "user_id": 9,
                      "subscription_period": 2592000, "product_info": null}},
            {"id": "c", "star_amount": {"star_count": -5}, "date": 3,
             "type": {"@type": "starTransactionTypeBotSubscriptionPurchase", "user_id": 9,
                      "subscription_period": 2592000}}
        ]
    }));
    assert_eq!(page.transactions.len(), 3);
    assert_eq!(page.transactions[0].title, "Subscription");
    assert_eq!(page.transactions[0].detail, "monthly");
    assert_eq!(page.transactions[1].title, "Subscription sale");
    assert_eq!(page.transactions[2].title, "Subscription");
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
            {"type": {"@type": "premiumLimitTypeOwnedBotCount"}, "default_value": 20, "premium_value": 40},
            {"type": {"@type": "premiumLimitTypeUnknown"}, "default_value": 1, "premium_value": 2}
        ]
    }));
    assert_eq!(info.features.len(), 2);
    assert_eq!(info.features[0].title(), "Doubled limits");
    assert_eq!(info.limits.len(), 3);
    assert_eq!(info.limits[0].title, Some("Folders"));
    assert_eq!(info.limits[0].premium_value, 20);
    assert_eq!(info.limits[1].title, None);
    assert_eq!(
        info.limit_pair(crate::premium_limits::PremiumLimitKind::OwnedBots)
            .map(|pair| pair.premium_value),
        Some(40)
    );
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
    assert_eq!(hub.stars.rows.len(), 2);
    assert!(hub.stars.offset.is_empty());
    hub.stars.open();
    hub.stars.select(Some("b".into()));
    assert_eq!(hub.selected_tx().map(|t| t.id.as_str()), Some("b"));
    hub.apply_transactions(StarTxPage::default(), false);
    assert!(hub.stars.rows.is_empty());
}

#[test]
fn only_own_gifts_are_mine() {
    let mut hub = PremiumHub::default();
    hub.received.owner = Some(MessageSender::User { user_id: 4 });
    assert!(hub.gifts_are_mine(Some(4)));
    assert!(!hub.gifts_are_mine(Some(5)));
    assert!(!hub.gifts_are_mine(None));
    hub.received.owner = Some(MessageSender::Chat { chat_id: 4 });
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

/// 04-051: the smallest i64 from a hostile `star_count` draws, not panics.
#[test]
fn the_label_of_the_smallest_star_count_does_not_overflow() {
    let amount = StarAmount {
        stars: i64::MIN,
        nanos: 0,
    };
    assert_eq!(amount.label(), "-9,223,372,036,854,775,808");
    assert!(amount.signed_label().starts_with('-'));
    let small = StarAmount {
        stars: -1234,
        nanos: 0,
    };
    assert_eq!(small.label(), "-1,234");
}

/// 04-026: a filter change replaces the history whole: rows, cursor, page
/// stage and the open transaction start over, the box stays open.
#[test]
fn another_stars_tab_replaces_the_whole_history() {
    use crate::premium_hub::page_stage::PageStage;
    let mut hub = PremiumHub::default();
    hub.stars.open();
    hub.stars.offset = "next".into();
    hub.stars.page = PageStage::Ready;
    hub.stars.select(Some("a".into()));
    assert_eq!(hub.stars.selected(), Some("a"));
    hub.stars.switch_filter(TxFilter::Incoming);
    assert_eq!(hub.stars.filter, TxFilter::Incoming);
    assert!(hub.stars.is_open() && hub.stars.selected().is_none());
    assert!(hub.stars.rows.is_empty() && hub.stars.offset.is_empty());
    assert_eq!(hub.stars.page, PageStage::Idle);
    // A closed box shows no details, and closing takes the selection.
    hub.stars.select(Some("b".into()));
    assert_eq!(hub.stars.selected(), Some("b"));
    hub.stars.close();
    assert!(!hub.stars.is_open() && hub.stars.selected().is_none());
    hub.stars.select(Some("c".into()));
    assert!(hub.stars.selected().is_none());
}

/// Sweep r3-11, 20-001: "Buy more Stars" is part of the open Stars view, so
/// every way of closing the box folds it, and it survives a tab change and a
/// transaction's details.
#[test]
fn the_packs_section_lives_and_dies_with_the_open_stars_box() {
    let mut hub = PremiumHub::default();
    assert!(!hub.stars.toggle_packs(), "nothing to expand while closed");
    hub.stars.expand_packs();
    assert!(!hub.stars.packs_expanded());
    hub.stars.open();
    assert!(!hub.stars.packs_expanded(), "folded when the box opens");
    assert!(hub.stars.toggle_packs());
    hub.stars.select(Some("a".into()));
    assert!(
        hub.stars.packs_expanded(),
        "kept under a transaction's details"
    );
    hub.stars.switch_filter(TxFilter::Incoming);
    assert!(hub.stars.packs_expanded(), "kept over a tab change");
    assert!(!hub.stars.toggle_packs());
    hub.stars.expand_packs();
    hub.stars.close();
    assert!(!hub.stars.packs_expanded());
    hub.stars.open();
    assert!(!hub.stars.packs_expanded(), "folded again after a close");
}

/// 04-025: opening the gifts of another owner replaces the whole list; a
/// change still out keeps its lock, and a refetch owed to the old list goes.
#[test]
fn opening_gifts_of_another_owner_replaces_the_list_but_keeps_a_change_in_flight() {
    use crate::ids::RequestId;
    use crate::state::fetch::ListOp;
    let user = MessageSender::User { user_id: 4 };
    let chat = MessageSender::Chat { chat_id: 9 };
    let mut list = ReceivedGifts::opened_for(user, ListOp::Idle);
    assert!(list.is_open() && list.owner == Some(user));
    list.total = 3;
    list.offset = "more".into();
    list.notice = Some("old".into());
    list.view.select(Some("g".into()));
    list.op.begin(RequestId(7));
    list = ReceivedGifts::opened_for(chat, list.op);
    assert_eq!(list.owner, Some(chat));
    assert_eq!(
        (list.total, list.offset.as_str(), list.notice.as_deref()),
        (0, "", None)
    );
    assert!(list.view.selected().is_none() && list.is_open());
    assert_eq!(list.op, ListOp::Mutating(RequestId(7)));
    list.op.mark_stale();
    list.restart();
    assert_eq!(list.owner, Some(chat), "the same owner's list from the top");
    assert!(list.op.is_mutating(), "the change in flight stays");
    let stale = ReceivedGifts::opened_for(user, ListOp::Stale);
    assert!(
        !stale.op.is_stale(),
        "a refetch owed to the old list is not owed"
    );
    // Closing keeps the list for a change still out, and shows no panel.
    let mut closed = stale;
    closed.close();
    assert!(!closed.is_open());
    closed.view.select(Some("x".into()));
    assert!(closed.view.selected().is_none());
}

/// Sweep r4-10: the Premium `/start` goes only for the flow that resolved
/// the bot; a second link to the same bot is another flow.
#[test]
fn the_premium_start_belongs_to_the_flow_that_resolved_the_bot() {
    let auto = PremiumAutoStart {
        username: "premiumbot".into(),
        start_ref: "x".into(),
        request: crate::ids::RequestId(5),
    };
    assert!(auto.is_for("PremiumBot", Some(crate::ids::RequestId(5))));
    assert!(!auto.is_for("PremiumBot", Some(crate::ids::RequestId(6))));
    assert!(!auto.is_for("PremiumBot", None));
    assert!(!auto.is_for("other", Some(crate::ids::RequestId(5))));
}

/// Sweep r4-10 round 4: the queue the deep-link open takes the `/start`
/// from. A chat opened by another flow takes nothing and ends the queue.
#[test]
fn another_flows_chat_takes_no_start_and_clears_it() {
    use crate::ids::RequestId;
    let queued = || PremiumHub {
        auto_start: Some(PremiumAutoStart {
            username: "premiumbot".into(),
            start_ref: "x".into(),
            request: RequestId(1),
        }),
        ..PremiumHub::default()
    };
    let mut hub = queued();
    assert_eq!(hub.take_auto_start("PremiumBot", Some(RequestId(2))), None);
    assert_eq!(hub.auto_start, None, "the flow is over");
    assert_eq!(hub.take_auto_start("PremiumBot", Some(RequestId(1))), None);
    let mut hub = queued();
    let taken = hub.take_auto_start("PremiumBot", Some(RequestId(1)));
    assert_eq!(taken.map(|auto| auto.start_ref), Some("x".to_string()));
    assert_eq!(hub.auto_start, None, "taken once");
}
