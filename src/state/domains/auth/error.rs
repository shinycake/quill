//! Failed requests for sign-in, registration and the TDLib session lifecycle.
use crate::auth::StartupFailure;
use crate::state::requests::LOST_REQUEST_ERROR_CODE;
use crate::state::*;

impl Session {
    /// Reacts to a failed auth request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_auth_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        _seq: u64,
    ) {
        match pending.map(|p| p.purpose) {
            // TDLib refused the `logOut`: the account is still signed in,
            // so the close the send began is undone and the chat list
            // loads again. Without this the session stayed in
            // `CloseRequested` with nothing to move it on. A quit asked
            // for meanwhile (`begin_close` forgets the request) stays.
            Some(RequestPurpose::LogOut)
                if extra.is_some_and(|id| self.shutdown == ShutdownPhase::LogoutRequested(id)) =>
            {
                self.shutdown = if matches!(self.auth, AuthorizationState::Ready) {
                    ShutdownPhase::LogoutRefused(err.clone())
                } else {
                    ShutdownPhase::CloseRequested
                };
            }
            // Slice A2: a failed 2FA management request clears the
            // in-flight flag and parks the honest, classified
            // error line on the overlay — never a fake success,
            // never an optimistic state change.
            Some(RequestPurpose::Auth(AuthPurpose::PasswordStateOp { op })) => {
                // A late error of an older request leaves the one in flight
                // (and its line) alone.
                self.auth_state
                    .fail_password_op(pending.map(|p| p.id), password_op_error_line(op, err));
            }
            // The country list failed: the boxes say so and offer Retry (a
            // user action, never a frame), instead of "Loading countries"
            // for good.
            Some(RequestPurpose::GetCountries) => {
                self.auth_state
                    .countries
                    .failed(pending.map(|p| p.id), error_reason(err), false);
            }
            // The default guess stays unknown; the picker needs none.
            Some(RequestPurpose::GetCountryCode) => {
                self.auth_state.guessed_country.failed(
                    pending.map(|p| p.id),
                    error_reason(err),
                    false,
                );
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
                if let Some(request) = extra {
                    self.auth_state.change_number.fail(
                        request,
                        sessions_error_line("send the verification code", err),
                    );
                }
            }
            Some(RequestPurpose::CheckPhoneNumberCode) => {
                if let Some(request) = extra {
                    self.auth_state.change_number.fail(
                        request,
                        sessions_error_line("check the verification code", err),
                    );
                }
            }
            // TDLib answers and stays in WaitTdlibParameters, which sends no
            // update: without this the card says "Connecting" forever.
            Some(RequestPurpose::SetParameters) if err.code != LOST_REQUEST_ERROR_CODE => {
                self.set_auth(AuthorizationState::StartupFailed(if err.code == 401 {
                    StartupFailure::WrongKey
                } else {
                    StartupFailure::DataFolder {
                        detail: err.database_detail.clone(),
                    }
                }));
            }
            _ => {}
        }
    }
}
