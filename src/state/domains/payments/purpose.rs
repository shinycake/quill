//! Request purposes for payments, Premium, Stars and gifts.
use crate::state::request_purpose::flat_purposes;

/// In-flight requests for payments, Premium, Stars and gifts; wrapped as
/// [`RequestPurpose::Payments`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentsPurpose {
    /// Slice P1: `getPaymentForm` after a Buy button press. Response is
    /// `paymentForm` (schema 1.8.67, line 15262).
    GetPaymentForm,
    GetMarketplaceGift,
    GetGiftTextLimit,
    SendMarketplaceGift,
    /// Slice P1: `validateOrderInfo` after the order-info form validates.
    /// Response is `validatedOrderInfo` (schema 1.8.67, line 15268).
    ValidateOrderInfo,
    /// Slice P1: `sendPaymentForm` from the checkout dialog. Response is
    /// `paymentResult` (schema 1.8.67, line 15277).
    SendPaymentForm,
    /// Slice P1: `getPaymentReceipt` for an invoice's
    /// `receipt_message_id`. Response is `paymentReceipt` (schema 1.8.67,
    /// line 15280).
    GetPaymentReceipt,
    /// Slice `parity:bots-payment-recurring`: `getStarSubscriptions`.
    /// Response is `starSubscriptions` (schema 1.8.67, line 16075).
    /// `append` = this is a follow-up page (offset was non-empty).
    GetStarSubscriptions {
        append: bool,
    },
    /// Slice `parity:bots-payment-recurring`: `editStarSubscription`
    /// (cancel / re-enable). Response is `ok` (schema 1.8.67, line 16086).
    EditStarSubscription,
    /// Slice `parity:bots-payment-recurring`: `reuseStarSubscription`
    /// (rejoin an expired channel subscription). Response is `ok`
    /// (schema 1.8.67, line 16095).
    ReuseStarSubscription,
    /// `parity:premium-stars-balance`: `getStarTransactions`; `append` =
    /// follow-up page of the same filter.
    GetStarTransactions {
        append: bool,
    },
    /// `parity:premium-received-gifts`: `getReceivedGifts`.
    GetReceivedGifts {
        append: bool,
    },
    /// `toggleGiftIsSaved` for the gift in `PremiumHub::gift_selected`.
    ToggleGiftSaved {
        saved: bool,
    },
    /// `sellGift` (convert to Stars) for the confirmed gift.
    SellGift,
    /// `parity:premium-promo-page`: `getPremiumFeatures`.
    GetPremiumFeatures,
    /// `getPremiumState`.
    GetPremiumState,
    /// Slice payments: `deleteSavedOrderInfo` (schema 1.8.67, line
    /// 15286). Response is `ok`; the saved info lives server-side, so
    /// there is no local state to invalidate — the `ok` just retires
    /// the pending request.
    DeleteSavedOrderInfo,
    /// Slice payments: `deleteSavedCredentials` (schema 1.8.67, line
    /// 15289). Response is `ok`; same no-local-state treatment as
    /// `DeleteSavedOrderInfo`.
    DeleteSavedCredentials,
    /// `getBankCardInfo` for a tapped card number. Response is
    /// `bankCardInfo`.
    GetBankCardInfo,
}

flat_purposes!(Payments(PaymentsPurpose) {
    GetPaymentForm,
    GetMarketplaceGift,
    GetGiftTextLimit,
    SendMarketplaceGift,
    ValidateOrderInfo,
    SendPaymentForm,
    GetPaymentReceipt,
    EditStarSubscription,
    ReuseStarSubscription,
    SellGift,
    GetPremiumFeatures,
    GetPremiumState,
    DeleteSavedOrderInfo,
    DeleteSavedCredentials,
    GetBankCardInfo,
});
