//! The invoices Quill's checkout can pay: `InputInvoice` in TDLib
//! (`schema/td_api.tl`, `inputInvoiceMessage` / `inputInvoiceName` /
//! `inputInvoiceTelegram`) and the `TelegramPaymentPurpose` behind a
//! payment toward Telegram.
//!
//! Telegram Desktop starts one checkout process per invoice id
//! (`payments/payments_form.h`, `InvoiceId`): a message, a slug from an
//! invoice link, a Premium gift or giveaway, or Stars. TDLib has one
//! `InputInvoice` for all of them, and the same value goes into
//! `getPaymentForm`, `validateOrderInfo` and `sendPaymentForm`, so the
//! checkout keeps it next to the form it fetched.
mod builders;

pub use builders::*;

use crate::ids::{ChatId, MessageId};
use serde_json::{Value, json};

/// A payment toward Telegram (`TelegramPaymentPurpose`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelegramPaymentPurpose {
    /// Gifting Premium to one user (`telegramPaymentPurposePremiumGift`).
    PremiumGift {
        currency: String,
        amount: i64,
        user_id: i64,
        month_count: i32,
        /// Message for the receiver; may be empty.
        text: String,
    },
    /// Premium gift codes for users, boosting a chat
    /// (`telegramPaymentPurposePremiumGiftCodes`).
    PremiumGiftCodes {
        boosted_chat_id: i64,
        currency: String,
        amount: i64,
        user_ids: Vec<i64>,
        month_count: i32,
        text: String,
    },
    /// A new Premium giveaway (`telegramPaymentPurposePremiumGiveaway`).
    /// `parameters` is the `giveawayParameters` object.
    PremiumGiveaway {
        parameters: Value,
        currency: String,
        amount: i64,
        winner_count: i32,
        month_count: i32,
    },
    /// Buying Stars (`telegramPaymentPurposeStars`). `chat_id` is 0 when
    /// no chat is to receive them.
    Stars {
        currency: String,
        amount: i64,
        star_count: i64,
        chat_id: i64,
    },
    /// Buying Stars for another user (`telegramPaymentPurposeGiftedStars`).
    GiftedStars {
        user_id: i64,
        currency: String,
        amount: i64,
        star_count: i64,
    },
    /// A new Stars giveaway (`telegramPaymentPurposeStarGiveaway`).
    StarGiveaway {
        parameters: Value,
        currency: String,
        amount: i64,
        winner_count: i32,
        star_count: i64,
    },
    /// Joining a chat with a Stars subscription
    /// (`telegramPaymentPurposeJoinChat`).
    JoinChat { invite_link: String },
}

/// What a checkout pays (`InputInvoice`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputInvoice {
    /// An invoice message or paid media (`inputInvoiceMessage`).
    Message {
        chat_id: ChatId,
        message_id: MessageId,
    },
    /// An invoice link's slug (`inputInvoiceName`).
    Name(String),
    /// A payment toward Telegram (`inputInvoiceTelegram`).
    Telegram(TelegramPaymentPurpose),
}

fn formatted_text(text: &str) -> Value {
    json!({"@type": "formattedText", "text": text, "entities": []})
}

impl TelegramPaymentPurpose {
    /// The `telegramPaymentPurpose*` object.
    pub fn to_json(&self) -> Value {
        match self {
            Self::PremiumGift {
                currency,
                amount,
                user_id,
                month_count,
                text,
            } => json!({
                "@type": "telegramPaymentPurposePremiumGift",
                "currency": currency,
                "amount": amount,
                "user_id": user_id,
                "month_count": month_count,
                "text": formatted_text(text),
            }),
            Self::PremiumGiftCodes {
                boosted_chat_id,
                currency,
                amount,
                user_ids,
                month_count,
                text,
            } => json!({
                "@type": "telegramPaymentPurposePremiumGiftCodes",
                "boosted_chat_id": boosted_chat_id,
                "currency": currency,
                "amount": amount,
                "user_ids": user_ids,
                "month_count": month_count,
                "text": formatted_text(text),
            }),
            Self::PremiumGiveaway {
                parameters,
                currency,
                amount,
                winner_count,
                month_count,
            } => json!({
                "@type": "telegramPaymentPurposePremiumGiveaway",
                "parameters": parameters,
                "currency": currency,
                "amount": amount,
                "winner_count": winner_count,
                "month_count": month_count,
            }),
            Self::Stars {
                currency,
                amount,
                star_count,
                chat_id,
            } => json!({
                "@type": "telegramPaymentPurposeStars",
                "currency": currency,
                "amount": amount,
                "star_count": star_count,
                "chat_id": chat_id,
            }),
            Self::GiftedStars {
                user_id,
                currency,
                amount,
                star_count,
            } => json!({
                "@type": "telegramPaymentPurposeGiftedStars",
                "user_id": user_id,
                "currency": currency,
                "amount": amount,
                "star_count": star_count,
            }),
            Self::StarGiveaway {
                parameters,
                currency,
                amount,
                winner_count,
                star_count,
            } => json!({
                "@type": "telegramPaymentPurposeStarGiveaway",
                "parameters": parameters,
                "currency": currency,
                "amount": amount,
                "winner_count": winner_count,
                "star_count": star_count,
            }),
            Self::JoinChat { invite_link } => json!({
                "@type": "telegramPaymentPurposeJoinChat",
                "invite_link": invite_link,
            }),
        }
    }
}

impl InputInvoice {
    /// The `inputInvoice*` object for `getPaymentForm`, `validateOrderInfo`
    /// and `sendPaymentForm`.
    pub fn to_json(&self) -> Value {
        match self {
            Self::Message {
                chat_id,
                message_id,
            } => json!({
                "@type": "inputInvoiceMessage",
                "chat_id": chat_id.0,
                "message_id": message_id.0,
            }),
            Self::Name(name) => json!({
                "@type": "inputInvoiceName",
                "name": name,
            }),
            Self::Telegram(purpose) => json!({
                "@type": "inputInvoiceTelegram",
                "purpose": purpose.to_json(),
            }),
        }
    }

    /// The chat and message when this is an invoice in a chat.
    pub fn message(&self) -> Option<(ChatId, MessageId)> {
        match self {
            Self::Message {
                chat_id,
                message_id,
            } => Some((*chat_id, *message_id)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
