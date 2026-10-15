//! Connect driver: signing in with a passkey from the QR screen
//! (`crate::passkeys::login`). The request options from the server, the
//! ceremony's assertion back to it, and the app config the link depends
//! on, all while signed out.
use super::*;
use crate::ids::RequestId;
use crate::passkeys::PasskeysPurpose;
use crate::passkeys::login::{
    HANDLE_INVALID, check_authentication_passkey, get_authentication_passkey_parameters,
    user_handle_valid,
};
use crate::state::{RequestPurpose, SettingsPurpose, ShutdownPhase};
use crate::telegram::envelope::AuthorizationState;
use crate::webauthn::Assertion;
use std::time::Instant;

fn purpose(passkeys: PasskeysPurpose) -> RequestPurpose {
    RequestPurpose::Settings(SettingsPurpose::Passkeys(passkeys))
}

impl<S: JsonSender> ConnectDriver<S> {
    /// The states TDLib takes `checkAuthenticationPasskey` in without a
    /// sign-in query pending (`AuthManager::finish_passkey_login`); Quill
    /// offers the link on the QR screen.
    fn passkey_login_state(&self) -> bool {
        matches!(
            self.session.auth,
            AuthorizationState::WaitPhoneNumber
                | AuthorizationState::WaitOtherDeviceConfirmation { .. }
        ) && matches!(self.session.shutdown, ShutdownPhase::Running)
    }

    fn passkey_login_in_flight(&self, passkeys: PasskeysPurpose) -> bool {
        self.session.requests.has_purpose(purpose(passkeys))
    }

    /// "Log in using passkey" (tdesktop `setupPasskeyLink`'s click): the
    /// options of the last try while they are within their timeout, else
    /// `getAuthenticationPasskeyParameters`. Once known they wait in
    /// `login.ceremony` for the dialog. `Ok(None)` when no request was
    /// needed.
    pub fn begin_login(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let state = &self.session.settings.passkeys;
        if !self.passkey_login_state()
            || !state.login_link_shown()
            || state.login.checking
            || self.session.requests.has_auth_submit()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        if let Some(pending) = &state.login.pending
            && pending.fresh(Instant::now())
        {
            let options = pending.options.clone();
            let login = &mut self.session.settings.passkeys.login;
            login.ceremony = Some(options);
            login.error = None;
            return Ok(None);
        }
        if self.passkey_login_in_flight(PasskeysPurpose::LoginParameters) {
            return Ok(None);
        }
        let extra = self
            .session
            .request(purpose(PasskeysPurpose::LoginParameters), None);
        let login = &mut self.session.settings.passkeys.login;
        login.pending = None;
        login.error = None;
        self.send_json_request(extra, &get_authentication_passkey_parameters(extra))
            .map(Some)
    }

    /// Whether the link waits for the server's options or answer.
    pub fn passkey_login_pending(&self) -> bool {
        self.passkey_login_in_flight(PasskeysPurpose::LoginParameters)
            || self.session.settings.passkeys.login.checking
    }

    /// The ceremony signed in: `checkAuthenticationPasskey` (tdesktop
    /// `FinishPasskeyLogin`). A user handle that names no data centre
    /// fails here with `PASSKEY_HANDLE_INVALID`, as in tdesktop, and
    /// nothing is sent (`Ok(None)`).
    pub fn finish_login(
        &mut self,
        assertion: &Assertion,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.passkey_login_state() || self.session.settings.passkeys.login.checking {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !user_handle_valid(&assertion.user_handle) {
            self.session
                .settings
                .passkeys
                .login
                .fail(HANDLE_INVALID.into());
            return Ok(None);
        }
        let extra = self
            .session
            .request(purpose(PasskeysPurpose::CheckLogin), None);
        let login = &mut self.session.settings.passkeys.login;
        login.checking = true;
        login.error = None;
        let sent = self.send_json_request(extra, &check_authentication_passkey(extra, assertion));
        if sent.is_err() {
            self.session.settings.passkeys.login.checking = false;
        }
        sent.map(Some)
    }

    /// On the sign-in screens: `getApplicationConfig` once, for
    /// `settings_display_passkeys` (TDLib answers it before authorization;
    /// tdesktop's `AppConfig` refreshes before login too and shows the
    /// link once the value arrives).
    pub(crate) fn maybe_request_login_config(&mut self) {
        if !self.passkey_login_state() || self.session.settings.passkeys.login.config_requested {
            return;
        }
        let extra = self
            .session
            .request(purpose(PasskeysPurpose::LoginConfig), None);
        self.session.settings.passkeys.login.config_requested = true;
        let json = crate::telegram::requests::get_application_config(extra);
        if self.send_json_request(extra, &json).is_err() {
            self.session.settings.passkeys.login.config_requested = false;
        }
    }
}
