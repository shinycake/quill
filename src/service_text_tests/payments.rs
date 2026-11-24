//! Service text wording: Premium, giveaways, Stars, gifts and paid messages.
use super::*;

wording!(
    payment_refunded,
    BY_DANA,
    r#"{"@type":"messagePaymentRefunded","owner_id":{"@type":"messageSenderChat","chat_id":-2002},"currency":"USD","total_amount":1999,"invoice_payload":"","telegram_payment_charge_id":"c","provider_payment_charge_id":"p"}"#,
    "Launch Channel refunded USD 19.99"
);
wording!(
    premium_gifted_to_me,
    BY_DANA,
    r#"{"@type":"messageGiftedPremium","gifter_user_id":1,"receiver_user_id":9,"text":{"text":"","entities":[]},"currency":"USD","amount":999,"cryptocurrency":"","cryptocurrency_amount":0,"month_count":3,"day_count":0}"#,
    "Dana sent you a gift for USD 9.99"
);
wording!(
    premium_gifted_by_me,
    BY_ME,
    r#"{"@type":"messageGiftedPremium","gifter_user_id":9,"receiver_user_id":1,"currency":"USD","amount":999,"month_count":3}"#,
    "You sent a gift for USD 9.99"
);
wording!(
    premium_gift_code_from_giveaway,
    BY_DANA,
    r#"{"@type":"messagePremiumGiftCode","creator_id":{"@type":"messageSenderChat","chat_id":-2002},"text":{"text":"","entities":[]},"is_from_giveaway":true,"is_unclaimed":false,"currency":"USD","amount":0,"month_count":3,"code":"abc"}"#,
    "You won a prize in a giveaway organized by Launch Channel."
);
wording!(
    premium_gift_code_unclaimed,
    BY_DANA,
    r#"{"@type":"messagePremiumGiftCode","creator_id":{"@type":"messageSenderChat","chat_id":-2002},"is_from_giveaway":true,"is_unclaimed":true,"currency":"USD","amount":0,"month_count":3,"code":"abc"}"#,
    "You have an unclaimed prize from a giveaway by Launch Channel."
);
wording!(
    premium_gift_code_from_channel,
    BY_DANA,
    r#"{"@type":"messagePremiumGiftCode","creator_id":{"@type":"messageSenderChat","chat_id":-2002},"is_from_giveaway":false,"is_unclaimed":false,"currency":"USD","amount":0,"month_count":3,"code":"abc"}"#,
    "You've received a gift from Launch Channel."
);
wording!(
    giveaway_started_in_channel,
    IN_CHANNEL,
    r#"{"@type":"messageGiveawayCreated","star_count":0}"#,
    "Someone just started a giveaway of Telegram Premium subscriptions for its followers."
);
wording!(
    stars_giveaway_started,
    BY_DANA,
    r#"{"@type":"messageGiveawayCreated","star_count":500}"#,
    "Dana Cole just started a giveaway of 500 Stars to its members."
);
wording!(
    premium_giveaway_card,
    IN_CHANNEL,
    r#"{"@type":"messageGiveaway","parameters":{"@type":"giveawayParameters","boosted_chat_id":-2002},"winner_count":5,"prize":{"@type":"giveawayPrizePremium","month_count":3}}"#,
    "Giveaway: 5 Telegram Premium subscriptions for 3 months"
);
wording!(
    giveaway_completed,
    IN_CHANNEL,
    r#"{"@type":"messageGiveawayCompleted","giveaway_message_id":5,"winner_count":4,"is_star_giveaway":false,"unclaimed_prize_count":0}"#,
    "4 winners of the giveaway were randomly selected by Telegram and received private messages with giftcodes."
);
wording!(
    giveaway_completed_some_unclaimed,
    IN_CHANNEL,
    r#"{"@type":"messageGiveawayCompleted","giveaway_message_id":5,"winner_count":4,"is_star_giveaway":true,"unclaimed_prize_count":1}"#,
    "Some winners of the giveaway were randomly selected by Telegram and received their prize."
);
wording!(
    giveaway_without_winners,
    IN_CHANNEL,
    r#"{"@type":"messageGiveawayWinners","boosted_chat_id":-2002,"giveaway_message_id":5,"winner_count":0,"unclaimed_prize_count":0,"winner_user_ids":[],"prize":{"@type":"giveawayPrizePremium","month_count":3}}"#,
    "No winners of the giveaway could be selected."
);
wording!(
    stars_gifted_to_me,
    BY_DANA,
    r#"{"@type":"messageGiftedStars","gifter_user_id":2,"receiver_user_id":9,"currency":"USD","amount":499,"star_count":250,"transaction_id":"t"}"#,
    "Omar sent you a gift for USD 4.99"
);
wording!(
    grams_gifted_to_me,
    BY_DANA,
    r#"{"@type":"messageGiftedGrams","gifter_user_id":3,"receiver_user_id":9,"gram_amount":250,"transaction_id":"t"}"#,
    "Lea sent you a gift for 2.5 Grams"
);
wording!(
    giveaway_stars_prize,
    BY_DANA,
    r#"{"@type":"messageGiveawayPrizeStars","star_count":100,"transaction_id":"t","boosted_chat_id":-2002,"giveaway_message_id":5,"is_unclaimed":false}"#,
    "You won a prize in a giveaway organized by Launch Channel."
);
wording!(
    gift_received,
    BY_DANA,
    r#"{"@type":"messageGift","gift":{"@type":"gift","id":"1","star_count":100},"sender_id":{"@type":"messageSenderUser","user_id":1},"receiver_id":{"@type":"messageSenderUser","user_id":9},"prepaid_upgrade_star_count":0}"#,
    "Dana sent you a gift for 100 Stars"
);
wording!(
    gift_sent,
    BY_ME,
    r#"{"@type":"messageGift","gift":{"@type":"gift","id":"1","star_count":1},"sender_id":{"@type":"messageSenderUser","user_id":9},"receiver_id":{"@type":"messageSenderUser","user_id":1}}"#,
    "You sent a gift for 1 Star"
);
wording!(
    gift_prepaid_upgrade,
    BY_DANA,
    r#"{"@type":"messageGift","gift":{"@type":"gift","id":"1","star_count":100},"sender_id":{"@type":"messageSenderUser","user_id":1},"receiver_id":{"@type":"messageSenderUser","user_id":9},"is_prepaid_upgrade":true,"prepaid_upgrade_star_count":300}"#,
    "Dana sent an upgrade worth 300 Stars for your gift."
);
wording!(
    gift_upgraded_by_me,
    BY_ME,
    r#"{"@type":"messageUpgradedGift","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"sender_id":{"@type":"messageSenderUser","user_id":1},"receiver_id":{"@type":"messageSenderUser","user_id":9},"origin":{"@type":"upgradedGiftOriginUpgrade","gift_message_id":0}}"#,
    "You turned the gift from Dana Cole into a unique collectible"
);
wording!(
    gift_upgraded_by_sender_for_me,
    BY_DANA,
    r#"{"@type":"messageUpgradedGift","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"sender_id":{"@type":"messageSenderUser","user_id":9},"receiver_id":{"@type":"messageSenderUser","user_id":1},"origin":{"@type":"upgradedGiftOriginUpgrade","gift_message_id":0}}"#,
    "Dana Cole turned the gift from you into a unique collectible"
);
wording!(
    gift_transferred_to_me,
    BY_DANA,
    r#"{"@type":"messageUpgradedGift","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"sender_id":{"@type":"messageSenderUser","user_id":1},"receiver_id":{"@type":"messageSenderUser","user_id":9},"origin":{"@type":"upgradedGiftOriginTransfer"}}"#,
    "Dana Cole transferred you a gift"
);
wording!(
    gift_resold_to_me,
    BY_DANA,
    r#"{"@type":"messageUpgradedGift","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"sender_id":{"@type":"messageSenderUser","user_id":1},"receiver_id":{"@type":"messageSenderUser","user_id":9},"origin":{"@type":"upgradedGiftOriginResale","price":{"@type":"giftResalePriceStar","star_count":900}}}"#,
    "Dana Cole sold you a gift for 900 Stars"
);
wording!(
    gift_crafted,
    BY_ME,
    r#"{"@type":"messageUpgradedGift","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"origin":{"@type":"upgradedGiftOriginCraft"}}"#,
    "You crafted a new gift"
);
wording!(
    gift_refunded,
    BY_DANA,
    r#"{"@type":"messageRefundedUpgradedGift","gift":{"@type":"gift","id":"1"},"origin":{"@type":"upgradedGiftOriginUpgrade","gift_message_id":0}}"#,
    "This gift was downgraded because a request to refund the payment related to this gift was made, and the money was returned."
);
wording!(
    gift_offer_pending,
    BY_DANA,
    r#"{"@type":"messageUpgradedGiftPurchaseOffer","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"state":{"@type":"giftPurchaseOfferStatePending"},"price":{"@type":"giftResalePriceStar","star_count":700},"expiration_date":1}"#,
    "An offer to buy this gift for 700 Stars."
);
wording!(
    gift_offer_accepted,
    BY_DANA,
    r#"{"@type":"messageUpgradedGiftPurchaseOffer","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"state":{"@type":"giftPurchaseOfferStateAccepted"},"price":{"@type":"giftResalePriceStar","star_count":700},"expiration_date":1}"#,
    "This offer was accepted."
);
wording!(
    gift_offer_rejected,
    BY_DANA,
    r#"{"@type":"messageUpgradedGiftPurchaseOfferRejected","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"price":{"@type":"giftResalePriceStar","star_count":700},"offer_message_id":5,"was_expired":false}"#,
    "Dana Cole rejected your offer to buy Plush Pepe #4 for 700 Stars."
);
wording!(
    gift_offer_expired,
    BY_ME,
    r#"{"@type":"messageUpgradedGiftPurchaseOfferRejected","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"price":{"@type":"giftResalePriceStar","star_count":700},"offer_message_id":5,"was_expired":true}"#,
    "Your offer to buy Plush Pepe #4 for 700 Stars has expired."
);
wording!(
    paid_messages_refunded,
    BY_DANA,
    r#"{"@type":"messagePaidMessagesRefunded","message_count":2,"star_count":20}"#,
    "Dana Cole refunded 20 Stars to you"
);
wording!(
    paid_message_price_set,
    BY_DANA,
    r#"{"@type":"messagePaidMessagePriceChanged","paid_message_star_count":5}"#,
    "Messages now cost 5 Stars each in this group."
);
wording!(
    paid_message_price_free,
    BY_DANA,
    r#"{"@type":"messagePaidMessagePriceChanged","paid_message_star_count":0}"#,
    "Messages are now free in this group."
);
wording!(
    direct_messages_paid,
    IN_CHANNEL,
    r#"{"@type":"messageDirectMessagePriceChanged","is_enabled":true,"paid_message_star_count":10}"#,
    "Channel allows Direct Messages for 10 Stars each."
);
wording!(
    direct_messages_disabled,
    IN_CHANNEL,
    r#"{"@type":"messageDirectMessagePriceChanged","is_enabled":false,"paid_message_star_count":0}"#,
    "Channel disabled Direct Messages."
);

