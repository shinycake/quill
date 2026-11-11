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
    stage: TwoFaBox,
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
    /// Batch 6: code entry (recovery email, password recovery, login
    /// email) and the inline confirmation on the recovery screen.
    pub(super) code: Entity<InputState>,
}

/// The box's stage: closed, or open at one view with that view's inline
/// confirmation and local notice. Moving to another view or closing
/// replaces the whole value, so neither outlives its view.
pub(crate) enum TwoFaBox {
    Closed,
    Open {
        view: TwofaView,
        confirm: Option<TwofaConfirm>,
        notice: Option<String>,
    },
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
            stage: TwoFaBox::Closed,
            current_password: twofa_current_password,
            check_password,
            status_check_password,
            new_password: twofa_new_password,
            hint: twofa_hint,
            email: twofa_email,
            code: twofa_code,
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        matches!(self.stage, TwoFaBox::Open { .. })
    }

    /// The view shown; the status screen while the box is closed.
    pub(crate) fn view(&self) -> TwofaView {
        match &self.stage {
            TwoFaBox::Open { view, .. } => *view,
            TwoFaBox::Closed => TwofaView::Status,
        }
    }

    pub(crate) fn notice(&self) -> Option<&String> {
        match &self.stage {
            TwoFaBox::Open { notice, .. } => notice.as_ref(),
            TwoFaBox::Closed => None,
        }
    }

    pub(crate) fn confirm(&self) -> Option<TwofaConfirm> {
        match &self.stage {
            TwoFaBox::Open { confirm, .. } => *confirm,
            TwoFaBox::Closed => None,
        }
    }

    /// Open the box on `view`, with no confirmation or notice.
    fn open_at(&mut self, view: TwofaView) {
        self.stage = TwoFaBox::Open {
            view,
            confirm: None,
            notice: None,
        };
    }

    /// Switch an open box to `view`, dropping its confirmation and notice.
    /// A closed box stays closed.
    fn goto(&mut self, view: TwofaView) {
        if self.is_open() {
            self.open_at(view);
        }
    }

    fn close(&mut self) {
        self.stage = TwoFaBox::Closed;
    }

    /// Set the local notice line; an open box only.
    pub(super) fn set_notice(&mut self, line: Option<String>) {
        if let TwoFaBox::Open { notice, .. } = &mut self.stage {
            *notice = line;
        }
    }

    pub(super) fn set_confirm(&mut self, ask: Option<TwofaConfirm>) {
        if let TwoFaBox::Open { confirm, .. } = &mut self.stage {
            *confirm = ask;
        }
    }
}

/// The box's stage (`TwoStepUi`, per account on screen) and the account's
/// recovery flow (`TwofaFlow` in the session: the emailed code a step awaits
/// and the finished step's notice) move together, and only from here: every
/// open, view change, close and finished step is one of these, so neither
/// outlives the other's view.
impl QuillApp {
    fn twofa_flow_mut(&mut self) -> Option<&mut quill::state::TwofaFlow> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session.auth_state.twofa_flow),
            None => self
                .demo_session
                .as_mut()
                .map(|session| &mut session.auth_state.twofa_flow),
        }
    }

    /// Forget the one-time notice and any half-finished recovery step.
    /// Every path that drops the box runs this: `twofa_close`, and
    /// `replace_screen` for the ones that drop the whole `TwoStepUi` (account
    /// switch, logout, a connection replacing the client).
    pub(in crate::ui) fn twofa_forget_flow(&mut self) {
        if let Some(flow) = self.twofa_flow_mut() {
            flow.notice = None;
            flow.code_wait = quill::state::CodeWait::None;
        }
    }

    /// Open the box on the status screen with a clean flow.
    pub(super) fn twofa_open(&mut self) {
        self.twofa.open_at(TwofaView::Status);
        self.twofa_forget_flow();
    }

    /// Close the box; the flow goes with it.
    pub(super) fn twofa_close(&mut self) {
        self.twofa.close();
        self.twofa_forget_flow();
    }

    /// Switch the 2FA view; a fresh form starts with no local notice line,
    /// and the status screen with no half-finished step.
    pub(super) fn goto_twofa_view(&mut self, view: TwofaView) {
        self.twofa.goto(view);
        if view == TwofaView::Status {
            self.twofa_forget_flow();
        } else if let Some(flow) = self.twofa_flow_mut() {
            flow.notice = None;
        }
    }

    /// A recovery / reset / login-email step finished: the box moves to the
    /// status screen, which reports the step's notice once.
    pub(super) fn twofa_step_done(&mut self) {
        self.twofa.goto(TwofaView::Status);
    }
}
