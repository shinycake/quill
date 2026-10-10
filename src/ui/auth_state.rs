//! Sign-in: inputs, registration, terms, recovery and the QR code.

use super::*;
use gpui_kit::component::input::InputState;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::telegram::envelope::AuthorizationState;
use std::sync::Arc;

pub(crate) struct AuthUi {
    pub(super) signin: super::signin_ui::SignInUi,
    pub(super) registration_first_input: Entity<TextareaState>,
    pub(super) registration_last_input: Entity<TextareaState>,
    pub(super) accepted_registration_terms: Option<quill::telegram::envelope::RegistrationTerms>,
    pub(super) registration_notify_contacts: bool,
    pub(super) email_input: Entity<TextareaState>,
    pub(super) phone_input: Entity<TextareaState>,
    pub(super) code_input: Entity<TextareaState>,
    pub(super) password_input: Entity<InputState>,
    /// Slice A10: recovery-code entry for 2FA password recovery. The code
    /// is never stored beyond the input widget — it is zeroized after
    /// submit (the A2 rule).
    pub(super) recovery_code_input: Entity<TextareaState>,
    /// Slice A10: the password screen is showing recovery-code entry
    /// instead of password entry. Pure UI state, reset when auth leaves
    /// WaitPassword.
    pub(super) recovery_mode: bool,
    /// Batch 4: the attempts the user just terminated from the new-login
    /// alert ("New Login Prevented" box), until acknowledged.
    pub(super) login_prevented: Option<Vec<String>>,
    /// Batch 4: terms of service prompt state (decline flow, age check).
    pub(super) terms_step: TermsStep,
    pub(super) terms_age_ok: bool,
    pub(super) terms_age_error: bool,
    /// Slice A1: decoded QR-login bitmap cached by link, rebuilt only when
    /// the link changes. The link itself is never logged.
    pub(super) qr_login_cache: Option<(String, Arc<RenderImage>)>,
    pub(super) demo_state: AuthorizationState,
    /// Phase 1 (kit adoption): the note text a dismiss timer is already armed
    /// for. The permanent debug status bar is gone; `status_note` now shows
    /// as a kit notification (auto-dismissing) instead.    /// Screenshot / synthetic demo: show the matching auth field without a live client.
    pub(super) demo_inputs: bool,
}

impl AuthUi {
    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        email_input: Entity<TextareaState>,
        phone_input: Entity<TextareaState>,
        code_input: Entity<TextareaState>,
        password_input: Entity<InputState>,
        recovery_code_input: Entity<TextareaState>,
        auth_demo: AuthorizationState,
        demo: Option<ScreenshotDemo>,
    ) -> Self {
        let registration_first_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("First name")
                .auto_grow(1, 1)
        });
        let registration_last_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Last name (optional)")
                .auto_grow(1, 1)
        });
        let signin = super::signin_ui::SignInUi::new(window, cx);
        Self {
            signin,
            registration_first_input,
            registration_last_input,
            accepted_registration_terms: None,
            registration_notify_contacts: false,
            email_input,
            phone_input,
            code_input,
            password_input,
            recovery_code_input,
            recovery_mode: false,
            login_prevented: None,
            terms_step: Default::default(),
            terms_age_ok: false,
            terms_age_error: false,
            qr_login_cache: None,
            demo_state: auth_demo,
            demo_inputs: demo.is_some_and(|d| d.auth_inputs()),
        }
    }
}