// TDLib 1.8.71: on-chain offers read as text (no buttons), in Grams.
#[test]
fn on_chain_gift_offers_read_by_who_made_them() {
    let offer = |is_buyer: bool, state: &str| {
        format!(
            r#"{{"@type":"messageUpgradedGiftOnChainPurchaseOffer","gift":{{"@type":"upgradedGift","title":"Plush Pepe","number":7}},"is_buyer":{is_buyer},"state":{{"@type":"{state}"}},"offer_address":"EQ","gram_amount":"12500000000","fee_basis_points":500,"expiration_date":1,"offer_message_id":0}}"#
        )
    };
    let incoming = say(
        &offer(false, "giftOnChainPurchaseOfferStatePending"),
        BY_DANA,
    );
    assert!(
        incoming.starts_with("An offer to buy this gift for 12.5"),
        "{incoming}"
    );
    let mine = say(
        &offer(true, "giftOnChainPurchaseOfferStateCanceling"),
        BY_ME,
    );
    assert!(mine.starts_with("You offered 12.5"), "{mine}");
    assert!(mine.ends_with("for Plush Pepe #7."), "{mine}");
    assert_eq!(
        say(&offer(true, "giftOnChainPurchaseOfferStateExpired"), BY_ME),
        "This offer was rejected."
    );
    assert_eq!(
        say(
            &offer(false, "giftOnChainPurchaseOfferStateAccepted"),
            BY_DANA
        ),
        "This offer was accepted."
    );
    // Settled is an accepted offer executed on chain.
    assert_eq!(
        say(&offer(true, "giftOnChainPurchaseOfferStateSettled"), BY_ME),
        "This offer was accepted."
    );
}

