//! Batch 6: the two-step verification screens added next to the A2 forms —
//! recovery-email code entry (tdesktop `CloudPasswordEmailConfirm`),
//! "Forgot password?" with the emailed code, reset with the 7-day wait
//! and its cancel (`settings_cloud_password_input.cpp`), and the login
//! email (`settings_cloud_password_login_email*.cpp`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputContentType, Textarea};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::state::{TwofaNotice, format_reset_duration};
use quill::telegram::envelope::PasswordState;
use zeroize::Zeroize;

/// An inline confirmation on the recovery screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TwofaConfirm {
    /// "Reset" the password (7-day wait); `with_email` picks the wording.
    Reset { with_email: bool },
    /// Cancel the pending reset.
    CancelReset,
}

fn now_secs() -> i64 {
    (quill::state::unix_ms_now() / 1000) as i64
}

fn muted_line(cx: &App, text: impl Into<SharedString>) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

fn heading(text: &'static str) -> Div {
    div().font_semibold().text_sm().child(text)
}

impl QuillApp {
    /// The one-time result line of a finished step (tdesktop's inform
    /// boxes), shown above the status screen.
    pub(super) fn twofa_notice_line(&self) -> Option<String> {
        let notice = self.session()?.auth_state.twofa_flow.notice?;
        Some(match notice {
            TwofaNotice::PasswordRemoved => tr(keys::CLOUD_PASSWORD_REMOVED).to_string(),
            TwofaNotice::PasswordRecovered => tr(keys::CLOUD_PASSWORD_UPDATED).to_string(),
            TwofaNotice::ResetPending { reset_date } => tr_with(
                keys::SETTINGS_CLOUD_PASSWORD_RESET_IN,
                &[(
                    "duration",
                    &(format_reset_duration(i64::from(reset_date) - now_secs())).to_string(),
                )],
            ),
            TwofaNotice::ResetDeclined { retry_date } => tr_with(
                keys::CLOUD_PASSWORD_RESET_LATER,
                &[(
                    "duration",
                    &(format_reset_duration(i64::from(retry_date) - now_secs())).to_string(),
                )],
            ),
            TwofaNotice::ResetCancelled => tr(keys::Q_PREFS_PASSWORD_RESET_CANCELED).to_string(),
            TwofaNotice::LoginEmailChanged => {
                tr(keys::SETTINGS_CLOUD_LOGIN_EMAIL_SUCCESS).to_string()
            }
            TwofaNotice::RecoveryEmailConfirmed => {
                tr(keys::Q_PREFS_YOUR_RECOVERY_EMAIL_IS_CONFIRMED).to_string()
            }
        })
    }

    /// Forget the one-time notice and any half-finished recovery step.
    pub(super) fn clear_twofa_flow(&mut self) {
        let flow = if let Some(live) = self.live.as_mut() {
            Some(&mut live.driver.session.auth_state.twofa_flow)
        } else {
            self.demo_session
                .as_mut()
                .map(|s| &mut s.auth_state.twofa_flow)
        };
        if let Some(flow) = flow {
            flow.notice = None;
            flow.recovery_code_sent_to = None;
            flow.login_email_code_sent_to = None;
        }
        self.twofa.confirm = None;
    }

