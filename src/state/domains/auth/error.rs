//! Failed requests for sign-in, registration and the TDLib session lifecycle.
use crate::state::*;

impl Session {
    /// Reacts to a failed auth request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_auth_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match pending.map(|p| p.purpose) {
            // Slice A2: a failed 2FA management request clears the
            // in-flight flag and parks the honest, classified
            // error line on the overlay — never a fake success,
            // never an optimistic state change.
            Some(RequestPurpose::Auth(AuthPurpose::PasswordStateOp { op })) => {
                self.auth_state.password_state_loading = false;
                self.auth_state.password_op_error = Some(password_op_error_line(op, err));
            }
            // Slice A8: a refused change-number op (code send /
            // resend, code check) clears the in-flight flags and
            // parks the honest, classified error line — never a
            // fake success. The pending number and timeout stay
            // put: a failed send never reached the server
            // (transport error), and a refused resend does not
            // abort the existing verification, so the pending
            // code is still valid — the user can retry or resend.
            // A failed check likewise keeps the pending number.
            Some(RequestPurpose::SendPhoneNumberCode | RequestPurpose::ResendPhoneNumberCode) => {
                self.auth_state.change_number_loading = false;
                self.auth_state.change_number_error =
                    Some(sessions_error_line("send the verification code", err));
            }
            Some(RequestPurpose::CheckPhoneNumberCode) => {
                self.auth_state.change_number_checking = false;
                self.auth_state.change_number_error =
                    Some(sessions_error_line("check the verification code", err));
            }
            _ => {}
        }
    }
}
