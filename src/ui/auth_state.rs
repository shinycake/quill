//! Sign-in: inputs, registration, terms, recovery and the QR code.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::input::InputState;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::composer::should_send_on_enter;
use quill::lang::{keys, tr};
use quill::telegram::envelope::{AuthorizationState, RegistrationTerms};

/// The sign-in flow's UI for the account on screen: its inputs, the
/// registration and terms steps, recovery and the QR code. A field of
/// `OnScreen`: a switch drops the typed phone, code and names.
pub(crate) struct AuthUi {
    pub(super) signin: super::signin_ui::SignInUi,
    pub(super) registration_first_input: Entity<TextareaState>,
    pub(super) registration_last_input: Entity<TextareaState>,
    /// The registration step and the terms the user ticked in it.
    pub(super) registration: RegistrationStep,
    /// The sign-in submit that is out, by its request.
    pub(super) submit: AuthSubmit,
    /// The proxy list as a box over the sign-in screen (tdesktop's
    /// `ProxiesBox`), which has no Settings window to hold it. It belongs to
    /// the sign-in screen: the account reaching Ready ends it
    /// (`follow_auth`), so it does not return if the pane connects again.
    pub(super) proxy_list_open: bool,
    pub(super) registration_notify_contacts: bool,
    pub(super) email_input: Entity<TextareaState>,
    pub(super) phone_input: Entity<TextareaState>,
    pub(super) code_input: Entity<TextareaState>,
    pub(super) password_input: Entity<InputState>,
    /// Slice A10: recovery-code entry for 2FA password recovery. The code
    /// is never stored beyond the input widget — it is zeroized after
    /// submit (the A2 rule).
    pub(super) recovery_code_input: Entity<TextareaState>,
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

/// The sign-in submit this screen sent. One goes at a time: a press while
/// one is out sends nothing ([`Self::send`]). The session is the one that
/// knows when it is answered: the submit is out while its own request is
/// still in the session's table, so another account's request, or an older
/// one, never reads as this one. The request numbers restart with a new
/// client, so the stage also holds the screen it was sent from: an `Out` of
/// another screen is no submit of this one.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthSubmit {
    #[default]
    Idle,
    Out {
        request: quill::ids::RequestId,
        screen: super::on_screen::ScreenEpoch,
    },
}

impl AuthSubmit {
    /// The submit's request is still unanswered in `requests`, and was
    /// sent from the screen `screen` that is on now.
    pub(super) fn is_out(
        self,
        requests: &quill::state::RequestRegistry,
        screen: super::on_screen::ScreenEpoch,
    ) -> bool {
        match self {
            Self::Idle => false,
            Self::Out {
                request,
                screen: sent_from,
            } => sent_from == screen && requests.get(request).is_some(),
        }
    }

    /// The stage after a look at the session's table: an answered request,
    /// a request of another screen, or no session at all, ends it, so an id
    /// never outlives its session and never reads as an unrelated request of
    /// the next one.
    pub(super) fn settle(
        &mut self,
        requests: Option<&quill::state::RequestRegistry>,
        screen: super::on_screen::ScreenEpoch,
    ) {
        if let Self::Out { .. } = *self
            && !requests.is_some_and(|requests| self.is_out(requests, screen))
        {
            *self = Self::Idle;
        }
    }

    /// Sends through `send` unless a submit is out already (`busy`, from
    /// [`Self::is_out`]): `None`, nothing sent. A send that went out holds
    /// the stage.
    pub(super) fn send<E>(
        &mut self,
        busy: bool,
        screen: super::on_screen::ScreenEpoch,
        send: impl FnOnce() -> Result<quill::ids::RequestId, E>,
    ) -> Option<Result<quill::ids::RequestId, E>> {
        if busy {
            return None;
        }
        let result = send();
        self.sent(&result, screen);
        Some(result)
    }

    /// A send that went out holds its request and its screen; a refused one
    /// leaves the stage as it was.
    pub(super) fn sent<E>(
        &mut self,
        result: &Result<quill::ids::RequestId, E>,
        screen: super::on_screen::ScreenEpoch,
    ) {
        if let Ok(request) = result {
            *self = Self::Out {
                request: *request,
                screen,
            };
        }
    }
}

