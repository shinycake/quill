//! The invoice each place that sells something builds: a Stars pack, a
//! Premium gift and a new giveaway (tdesktop `settings_credits_graphics.cpp`,
//! `star_gift_box.cpp` `SendGift`, `create_giveaway_box.cpp`). Pure, so the
//! routing from each entry point is tested without a window.
use super::{InputInvoice, TelegramPaymentPurpose};
use crate::gift_manage::{GiftRecipient, PremiumGiftOption};
use crate::giveaway::{GiveawayDraft, GiveawayKind, gift_code_price};
use crate::star_packs::StarPack;
use crate::state::GiveawayOptions;
use crate::telegram::envelope::MessageSender;

/// "Buy more Stars": `telegramPaymentPurposeStars`. No chat is to receive
/// the Stars, so `chat_id` is 0.
pub fn stars_pack_invoice(pack: &StarPack) -> InputInvoice {
    InputInvoice::Telegram(TelegramPaymentPurpose::Stars {
        currency: pack.currency.clone(),
        amount: pack.amount,
        star_count: pack.star_count,
        chat_id: 0,
    })
}

/// The currency Telegram uses for a payment made with Stars.
pub const STARS_CURRENCY: &str = "XTR";

/// A Premium gift option in the send-gift dialog. Only a user who is not
/// the account itself can get Premium; `text` is cut to `text_limit`
/// characters (`gift_text_length_max`, 0 when unknown).
pub fn premium_gift_invoice(
    option: &PremiumGiftOption,
    recipient: &GiftRecipient,
    text: &str,
    text_limit: usize,
) -> Option<InputInvoice> {
    premium_gift(option, recipient, text, text_limit, false)
}

/// The same gift paid with Stars (tdesktop's "Pay with Stars" switch, shown
/// when the option has a Stars price). The purpose names the Stars
/// currency and the Stars price (`telegramPaymentPurposePremiumGift`
/// documents `XTR`).
pub fn premium_gift_stars_invoice(
    option: &PremiumGiftOption,
    recipient: &GiftRecipient,
    text: &str,
    text_limit: usize,
) -> Option<InputInvoice> {
    premium_gift(option, recipient, text, text_limit, true)
}

fn premium_gift(
    option: &PremiumGiftOption,
    recipient: &GiftRecipient,
    text: &str,
    text_limit: usize,
    by_stars: bool,
) -> Option<InputInvoice> {
    let MessageSender::User { user_id } = recipient.sender else {
        return None;
    };
    let (currency, amount) = if by_stars {
        (STARS_CURRENCY.to_string(), option.star_count)
    } else {
        (option.currency.clone(), option.amount)
    };
    if recipient.is_self || option.month_count <= 0 || amount <= 0 {
        return None;
    }
    let text: String = text.trim().chars().take(text_limit).collect();
    Some(InputInvoice::Telegram(
        TelegramPaymentPurpose::PremiumGift {
            currency,
            amount,
            user_id,
            month_count: option.month_count,
            text,
        },
    ))
}

/// "Gift Stars": a pack for another user, `telegramPaymentPurposeGiftedStars`.
pub fn gifted_stars_invoice(pack: &StarPack, user_id: i64) -> Option<InputInvoice> {
    (user_id > 0 && pack.star_count > 0 && pack.amount > 0).then(|| {
        InputInvoice::Telegram(TelegramPaymentPurpose::GiftedStars {
            user_id,
            currency: pack.currency.clone(),
            amount: pack.amount,
            star_count: pack.star_count,
        })
    })
}

/// "Start Giveaway" on a new giveaway: the chosen payment option as a
/// Premium or Stars giveaway purchase. `None` for a prepaid giveaway
/// (`launchPrepaidGiveaway` starts that one) or when the draft's choice is
/// not among the offered options.
pub fn giveaway_invoice(draft: &GiveawayDraft, options: &GiveawayOptions) -> Option<InputInvoice> {
    if draft.prepaid.is_some() {
        return None;
    }
    if draft.is_specific_users() {
        return gift_codes_invoice(draft, options);
    }
    let parameters = draft.parameters_json();
    let purpose = match draft.kind {
        GiveawayKind::Premium => {
            let option = options.premium.iter().find(|o| {
                o.winner_count == draft.premium_winners && o.month_count == draft.premium_months
            })?;
            TelegramPaymentPurpose::PremiumGiveaway {
                parameters,
                currency: option.currency.clone(),
                amount: option.amount,
                winner_count: option.winner_count,
                month_count: option.month_count,
            }
        }
        GiveawayKind::Stars => {
            let option = options.stars.get(draft.star_option)?;
            if !option.winner_counts.contains(&draft.star_winners) {
                return None;
            }
            TelegramPaymentPurpose::StarGiveaway {
                parameters,
                currency: option.currency.clone(),
                amount: option.amount,
                winner_count: draft.star_winners,
                star_count: option.star_count,
            }
        }
    };
    Some(InputInvoice::Telegram(purpose))
}

/// An invoice link's slug (`internalLinkTypeInvoice`). A link without a
/// name is broken.
pub fn invoice_link_invoice(name: &str) -> Option<InputInvoice> {
    let name = name.trim();
    (!name.is_empty()).then(|| InputInvoice::Name(name.to_string()))
}

/// Gift codes for chosen members (`telegramPaymentPurposePremiumGiftCodes`):
/// one code per member, boosting the giveaway's chat.
fn gift_codes_invoice(draft: &GiveawayDraft, options: &GiveawayOptions) -> Option<InputInvoice> {
    let (currency, amount) = gift_code_price(
        &options.premium,
        draft.premium_months,
        draft.award_user_ids.len(),
    )?;
    Some(InputInvoice::Telegram(
        TelegramPaymentPurpose::PremiumGiftCodes {
            boosted_chat_id: draft.boosted_chat_id,
            currency,
            amount,
            user_ids: draft.award_user_ids.clone(),
            month_count: draft.premium_months,
            text: String::new(),
        },
    ))
}
