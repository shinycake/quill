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

pub(crate) struct SuggestedPostsUi {
    pub(super) price: Option<PriceBox>,
    pub(super) time: Option<TimeBox>,
    pub(super) accept: Option<AcceptBox>,
    pub(super) decline: Option<DeclineBox>,
    /// "{amount} Grams Needed".
    pub(super) grams_needed: Option<SuggestedPrice>,
}

impl SuggestedPostsUi {
    pub(super) fn new() -> Self {
        Self {
            price: None,
            time: None,
            accept: None,
            decline: None,
            grams_needed: None,
        }
    }
}
