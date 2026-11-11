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
    pub(super) stage: TwofaStage,
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
    /// Batch 6: code entry (recovery email, password recovery, login
    /// email) and the inline confirmation on the recovery screen.
    pub(super) code: Entity<InputState>,
}

/// The two-step verification box: closed, or open on one view with its
/// local notice and inline confirmation. Closing drops all of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum TwofaStage {
    #[default]
    Closed,
    Open {
        view: TwofaView,
        /// Local validation notice for the forms ("enter your current
        /// password"): the driver rejects doomed requests silently, so the
        /// form must speak before sending.
        notice: Option<String>,
        /// The inline confirmation on the recovery screen.
        confirm: Option<TwofaConfirm>,
    },
}

impl crate::ui::on_screen::Farewell for TwoStepUi {}

impl TwoStepUi {
    pub(crate) fn is_open(&self) -> bool {
        self.stage != TwofaStage::Closed
    }

    /// The view on show (the status screen while closed).
    pub(crate) fn view(&self) -> TwofaView {
        match &self.stage {
            TwofaStage::Open { view, .. } => *view,
            TwofaStage::Closed => TwofaView::Status,
        }
    }

    pub(crate) fn notice(&self) -> Option<&String> {
        match &self.stage {
            TwofaStage::Open { notice, .. } => notice.as_ref(),
            TwofaStage::Closed => None,
        }
    }

    pub(crate) fn confirm(&self) -> Option<TwofaConfirm> {
        match &self.stage {
            TwofaStage::Open { confirm, .. } => *confirm,
            TwofaStage::Closed => None,
        }
    }

    /// Open on the status screen, with no notice and no confirmation.
    pub(crate) fn open(&mut self) {
        self.set_view(TwofaView::Status);
    }

    /// Close; the notice and confirmation go with it.
    pub(crate) fn close(&mut self) {
        self.stage = TwofaStage::Closed;
    }

    /// Switch the view; a fresh form starts with no notice and no
    /// confirmation. Opens the box when it was closed.
    pub(crate) fn set_view(&mut self, view: TwofaView) {
        self.stage = TwofaStage::Open {
            view,
            notice: None,
            confirm: None,
        };
    }

    /// Set the local notice line (nothing while the box is closed).
    pub(crate) fn set_notice(&mut self, line: Option<String>) {
        if let TwofaStage::Open { notice, .. } = &mut self.stage {
            *notice = line;
        }
    }

    /// Ask (`Some`) or drop (`None`) the inline confirmation.
    pub(crate) fn set_confirm(&mut self, ask: Option<TwofaConfirm>) {
        if let TwofaStage::Open { confirm, .. } = &mut self.stage {
            *confirm = ask;
        }
    }
}

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
            stage: TwofaStage::Closed,
            current_password: twofa_current_password,
            check_password,
            status_check_password,
            new_password: twofa_new_password,
            hint: twofa_hint,
            email: twofa_email,
            code: twofa_code,
        }
    }
}
