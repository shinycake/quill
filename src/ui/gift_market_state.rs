//! The gift market's open boxes (`gift_market_ui.rs`): a stack, so Escape
//! and "Back" return to the box below, like tdesktop's layered boxes. The
//! data itself (listings, auction states, bids) lives in the session
//! (`quill::gift_market::GiftMarketState`).
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::input::InputState;
use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use quill::gift_manage::GiftRecipient;
use quill::gift_market::CraftGift;
use std::time::Instant;

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
    AuctionBid {
        gift_id: i64,
        /// Built on the first draw; goes with the box.
        slider: Option<BidSliderUi>,
    },
    /// "{n} Items Bought".
    AuctionBought { gift_id: i64 },
    /// "Change Recipient": the bid is for someone else.
    ChangeRecipient { gift_id: i64 },
    /// "{n} Active Auctions" from the chat-list bar.
    ActiveAuctions,
    /// The models a collectible of this gift can get.
    Variants { gift_id: i64 },
    /// "Gift Crafting": how crafting works, before the craft box.
    CraftInfo,
    /// "Craft Gift": the slots of `session.payments.gift_market.craft`,
    /// then the forge while the craft runs.
    Craft,
    /// "Select Gifts": the user's gifts and the listings for a slot.
    CraftSelect,
    /// The craft's outcome: the new gift or "Crafting Failed".
    CraftResult,
    /// An information box ("Crafting unavailable").
    Notice { title: String, text: String },
}

impl MarketBox {
    /// A "Place a Bid" box without its slider yet.
    pub(super) fn bid(gift_id: i64) -> Self {
        Self::AuctionBid {
            gift_id,
            slider: None,
        }
    }

    /// The auction this box shows, to follow its updates.
    pub(super) fn auction(&self) -> Option<i64> {
        match self {
            Self::AuctionInfo { gift_id }
            | Self::AuctionSend { gift_id }
            | Self::AuctionBid { gift_id, .. }
            | Self::AuctionBought { gift_id }
            | Self::ChangeRecipient { gift_id } => Some(*gift_id),
            _ => None,
        }
    }
}

/// The bid slider of the open bid box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BidSliderUi {
    pub(super) state: Entity<SliderState>,
    /// The amount the slider (or the custom field) chose.
    pub(super) chosen: i64,
    /// The `(min, max)` the track was built for; rebuilt when a new bid
    /// moves them.
    pub(super) bounds: (i64, i64),
    /// The "Custom Amount" field, open only over this slider.
    pub(super) custom: Option<CustomBidUi>,
}

/// "Custom Amount" for a bid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CustomBidUi {
    pub(super) input: Entity<InputState>,
    pub(super) error: Option<String>,
}

/// What the open boxes share: who a bid or a purchase is for, "Hide My
/// Name", and whether an auction's state is being asked for to open its
/// box. One value for the whole stack, so it goes with the stack
/// (`GiftMarketUi::drop_orphans`): the last box leaving, however it left,
/// ends a pending open and forgets the recipient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MarketFlow {
    Boxes {
        /// The send-gift box's recipient, or a raised bid's owner; none:
        /// the user.
        recipient: Option<GiftRecipient>,
        hide_name: bool,
    },
    /// Like `Boxes`, and the auction state asked as `request` opens its
    /// box when it arrives. An answer to any other request is not this
    /// one's.
    Opening {
        request: u64,
        /// Asked from the send-gift box, whose recipient the bid uses: the
        /// answer is dropped once that box is gone.
        from_send_box: bool,
        recipient: Option<GiftRecipient>,
        hide_name: bool,
    },
}

impl Default for MarketFlow {
    fn default() -> Self {
        Self::Boxes {
            recipient: None,
            hide_name: false,
        }
    }
}

impl MarketFlow {
    fn names(&mut self) -> (&mut Option<GiftRecipient>, &mut bool) {
        match self {
            Self::Boxes {
                recipient,
                hide_name,
            }
            | Self::Opening {
                recipient,
                hide_name,
                ..
            } => (recipient, hide_name),
        }
    }

