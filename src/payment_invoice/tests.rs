use crate::gift_manage::{GiftRecipient, PremiumGiftOption};
use crate::giveaway::{GiveawayDraft, GiveawayKind};
use crate::ids::{ChatId, MessageId, RequestId};
use crate::payment_invoice::{
    InputInvoice, TelegramPaymentPurpose, giveaway_invoice, invoice_link_invoice,
    premium_gift_invoice, stars_pack_invoice,
};
use crate::star_packs::StarPack;
use crate::state::GiveawayOptions;
use crate::telegram::envelope::{
    MessageSender, OrderInfoData, PremiumGiveawayOption, StarGiveawayOption,
};
use crate::telegram::requests::{get_payment_form, send_payment_form, validate_order_info};
use serde_json::{Value, json};

const NOW: i32 = 1_000_000;

fn purpose_json(purpose: TelegramPaymentPurpose) -> Value {
    InputInvoice::Telegram(purpose).to_json()
}

#[test]
fn message_and_name_invoices_match_the_schema() {
    let message = InputInvoice::Message {
        chat_id: ChatId(-100),
        message_id: MessageId(7),
    };
    assert_eq!(
        message.to_json(),
        json!({"@type": "inputInvoiceMessage", "chat_id": -100, "message_id": 7})
    );
    assert_eq!(message.message(), Some((ChatId(-100), MessageId(7))));
    let name = InputInvoice::Name("abc_DEF-1".into());
    assert_eq!(
        name.to_json(),
        json!({"@type": "inputInvoiceName", "name": "abc_DEF-1"})
    );
    assert_eq!(name.message(), None);
}

#[test]
fn every_payment_purpose_serializes_with_its_schema_fields() {
    let params = json!({"@type": "giveawayParameters", "boosted_chat_id": -1001});
    let cases = [
        (
            TelegramPaymentPurpose::PremiumGift {
                currency: "USD".into(),
                amount: 999,
                user_id: 42,
                month_count: 12,
                text: "Happy birthday".into(),
            },
            json!({
                "@type": "telegramPaymentPurposePremiumGift",
                "currency": "USD", "amount": 999, "user_id": 42, "month_count": 12,
                "text": {"@type": "formattedText", "text": "Happy birthday", "entities": []},
            }),
        ),
        (
            TelegramPaymentPurpose::PremiumGiftCodes {
                boosted_chat_id: -1001,
                currency: "EUR".into(),
                amount: 2000,
                user_ids: vec![1, 2],
                month_count: 3,
                text: String::new(),
            },
            json!({
                "@type": "telegramPaymentPurposePremiumGiftCodes",
                "boosted_chat_id": -1001, "currency": "EUR", "amount": 2000,
                "user_ids": [1, 2], "month_count": 3,
                "text": {"@type": "formattedText", "text": "", "entities": []},
            }),
        ),
        (
            TelegramPaymentPurpose::PremiumGiveaway {
                parameters: params.clone(),
                currency: "USD".into(),
                amount: 1500,
                winner_count: 5,
                month_count: 3,
            },
            json!({
                "@type": "telegramPaymentPurposePremiumGiveaway",
                "parameters": params, "currency": "USD", "amount": 1500,
                "winner_count": 5, "month_count": 3,
            }),
        ),
        (
            TelegramPaymentPurpose::Stars {
                currency: "USD".into(),
                amount: 199,
                star_count: 100,
                chat_id: 0,
            },
            json!({
                "@type": "telegramPaymentPurposeStars",
                "currency": "USD", "amount": 199, "star_count": 100, "chat_id": 0,
            }),
        ),
        (
            TelegramPaymentPurpose::GiftedStars {
                user_id: 9,
                currency: "USD".into(),
                amount: 199,
                star_count: 100,
            },
            json!({
                "@type": "telegramPaymentPurposeGiftedStars",
                "user_id": 9, "currency": "USD", "amount": 199, "star_count": 100,
            }),
        ),
        (
            TelegramPaymentPurpose::StarGiveaway {
                parameters: params.clone(),
                currency: "USD".into(),
                amount: 100,
                winner_count: 10,
                star_count: 1000,
            },
            json!({
                "@type": "telegramPaymentPurposeStarGiveaway",
                "parameters": params, "currency": "USD", "amount": 100,
                "winner_count": 10, "star_count": 1000,
            }),
        ),
        (
            TelegramPaymentPurpose::JoinChat {
                invite_link: "https://t.me/+abc".into(),
            },
            json!({
                "@type": "telegramPaymentPurposeJoinChat",
                "invite_link": "https://t.me/+abc",
            }),
        ),
    ];
    for (purpose, expected) in cases {
        assert_eq!(
            purpose_json(purpose),
            json!({"@type": "inputInvoiceTelegram", "purpose": expected})
        );
    }
}

