//! The giveaway form's state (`giveaway_create_ui.rs`).

use gpui_kit::component::input::InputState;
use gpui_kit::*;
use quill::giveaway::GiveawayDraft;

/// The open "Boosts via Gifts" form.
pub(crate) struct GiveawayForm {
    pub(super) draft: GiveawayDraft,
    /// The additional prize text.
    pub(super) prize_input: Entity<InputState>,
    /// A note under the form (a refused field), cleared on the next change.
    pub(super) note: Option<String>,
    /// The "are you sure" step of a prepaid launch is showing.
    pub(super) confirming: bool,
}

pub(crate) struct GiveawayUi {
    pub(super) form: Option<GiveawayForm>,
}

impl GiveawayUi {
    pub(super) fn new() -> Self {
        Self { form: None }
    }
}
