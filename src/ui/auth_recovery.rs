use super::*;

/// Slice A10: 2FA password recovery UI. "Forgot password?" on the auth
/// password screen sends `requestAuthenticationPasswordRecovery` (the code
/// is emailed); the screen switches to recovery-code entry, which submits
/// `recoverAuthenticationPassword`. The code is zeroized after the send and
/// never stored — the A2 rule (auth secrets ride the request JSON only).
///
/// Recovery removes 2FA (empty new password/hint); the status note points
/// at Settings → Two-Step Verification to re-enable it (slice A2).
/// `checkAuthenticationPasswordRecoveryCode` is deliberately not called:
/// `recoverAuthenticationPassword` validates the code itself, so the extra
/// round-trip adds nothing.

/// Slice A10: wire Enter in the recovery-code field to submit. Called
/// from `QuillApp::new_with_demo` next to the other auth-input
/// subscriptions.
pub(crate) fn subscribe_recovery_input(
    input: &Entity<TextareaState>,
    window: &mut Window,
    cx: &mut Context<QuillApp>,
) {
    cx.subscribe_in(
        input,
        window,
        |this, state, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { secondary, shift } = event {
                let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                if should_send_on_enter(quill::composer::enter_event_from_kit(
                    *shift, *secondary, marked,
                )) {
                    this.submit_recovery_code(window, cx);
                }
            }
        },
    )
    .detach();
}

impl QuillApp {
    /// Slice A10: start 2FA password recovery. No local cooldown: a
    /// too-early request fails server-side (429) and surfaces via
    /// `last_auth_error`.
    pub(crate) fn request_password_recovery(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(
            live.driver.session.auth,
            AuthorizationState::WaitPassword {
                has_recovery_email: true
            }
        ) {
            return;
        }
        match live.driver.request_password_recovery() {
            Ok(_) => {
                self.recovery_mode = true;
                self.recovery_code_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.status_note = "recovery code sent — check your email".into();
            }
            Err(_) => {
                self.status_note = "could not request a recovery code".into();
            }
        }
        cx.notify();
    }

    /// Slice A10: submit the emailed recovery code.
    pub(crate) fn submit_recovery_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(
            live.driver.session.auth,
            AuthorizationState::WaitPassword { .. }
        ) {
            return;
        }
        let mut code = self.recovery_code_input.read(cx).value().to_string();
        let result = live.driver.submit_recovery_code(&code);
        code.zeroize();
        match result {
            Ok(_) => {
                self.recovery_code_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.status_note =
                    "recovery code submitted — 2FA will be removed; re-enable it in Settings"
                        .into();
            }
            Err(_) => {
                self.status_note = "could not submit recovery code".into();
            }
        }
        cx.notify();
    }

    /// Slice A10: leave recovery-code entry, back to password entry.
    pub(crate) fn cancel_password_recovery(&mut self, cx: &mut Context<Self>) {
        self.recovery_mode = false;
        cx.notify();
    }

    /// Slice A10: the auth password-screen section — password entry, or
    /// recovery-code entry when `recovery_mode` is set. "Forgot password?"
    /// only appears when the server advertised a recovery email.
    pub(crate) fn auth_password_section(
        &self,
        recovery_mode: bool,
        has_recovery_email: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        if recovery_mode {
            div()
                .child(
                    div()
                        .mt_2()
                        .font_semibold()
                        .text_sm()
                        .child("Recovery code"),
                )
                .child(Textarea::new(&self.recovery_code_input).h(px(40.)))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Sent to TDLib only — never logged"),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("submit-recovery-code")
                                .label("Submit recovery code")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_recovery_code(window, cx);
                                })),
                        )
                        .child(
                            Button::new("resend-recovery-code")
                                .label("Resend code")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.request_password_recovery(window, cx);
                                })),
                        )
                        .child(
                            Button::new("cancel-recovery")
                                .label("Back")
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cancel_password_recovery(cx);
                                })),
                        ),
                )
        } else {
            div()
                .child(
                    div()
                        .mt_2()
                        .font_semibold()
                        .text_sm()
                        .child("Two-step password"),
                )
                .child(Textarea::new(&self.password_input).h(px(40.)))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Sent to TDLib only — never logged"),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("submit-password")
                                .label("Submit password")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_password(window, cx);
                                })),
                        )
                        .when(has_recovery_email, |this| {
                            this.child(
                                Button::new("forgot-password")
                                    .label("Forgot password?")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.request_password_recovery(window, cx);
                                    })),
                            )
                        }),
                )
        }
    }
}