/// The registration step by the authorization state: it exists only while
/// `WaitRegistration` does ([`AuthUi::follow_auth`] builds it from the
/// state), and what was ticked holds only for the terms it was ticked for.
#[derive(Default, Clone, PartialEq)]
pub(crate) enum RegistrationStep {
    /// Authorization is not at the registration step.
    #[default]
    Idle,
    /// The terms (none when there are none to accept) wait for a tick.
    Unticked(Option<RegistrationTerms>),
    /// The user accepted these terms; with none to accept, it starts so.
    Accepted(Option<RegistrationTerms>),
}

impl RegistrationStep {
    pub(super) fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted(_))
    }

    /// The terms the registration is sent with.
    pub(super) fn accepted_terms(&self) -> Option<&RegistrationTerms> {
        match self {
            Self::Accepted(terms) => terms.as_ref(),
            _ => None,
        }
    }

    /// The tick of the terms checkbox.
    pub(super) fn tick(&mut self, terms: &RegistrationTerms, on: bool) {
        let terms = Some(terms.clone());
        *self = if on {
            Self::Accepted(terms)
        } else {
            Self::Unticked(terms)
        };
    }
}

impl AuthUi {
    /// The authorization state moved on: the registration step follows it.
    /// A tick holds only while those same terms are asked.
    pub(super) fn follow_auth(&mut self, auth: &AuthorizationState) {
        if matches!(auth, AuthorizationState::Ready) {
            self.proxy_list_open = false;
        }
        let AuthorizationState::WaitRegistration { terms } = auth else {
            self.registration = RegistrationStep::Idle;
            return;
        };
        let same = matches!(
            &self.registration,
            RegistrationStep::Unticked(asked) | RegistrationStep::Accepted(asked) if asked == terms
        );
        if !same {
            self.registration = match terms {
                Some(_) => RegistrationStep::Unticked(terms.clone()),
                None => RegistrationStep::Accepted(None),
            };
        }
    }

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
            registration: RegistrationStep::Idle,
            submit: AuthSubmit::Idle,
            proxy_list_open: false,
            registration_notify_contacts: false,
            email_input,
            phone_input,
            code_input,
            password_input,
            recovery_code_input,
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
    pub(super) stage: super::connect_ui::ConnectStage,
    /// Screenshot / synthetic demo: show the matching auth field without a live client.
    pub(super) demo_inputs: bool,
}

impl AuthDemo {
    pub(super) fn new(
        stage: super::connect_ui::ConnectStage,
        demo: Option<ScreenshotDemo>,
    ) -> Self {
        Self {
            stage,
            demo_inputs: demo.is_some_and(|d| d.auth_inputs()),
        }
    }
}

/// The recording driver the sign-in, bottom-action and report tests send
/// through; test-only, declared here so the shared module list stays as is.
#[cfg(test)]
#[path = "driver_fixture.rs"]
pub(in crate::ui) mod driver_fixture;

#[cfg(test)]
mod submit_tests {
    use super::AuthSubmit;
    use crate::ui::on_screen::ScreenEpoch;
    use quill::state::{RequestPurpose, RequestRegistry};

    fn screen() -> ScreenEpoch {
        ScreenEpoch::default()
    }

    fn registry_with_reset() -> (RequestRegistry, quill::ids::RequestId) {
        let mut registry = RequestRegistry::default();
        let id = registry.register(
            quill::ids::AccountGeneration::default(),
            RequestPurpose::ResetAuthenticationEmail,
            None,
            None,
        );
        (registry, id)
    }

    /// A reset that went out holds the submit busy until its answer.
    #[test]
    fn a_sent_reset_holds_the_submit_busy_until_answered() {
        let (mut registry, id) = registry_with_reset();
        let mut submit = AuthSubmit::Idle;
        submit.sent::<()>(&Ok(id), screen());
        assert!(submit.is_out(&registry, screen()));
        registry.take(id);
        submit.settle(Some(&registry), screen());
        assert!(!submit.is_out(&registry, screen()));
        assert_eq!(submit, AuthSubmit::Idle, "the answer ends the stage");
    }

