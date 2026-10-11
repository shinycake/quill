//! The giveaway form's state (`giveaway_create_ui.rs`).

use gpui_kit::component::input::InputState;
use gpui_kit::*;
use quill::giveaway::GiveawayDraft;

/// The page of the form that shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GiveawayPage {
    Form,
    /// Members to award a Premium gift code.
    Users,
    /// Countries that may take part.
    Countries,
}

/// The open "Boosts via Gifts" form.
pub(crate) struct GiveawayForm {
    pub(super) draft: GiveawayDraft,
    /// The additional prize text.
    pub(super) prize_input: Entity<InputState>,
    /// A note under the form (a refused field), cleared on the next change.
    pub(super) note: Option<String>,
    /// The "are you sure" step of a prepaid launch is showing.
    pub(super) confirming: bool,
    /// The list page that shows instead of the form.
    pub(super) page: GiveawayPage,
}

pub(crate) struct GiveawayUi {
    pub(super) form: Option<GiveawayForm>,
}

impl GiveawayUi {
    pub(super) fn new() -> Self {
        Self { form: None }
    }
}
