//! The gift market's open boxes (`gift_market_ui.rs`): a stack, so Escape
//! and "Back" return to the box below, like tdesktop's layered boxes. The
//! data itself (listings, auction states, bids) lives in the session
//! (`quill::gift_market::GiftMarketState`).
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::input::InputState;
use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use quill::gift_manage::GiftRecipient;

/// One box of the gift market.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MarketBox {
    /// The listings of one collection with the filter pills
    /// (`session.payments.gift_market.resale`).
    Resale,
    /// Buying the listing in `session.payments.gift_market.buy`.
    Buy,
    /// "Auction": how auctions work, shown once.
    AuctionAbout { gift_id: i64 },
    /// The auction's dates, rounds and availability.
    AuctionInfo { gift_id: i64 },
    /// "Send a Gift" for an auction gift: message and "Hide My Name".
    AuctionSend { gift_id: i64 },
    /// "Place a Bid": the slider, the places and the top winners.
    AuctionBid { gift_id: i64 },
    /// "{n} Items Bought".
    AuctionBought { gift_id: i64 },
    /// "Change Recipient": the bid is for someone else.
    ChangeRecipient { gift_id: i64 },
    /// "{n} Active Auctions" from the chat-list bar.
    ActiveAuctions,
    /// The models a collectible of this gift can get.
    Variants { gift_id: i64 },
}

impl MarketBox {
    /// The auction this box shows, to follow its updates.
    pub(super) fn auction(&self) -> Option<i64> {
        match self {
            Self::AuctionInfo { gift_id }
            | Self::AuctionSend { gift_id }
            | Self::AuctionBid { gift_id }
            | Self::AuctionBought { gift_id }
            | Self::ChangeRecipient { gift_id } => Some(*gift_id),
            _ => None,
        }
    }
}

/// The bid slider of the open bid box.
pub(crate) struct BidSliderUi {
    pub(super) gift_id: i64,
    pub(super) state: Entity<SliderState>,
    /// The amount the slider (or the custom field) chose.
    pub(super) chosen: i64,
    /// The `(min, max)` the track was built for; rebuilt when a new bid
    /// moves them.
    pub(super) bounds: (i64, i64),
}

/// "Custom Amount" for a bid.
pub(crate) struct CustomBidUi {
    pub(super) input: Entity<InputState>,
    pub(super) error: Option<String>,
}

pub(crate) struct GiftMarketUi {
    pub(super) stack: Vec<MarketBox>,
    /// Who an auction bid is for: the send-gift box's recipient, or the
    /// user.
    pub(super) recipient: Option<GiftRecipient>,
    pub(super) hide_name: bool,
    pub(super) slider: Option<BidSliderUi>,
    pub(super) custom: Option<CustomBidUi>,
    /// A tile or link asked for this auction; its box opens when the
    /// state arrives.
    pub(super) pending_auction: bool,
    /// The sticker files last asked for; downloads go out only when the
    /// set changes, not every frame.
    pub(super) requested_files: Vec<quill::ids::FileId>,
    /// How often the chat-list bar opened its box (one tap, one open).
    #[cfg(test)]
    pub(super) bar_opens: usize,
}

impl GiftMarketUi {
    pub(super) fn new() -> Self {
        Self {
            stack: Vec::new(),
            recipient: None,
            hide_name: false,
            slider: None,
            custom: None,
            pending_auction: false,
            requested_files: Vec::new(),
            #[cfg(test)]
            bar_opens: 0,
        }
    }

    pub(super) fn top(&self) -> Option<&MarketBox> {
        self.stack.last()
    }

    /// Opens `next` above the current box (or replaces an equal top).
    pub(super) fn push(&mut self, next: MarketBox) {
        if self.stack.last() != Some(&next) {
            self.stack.push(next);
        }
    }
}
