//! The invoice each place that sells something builds: a Stars pack, a
//! Premium gift and a new giveaway (tdesktop `settings_credits_graphics.cpp`,
//! `star_gift_box.cpp` `SendGift`, `create_giveaway_box.cpp`). Pure, so the
//! routing from each entry point is tested without a window.
use super::{InputInvoice, TelegramPaymentPurpose};
use crate::gift_manage::{GiftRecipient, PremiumGiftOption};
use crate::giveaway::{GiveawayDraft, GiveawayKind};
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

/// A Premium gift option in the send-gift dialog. Only a user who is not
/// the account itself can get Premium; `text` is cut to `text_limit`
/// characters (`gift_text_length_max`, 0 when unknown).
pub fn premium_gift_invoice(
    option: &PremiumGiftOption,
    recipient: &GiftRecipient,
    text: &str,
    text_limit: usize,
) -> Option<InputInvoice> {
    let MessageSender::User { user_id } = recipient.sender else {
        return None;
    };
    if recipient.is_self || option.month_count <= 0 || option.amount <= 0 {
        return None;
    }
    let text: String = text.trim().chars().take(text_limit).collect();
    Some(InputInvoice::Telegram(
        TelegramPaymentPurpose::PremiumGift {
            currency: option.currency.clone(),
            amount: option.amount,
            user_id,
            month_count: option.month_count,
            text,
        },
    ))
}

/// "Start Giveaway" on a new giveaway: the chosen payment option as a
/// Premium or Stars giveaway purchase. `None` for a prepaid giveaway
/// (`launchPrepaidGiveaway` starts that one) or when the draft's choice is
/// not among the offered options.
pub fn giveaway_invoice(draft: &GiveawayDraft, options: &GiveawayOptions) -> Option<InputInvoice> {
    if draft.prepaid.is_some() {
        return None;
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
