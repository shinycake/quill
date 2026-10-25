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
    /// `businessFeatures` — the `getBusinessFeatures` answer.
    BusinessFeatures(Vec<crate::premium_hub::BusinessFeature>),
    /// `bankCardInfo` — the `getBankCardInfo` answer.
    BankCardInfo(BankCardInfoData),
    /// `collectibleItemInfo` — the `getCollectibleItemInfo` answer.
    CollectibleInfo(crate::collectible_info::CollectibleInfo),
    /// `premiumGiftCodeInfo` — the `checkPremiumGiftCode` answer.
    GiftCodeInfo(GiftCodeInfoData),
    /// `giveawayInfoOngoing` / `giveawayInfoCompleted` — the `getGiveawayInfo` answer.
    GiveawayInfo(GiveawayInfoData),
    /// `premiumGiveawayPaymentOptions`.
    PremiumGiveawayOptions(Vec<PremiumGiveawayOption>),
    /// `starGiveawayPaymentOptions`.
    StarGiveawayOptions(Vec<StarGiveawayOption>),
    /// `starPaymentOptions` — the `getStarPaymentOptions` answer.
    StarPackOptions(Vec<crate::star_packs::StarPack>),
    /// `updateOwnedStarCount`: the signed-in user's Stars balance changed.
    UpdateOwnedStarCount(crate::premium_hub::StarAmount),
    /// Answers of gift management and sending (`crate::gift_manage`).
    GiftManage(crate::gift_manage::GiftManagePayload),
    /// The read-only TON wallet's answers and updates (`crate::ton_wallet`).
    TonWallet(crate::ton_wallet::TonWalletPayload),
    /// Grams balance, history and revenue (`crate::premium_hub::ton`).
    Grams(crate::premium_hub::ton::GramsPayload),
    /// The value of an upgraded gift (`crate::premium_hub::gift_value`).
    GiftValue(crate::premium_hub::gift_value::GiftValuePayload),
    /// Revenue statistics and affiliate programs (`crate::earn`).
    Earn(crate::earn::EarnPayload),
    /// Suggested posts in channel direct messages (`crate::suggested_posts`).
    SuggestedPost(crate::suggested_posts::SuggestedPostPayload),
    /// Resale listings, gift auctions and crafting (`crate::gift_market`).
    GiftMarket(crate::gift_market::GiftMarketPayload),
}
