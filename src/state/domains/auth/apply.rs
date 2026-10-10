//! Applies TDLib updates and answers for sign-in, registration and the TDLib session lifecycle.
use crate::state::*;
use crate::telegram::envelope::AuthPayload;

impl Session {
    /// Applies one auth payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_auth_payload(
        &mut self,
        payload: AuthPayload,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match payload {
            AuthPayload::UpdateAuthorizationState(state) => self.set_auth(state),
            AuthPayload::UpdateTermsOfService { terms } => {
                self.notices.terms = Some(terms);
                self.notices.terms_error = None;
            }
            AuthPayload::EmailCodeInfo { pattern, .. } => {
                // Batch 6: only our own in-flight 2FA step takes the
                // answer (matched by `@extra`).
                if let Some(RequestPurpose::Auth(AuthPurpose::PasswordStateOp { op })) =
                    pending.map(|p| p.purpose)
                {
                    self.password_state_loading = false;
                    self.password_op_error = None;
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
                ) {
                    self.change_number_phone = Some(phone_number);
                    self.change_number_timeout = Some(timeout);
                    self.change_number_loading = false;
                    self.change_number_error = None;
                }
            }
            // Phase C2g: `joinVideoChat` returns `text` — the tgcalls
            // join answer, stored on the tracked call and consumed by
            // the driver pump (`ntg_connect`).
            // `startGroupCallScreenSharing` returns `text` — the
            // presentation answer, consumed by the driver pump.
            AuthPayload::Countries { countries } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCountries)
                    && !countries.is_empty()
                {
                    self.countries = Some(countries);
                }
            }
        }
    }
}
