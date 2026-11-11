//! Suggested posts: the open boxes (`suggested_posts_ui.rs`,
//! `suggested_posts_boxes.rs`, `suggested_posts_decide.rs`). The suggest
//! bar itself lives in the session (`SuggestedPostsState::bars`) so the
//! draft save can read it.
#![deny(clippy::indexing_slicing)]

use crate::ui::scheduled::TimePicker;
use gpui_kit::component::input::InputState;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::suggested_posts::{SuggestMode, SuggestedPrice};

/// What a price or time choice is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SuggestTarget {
    /// The composer's suggest bar in this chat.
    Bar(ChatId),
    /// `addOffer` on this message: new terms for a suggestion ("Edit
    /// Price", "Edit Time") or an offer for a plain message ("Add Offer").
    Offer {
        chat_id: ChatId,
        message_id: MessageId,
    },
}

/// The "Suggest a Message" box (tdesktop `ChooseSuggestPriceBox`).
#[derive(Clone)]
pub(crate) struct PriceBox {
    pub(super) target: SuggestTarget,
    pub(super) mode: SuggestMode,
    /// The channel's admin countering: "Request Stars", what they get.
    pub(super) admin: bool,
    /// The Grams tab is active.
    pub(super) ton: bool,
    pub(super) input: Entity<InputState>,
    /// The chosen time; 0 for any time.
    pub(super) send_date: i32,
    /// The field's error under it.
    pub(super) error: Option<String>,
}

/// Where the time box sends its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeTarget {
    /// Back into the open price box.
    PriceBox,
    /// "Edit Time" of a suggestion: `addOffer` with this price.
    Offer {
        chat_id: ChatId,
        message_id: MessageId,
        price: Option<SuggestedPrice>,
    },
    /// Accepting a post without a time: the Accept Terms box follows.
    Accept {
        chat_id: ChatId,
        message_id: MessageId,
    },
}

/// The time box (tdesktop `ChooseSuggestTimeBox`).
pub(crate) struct TimeBox {
    pub(super) target: TimeTarget,
    pub(super) mode: SuggestMode,
    pub(super) picker: TimePicker,
    /// The picked time was out of range.
    pub(super) error: Option<String>,
}

/// The Accept Terms box (tdesktop `ConfirmApproval`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AcceptBox {
    pub(super) chat_id: ChatId,
    pub(super) message_id: MessageId,
    /// 0 publishes now.
    pub(super) send_date: i32,
    pub(super) text: String,
    /// What a member pays, checked against the balance on Publish.
    pub(super) pays: Option<SuggestedPrice>,
}

/// The Decline box with its optional reason.
pub(crate) struct DeclineBox {
    pub(super) chat_id: ChatId,
    pub(super) message_id: MessageId,
    pub(super) text: String,
    pub(super) reason: Entity<InputState>,
}

/// What a missing balance was for: the Stars note names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BalanceFor {
    /// Sending or changing an offer.
    Offer,
    /// Accepting a channel's counter-offer.
    Accept,
}

/// The box a suggested-post flow has open, as one value of the legal
/// flows. Kit dialogs never stack, so a flow is a box with at most one box
/// over it: the price box with the time box or "Grams Needed" over it, or
/// the Accept Terms box with "Grams Needed" over it; the time box of an
/// offer and a lone "Grams Needed" stand alone.
pub(crate) struct SuggestedPostsUi {
    flow: Flow,
}

#[derive(Default)]
enum Flow {
    #[default]
    None,
    Price(PriceBox),
    PriceTime(PriceBox, TimeBox),
    /// "{amount} Grams Needed" over the price box.
    PriceNeeds(PriceBox, SuggestedPrice),
    Accept(AcceptBox),
    AcceptNeeds(AcceptBox, SuggestedPrice),
    Decline(DeclineBox),
    /// The time box of an offer, or of an accept still without a time.
    Time(TimeBox),
    /// "{amount} Grams Needed" alone.
    Needs(SuggestedPrice),
}

impl crate::ui::on_screen::Farewell for SuggestedPostsUi {}

impl SuggestedPostsUi {
    pub(super) fn new() -> Self {
        Self { flow: Flow::None }
    }

    fn take(&mut self) -> Flow {
        std::mem::take(&mut self.flow)
    }

    pub(super) fn price(&self) -> Option<&PriceBox> {
        match &self.flow {
            Flow::Price(price) | Flow::PriceTime(price, _) | Flow::PriceNeeds(price, _) => {
                Some(price)
            }
            _ => None,
        }
    }

    pub(super) fn price_mut(&mut self) -> Option<&mut PriceBox> {
        match &mut self.flow {
            Flow::Price(price) | Flow::PriceTime(price, _) | Flow::PriceNeeds(price, _) => {
                Some(price)
            }
            _ => None,
        }
    }

