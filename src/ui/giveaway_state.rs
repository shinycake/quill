//! The giveaway form's state (`giveaway_create_ui.rs`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::input::InputState;
use gpui_kit::*;
use quill::giveaway::GiveawayDraft;

/// The page of the form that shows. `Confirming` is the "are you sure" step
/// of a prepaid launch; it leaves for `Form` on "Cancel" or when the start
/// went out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GiveawayPage {
    Form,
    Confirming,
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
    /// The page that shows: the form, its "are you sure" step, or a list.
    pub(super) page: GiveawayPage,
}

impl GiveawayPage {
    /// Whether a list (members, countries) shows instead of the form.
    pub(super) fn is_list(self) -> bool {
        matches!(self, Self::Users | Self::Countries)
    }
}

pub(crate) struct GiveawayUi {
    pub(super) form: Option<GiveawayForm>,
}

/// A giveaway just launched asks the old client for the chat's boost
/// status, and the session's launch state is reset (`close_giveaway_form`).
/// The info box's lookup is the session's, so it closes too.
impl crate::ui::on_screen::Farewell for GiveawayUi {
    fn farewell(
        app: &mut QuillApp,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<QuillApp>,
    ) {
        app.close_giveaway_form(cx);
        app.close_giveaway_info(cx);
    }
}

impl GiveawayUi {
    pub(super) fn new() -> Self {
        Self { form: None }
    }
}