#[test]
fn a_gift_bought_on_chain_reads_as_a_sale() {
    let text = say(
        r#"{"@type":"messageUpgradedGift","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":7},"sender_id":{"@type":"messageSenderUser","user_id":1},"receiver_id":{"@type":"messageSenderUser","user_id":9},"origin":{"@type":"upgradedGiftOriginOnChainResale","gram_amount":"40000000000"},"received_gift_id":"","text":{"@type":"formattedText","text":"","entities":[]},"is_private":false,"is_saved":false,"can_be_transferred":false,"was_transferred":false,"transfer_star_count":0,"drop_original_details_star_count":0,"next_transfer_date":0,"next_resale_date":0,"mint_date":0,"craft_date":0}"#,
        BY_DANA,
    );
    assert!(text.contains("40"), "{text}");
}

#[test]
fn a_bot_invite_names_the_inviter() {
    let json = r#"{"@type":"messageBotInvite","inviter_user_id":2}"#;
    assert_eq!(say(json, BY_DANA), "Dana Cole was invited by Omar Haddad.");
    let (content, _) = parse(json);
    let ctx = ServiceCtx {
        actor: Some(MessageSender::User { user_id: 77 }),
        is_outgoing: false,
        my_id: Some(ME),
        is_channel: false,
        is_secret: false,
        chat_id: 77,
        now: 1_800_000_000,
        names: &Names,
    };
    let from_bot = render(&content, &ctx).expect("service").plain();
    assert_eq!(from_bot, "Omar Haddad invited you to this bot.");
}
