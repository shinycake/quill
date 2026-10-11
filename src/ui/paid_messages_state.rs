//! Paid messages: the open boxes and the staged price
//! (`paid_messages_privacy.rs`, `paid_messages_send.rs`,
//! `paid_messages_channel.rs`).

use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use quill::paid_messages::PaidConfirm;

/// A price chosen on a slider: the steps it moves over and the value.
pub(crate) struct PriceSlider {
    pub(super) steps: Vec<i64>,
    pub(super) price: i64,
    pub(super) slider: Entity<SliderState>,
}

/// The Remove Fee box over one user.
pub(crate) struct RemoveFeeBox {
    pub(super) user_id: i64,
    pub(super) name: String,
    /// "Refund already paid N Stars" is ticked.
    pub(super) refund: bool,
}

/// The channel's Direct Messages box.
pub(crate) struct DirectMessagesBox {
    pub(super) chat_id: i64,
    pub(super) enabled: bool,
    pub(super) price: PriceSlider,
    /// The slider was moved: the price is a deliberate choice.
    pub(super) touched: bool,
}

pub(crate) struct PaidMessagesUi {
    /// Messages privacy: the "Charge for messages" choice is open with
    /// this staged price. Nothing is charged until Save.
    pub(super) charge: Option<PriceSlider>,
    /// The confirmation before a paid send.
    pub(super) confirm: Option<PaidConfirm>,
    /// "Don't ask me again" in the confirmation.
    pub(super) confirm_trust: bool,
    /// `(chat id, Stars per message)` the user just confirmed: the send
    /// that follows carries that price, and only that one.
    pub(super) confirmed: Option<(i64, i64)>,
    pub(super) remove_fee: Option<RemoveFeeBox>,
    pub(super) direct: Option<DirectMessagesBox>,
}

impl PaidMessagesUi {
    pub(super) fn new() -> Self {
        Self {
            charge: None,
            confirm: None,
            confirm_trust: false,
            confirmed: None,
            remove_fee: None,
            direct: None,
        }
    }
}
