//! The Star reaction box's state (`paid_reaction_ui.rs`).

use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};

/// The open box: how many Stars, and who they show as.
pub(crate) struct PaidBox {
    pub(super) chat_id: ChatId,
    pub(super) message_id: MessageId,
    pub(super) amount: i64,
    /// How the Stars show in Top Senders.
    pub(super) sender: quill::paid_reaction::PaidSender,
    pub(super) slider: Entity<SliderState>,
}

pub(crate) struct PaidReactionUi {
    pub(super) boxed: Option<PaidBox>,
}

impl PaidReactionUi {
    pub(super) fn new() -> Self {
        Self { boxed: None }
    }
}
