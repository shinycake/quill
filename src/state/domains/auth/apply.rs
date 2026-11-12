//! Applies TDLib updates and answers for sign-in, registration and the TDLib session lifecycle.
use crate::state::*;
use crate::telegram::envelope::AuthPayload;

impl Session {
    /// Leave recovery-code entry for password entry (the Back button).
    pub fn cancel_password_recovery(&mut self) {
        self.auth_state.cancel_password_recovery();
    }

    /// Applies one auth payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_auth_payload(
        &mut self,
        payload: AuthPayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        _seq: u64,
    ) {
        match payload {
            AuthPayload::UpdateAuthorizationState(state) => self.set_auth(state),
            AuthPayload::UpdateTermsOfService { terms } => {
                self.settings.notices.terms.offer(terms);
            }
            AuthPayload::EmailCodeInfo { pattern, .. } => {
                // Batch 6: only our own in-flight 2FA step takes the
                // answer (matched by `@extra`).
                if let Some(RequestPurpose::Auth(AuthPurpose::PasswordStateOp { op })) =
                    pending.map(|p| p.purpose)
                {
                    self.auth_state.settle_password_op(pending.map(|p| p.id));
                    self.apply_email_code_info(op, pattern);
                }
            }
            AuthPayload::AuthenticationCodeInfo {
                phone_number,
                timeout,
            } => {
                // Slice A8: the `sendPhoneNumberCode` /
                // `resendPhoneNumberCode` answer — only our own in-flight
                // request writes the pending number/timeout (matched by
                // `@extra`); a late answer for a superseded send has no
                // pending entry and is ignored.
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(
                        RequestPurpose::SendPhoneNumberCode | RequestPurpose::ResendPhoneNumberCode
                    )
                ) && let Some(request) = extra
                {
                    self.auth_state.change_number.code_sent(
                        request,
                        CodeSent {
                            phone: phone_number,
                            timeout,
                        },
                    );
                }
            }
            // Phase C2g: `joinVideoChat` returns `text` — the tgcalls
            // join answer, stored on the tracked call and consumed by
            // the driver pump (`CallEngine::connect_group_call`).
            // `startGroupCallScreenSharing` returns `text` — the
            // presentation answer, consumed by the driver pump.
            AuthPayload::Countries { countries } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCountries) {
                    let id = pending.map(|p| p.id);
                    if countries.is_empty() {
                        self.auth_state.countries.failed(id, "empty list", false);
                    } else {
                        self.auth_state.countries.answered(id, countries);
                    }
                }
            }
        }
    }
}
