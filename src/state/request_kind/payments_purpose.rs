//! Generated-once classification of every `PaymentsPurpose` variant.
use super::RequestKind;
use crate::state::PaymentsPurpose;

impl PaymentsPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::TonWallet(inner) => inner.kind(),
            Self::Grams(inner) => inner.kind(),
            Self::Earn(inner) => inner.kind(),
            Self::SuggestedPost(inner) => inner.kind(),
            Self::GiftMarket(inner) => inner.kind(),
            Self::GetPaymentForm
            | Self::GetMarketplaceGift
            | Self::GetGiftTextLimit
            | Self::GetPaymentReceipt
            | Self::GetStarSubscriptions { .. }
            | Self::GetStarTransactions { .. }
            | Self::GetReceivedGifts { .. }
            | Self::GetWearableGifts
            | Self::GetPremiumFeatures
            | Self::GetPremiumState
            | Self::GetBusinessFeatures
            | Self::GetPremiumStickerExamples
            | Self::GetBankCardInfo
            | Self::GetCollectibleInfo
            | Self::GetGiftValueInfo
            | Self::GetTonExplorerUrl
            | Self::GetGiveawayInfo
            | Self::GetPremiumGiveawayOptions
            | Self::GetStarPackOptions
            | Self::GetStarGiftOptions
            | Self::GetPaidReactionSenders
            | Self::GetStarGiveawayOptions
            | Self::GetGiftOption(..)
            | Self::GiftExportProbe
            | Self::GetGiftCollections { .. }
            | Self::GetAvailableGifts
            | Self::GetPremiumGiftOptions
            | Self::GetAwardMembers { .. } => RequestKind::Lookup,
            Self::SendMarketplaceGift
            | Self::ValidateOrderInfo
            | Self::SendPaymentForm
            | Self::EditStarSubscription
            | Self::ReuseStarSubscription
            | Self::ToggleGiftSaved { .. }
            | Self::SellGift
            | Self::DeleteSavedOrderInfo
            | Self::DeleteSavedCredentials
            | Self::CheckPremiumGiftCode
            | Self::ApplyPremiumGiftCode
            | Self::AddPendingPaidReaction { .. }
            | Self::CommitPaidReaction { .. }
            | Self::RemovePendingPaidReaction { .. }
            | Self::LaunchPrepaidGiveaway
            | Self::SetPinnedGifts
            | Self::UpgradeGift
            | Self::TransferGift
            | Self::GiftExportTempPassword
            | Self::GiftExportUrl
            | Self::SetGiftResalePrice
            | Self::ChangeGiftCollection
            | Self::DeleteGiftCollection
            | Self::DeleteGift
            | Self::SendStarGift => RequestKind::UserAction,
        }
    }
}