#[test]
fn every_checkout_request_carries_the_same_invoice() {
    let invoice = InputInvoice::Name("slug".into());
    let expected = json!({"@type": "inputInvoiceName", "name": "slug"});
    let form: Value = serde_json::from_str(&get_payment_form(RequestId(1), &invoice)).unwrap();
    assert_eq!(form["@type"], "getPaymentForm");
    assert_eq!(form["input_invoice"], expected);
    let validate: Value = serde_json::from_str(&validate_order_info(
        RequestId(2),
        &invoice,
        &OrderInfoData::default(),
        false,
    ))
    .unwrap();
    assert_eq!(validate["input_invoice"], expected);
    // A Stars payment sends no credentials.
    let send: Value = serde_json::from_str(&send_payment_form(
        RequestId(3),
        &invoice,
        5,
        "",
        "",
        Value::Null,
        0,
    ))
    .unwrap();
    assert_eq!(send["input_invoice"], expected);
    assert_eq!(send["credentials"], Value::Null);
    assert_eq!(send["payment_form_id"], 5);
}

fn pack() -> StarPack {
    StarPack {
        currency: "USD".into(),
        amount: 199,
        star_count: 100,
        is_additional: false,
    }
}

#[test]
fn a_stars_pack_buys_stars_for_no_chat() {
    assert_eq!(
        stars_pack_invoice(&pack()),
        InputInvoice::Telegram(TelegramPaymentPurpose::Stars {
            currency: "USD".into(),
            amount: 199,
            star_count: 100,
            chat_id: 0,
        })
    );
}

fn gift_option() -> PremiumGiftOption {
    PremiumGiftOption {
        month_count: 6,
        currency: "USD".into(),
        amount: 1999,
        star_count: 1000,
        discount_percentage: 10,
    }
}

fn recipient(sender: MessageSender, is_self: bool) -> GiftRecipient {
    GiftRecipient {
        sender,
        name: "Ada".into(),
        is_self,
        is_channel: false,
    }
}

#[test]
fn a_premium_gift_goes_to_the_user_with_a_cut_message() {
    let user = recipient(MessageSender::User { user_id: 77 }, false);
    let invoice = premium_gift_invoice(&gift_option(), &user, "  Enjoy it  ", 5).unwrap();
    assert_eq!(
        invoice,
        InputInvoice::Telegram(TelegramPaymentPurpose::PremiumGift {
            currency: "USD".into(),
            amount: 1999,
            user_id: 77,
            month_count: 6,
            text: "Enjoy".into(),
        })
    );
}

#[test]
fn premium_cannot_be_gifted_to_a_channel_or_to_yourself() {
    let channel = recipient(MessageSender::Chat { chat_id: -1001 }, false);
    assert_eq!(premium_gift_invoice(&gift_option(), &channel, "", 0), None);
    let me = recipient(MessageSender::User { user_id: 1 }, true);
    assert_eq!(premium_gift_invoice(&gift_option(), &me, "", 0), None);
    let user = recipient(MessageSender::User { user_id: 2 }, false);
    let mut free = gift_option();
    free.amount = 0;
    assert_eq!(premium_gift_invoice(&free, &user, "", 0), None);
}

fn options() -> GiveawayOptions {
    GiveawayOptions {
        premium: vec![
            PremiumGiveawayOption {
                currency: "USD".into(),
                amount: 1500,
                winner_count: 5,
                month_count: 3,
            },
            PremiumGiveawayOption {
                currency: "USD".into(),
                amount: 2500,
                winner_count: 5,
                month_count: 6,
            },
        ],
        stars: vec![StarGiveawayOption {
            currency: "USD".into(),
            amount: 100,
            star_count: 1000,
            yearly_boost_count: 6,
            winner_counts: vec![10, 20],
            is_default: true,
            is_additional: false,
        }],
        ..Default::default()
    }
}

#[test]
fn a_new_premium_giveaway_pays_the_chosen_option() {
    let mut draft = GiveawayDraft::new(-1001, None, NOW);
    draft.premium_winners = 5;
    draft.premium_months = 6;
    let invoice = giveaway_invoice(&draft, &options()).unwrap();
    assert_eq!(
        invoice,
        InputInvoice::Telegram(TelegramPaymentPurpose::PremiumGiveaway {
            parameters: draft.parameters_json(),
            currency: "USD".into(),
            amount: 2500,
            winner_count: 5,
            month_count: 6,
        })
    );
    let json = invoice.to_json();
    assert_eq!(json["purpose"]["parameters"]["@type"], "giveawayParameters");
    assert_eq!(json["purpose"]["parameters"]["boosted_chat_id"], -1001);
}