    fn driver_in(
        auth: &str,
    ) -> (
        quill::connect::ConnectDriver<std::sync::Arc<quill::connect::RecordingSender>>,
        std::sync::Arc<quill::connect::RecordingSender>,
    ) {
        crate::ui::auth_state::driver_fixture::driver_with(&[format!(
            r#"{{"@type":"updateAuthorizationState","authorization_state":{auth}}}"#
        )])
    }

    /// The real driver's reset holds the submit busy, and a second press
    /// while it is out sends nothing.
    #[test]
    fn the_reset_through_the_driver_holds_the_submit_and_blocks_a_second() {
        let (mut driver, recorder) = driver_in(
            r#"{"@type":"authorizationStateWaitEmailCode","code_info":{"email_address_pattern":"a***@b.c","length":5},"email_address_reset_state":{"@type":"emailAddressResetStateAvailable","wait_period":3600}}"#,
        );
        let before = recorder.snapshot().len();
        let mut submit = AuthSubmit::Idle;
        let busy = |submit: AuthSubmit, driver: &quill::connect::ConnectDriver<_>| {
            submit.is_out(&driver.session.requests, screen())
        };
        assert!(!busy(submit, &driver));
        let first = submit.send(false, screen(), || driver.reset_login_email());
        assert!(matches!(first, Some(Ok(_))), "it went out");
        assert!(busy(submit, &driver), "the submit is out");
        let second = submit.send(busy(submit, &driver), screen(), || driver.reset_login_email());
        assert!(second.is_none(), "a second press sends nothing");
        assert_eq!(
            crate::ui::auth_state::driver_fixture::sent_types(&recorder, before).len(),
            1
        );
    }

    /// A resend of the recovery code while the first request is out is not
    /// sent: it would overwrite the stage of the one still in flight.
    #[test]
    fn a_recovery_resend_waits_for_the_request_out() {
        let (mut driver, recorder) = driver_in(
            r#"{"@type":"authorizationStateWaitPassword","has_recovery_email_address":true}"#,
        );
        let before = recorder.snapshot().len();
        let mut submit = AuthSubmit::Idle;
        let first = submit.send(false, screen(), || driver.request_password_recovery());
        let Some(Ok(request)) = first else {
            panic!("the request went out: {first:?}");
        };
        assert_eq!(
            submit,
            AuthSubmit::Out {
                request,
                screen: screen()
            }
        );
        let busy = submit.is_out(&driver.session.requests, screen());
        assert!(
            submit
                .send(busy, screen(), || driver.request_password_recovery())
                .is_none()
        );
        assert_eq!(
            submit,
            AuthSubmit::Out {
                request,
                screen: screen()
            },
            "still the first"
        );
        assert_eq!(
            crate::ui::auth_state::driver_fixture::sent_types(&recorder, before).len(),
            1
        );
    }

    /// The request numbers restart with a new client: an `Out` sent from
    /// another screen is no submit of this one, though its number is still
    /// in the table (it names the new client's request).
    #[test]
    fn a_submit_of_another_screen_is_not_out() {
        let (registry, id) = registry_with_reset();
        let mut submit = AuthSubmit::Out {
            request: id,
            screen: screen(),
        };
        assert!(submit.is_out(&registry, screen()));
        assert!(!submit.is_out(&registry, screen().next()));
        submit.settle(Some(&registry), screen().next());
        assert_eq!(submit, AuthSubmit::Idle, "the next screen's look ends it");
    }

    /// A refused send leaves the stage as it was.
    #[test]
    fn a_refused_send_holds_nothing() {
        let (registry, _) = registry_with_reset();
        let mut submit = AuthSubmit::Idle;
        submit.sent(&Err::<quill::ids::RequestId, ()>(()), screen());
        assert!(!submit.is_out(&registry, screen()));
    }

    /// A replaced session ends the stage, so an old id never matches an
    /// unrelated request of the new session.
    #[test]
    fn a_replaced_session_ends_the_stage() {
        let (registry, id) = registry_with_reset();
        let mut submit = AuthSubmit::Out {
            request: id,
            screen: screen(),
        };
        submit.settle(None, screen());
        assert_eq!(submit, AuthSubmit::Idle);
        assert!(!submit.is_out(&registry, screen()));
    }
}
