//! TDLib updates and answers for payments, Premium, Stars and gifts.
mod parse;

use crate::telegram::envelope::*;
pub(crate) use parse::parse_payments_payload;

/// Payloads for payments, Premium, Stars and gifts; wrapped as
/// [`EnvelopePayload::Payments`].
#[derive(Debug, Clone, PartialEq)]
pub enum PaymentsPayload {
    /// Slice P1: `paymentForm` — the `getPaymentForm` answer after a Buy
    /// button press (schema/td_api.tl:4734).
    PaymentForm(PaymentFormData),
    MarketplaceGift(Option<crate::marketplace::GiftQuote>),
    GiftTextLimit(i64),
    GiftPurchaseResult(crate::marketplace::GiftPurchaseResult),
    /// Slice P1: `validatedOrderInfo` — the `validateOrderInfo` answer
    /// (schema/td_api.tl:4737).
    ValidatedOrderInfo(ValidatedOrderInfoData),
    /// Slice P1: `paymentResult` — the `sendPaymentForm` answer
    /// (schema/td_api.tl:4740).
    PaymentResult(PaymentResultData),
    /// Slice P1: `paymentReceipt` — the `getPaymentReceipt` answer
    /// (schema/td_api.tl:4765).
    PaymentReceipt(PaymentReceiptData),
    /// Slice `parity:bots-payment-recurring`: `starSubscriptions` — the
    /// `getStarSubscriptions` answer (schema/td_api.tl:1269).
    StarSubscriptions(StarSubscriptionsData),
    /// `starTransactions` — the `getStarTransactions` answer.
    StarTransactions(crate::premium_hub::StarTxPage),
    /// `receivedGifts` — the `getReceivedGifts` answer.
    ReceivedGifts(crate::premium_hub::GiftsPage),
    /// `premiumFeatures` — the `getPremiumFeatures` answer.
    PremiumFeatures(crate::premium_hub::PremiumInfo),
    /// `premiumState` — the `getPremiumState` answer.
    PremiumState(crate::premium_hub::PremiumStateInfo),
    /// `bankCardInfo` — the `getBankCardInfo` answer.
    BankCardInfo(BankCardInfoData),
    /// `updateOwnedStarCount`: the signed-in user's Stars balance changed.
    UpdateOwnedStarCount(crate::premium_hub::StarAmount),
}