#[test]
fn a_new_stars_giveaway_pays_the_chosen_option() {
    let mut draft = GiveawayDraft::new(-1001, None, NOW);
    draft.kind = GiveawayKind::Stars;
    draft.star_option = 0;
    draft.star_winners = 20;
    let invoice = giveaway_invoice(&draft, &options()).unwrap();
    assert_eq!(
        invoice,
        InputInvoice::Telegram(TelegramPaymentPurpose::StarGiveaway {
            parameters: draft.parameters_json(),
            currency: "USD".into(),
            amount: 100,
            winner_count: 20,
            star_count: 1000,
        })
    );
}

#[test]
fn a_giveaway_choice_that_is_not_offered_makes_no_invoice() {
    let mut draft = GiveawayDraft::new(-1001, None, NOW);
    draft.premium_winners = 7;
    draft.premium_months = 6;
    assert_eq!(giveaway_invoice(&draft, &options()), None);
    draft.kind = GiveawayKind::Stars;
    draft.star_option = 0;
    draft.star_winners = 3;
    assert_eq!(giveaway_invoice(&draft, &options()), None);
    draft.star_option = 4;
    assert_eq!(giveaway_invoice(&draft, &options()), None);
}

#[test]
fn an_invoice_link_uses_its_slug_and_a_nameless_one_is_broken() {
    assert_eq!(
        invoice_link_invoice(" slug "),
        Some(InputInvoice::Name("slug".into()))
    );
    assert_eq!(invoice_link_invoice("  "), None);
}

#[test]
fn gifted_stars_name_the_user_and_the_pack() {
    let invoice = crate::payment_invoice::gifted_stars_invoice(&pack(), 9).unwrap();
    assert_eq!(
        invoice.to_json()["purpose"],
        json!({
            "@type": "telegramPaymentPurposeGiftedStars",
            "user_id": 9, "currency": "USD", "amount": 199, "star_count": 100,
        })
    );
    assert_eq!(
        crate::payment_invoice::gifted_stars_invoice(&pack(), 0),
        None
    );
}

#[test]
fn a_premium_gift_paid_with_stars_uses_the_stars_currency() {
    let user = recipient(MessageSender::User { user_id: 77 }, false);
    let invoice =
        crate::payment_invoice::premium_gift_stars_invoice(&gift_option(), &user, "Hi", 10)
            .unwrap();
    let purpose = invoice.to_json()["purpose"].clone();
    assert_eq!(purpose["currency"], "XTR");
    assert_eq!(purpose["amount"], 1000);
    assert_eq!(purpose["month_count"], 6);
    assert_eq!(purpose["user_id"], 77);
    let mut no_stars = gift_option();
    no_stars.star_count = 0;
    assert_eq!(
        crate::payment_invoice::premium_gift_stars_invoice(&no_stars, &user, "", 0),
        None
    );
}

#[test]
fn chosen_members_get_gift_codes_priced_per_member() {
    let mut opts = options();
    opts.premium.push(PremiumGiveawayOption {
        currency: "USD".into(),
        amount: 400,
        winner_count: 1,
        month_count: 3,
    });
    let mut draft = GiveawayDraft::new(-1001, None, NOW);
    draft.premium_months = 3;
    draft.award_user_ids = vec![11, 12, 13];
    let invoice = giveaway_invoice(&draft, &opts).unwrap();
    assert_eq!(
        invoice,
        InputInvoice::Telegram(TelegramPaymentPurpose::PremiumGiftCodes {
            boosted_chat_id: -1001,
            currency: "USD".into(),
            amount: 1200,
            user_ids: vec![11, 12, 13],
            month_count: 3,
            text: String::new(),
        })
    );
    // Without members it is a random giveaway again.
    draft.award_user_ids.clear();
    draft.premium_winners = 5;
    assert!(matches!(
        giveaway_invoice(&draft, &opts),
        Some(InputInvoice::Telegram(
            TelegramPaymentPurpose::PremiumGiveaway { .. }
        ))
    ));
}

#[test]
fn a_gift_code_for_an_unoffered_duration_makes_no_invoice() {
    let mut draft = GiveawayDraft::new(-1001, None, NOW);
    draft.premium_months = 12;
    draft.award_user_ids = vec![1];
    assert_eq!(giveaway_invoice(&draft, &options()), None);
}