    /// The pending recovery email: code entry, resend, abort.
    pub(super) fn twofa_pending_email_block(
        &self,
        cx: &mut Context<Self>,
        pattern: &str,
        loading: bool,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading(tr(keys::CLOUD_PASSWORD_CONFIRM)))
            .child(muted_line(
                cx,
                tr_with(keys::CLOUD_PASSWORD_WAITING_CODE, &[("email", pattern)]),
            ))
            .child(
                Input::new(&self.twofa.code)
                    .aria_label(tr(keys::Q_PREFS_RECOVERY_EMAIL_CONFIRMATION_CODE))
                    .h(px(40.)),
            )
            .when_some(self.twofa.notice.clone(), |this, note| {
                this.child(div().text_xs().text_color(danger()).child(note))
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("twofa-confirm-code")
                            .label(tr(keys::SETTINGS_CLOUD_PASSWORD_EMAIL_CONFIRM))
                            .primary()
                            .disabled(loading)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_twofa_email_code(window, cx);
                            })),
                    )
                    .child(
                        Button::new("twofa-resend-code")
                            .label(tr(keys::CLOUD_PASSWORD_RESEND))
                            .ghost()
                            .disabled(loading)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.resend_twofa_code(cx);
                            })),
                    )
                    .child(
                        // TGX `AbortRecoveryEmail`, confirmed like TGX's
                        // `AbortRecoveryEmailConfirm` through the shared
                        // dialog. No chat is involved, so its chat id is
                        // a dummy.
                        Button::new("twofa-abort-email")
                            .label(tr(keys::Q_PREFS_ABORT_RECOVERY_EMAIL_SETUP))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_group_confirm(
                                    quill::ids::ChatId(0),
                                    GroupConfirmAction::AbortRecoveryEmailSetup,
                                    cx,
                                );
                            })),
                    ),
            )
    }

    pub(super) fn submit_twofa_email_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut code = self.twofa.code.read(cx).value().trim().to_string();
        if code.is_empty() {
            self.twofa.notice = Some(tr(keys::Q_PREFS_ENTER_THE_CODE_FROM_THE).into());
            cx.notify();
            return;
        }
        self.twofa.notice = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.check_recovery_email_code(&code);
        }
        code.zeroize();
        self.twofa
            .code
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    /// Status-screen row for the login email, when the account has one.
    pub(super) fn twofa_login_email_row(
        &self,
        cx: &mut Context<Self>,
        state: &PasswordState,
    ) -> Option<Div> {
        let pattern = state.login_email_address_pattern.clone();
        if pattern.is_empty() {
            return None;
        }
        // Telegram sends a blank-ish pattern for "off".
        let shown = if pattern.contains(' ') {
            tr(keys::SETTINGS_CLOUD_PASSWORD_OFF).to_string()
        } else {
            pattern
        };
        Some(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .font_semibold()
                        .text_sm()
                        .child(tr(keys::SETTINGS_CLOUD_LOGIN_EMAIL_SECTION_TITLE)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_sm().child(shown))
                        .child(
                            Button::new("twofa-goto-login-email")
                                .label(tr(keys::SETTINGS_CLOUD_LOGIN_EMAIL_BOX_OK))
                                .ghost()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.goto_twofa_view(TwofaView::LoginEmail);
                                    cx.notify();
                                })),
                        ),
                ),
        )
    }

    /// "Forgot password?" link under the forms that ask for the current
    /// password.
    pub(super) fn twofa_forgot_link(&self, cx: &mut Context<Self>) -> Div {
        div().child(
            Button::new("twofa-forgot")
                .label(tr(keys::Q_PREFS_FORGOT_PASSWORD))
                .ghost()
                .small()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.goto_twofa_view(TwofaView::Recover);
                    cx.notify();
                })),
        )
    }

    /// Recovery / reset screen.
    pub(super) fn twofa_recover_body(&self, cx: &mut Context<Self>, body: Div) -> Div {
        let session = self.session();
        let state = session
            .as_ref()
            .and_then(|s| s.auth_state.password_state.clone());
        let code_sent_to = session
            .as_ref()
            .and_then(|s| s.auth_state.twofa_flow.recovery_code_sent_to.clone());
        let loading = session
            .as_ref()
            .is_some_and(|s| s.auth_state.password_state_loading);
        let Some(state) = state else {
            return body.child(muted_line(cx, tr(keys::Q_PREFS_LOADING)));
        };
        let mut body = body.child(heading(tr(keys::Q_PREFS_FORGOT_PASSWORD)));
        // A reset is already counting down.
        if state.pending_reset_date > 0 {
            let left = i64::from(state.pending_reset_date) - now_secs();
            if left > 0 {
                body = body
                    .child(div().text_sm().child(tr_with(
                        keys::SETTINGS_CLOUD_PASSWORD_RESET_IN,
                        &[("duration", &(format_reset_duration(left)).to_string())],
                    )))
                    .child(self.twofa_confirm_or(
                        cx,
                        tr(keys::CLOUD_PASSWORD_RESET_CANCEL_TITLE),
                        tr(keys::CLOUD_PASSWORD_RESET_CANCEL_SURE),
                        TwofaConfirm::CancelReset,
                        tr(keys::BOX_YES),
                        tr(keys::BOX_NO),
                        loading,
                    ));
            } else {
                body = body
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::Q_PREFS_THE_WAITING_PERIOD_IS_OVER)),
                    )
                    .child(
                        Button::new("twofa-reset-now")
                            .label(tr(keys::CLOUD_PASSWORD_RESET_READY))
                            .primary()
                            .disabled(loading)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(live) = this.live.as_mut() {
                                    let _ = live.driver.reset_twofa_password();
                                }
                                cx.notify();
                            })),
                    );
            }
            return self.twofa_back_row(cx, body);
        }
        if let Some(pattern) = code_sent_to {
            body = body
                .child(div().text_sm().child(tr_with(
                    keys::SIGNIN_RECOVER_DESC,
                    &[("email", &pattern.to_string())],
                )))
                .child(
                    Input::new(&self.twofa.code)
                        .aria_label(tr(keys::Q_PREFS_PASSWORD_RECOVERY_CODE))
                        .h(px(40.)),
                )
                .child(
                    div()
                        .mt_1()
                        .font_semibold()
                        .text_sm()
                        .child(tr(keys::Q_PREFS_NEW_PASSWORD)),
                )
                .child(
                    Input::new(&self.twofa.new_password)
                        .aria_label(tr(keys::Q_PREFS_NEW_TWO_STEP_VERIFICATION_PASSWORD))
                        .content_type(InputContentType::Password)
                        .h(px(40.)),
                )
                .child(
                    Textarea::new(&self.twofa.hint)
                        .aria_label(tr(keys::SETTINGS_CLOUD_PASSWORD_HINT_TITLE))
                        .h(px(40.)),
                )
                .child(muted_line(
                    cx,
                    tr(keys::Q_PREFS_LEAVE_THE_PASSWORD_EMPTY_TO),
                ))
                .when_some(self.twofa.notice.clone(), |this, note| {
                    this.child(div().text_xs().text_color(danger()).child(note))
                })
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("twofa-recover-submit")
                                .label(tr(keys::SETTINGS_SAVE))
                                .primary()
                                .disabled(loading)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_twofa_recovery(window, cx);
                                })),
                        )
                        .child(
                            Button::new("twofa-recover-resend")
                                .label(tr(keys::CLOUD_PASSWORD_RESEND))
                                .ghost()
                                .disabled(loading)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(live) = this.live.as_mut() {
                                        let _ = live.driver.request_twofa_recovery_code();
                                    }
                                    cx.notify();
                                })),
                        ),
                )
                .child(self.twofa_confirm_or(
                    cx,
                    tr(keys::SIGNIN_TRY_PASSWORD),
                    tr(keys::CLOUD_PASSWORD_RESET_WITH_EMAIL),
                    TwofaConfirm::Reset { with_email: true },
                    tr(keys::CLOUD_PASSWORD_RESET_OK),
                    tr(keys::CANCEL),
                    loading,
                ));
            return self.twofa_back_row(cx, body);
        }
        if state.has_recovery_email_address {
            body = body
                .child(div().text_sm().child(tr(keys::Q_PREFS_WELL_SEND_A_CODE_TO)))
                .child(
                    Button::new("twofa-send-recovery-code")
                        .label(tr(keys::Q_PREFS_SEND_CODE))
                        .primary()
                        .disabled(loading)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(live) = this.live.as_mut() {
                                let _ = live.driver.request_twofa_recovery_code();
                            }
                            cx.notify();
                        })),
                )
                .child(self.twofa_confirm_or(
                    cx,
                    tr(keys::SIGNIN_TRY_PASSWORD),
                    tr(keys::CLOUD_PASSWORD_RESET_WITH_EMAIL),
                    TwofaConfirm::Reset { with_email: true },
                    tr(keys::CLOUD_PASSWORD_RESET_OK),
                    tr(keys::CANCEL),
                    loading,
                ));
        } else {
            body = body.child(self.twofa_confirm_or(
                cx,
                tr(keys::CLOUD_PASSWORD_RESET_READY),
                tr(keys::CLOUD_PASSWORD_RESET_NO_EMAIL),
                TwofaConfirm::Reset { with_email: false },
                tr(keys::CLOUD_PASSWORD_RESET_OK),
                tr(keys::CANCEL),
                loading,
            ));
        }
        self.twofa_back_row(cx, body)
    }

    fn twofa_back_row(&self, cx: &mut Context<Self>, body: Div) -> Div {
        body.child(
            Button::new("twofa-recover-back")
                .label(tr(keys::Q_PREFS_BACK))
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.goto_twofa_view(TwofaView::Status);
                    cx.notify();
                })),
        )
    }

    /// A link button that opens its own inline confirmation; the
    /// confirmation runs the matching request.
    #[allow(clippy::too_many_arguments)]
    fn twofa_confirm_or(
        &self,
        cx: &mut Context<Self>,
        trigger_label: &'static str,
        question: &'static str,
        confirm: TwofaConfirm,
        yes: &'static str,
        no: &'static str,
        loading: bool,
    ) -> Div {
        if self.twofa.confirm != Some(confirm) {
            return div().child(
                Button::new(format!("twofa-confirm-open-{trigger_label}"))
                    .label(trigger_label)
                    .ghost()
                    .small()
                    .disabled(loading)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.twofa.confirm = Some(confirm);
                        cx.notify();
                    })),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().child(question))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("twofa-confirm-yes")
                            .label(yes)
                            .primary()
                            .disabled(loading)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.twofa.confirm = None;
                                if let Some(live) = this.live.as_mut() {
                                    let _ = match confirm {
                                        TwofaConfirm::Reset { .. } => {
                                            live.driver.reset_twofa_password()
                                        }
                                        TwofaConfirm::CancelReset => {
                                            live.driver.cancel_twofa_password_reset()
                                        }
                                    };
                                }
                                cx.notify();
                            })),
                    )
                    .child(Button::new("twofa-confirm-no").label(no).ghost().on_click(
                        cx.listener(|this, _, _, cx| {
                            this.twofa.confirm = None;
                            cx.notify();
                        }),
                    )),
            )
    }

    /// Save the recovered password (or turn it off when empty).
    pub(super) fn submit_twofa_recovery(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut code = self.twofa.code.read(cx).value().trim().to_string();
        let mut new = self.twofa.new_password.read(cx).value().to_string();
        let hint = self.twofa.hint.read(cx).value().to_string();
        if code.is_empty() {
            self.twofa.notice = Some(tr(keys::Q_PREFS_ENTER_THE_CODE_FROM_THE).into());
            code.zeroize();
            new.zeroize();
            cx.notify();
            return;
        }
        self.twofa.notice = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.recover_twofa_password(&code, &new, &hint);
        }
        code.zeroize();
        new.zeroize();
        for input in [&self.twofa.code, &self.twofa.new_password] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.twofa
            .hint
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    /// Login email: new address, then the code sent to it.
    pub(super) fn twofa_login_email_body(&self, cx: &mut Context<Self>, body: Div) -> Div {
        let session = self.session();
        let code_sent_to = session
            .as_ref()
            .and_then(|s| s.auth_state.twofa_flow.login_email_code_sent_to.clone());
        let loading = session
            .as_ref()
            .is_some_and(|s| s.auth_state.password_state_loading);
        let body = if let Some(pattern) = code_sent_to {
            body.child(heading(tr(keys::SETTINGS_CLOUD_LOGIN_EMAIL_CODE_TITLE)))
                .child(div().text_sm().child(tr_with(
                    keys::SETTINGS_CLOUD_LOGIN_EMAIL_CODE_ABOUT,
                    &[("email", &pattern.to_string())],
                )))
                .child(
                    Input::new(&self.twofa.code)
                        .aria_label(tr(keys::Q_PREFS_LOGIN_EMAIL_CONFIRMATION_CODE))
                        .h(px(40.)),
                )
                .when_some(self.twofa.notice.clone(), |this, note| {
                    this.child(div().text_xs().text_color(danger()).child(note))
                })
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("twofa-login-email-confirm")
                                .label(tr(keys::SETTINGS_CLOUD_LOGIN_EMAIL_CONFIRM))
                                .primary()
                                .disabled(loading)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_login_email_code(window, cx);
                                })),
                        )
                        .child(
                            Button::new("twofa-login-email-resend")
                                .label(tr(keys::CLOUD_PASSWORD_RESEND))
                                .ghost()
                                .disabled(loading)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(live) = this.live.as_mut() {
                                        let _ = live.driver.resend_login_email_code();
                                    }
                                    cx.notify();
                                })),
                        ),
                )
        } else {
            body.child(heading(tr(keys::SETTINGS_CLOUD_LOGIN_EMAIL_TITLE)))
                .child(muted_line(cx, tr(keys::SETTINGS_CLOUD_LOGIN_EMAIL_ABOUT)))
                .child(
                    Textarea::new(&self.twofa.email)
                        .aria_label(tr(keys::Q_PREFS_LOGIN_EMAIL_ADDRESS))
                        .h(px(40.)),
                )
                .when_some(self.twofa.notice.clone(), |this, note| {
                    this.child(div().text_xs().text_color(danger()).child(note))
                })
                .child(
                    Button::new("twofa-login-email-submit")
                        .label(tr(keys::SETTINGS_CLOUD_LOGIN_EMAIL_CONFIRM))
                        .primary()
                        .disabled(loading)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_login_email(window, cx);
                        })),
                )
        };
        body.child(
            Button::new("twofa-login-email-back")
                .label(tr(keys::Q_PREFS_BACK))
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.goto_twofa_view(TwofaView::Status);
                    cx.notify();
                })),
        )
    }

    fn submit_login_email(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let email = self.twofa.email.read(cx).value().trim().to_string();
        if email.is_empty() {
            self.twofa.notice = Some(tr(keys::Q_PREFS_ENTER_THE_NEW_LOGIN_EMAIL).into());
            cx.notify();
            return;
        }
        self.twofa.notice = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.set_login_email(&email);
        }
        self.twofa
            .email
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    fn submit_login_email_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut code = self.twofa.code.read(cx).value().trim().to_string();
        if code.is_empty() {
            self.twofa.notice = Some(tr(keys::Q_PREFS_ENTER_THE_CODE_FROM_THE).into());
            cx.notify();
            return;
        }
        self.twofa.notice = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.check_login_email_code(&code);
        }
        code.zeroize();
        self.twofa
            .code
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    /// Whether the two-step dialog should fall back to the status
    /// screen: a finished step reports there.
    pub(super) fn twofa_step_finished(&self) -> bool {
        self.session()
            .is_some_and(|s| s.auth_state.twofa_flow.notice.is_some())
    }
}
