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

/// The box a suggested-post flow has open. Kit dialogs never stack, so a
/// flow is a box with at most one box over it: the price box with the time
/// box or "Grams Needed" over it, or the Accept Terms box with "Grams
/// Needed" over it.
pub(crate) struct SuggestedPostsUi {
    base: Base,
    over: Over,
}

/// The box a flow starts from.
enum Base {
    None,
    Price(PriceBox),
    Accept(AcceptBox),
    Decline(DeclineBox),
}

/// The box shown over the base (or alone: the time box of an offer).
enum Over {
    None,
    Time(TimeBox),
    /// "{amount} Grams Needed".
    GramsNeeded(SuggestedPrice),
}

impl crate::ui::on_screen::Farewell for SuggestedPostsUi {}

impl SuggestedPostsUi {
    pub(super) fn new() -> Self {
        Self {
            base: Base::None,
            over: Over::None,
        }
    }

    pub(super) fn price(&self) -> Option<&PriceBox> {
        match &self.base {
            Base::Price(price) => Some(price),
            _ => None,
        }
    }

    pub(super) fn price_mut(&mut self) -> Option<&mut PriceBox> {
        match &mut self.base {
            Base::Price(price) => Some(price),
            _ => None,
        }
    }

    pub(super) fn set_price(&mut self, price: PriceBox) {
        self.base = Base::Price(price);
    }

    /// Close the price box, handing it back; another box stays.
    pub(super) fn take_price(&mut self) -> Option<PriceBox> {
        match std::mem::replace(&mut self.base, Base::None) {
            Base::Price(price) => Some(price),
            other => {
                self.base = other;
                None
            }
        }
    }

    pub(super) fn close_price(&mut self) {
        self.take_price();
    }

    pub(super) fn close_decline(&mut self) {
        self.take_decline();
    }

    pub(super) fn accept(&self) -> Option<&AcceptBox> {
        match &self.base {
            Base::Accept(accept) => Some(accept),
            _ => None,
        }
    }

    pub(super) fn set_accept(&mut self, accept: AcceptBox) {
        self.base = Base::Accept(accept);
    }

    pub(super) fn close_accept(&mut self) {
        if matches!(self.base, Base::Accept(_)) {
            self.base = Base::None;
        }
    }

    pub(super) fn decline(&self) -> Option<&DeclineBox> {
        match &self.base {
            Base::Decline(decline) => Some(decline),
            _ => None,
        }
    }

    pub(super) fn set_decline(&mut self, decline: DeclineBox) {
        self.base = Base::Decline(decline);
    }

    /// Close the Decline box, handing it back; another box stays.
    pub(super) fn take_decline(&mut self) -> Option<DeclineBox> {
        match std::mem::replace(&mut self.base, Base::None) {
            Base::Decline(decline) => Some(decline),
            other => {
                self.base = other;
                None
            }
        }
    }

    pub(super) fn time(&self) -> Option<&TimeBox> {
        match &self.over {
            Over::Time(time) => Some(time),
            _ => None,
        }
    }

    pub(super) fn time_mut(&mut self) -> Option<&mut TimeBox> {
        match &mut self.over {
            Over::Time(time) => Some(time),
            _ => None,
        }
    }

    pub(super) fn set_time(&mut self, time: TimeBox) {
        self.over = Over::Time(time);
    }

    pub(super) fn close_time(&mut self) {
        if matches!(self.over, Over::Time(_)) {
            self.over = Over::None;
        }
    }

    /// The missing amount "Grams Needed" names.
    pub(super) fn grams_needed(&self) -> Option<SuggestedPrice> {
        match self.over {
            Over::GramsNeeded(price) => Some(price),
            _ => None,
        }
    }

    pub(super) fn set_grams_needed(&mut self, price: SuggestedPrice) {
        self.over = Over::GramsNeeded(price);
    }

    pub(super) fn close_grams_needed(&mut self) {
        if matches!(self.over, Over::GramsNeeded(_)) {
            self.over = Over::None;
        }
    }
}
