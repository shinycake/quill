//! Paid messages: the open boxes and the staged price
//! (`paid_messages_privacy.rs`, `paid_messages_send.rs`,
//! `paid_messages_channel.rs`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::composer_sends::SendSlot;
use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::paid_messages::PaidConfirm;

/// The confirmation before a composer send, and the chat it was asked in:
/// Pay sends only there.
pub(crate) struct ComposerPaidConfirm {
    pub(super) ask: PaidConfirm,
    pub(super) slot: SendSlot,
    /// The send's one-time options (silent, effect): it goes out with them
    /// after Pay.
    pub(super) once: super::composer_sends::SendOnce,
}

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
    /// One conversation of a channel's direct messages, for its admins;
    /// `None` = the account's own private chats.
    pub(super) direct: Option<(ChatId, i64)>,
}

/// The channel's Direct Messages box.
pub(crate) struct DirectMessagesBox {
    pub(super) chat_id: i64,
    pub(super) enabled: bool,
    pub(super) price: PriceSlider,
    /// The slider was moved: the price is a deliberate choice.
    pub(super) touched: bool,
    /// The slider starts from the channel's price, not the default shown
    /// while that price was still being fetched.
    pub(super) seeded: bool,
}

pub(crate) struct PaidMessagesUi {
    /// Messages privacy: the "Charge for messages" choice is open with
    /// this staged price. Nothing is charged until Save.
    pub(super) charge: Option<PriceSlider>,
    /// The confirmation before a paid send.
    pub(super) confirm: Option<ComposerPaidConfirm>,
    /// "Don't ask me again", ticked on the confirmation for this chat and
    /// price (`chat_id`, `per_message`). Another confirmation shows it
    /// unticked, and Pay or Cancel clears it, so a tick never trusts a
    /// price the user did not see.
    pub(super) confirm_trust: Option<(i64, i64)>,
    pub(super) remove_fee: Option<RemoveFeeBox>,
    pub(super) direct: Option<DirectMessagesBox>,
}

/// A send held for the paid confirmation is discarded in the old client's
/// sender (`cancel_paid_confirm`): it is the old account's message.
impl crate::ui::on_screen::Farewell for PaidMessagesUi {
    fn farewell(
        app: &mut QuillApp,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<QuillApp>,
    ) {
        app.cancel_paid_confirm(cx);
    }
}

impl PaidMessagesUi {
    pub(super) fn new() -> Self {
        Self {
            charge: None,
            confirm: None,
            confirm_trust: None,
            remove_fee: None,
            direct: None,
        }
    }
}