    /// The price box opens; a box over the price box that is open stays.
    pub(super) fn set_price(&mut self, price: PriceBox) {
        self.flow = match self.take() {
            Flow::PriceTime(_, time) => Flow::PriceTime(price, time),
            Flow::PriceNeeds(_, needs) => Flow::PriceNeeds(price, needs),
            _ => Flow::Price(price),
        };
    }

    /// Close the price box; a box that was over it stays alone.
    pub(super) fn close_price(&mut self) {
        self.flow = match self.take() {
            Flow::Price(_) => Flow::None,
            Flow::PriceTime(_, time) => Flow::Time(time),
            Flow::PriceNeeds(_, needs) => Flow::Needs(needs),
            other => other,
        };
    }

    pub(super) fn close_decline(&mut self) {
        self.take_decline();
    }

    pub(super) fn accept(&self) -> Option<&AcceptBox> {
        match &self.flow {
            Flow::Accept(accept) | Flow::AcceptNeeds(accept, _) => Some(accept),
            _ => None,
        }
    }

    pub(super) fn set_accept(&mut self, accept: AcceptBox) {
        self.flow = match self.take() {
            Flow::AcceptNeeds(_, needs) => Flow::AcceptNeeds(accept, needs),
            _ => Flow::Accept(accept),
        };
    }

    pub(super) fn close_accept(&mut self) {
        self.flow = match self.take() {
            Flow::Accept(_) => Flow::None,
            Flow::AcceptNeeds(_, needs) => Flow::Needs(needs),
            other => other,
        };
    }

    pub(super) fn decline(&self) -> Option<&DeclineBox> {
        match &self.flow {
            Flow::Decline(decline) => Some(decline),
            _ => None,
        }
    }

    pub(super) fn set_decline(&mut self, decline: DeclineBox) {
        self.flow = Flow::Decline(decline);
    }

    /// Close the Decline box, handing it back; another box stays.
    pub(super) fn take_decline(&mut self) -> Option<DeclineBox> {
        match self.take() {
            Flow::Decline(decline) => Some(decline),
            other => {
                self.flow = other;
                None
            }
        }
    }

    pub(super) fn time(&self) -> Option<&TimeBox> {
        match &self.flow {
            Flow::PriceTime(_, time) | Flow::Time(time) => Some(time),
            _ => None,
        }
    }

    pub(super) fn time_mut(&mut self) -> Option<&mut TimeBox> {
        match &mut self.flow {
            Flow::PriceTime(_, time) | Flow::Time(time) => Some(time),
            _ => None,
        }
    }

    /// The time box opens: over the price box when it is open, else alone.
    pub(super) fn set_time(&mut self, time: TimeBox) {
        self.flow = match self.take() {
            Flow::Price(price) | Flow::PriceTime(price, _) | Flow::PriceNeeds(price, _) => {
                Flow::PriceTime(price, time)
            }
            _ => Flow::Time(time),
        };
    }

    pub(super) fn close_time(&mut self) {
        self.flow = match self.take() {
            Flow::PriceTime(price, _) => Flow::Price(price),
            Flow::Time(_) => Flow::None,
            other => other,
        };
    }

    /// The missing amount "Grams Needed" names.
    pub(super) fn grams_needed(&self) -> Option<SuggestedPrice> {
        match self.flow {
            Flow::PriceNeeds(_, price) | Flow::AcceptNeeds(_, price) | Flow::Needs(price) => {
                Some(price)
            }
            _ => None,
        }
    }

    /// "Grams Needed" opens over the price or accept box. An open time
    /// box is not replaced (the date being picked stays): `false` then.
    pub(super) fn set_grams_needed(&mut self, price: SuggestedPrice) -> bool {
        self.flow = match self.take() {
            Flow::Price(base) | Flow::PriceNeeds(base, _) => Flow::PriceNeeds(base, price),
            Flow::Accept(base) | Flow::AcceptNeeds(base, _) => Flow::AcceptNeeds(base, price),
            Flow::None | Flow::Needs(_) => Flow::Needs(price),
            held @ (Flow::PriceTime(..) | Flow::Time(_) | Flow::Decline(_)) => {
                self.flow = held;
                return false;
            }
        };
        true
    }

    pub(super) fn close_grams_needed(&mut self) {
        self.flow = match self.take() {
            Flow::PriceNeeds(price, _) => Flow::Price(price),
            Flow::AcceptNeeds(accept, _) => Flow::Accept(accept),
            Flow::Needs(_) => Flow::None,
            other => other,
        };
    }
}
