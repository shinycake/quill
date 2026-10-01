//! Connect driver: phone auth and two-step verification.
use super::*;
use crate::ids::RequestId;
use crate::state::{PasswordOp, RequestPurpose};
use crate::telegram::envelope::AuthorizationState;
use crate::telegram::requests::{
    cancel_recovery_email_address_verification, check_authentication_code,
    check_authentication_email_code, check_authentication_password, check_phone_number_code,
    get_password_state, recover_authentication_password, request_authentication_password_recovery,
    request_qr_code_authentication, resend_authentication_code, resend_phone_number_code,
    resend_recovery_email_address_code, send_phone_number_code, set_authentication_email_address,
    set_authentication_phone_number, set_password, set_recovery_email_address,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Slice A8: send `sendPhoneNumberCode` with
    /// `phoneNumberCodeTypeChange` (schema 1.8.67, line 14877) — the
    /// while-authorized change-number flow, NOT the auth flow. Guarded on
    /// the authorized chats path; one change-number op at a time (the
    /// server aborts the previous verification anyway). The phone number
    /// and any later code ride the request JSON only — never stored on
    /// the session or diagnostics (the A2 rule; the target number lives
    /// in `change_number_phone` because the code-entry UI needs it).
    pub fn send_phone_number_code(
        &mut self,
        phone_number: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active()
            || self.session.change_number_loading
            || self.session.change_number_checking
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.change_number_error = None;
        let extra = self
            .session
            .request(RequestPurpose::SendPhoneNumberCode, None);
        self.session.change_number_loading = true;
        match self
            .sender
            .send_json(&send_phone_number_code(extra, phone_number))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.change_number_loading = false;
                Err(err)
            }
        }
    }

    /// Slice A8: send `resendPhoneNumberCode` (schema 1.8.67, line
    /// 14888). Requires a code already sent (the timeout in
    /// `change_number_timeout` gates the UI); one change-number op at a
    /// time.
    pub fn resend_phone_number_code(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active()
            || self.session.change_number_phone.is_none()
            || self.session.change_number_loading
            || self.session.change_number_checking
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.change_number_error = None;
        let extra = self
            .session
            .request(RequestPurpose::ResendPhoneNumberCode, None);
        self.session.change_number_loading = true;
        match self.sender.send_json(&resend_phone_number_code(extra)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.change_number_loading = false;
                Err(err)
            }
        }
    }

    /// Slice A8: send `checkPhoneNumberCode` (schema 1.8.67, line 14891).
    /// Requires a code already sent; one check at a time. The code rides
    /// the request JSON only — never stored on the session or
    /// diagnostics (the A2 rule).
    pub fn check_phone_number_code(&mut self, code: &str) -> Result<RequestId, ConnectSendError> {
        // Intentional asymmetry with the send guard: a check may run during a
        // send/resend because resend is number-stable, a fresh send aborts the
        // previous verification server-side (a stale check gets an honest
        // server refusal), and a user may verify an already-received code
        // while a resend round-trips.
        if !self.chats_path_active() || self.session.change_number_checking {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.change_number_phone.is_none() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.change_number_error = None;
        let extra = self
            .session
            .request(RequestPurpose::CheckPhoneNumberCode, None);
        self.session.change_number_checking = true;
        match self.sender.send_json(&check_phone_number_code(extra, code)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.change_number_checking = false;
                Err(err)
            }
        }
    }

    /// Send `setAuthenticationPhoneNumber` when auth is WaitPhoneNumber.
    /// Phone value is never stored on the session or diagnostics.
    pub fn submit_phone(&mut self, phone: &str) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitPhoneNumber) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let phone = phone.trim();
        if phone.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self.session.request(RequestPurpose::SetPhoneNumber, None);
        self.sender
            .send_json(&set_authentication_phone_number(extra, phone))?;
        Ok(extra)
    }

    /// Explicit email submission, only when requested by TDLib.
    pub fn submit_email(&mut self, email: &str) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitEmailAddress) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let email = email.trim();
        let valid = email.split_once('@').is_some_and(|(local, host)| {
            !local.is_empty() && !host.is_empty() && !host.contains('@')
        });
        if !valid || email.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.login_request(RequestPurpose::SetAuthenticationEmail, |id| {
            set_authentication_email_address(id, email)
        })
    }

    /// Shared phone/email code entry; never retains the submitted code.
    pub fn submit_code(&mut self, code: &str) -> Result<RequestId, ConnectSendError> {
        let purpose = match self.session.auth {
            AuthorizationState::WaitCode { .. } => RequestPurpose::CheckAuthenticationCode,
            AuthorizationState::WaitEmailCode { .. } => {
                RequestPurpose::CheckAuthenticationEmailCode
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let code = code.trim();
        if code.is_empty() || code.chars().any(char::is_control) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.login_request(purpose, |id| {
            if purpose == RequestPurpose::CheckAuthenticationEmailCode {
                check_authentication_email_code(id, code)
            } else {
                check_authentication_code(id, code)
            }
        })
    }

    pub fn resend_code(&mut self) -> Result<RequestId, ConnectSendError> {
        if !matches!(
            self.session.auth,
            AuthorizationState::WaitCode { .. } | AuthorizationState::WaitEmailCode { .. }
        ) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.login_request(
            RequestPurpose::ResendAuthenticationCode,
            resend_authentication_code,
        )
    }

    fn login_request(
        &mut self,
        purpose: RequestPurpose,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        if self.session.requests.has_purpose(purpose) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&build(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Send `requestQrCodeAuthentication` when auth is WaitPhoneNumber.
    /// TDLib answers with `updateAuthorizationState` carrying
    /// `authorizationStateWaitOtherDeviceConfirmation` (with the QR link).
    pub fn request_qr_login(&mut self) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitPhoneNumber) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self
            .session
            .request(RequestPurpose::RequestQrCodeAuthentication, None);
        self.sender
            .send_json(&request_qr_code_authentication(extra))?;
        Ok(extra)
    }

    /// Slice A2: shared send path for every 2FA management request. All
    /// five answer `passwordState`; one request is in flight at a time
    /// so double-clicks can't double-send a password change. Passwords
    /// are never stored on the session or diagnostics — they ride the
    /// request JSON only.
    fn password_op_send(
        &mut self,
        op: PasswordOp,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.password_state_loading {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.password_op_error = None;
        let extra = self
            .session
            .request(RequestPurpose::PasswordStateOp { op }, None);
        self.session.password_state_loading = true;
        match self.sender.send_json(&build(extra)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.password_state_loading = false;
                Err(err)
            }
        }
    }

    /// Slice A2: send `getPasswordState` (schema 1.8.67, line 11426).
    /// Cached state is reused and an in-flight fetch is never
    /// duplicated; `password_op_send` enforces the connection gate.
    /// `Ok(None)` = no request needed.
    pub fn fetch_password_state(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if self.session.password_state.is_some() || self.session.password_state_loading {
            return Ok(None);
        }
        self.password_op_send(PasswordOp::Fetch, get_password_state)
            .map(Some)
    }

    /// Slice A2: send `setPassword` (schema 1.8.67, line 11434) — enable
    /// (empty `old_password`, the TGX MODE_NEW convention), change, or
    /// disable (empty `new_password`). `recovery_email` is sent in the
    /// same call on first-time enable, like TGX's password controller;
    /// otherwise `None`. Not trimmed: spaces can be significant.
    pub fn set_two_step_password(
        &mut self,
        old_password: &str,
        new_password: &str,
        new_hint: &str,
        recovery_email: Option<&str>,
    ) -> Result<RequestId, ConnectSendError> {
        let op = if new_password.is_empty() {
            PasswordOp::DisablePassword
        } else {
            PasswordOp::SetPassword
        };
        // Change/disable need the current password. A doomed request is
        // rejected before it leaves; TDLib validates the rest honestly.
        if new_password.is_empty() && old_password.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.password_op_send(op, |extra| {
            set_password(extra, old_password, new_password, new_hint, recovery_email)
        })
    }

    /// Slice A2: send `setRecoveryEmailAddress` (schema 1.8.67, line
    /// 11458). Requires the current two-step password; the change is
    /// not applied until the new address is confirmed.
    pub fn set_recovery_email(
        &mut self,
        password: &str,
        email: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if password.is_empty() || email.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.password_op_send(PasswordOp::SetRecoveryEmail, |extra| {
            set_recovery_email_address(extra, password, email)
        })
    }

    /// Slice A2: send `resendRecoveryEmailAddressCode` (schema 1.8.67,
    /// line 11464). Only meaningful while an email confirmation is
    /// pending; TDLib enforces its own server-side cooldown (429 on
    /// too-early resend), so no local countdown is invented.
    pub fn resend_recovery_email_code(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self
            .session
            .password_state
            .as_ref()
            .is_some_and(|s| s.pending_email_pattern.is_some())
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.password_op_send(PasswordOp::ResendCode, resend_recovery_email_address_code)
    }

    /// Slice A2: send `cancelRecoveryEmailAddressVerification` (schema
    /// 1.8.67, line 11467). Only meaningful while an email confirmation
    /// is pending.
    pub fn cancel_recovery_email_setup(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self
            .session
            .password_state
            .as_ref()
            .is_some_and(|s| s.pending_email_pattern.is_some())
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.password_op_send(
            PasswordOp::AbortEmailSetup,
            cancel_recovery_email_address_verification,
        )
    }

    /// Send `checkAuthenticationPassword` when auth is WaitPassword.
    /// The password is never stored on the session or diagnostics. Not trimmed
    /// (leading/trailing spaces can be significant).
    pub fn submit_password(&mut self, password: &str) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitPassword { .. }) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if password.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self
            .session
            .request(RequestPurpose::CheckAuthenticationPassword, None);
        self.sender
            .send_json(&check_authentication_password(extra, password))?;
        Ok(extra)
    }

    /// Slice A10: send `requestAuthenticationPasswordRecovery` when auth
    /// is WaitPassword and the server advertised a recovery email. The
    /// resend path re-issues this same call — TDLib enforces the
    /// server-side cooldown (429 surfaces via `last_auth_error`), so no
    /// local countdown is invented.
    pub fn request_password_recovery(&mut self) -> Result<RequestId, ConnectSendError> {
        if !matches!(
            self.session.auth,
            AuthorizationState::WaitPassword {
                has_recovery_email: true
            }
        ) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self
            .session
            .request(RequestPurpose::RequestAuthenticationPasswordRecovery, None);
        self.sender
            .send_json(&request_authentication_password_recovery(extra))?;
        Ok(extra)
    }

    /// Slice A10: send `recoverAuthenticationPassword` with the emailed
    /// recovery code. The code rides the request JSON only — it is never
    /// stored on the session or diagnostics (the A2 rule); the caller
    /// zeroizes its copy after the send.
    pub fn submit_recovery_code(&mut self, code: &str) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitPassword { .. }) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if code.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self
            .session
            .request(RequestPurpose::RecoverAuthenticationPassword, None);
        self.sender
            .send_json(&recover_authentication_password(extra, code))?;
        Ok(extra)
    }
}