    pub(super) fn recipient(&self) -> Option<&GiftRecipient> {
        match self {
            Self::Boxes { recipient, .. } | Self::Opening { recipient, .. } => recipient.as_ref(),
        }
    }

    pub(super) fn hide_name(&self) -> bool {
        match self {
            Self::Boxes { hide_name, .. } | Self::Opening { hide_name, .. } => *hide_name,
        }
    }

    pub(super) fn set_recipient(&mut self, next: Option<GiftRecipient>) {
        *self.names().0 = next;
    }

    pub(super) fn set_hide_name(&mut self, on: bool) {
        *self.names().1 = on;
    }

    /// The auction state was asked as `request`: its box opens on the
    /// answer. A new ask replaces the one before (the session keeps only
    /// the newest fetch).
    pub(super) fn begin_opening(&mut self, request: u64) {
        let (recipient, hide_name) = {
            let (recipient, hide_name) = self.names();
            (recipient.take(), *hide_name)
        };
        *self = Self::Opening {
            request,
            from_send_box: recipient.is_some(),
            recipient,
            hide_name,
        };
    }

    /// The pending open ended (its box opened, or it failed or was
    /// replaced).
    pub(super) fn end_opening(&mut self) {
        if let Self::Opening {
            recipient,
            hide_name,
            ..
        } = self
        {
            *self = Self::Boxes {
                recipient: recipient.take(),
                hide_name: *hide_name,
            };
        }
    }

    /// The request of the pending open and whether the send-gift box asked.
    pub(super) fn opening(&self) -> Option<(u64, bool)> {
        match self {
            Self::Opening {
                request,
                from_send_box,
                ..
            } => Some((*request, *from_send_box)),
            Self::Boxes { .. } => None,
        }
    }
}

/// What the market asked the file layer for a sticker file: the request
/// went out, or the send was refused (not asked again until the set of
/// files changes or the boxes are reopened).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileAsk {
    Sent,
    Refused,
}

pub(crate) struct GiftMarketUi {
    pub(super) stack: Vec<MarketBox>,
    /// Who a bid is for, "Hide My Name", and an auction waiting to open.
    pub(super) flow: MarketFlow,
    /// The sticker files asked for, each sent or refused; downloads go out
    /// once per file, not every frame.
    pub(super) requested_files: Vec<(quill::ids::FileId, FileAsk)>,
    /// How often the chat-list bar opened its box (one tap, one open).
    #[cfg(test)]
    pub(super) bar_opens: usize,
    /// Where the craft stands: the gift its boxes were opened for, and the
    /// forge once it runs.
    pub(super) craft: CraftStage,
}

/// The craft flow of the open boxes: one value, so the forge cannot run
/// without the gift it was started for.
#[derive(Debug, Clone, Default)]
pub(crate) enum CraftStage {
    #[default]
    Idle,
    /// The gift the craft info box was opened for.
    Chosen(CraftGift),
    /// The forge runs at least `FORGE_SECONDS` from `since`; it ends when
    /// its answer is in, and `primary` is kept for the boxes below.
    Forging {
        primary: Option<CraftGift>,
        since: Instant,
    },
}

impl CraftStage {
    pub(super) fn primary(&self) -> Option<&CraftGift> {
        match self {
            Self::Idle => None,
            Self::Chosen(gift) => Some(gift),
            Self::Forging { primary, .. } => primary.as_ref(),
        }
    }

    pub(super) fn forge_since(&self) -> Option<Instant> {
        match self {
            Self::Forging { since, .. } => Some(*since),
            _ => None,
        }
    }

    /// The craft info box opens for `gift`. A running forge is never
    /// replaced (its answer would have no stage to end): returns whether
    /// the gift was taken.
    pub(super) fn choose(&mut self, gift: CraftGift) -> bool {
        if matches!(self, Self::Forging { .. }) {
            return false;
        }
        *self = Self::Chosen(gift);
        true
    }

