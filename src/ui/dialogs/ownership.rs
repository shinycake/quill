use super::super::app::QuillApp;
use gpui_kit::component::input::InputState;
use gpui_kit::*;
use quill::ids::ChatId;

/// Where the ownership dialog is. tdesktop splits this over
/// `select_future_owner_box.cpp` (leaving as owner) and
/// `channel_ownership_transfer.cpp` (the security check and password).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipStage {
    /// The owner is leaving: who inherits, appoint someone else, or leave.
    Leave,
    /// Choose the new owner. `leave_after` carries the "appoint and leave"
    /// intent from the leave box.
    Pick { leave_after: bool },
    /// Security check result and the 2-step verification password.
    Confirm { user_id: i64, leave_after: bool },
}

/// The transfer / owner-leave dialog. The password lives only in
/// `password_input`, is read once when the transfer is sent and is cleared
/// right after.
pub struct OwnershipDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) stage: OwnershipStage,
    pub(crate) password_input: Entity<InputState>,
}

impl OwnershipDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        chat_id: ChatId,
        stage: OwnershipStage,
    ) -> Self {
        let password_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("2-step verification password")
                .masked(true)
                .submit_on_enter(false)
        });
        Self {
            chat_id,
            stage,
            password_input,
        }
    }
}
