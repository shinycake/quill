//! Batch 6: the two-step verification screens added next to the A2 forms —
//! recovery-email code entry (tdesktop `CloudPasswordEmailConfirm`),
//! "Forgot password?" with the emailed code, reset with the 7-day wait
//! and its cancel (`settings_cloud_password_input.cpp`), and the login
//! email (`settings_cloud_password_login_email*.cpp`).

use super::app::QuillApp;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputContentType, Textarea};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
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
        let notice = self.session()?.twofa_flow.notice?;
        Some(match notice {
            TwofaNotice::PasswordRemoved => "Two-step verification was disabled.".to_string(),
            TwofaNotice::PasswordRecovered => "Your cloud password was updated.".to_string(),
            TwofaNotice::ResetPending { reset_date } => format!(
                "You can reset your password in {}.",
                format_reset_duration(i64::from(reset_date) - now_secs())
            ),
            TwofaNotice::ResetDeclined { retry_date } => format!(
                "You recently requested a password reset that was canceled. Please wait {} before making a new request.",
                format_reset_duration(i64::from(retry_date) - now_secs())
            ),
            TwofaNotice::ResetCancelled => "Password reset canceled.".to_string(),
            TwofaNotice::LoginEmailChanged => "Your email has been changed.".to_string(),
            TwofaNotice::RecoveryEmailConfirmed => "Your recovery email is confirmed.".to_string(),
        })
    }

    /// Forget the one-time notice and any half-finished recovery step.
    pub(super) fn clear_twofa_flow(&mut self) {
        let flow = if let Some(live) = self.live.as_mut() {
            Some(&mut live.driver.session.twofa_flow)
        } else {
            self.demo_session.as_mut().map(|s| &mut s.twofa_flow)
        };
        if let Some(flow) = flow {
            flow.notice = None;
            flow.recovery_code_sent_to = None;
            flow.login_email_code_sent_to = None;
        }
        self.twofa_confirm = None;
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
            .child(heading("Confirm recovery email"))
            .child(muted_line(
                cx,
                format!("Confirmation code sent to {pattern}\u{2026}"),
            ))
            .child(
                Input::new(&self.twofa_code)
                    .aria_label("Recovery email confirmation code")
                    .h(px(40.)),
            )
            .when_some(self.twofa_notice.clone(), |this, note| {
                this.child(div().text_xs().text_color(danger()).child(note))
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("twofa-confirm-code")
                            .label("Confirm and Finish")
                            .primary()
                            .disabled(loading)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_twofa_email_code(window, cx);
                            })),
                    )
                    .child(
                        Button::new("twofa-resend-code")
                            .label("Resend code")
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
                            .label("Abort recovery email setup")
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
        let mut code = self.twofa_code.read(cx).value().trim().to_string();
        if code.is_empty() {
            self.twofa_notice = Some("enter the code from the email".into());
            cx.notify();
            return;
        }
        self.twofa_notice = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.check_recovery_email_code(&code);
        }
        code.zeroize();
        self.twofa_code
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
            "Off".to_string()
        } else {
            pattern
        };
        Some(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(div().font_semibold().text_sm().child("Login Email"))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_sm().child(shown))
                        .child(
                            Button::new("twofa-goto-login-email")
                                .label("Change email")
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
                .label("Forgot password?")
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
        let state = session.as_ref().and_then(|s| s.password_state.clone());
        let code_sent_to = session
            .as_ref()
            .and_then(|s| s.twofa_flow.recovery_code_sent_to.clone());
        let loading = session.as_ref().is_some_and(|s| s.password_state_loading);
        let Some(state) = state else {
            return body.child(muted_line(cx, "Loading\u{2026}"));
        };
        let mut body = body.child(heading("Forgot password?"));
        // A reset is already counting down.
        if state.pending_reset_date > 0 {
            let left = i64::from(state.pending_reset_date) - now_secs();
            if left > 0 {
                body = body
                    .child(div().text_sm().child(format!(
                        "You can reset your password in {}.",
                        format_reset_duration(left)
                    )))
                    .child(self.twofa_confirm_or(
                        cx,
                        "Cancel reset",
                        "Cancel the password reset process? If you request a new reset later, it will take another 7 days.",
                        TwofaConfirm::CancelReset,
                        "Yes",
                        "No",
                        loading,
                    ));
            } else {
                body = body
                    .child(div().text_sm().child(
                        "The waiting period is over. Resetting removes your password and keeps your account.",
                    ))
                    .child(
                        Button::new("twofa-reset-now")
                            .label("Reset password")
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
                .child(div().text_sm().child(format!(
                    "Please enter the code from the email {pattern}"
                )))
                .child(
                    Input::new(&self.twofa_code)
                        .aria_label("Password recovery code")
                        .h(px(40.)),
                )
                .child(div().mt_1().font_semibold().text_sm().child("New password"))
                .child(
                    Input::new(&self.twofa_new_password)
                        .aria_label("New two-step verification password")
                        .content_type(InputContentType::Password)
                        .h(px(40.)),
                )
                .child(
                    Textarea::new(&self.twofa_hint)
                        .aria_label("Password hint")
                        .h(px(40.)),
                )
                .child(muted_line(
                    cx,
                    "Leave the password empty to turn off two-step verification.",
                ))
                .when_some(self.twofa_notice.clone(), |this, note| {
                    this.child(div().text_xs().text_color(danger()).child(note))
                })
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("twofa-recover-submit")
                                .label("Save")
                                .primary()
                                .disabled(loading)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_twofa_recovery(window, cx);
                                })),
                        )
                        .child(
                            Button::new("twofa-recover-resend")
                                .label("Resend code")
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
                    "Unable to access your email?",
                    "If you don't have access to your recovery email, your remaining options are either to remember your password or wait 7 days until your password resets.",
                    TwofaConfirm::Reset { with_email: true },
                    "Reset",
                    "Cancel",
                    loading,
                ));
            return self.twofa_back_row(cx, body);
        }
        if state.has_recovery_email_address {
            body = body
                .child(div().text_sm().child(
                    "We'll send a code to your recovery email. It is the only way to recover a forgotten password.",
                ))
                .child(
                    Button::new("twofa-send-recovery-code")
                        .label("Send code")
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
                    "Unable to access your email?",
                    "If you don't have access to your recovery email, your remaining options are either to remember your password or wait 7 days until your password resets.",
                    TwofaConfirm::Reset { with_email: true },
                    "Reset",
                    "Cancel",
                    loading,
                ));
        } else {
            body = body.child(self.twofa_confirm_or(
                cx,
                "Reset password",
                "Since you didn't provide a recovery email when setting up your password, your remaining options are either to remember your password or wait 7 days until your password is reset.",
                TwofaConfirm::Reset { with_email: false },
                "Reset",
                "Cancel",
                loading,
            ));
        }
        self.twofa_back_row(cx, body)
    }

    fn twofa_back_row(&self, cx: &mut Context<Self>, body: Div) -> Div {
        body.child(
            Button::new("twofa-recover-back")
                .label("Back")
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
        if self.twofa_confirm != Some(confirm) {
            return div().child(
                Button::new(format!("twofa-confirm-open-{trigger_label}"))
                    .label(trigger_label)
                    .ghost()
                    .small()
                    .disabled(loading)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.twofa_confirm = Some(confirm);
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
                                this.twofa_confirm = None;
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
                            this.twofa_confirm = None;
                            cx.notify();
                        }),
                    )),
            )
    }

    /// Save the recovered password (or turn it off when empty).
    pub(super) fn submit_twofa_recovery(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut code = self.twofa_code.read(cx).value().trim().to_string();
        let mut new = self.twofa_new_password.read(cx).value().to_string();
        let hint = self.twofa_hint.read(cx).value().to_string();
        if code.is_empty() {
            self.twofa_notice = Some("enter the code from the email".into());
            code.zeroize();
            new.zeroize();
            cx.notify();
            return;
        }
        self.twofa_notice = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.recover_twofa_password(&code, &new, &hint);
        }
        code.zeroize();
        new.zeroize();
        for input in [&self.twofa_code, &self.twofa_new_password] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.twofa_hint
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    /// Login email: new address, then the code sent to it.
    pub(super) fn twofa_login_email_body(&self, cx: &mut Context<Self>, body: Div) -> Div {
        let session = self.session();
        let code_sent_to = session
            .as_ref()
            .and_then(|s| s.twofa_flow.login_email_code_sent_to.clone());
        let loading = session.as_ref().is_some_and(|s| s.password_state_loading);
        let body = if let Some(pattern) = code_sent_to {
            body.child(heading("Check Your New Email"))
                .child(div().text_sm().child(format!(
                    "Please enter the code we have sent to your new email {pattern}"
                )))
                .child(
                    Input::new(&self.twofa_code)
                        .aria_label("Login email confirmation code")
                        .h(px(40.)),
                )
                .when_some(self.twofa_notice.clone(), |this, note| {
                    this.child(div().text_xs().text_color(danger()).child(note))
                })
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("twofa-login-email-confirm")
                                .label("Confirm")
                                .primary()
                                .disabled(loading)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_login_email_code(window, cx);
                                })),
                        )
                        .child(
                            Button::new("twofa-login-email-resend")
                                .label("Resend code")
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
            body.child(heading("Enter New Email"))
                .child(muted_line(
                    cx,
                    "You will receive Telegram login codes via email and not SMS. Please enter an email address to which you have access.",
                ))
                .child(
                    Textarea::new(&self.twofa_email)
                        .aria_label("Login email address")
                        .h(px(40.)),
                )
                .when_some(self.twofa_notice.clone(), |this, note| {
                    this.child(div().text_xs().text_color(danger()).child(note))
                })
                .child(
                    Button::new("twofa-login-email-submit")
                        .label("Confirm")
                        .primary()
                        .disabled(loading)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_login_email(window, cx);
                        })),
                )
        };
        body.child(
            Button::new("twofa-login-email-back")
                .label("Back")
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.goto_twofa_view(TwofaView::Status);
                    cx.notify();
                })),
        )
    }

    fn submit_login_email(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let email = self.twofa_email.read(cx).value().trim().to_string();
        if email.is_empty() {
            self.twofa_notice = Some("enter the new login email".into());
            cx.notify();
            return;
        }
        self.twofa_notice = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.set_login_email(&email);
        }
        self.twofa_email
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    fn submit_login_email_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut code = self.twofa_code.read(cx).value().trim().to_string();
        if code.is_empty() {
            self.twofa_notice = Some("enter the code from the email".into());
            cx.notify();
            return;
        }
        self.twofa_notice = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.check_login_email_code(&code);
        }
        code.zeroize();
        self.twofa_code
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    /// Whether the two-step dialog should fall back to the status
    /// screen: a finished step reports there.
    pub(super) fn twofa_step_finished(&self) -> bool {
        self.session()
            .is_some_and(|s| s.twofa_flow.notice.is_some())
    }
}
