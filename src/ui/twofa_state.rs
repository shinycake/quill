//! Two-step verification box: its view, inputs and pending confirmation.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::*;
use quill::lang::{keys, tr};

pub(crate) struct TwoStepUi {
    /// Slice A2: two-step verification overlay. `view` picks the
    /// status screen or one of the forms; the four textareas back the
    /// enable/change/disable/recovery-email forms. Passwords live in the
    /// inputs only and are cleared on submit/close — never on the session.
    pub(super) open: bool,
    pub(super) view: TwofaView,
    pub(super) current_password: Entity<InputState>,
    /// The Privacy page's "Your password" check card. Its own field: the
    /// 2FA box stays over that page, so the box's forms and the card are
    /// drawn in the same frame, and one field painted in two places
    /// notifies at every paint (`lock-field-per-window.md`).
    pub(super) check_password: Entity<InputState>,
    /// The same check card on the 2FA box's Status view. Its own field for
    /// the same reason: the box is drawn over the Privacy page.
    pub(super) status_check_password: Entity<InputState>,
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

impl crate::ui::on_screen::Farewell for TwoStepUi {}

impl TwoStepUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        // Slice A2: two-step verification overlay inputs. Passwords live
        // here only and are cleared on submit/close — never on the
        // session.
        let twofa_current_password = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(tr(keys::Q_PREFS_CURRENT_PASSWORD))
                .submit_on_enter(false)
        });
        let check_field = |window: &mut Window, cx: &mut Context<QuillApp>| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder(tr(keys::Q_PREFS_CURRENT_PASSWORD))
                    .submit_on_enter(false)
            })
        };
        let check_password = check_field(window, cx);
        let status_check_password = check_field(window, cx);
        let twofa_new_password = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(tr(keys::Q_PREFS_NEW_PASSWORD))
                .submit_on_enter(false)
        });
        let twofa_hint = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_PREFS_HINT_OPTIONAL))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let twofa_code = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::Q_PREFS_CODE))
                .submit_on_enter(false)
        });
        let twofa_email = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_PREFS_RECOVERY_EMAIL))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            open: false,
            view: TwofaView::Status,
            current_password: twofa_current_password,
            check_password,
            status_check_password,
            new_password: twofa_new_password,
            hint: twofa_hint,
            email: twofa_email,
            notice: None,
            code: twofa_code,
            confirm: None,
        }
    }
}
