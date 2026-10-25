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
    /// `getReceivedGifts` filtered to gifts with wearable colors, for the
    /// color editor's collectible row.
    GetWearableGifts,
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
    /// `getBusinessFeatures`: the Business screen's rows and preview order.
    GetBusinessFeatures,
    /// `getPremiumStickerExamples`. Response is `stickers`, kept in the
    /// Premium hub for the stickers feature preview.
    GetPremiumStickerExamples,
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
    /// `getCollectibleItemInfo` for a tapped collectible username or phone
    /// number. Response is `collectibleItemInfo`.
    GetCollectibleInfo,
    /// `getUpgradedGiftValueInfo` for the unique gift box's Value row.
    /// Response is `upgradedGiftValueInfo`.
    GetGiftValueInfo,
    /// `getApplicationConfig` for `ton_blockchain_explorer_url`, asked
    /// when a TON address menu opens. Response is `jsonValueObject`.
    GetTonExplorerUrl,
    /// A read-only TON wallet request (`crate::ton_wallet`).
    TonWallet(crate::ton_wallet::TonWalletPurpose),
    /// Settings → My Grams (`crate::premium_hub::ton`).
    Grams(crate::premium_hub::ton::GramsPurpose),
    /// `checkPremiumGiftCode` for a gift code link. Response is
    /// `premiumGiftCodeInfo`.
    CheckPremiumGiftCode,
    /// `applyPremiumGiftCode`, sent only on the Apply button. Response is `ok`.
    ApplyPremiumGiftCode,
    /// `getGiveawayInfo` for the "About this giveaway" box. Response is
    /// `giveawayInfoOngoing` or `giveawayInfoCompleted`.
    GetGiveawayInfo,
    /// `getPremiumGiveawayPaymentOptions`. Response is
    /// `premiumGiveawayPaymentOptions`.
    GetPremiumGiveawayOptions,
    /// `getStarPaymentOptions` for "Buy more Stars". Response is
    /// `starPaymentOptions`.
    GetStarPackOptions,
    /// `getStarGiftPaymentOptions` for "Gift Stars". Response is
    /// `starPaymentOptions`.
    GetStarGiftOptions,
    /// `addPendingPaidMessageReaction` for a Star reaction tap; the Stars
    /// are rolled back if TDLib refuses.
    AddPendingPaidReaction {
        message_id: i64,
        stars: i64,
    },
    /// `commitPendingPaidMessageReactions` once the toast ran out.
    CommitPaidReaction {
        message_id: i64,
    },
    /// `removePendingPaidMessageReactions`: the toast's Undo.
    RemovePendingPaidReaction {
        message_id: i64,
    },
    /// `getChatAvailablePaidMessageReactionSenders`. Response is
    /// `messageSenders`.
    GetPaidReactionSenders,
    /// `getStarGiveawayPaymentOptions`. Response is
    /// `starGiveawayPaymentOptions`.
    GetStarGiveawayOptions,
    /// `launchPrepaidGiveaway`, sent only after a confirmation. Response is `ok`.
    LaunchPrepaidGiveaway,
    /// `getOption` for one of the gift limits. Response is `optionValueInteger`.
    GetGiftOption(crate::gift_manage::GiftOption),
    /// `setPinnedGifts` after a Pin or Unpin tap. Response is `ok`.
    SetPinnedGifts,
    /// `upgradeGift`, sent only after the confirmation. Response is
    /// `upgradeGiftResult`.
    UpgradeGift,
    /// `transferGift`, sent only after the confirmation. Response is `ok`.
    TransferGift,
    /// `createTemporaryPassword` from the password box of "Send via
    /// Blockchain" (TDLib 1.8.69 withdraws with a temporary password).
    /// Response is `temporaryPasswordState`.
    GiftExportTempPassword,
    /// `getUpgradedGiftWithdrawalUrl`, sent once the temporary password
    /// exists. Response is `httpUrl`.
    GiftExportUrl,
    /// The password-less `getUpgradedGiftWithdrawalUrl` sent on "Open
    /// Fragment" (tdesktop `inputCheckPasswordEmpty`): its error picks the
    /// next step; an `httpUrl` answer is dropped, never opened.
    GiftExportProbe,
    /// `setGiftResalePrice`. Response is `ok`.
    SetGiftResalePrice,
    /// `getGiftCollections`. Response is `giftCollections`.
    GetGiftCollections,
    /// `createGiftCollection`, `setGiftCollectionName`,
    /// `addGiftCollectionGifts` and `removeGiftCollectionGifts` answer with
    /// `giftCollection`; `deleteGiftCollection` with `ok`.
    ChangeGiftCollection,
    DeleteGiftCollection,
    /// `deleteGift`. Response is `ok`.
    DeleteGift,
    /// `getAvailableGifts` for the send-gift dialog.
    GetAvailableGifts,
    /// `getPremiumGiftPaymentOptions` for the send-gift dialog.
    GetPremiumGiftOptions,
    /// `sendGift`: spends Stars, sent only on the send button.
    SendStarGift,
    /// `getSupergroupMembers` for a page of the "Choose members" list;
    /// `generation` tells an older search from the current one.
    GetAwardMembers {
        generation: u64,
    },
    /// Revenue screens and affiliate programs (`crate::earn`).
    Earn(crate::earn::EarnPurpose),
    /// Suggested posts in channel direct messages
    /// (`crate::suggested_posts`).
    SuggestedPost(crate::suggested_posts::SuggestedPostPurpose),
    /// Resale listings, gift auctions and crafting (`crate::gift_market`).
    GiftMarket(crate::gift_market::GiftMarketPurpose),
}

impl PaymentsPurpose {
    /// Requests of the gift management feature; their failures are handled
    /// in `gift_manage.rs`.
    pub fn is_gift_manage(self) -> bool {
        matches!(
            self,
            Self::GetGiftOption(_)
                | Self::SetPinnedGifts
                | Self::UpgradeGift
                | Self::TransferGift
                | Self::GiftExportTempPassword
                | Self::GiftExportUrl
                | Self::GiftExportProbe
                | Self::SetGiftResalePrice
                | Self::GetGiftCollections
                | Self::ChangeGiftCollection
                | Self::DeleteGiftCollection
                | Self::DeleteGift
                | Self::GetAvailableGifts
                | Self::GetPremiumGiftOptions
                | Self::SendStarGift
        )
    }
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
    GetBusinessFeatures,
    GetPremiumStickerExamples,
    DeleteSavedOrderInfo,
    DeleteSavedCredentials,
    GetBankCardInfo,
    GetCollectibleInfo,
    GetGiftValueInfo,
    GetTonExplorerUrl,
    CheckPremiumGiftCode,
    ApplyPremiumGiftCode,
    GetGiveawayInfo,
    GetPremiumGiveawayOptions,
    GetStarGiveawayOptions,
    LaunchPrepaidGiveaway,
    GetStarPackOptions,
    GetStarGiftOptions,
    GetPaidReactionSenders,
    SetPinnedGifts,
    UpgradeGift,
    TransferGift,
    GiftExportTempPassword,
    GiftExportUrl,
    GiftExportProbe,
    SetGiftResalePrice,
    GetGiftCollections,
    ChangeGiftCollection,
    DeleteGiftCollection,
    DeleteGift,
    GetAvailableGifts,
    GetPremiumGiftOptions,
    SendStarGift,
});
