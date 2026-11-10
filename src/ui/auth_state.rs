//! Sign-in: inputs, registration, terms, recovery and the QR code.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::input::InputState;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::composer::should_send_on_enter;
use quill::lang::{keys, tr};
use quill::telegram::envelope::AuthorizationState;

/// The sign-in flow's UI for the account on screen: its inputs, the
/// registration and terms steps, recovery and the QR code. A field of
/// `OnScreen`: a switch drops the typed phone, code and names.
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
    pub(super) terms_prompt: Option<TermsPrompt>,
    /// Slice A1: decoded QR-login bitmap cached by link, rebuilt only when
    /// the link changes. The link itself is never logged.
    pub(super) qr_login_cache: Option<(String, super::owned_image::OwnedImage)>,
}

/// Enter in a sign-in field submits that step (not a chat composer: the
/// send-key setting does not apply). A macro, as the fields are two kit
/// types.
macro_rules! submit_on_enter {
    ($input:expr, $window:expr, $cx:expr, $submit:path) => {
        $cx.subscribe_in(
            $input,
            $window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter,
                    ) {
                        $submit(this, window, cx);
                    }
                }
            },
        )
        .detach();
    };
}

impl AuthUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let email_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_EMAIL_ADDRESS))
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let phone_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_PHONE_COUNTRY_CODE))
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        // Group the digits as the user types or pastes.
        cx.subscribe_in(
            &phone_input,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.signin_phone_changed(window, cx);
                }
            },
        )
        .detach();
        let code_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_VERIFICATION_CODE))
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let password_input = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(tr(keys::Q_MISC_TWO_STEP_VERIFICATION_PASSWORD))
                .submit_on_enter(true)
        });
        let recovery_code_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_RECOVERY_CODE_FROM_EMAIL))
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        submit_on_enter!(&email_input, window, cx, QuillApp::submit_email);
        submit_on_enter!(&phone_input, window, cx, QuillApp::submit_phone);
        submit_on_enter!(&code_input, window, cx, QuillApp::submit_code);
        submit_on_enter!(&password_input, window, cx, QuillApp::submit_password);
        auth_recovery::subscribe_recovery_input(&recovery_code_input, window, cx);
        let registration_first_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_FIRST_NAME))
                .auto_grow(1, 1)
        });
        let registration_last_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_LAST_NAME_OPTIONAL))
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
            terms_prompt: None,
            qr_login_cache: None,
        }
    }
}

/// Inputs and flags of the sign-in switched away from; a client still
/// signing in is closed by the retire itself (`background_status`).
impl super::on_screen::Farewell for AuthUi {}

/// The synthetic mode's authorization state (no client, no demo session)
/// and whether a screenshot demo shows the sign-in inputs. The process's,
/// next to `demo_session`: a `QuillApp` field (`auth_demo`).
pub(crate) struct AuthDemo {
    pub(super) demo_state: AuthorizationState,
    /// Screenshot / synthetic demo: show the matching auth field without a live client.
    pub(super) demo_inputs: bool,
}

impl AuthDemo {
    pub(super) fn new(demo_state: AuthorizationState, demo: Option<ScreenshotDemo>) -> Self {
        Self {
            demo_state,
            demo_inputs: demo.is_some_and(|d| d.auth_inputs()),
        }
    }
}
