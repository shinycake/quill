//! Two-step verification box: its view, inputs and pending confirmation.

use super::*;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::*;

pub(crate) struct TwoStepUi {
    /// Slice A2: two-step verification overlay. `view` picks the
    /// status screen or one of the forms; the four textareas back the
    /// enable/change/disable/recovery-email forms. Passwords live in the
    /// inputs only and are cleared on submit/close — never on the session.
    pub(super) open: bool,
    pub(super) view: TwofaView,
    pub(super) current_password: Entity<InputState>,
    pub(super) new_password: Entity<InputState>,
    pub(super) hint: Entity<TextareaState>,
    pub(super) email: Entity<TextareaState>,
    /// Slice A2 fixup: local validation notice for the 2FA forms ("enter
    /// your current password") — the driver rejects doomed requests
    /// silently, so the form must speak before sending.
    pub(super) notice: Option<String>,
    /// Batch 6: code entry (recovery email, password recovery, login
    /// email) and the inline confirmation on the recovery screen.
    pub(super) code: Entity<InputState>,
    pub(super) confirm: Option<TwofaConfirm>,
}

impl TwoStepUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        // Slice A2: two-step verification overlay inputs. Passwords live
        // here only and are cleared on submit/close — never on the
        // session.
        let twofa_current_password = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Current password")
                .submit_on_enter(false)
        });
        let twofa_new_password = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("New password")
                .submit_on_enter(false)
        });
        let twofa_hint = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Hint (optional)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let twofa_code = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Code")
                .submit_on_enter(false)
        });
        let twofa_email = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Recovery email")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            open: false,
            view: TwofaView::Status,
            current_password: twofa_current_password,
            new_password: twofa_new_password,
            hint: twofa_hint,
            email: twofa_email,
            notice: None,
            code: twofa_code,
            confirm: None,
        }
    }
}
