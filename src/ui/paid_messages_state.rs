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

/// The confirmation before a paid send, with its "Don't ask me again" tick:
/// the tick is part of the stage, so it ends with the confirmation (Pay,
/// Cancel, another confirmation, the account going away) and never trusts a
/// price the user did not see.
pub(crate) enum PaidConfirmStage {
    /// Nothing is asked.
    None,
    /// A composer send waits for Pay.
    Composer {
        confirm: ComposerPaidConfirm,
        /// "Don't ask me again" is ticked.
        trust: bool,
    },
    /// The send the guard holds (a sticker, GIF, voice note, forward...) is
    /// on screen with the tick set for (`chat_id`, `per_message`); an
    /// untick or a held send for another price is `None`.
    Held { trusted: (i64, i64) },
}

impl PaidConfirmStage {
    /// The composer send waiting for Pay, if that is what is asked.
    pub(super) fn composer(&self) -> Option<&ComposerPaidConfirm> {
        match self {
            Self::Composer { confirm, .. } => Some(confirm),
            Self::None | Self::Held { .. } => None,
        }
    }
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
    /// Where the slider's price came from.
    pub(super) source: PriceSource,
}

/// Where a Direct Messages box's price came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PriceSource {
    /// The default shown while the channel's price is still being fetched.
    Default,
    /// The slider starts from the channel's price.
    Channel,
    /// The slider was moved: the price is a deliberate choice, and the
    /// channel's price no longer replaces it.
    Chosen,
}

pub(crate) struct PaidMessagesUi {
    /// Messages privacy: the "Charge for messages" choice is open with
    /// this staged price. Nothing is charged until Save.
    pub(super) charge: Option<PriceSlider>,
    /// The confirmation before a paid send and its tick.
    pub(super) confirm: PaidConfirmStage,
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
            confirm: PaidConfirmStage::None,
            remove_fee: None,
            direct: None,
        }
    }
}
