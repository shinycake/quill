//! Connect driver: signing in with a passkey from the QR screen
//! (`crate::passkeys::login`). The request options from the server, the
//! ceremony's assertion back to it, and the app config the link depends
//! on, all while signed out.
use super::*;
use crate::ids::RequestId;
use crate::passkeys::PasskeysPurpose;
use crate::passkeys::login::{
    HANDLE_INVALID, LoginStage, check_authentication_passkey,
    get_authentication_passkey_parameters, user_handle_valid,
};
use crate::state::{RequestPurpose, SettingsPurpose};
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
        ) && self.session.shutdown.is_running()
    }

    /// "Log in using passkey" (tdesktop `setupPasskeyLink`'s click): the
    /// options of the last try while they are within their timeout, else
    /// `getAuthenticationPasskeyParameters`. Once known they wait in
    /// `login.stage` for the dialog. `Ok(None)` when no request was
    /// needed.
    pub fn begin_login(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let state = &self.session.settings.passkeys;
        let blocked = !self.passkey_login_state()
            || !state.login_link_shown()
            || matches!(state.login.stage, LoginStage::Checking(_));
        if blocked {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .settings
            .passkeys
            .login
            .reuse_options(Instant::now())
        {
            return Ok(None);
        }
        if matches!(
            self.session.settings.passkeys.login.stage,
            LoginStage::FetchingOptions(_)
        ) {
            return Ok(None);
        }
        let extra = self
            .session
            .request(purpose(PasskeysPurpose::LoginParameters), None);
        self.send_json_request(extra, &get_authentication_passkey_parameters(extra))?;
        // Only an ask that went out is waited for; the old options and
        // error go with it.
        self.session.settings.passkeys.login.stage = LoginStage::FetchingOptions(extra);
        Ok(Some(extra))
    }

    /// Whether the link waits for the server's options or answer.
    pub fn passkey_login_pending(&self) -> bool {
        self.session.settings.passkeys.login.is_waiting()
    }

    /// A step whose request is gone without an answer (a dropped
    /// connection) ends instead of waiting for good.
    pub(crate) fn drop_lost_passkey_login(&mut self) {
        let requests = &self.session.requests;
        self.session
            .settings
            .passkeys
            .login
            .drop_lost(|id| requests.get(id).is_some());
    }

    /// The ceremony signed in: `checkAuthenticationPasskey` (tdesktop
    /// `FinishPasskeyLogin`). A user handle that names no data centre
    /// fails here with `PASSKEY_HANDLE_INVALID`, as in tdesktop, and
    /// nothing is sent (`Ok(None)`).
    pub fn finish_login(
        &mut self,
        assertion: &Assertion,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.passkey_login_state()
            || matches!(
                self.session.settings.passkeys.login.stage,
                LoginStage::Checking(_)
            )
        {
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
        self.send_json_request(extra, &check_authentication_passkey(extra, assertion))?;
        self.session.settings.passkeys.login.stage = LoginStage::Checking(extra);
        Ok(Some(extra))
    }

    /// On the sign-in screens: `getApplicationConfig` once, for
    /// `settings_display_passkeys` (TDLib answers it before authorization;
    /// tdesktop's `AppConfig` refreshes before login too and shows the
    /// link once the value arrives).
    pub(crate) fn maybe_request_login_config(&mut self) {
        if !self.passkey_login_state() || !self.session.settings.passkeys.login.config.is_idle() {
            return;
        }
        let extra = self
            .session
            .request(purpose(PasskeysPurpose::LoginConfig), None);
        let json = crate::telegram::requests::get_application_config(extra);
        // A send that failed leaves the ask open for the next ingest.
        if self.send_json_request(extra, &json).is_ok() {
            let config = &mut self.session.settings.passkeys.login.config;
            config.begin_with(extra);
        }
    }
}
