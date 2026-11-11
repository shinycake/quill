//! Generated-once classification of every `GiftMarketPurpose` variant.
use super::RequestKind;
use crate::gift_market::GiftMarketPurpose;

impl GiftMarketPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::SearchResale { .. }
            | Self::AuctionState { .. }
            | Self::AcquiredGifts { .. }
            | Self::UpgradePreview { .. }
            | Self::GiftsForCrafting { .. } => RequestKind::Lookup,
            Self::BuyResale
            | Self::OpenAuction
            | Self::CloseAuction
            | Self::PlaceBid { .. }
            | Self::IncreaseBid { .. }
            | Self::Craft => RequestKind::UserAction,
        }
    }
}