    /// The forge starts (or restarts) at `since`.
    pub(super) fn start_forge(&mut self, since: Instant) {
        let primary = self.primary().cloned();
        *self = Self::Forging { primary, since };
    }

    /// The forge ended: the chosen gift stays for the boxes below.
    pub(super) fn end_forge(&mut self) {
        if let Self::Forging { primary, .. } = self {
            *self = match primary.take() {
                Some(gift) => Self::Chosen(gift),
                None => Self::Idle,
            };
        }
    }
}

/// The auction a bid box follows (`openGiftAuction`) is followed by the
/// account's own client: it stops with the box, as tdesktop closes it, so
/// the account going to the background does not keep applying its updates
/// (the next account's frame upkeep follows nothing on its own driver).
impl crate::ui::on_screen::Farewell for GiftMarketUi {
    fn farewell(
        app: &mut crate::ui::app::QuillApp,
        _: &mut gpui_kit::Window,
        _: &mut gpui_kit::Context<crate::ui::app::QuillApp>,
    ) {
        if let Some(live) = app.live.as_mut() {
            let _ = live.driver.follow_gift_auction(None);
        }
    }
}

impl GiftMarketUi {
    pub(super) fn new() -> Self {
        Self {
            stack: Vec::new(),
            flow: MarketFlow::default(),
            requested_files: Vec::new(),
            #[cfg(test)]
            bar_opens: 0,
            craft: CraftStage::Idle,
        }
    }

    /// Closes the top box. What only a closed box owned goes with it.
    pub(super) fn pop(&mut self) -> Option<MarketBox> {
        let popped = self.stack.pop();
        self.drop_orphans();
        popped
    }

    /// Closes every box.
    pub(super) fn clear(&mut self) {
        self.stack.clear();
        self.drop_orphans();
    }

    /// Closes the boxes `done` picks, wherever they sit in the stack.
    pub(super) fn close_where(&mut self, done: impl Fn(&MarketBox) -> bool) {
        self.stack.retain(|entry| !done(entry));
        self.drop_orphans();
    }

    /// What only the open boxes owned goes with the last of them, however
    /// it left (back, replace, clear): the recipient, a pending open and
    /// the file asks. "Hide My Name" belongs to the send, change and bid
    /// boxes of one bid: when the last of those leaves, it goes, so the
    /// next auction starts clean.
    fn drop_orphans(&mut self) {
        if self.stack.is_empty() {
            self.flow = MarketFlow::default();
            self.requested_files.clear();
        }
        let naming = self.stack.iter().any(|entry| {
            matches!(
                entry,
                MarketBox::AuctionSend { .. }
                    | MarketBox::AuctionBid { .. }
                    | MarketBox::ChangeRecipient { .. }
            )
        });
        if !naming {
            self.flow.set_hide_name(false);
        }
    }

    /// The slider (and custom field) of the open bid box for `gift_id`.
    pub(super) fn bid(&self, gift_id: i64) -> Option<&BidSliderUi> {
        self.stack.iter().rev().find_map(|entry| match entry {
            MarketBox::AuctionBid {
                gift_id: id,
                slider,
            } if *id == gift_id => slider.as_ref(),
            _ => None,
        })
    }

    pub(super) fn bid_mut(&mut self, gift_id: i64) -> Option<&mut BidSliderUi> {
        self.stack.iter_mut().rev().find_map(|entry| match entry {
            MarketBox::AuctionBid {
                gift_id: id,
                slider,
            } if *id == gift_id => slider.as_mut(),
            _ => None,
        })
    }

    /// The slider slot of the bid box for `gift_id`.
    pub(super) fn bid_slot(&mut self, gift_id: i64) -> Option<&mut Option<BidSliderUi>> {
        self.stack.iter_mut().rev().find_map(|entry| match entry {
            MarketBox::AuctionBid {
                gift_id: id,
                slider,
            } if *id == gift_id => Some(slider),
            _ => None,
        })
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
