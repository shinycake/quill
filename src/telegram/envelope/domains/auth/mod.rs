//! TDLib updates and answers for sign-in, registration and the TDLib session lifecycle.
mod parse;

use crate::telegram::envelope::*;
pub(crate) use parse::parse_auth_payload;

/// Payloads for sign-in, registration and the TDLib session lifecycle; wrapped as
/// [`EnvelopePayload::Auth`].
#[derive(Debug, Clone, PartialEq)]
pub enum AuthPayload {
    UpdateAuthorizationState(AuthorizationState),
    /// Batch 4: `updateTermsOfService` — terms that must be accepted.
    UpdateTermsOfService {
        terms: TermsOfService,
    },
    /// Batch 6: `emailAddressAuthenticationCodeInfo` — the answer of
    /// `requestPasswordRecovery` / `setLoginEmailAddress` /
    /// `resendLoginEmailAddressCode`.
    EmailCodeInfo {
        pattern: String,
        length: i32,
    },
    /// Slice A8: `authenticationCodeInfo` — the `sendPhoneNumberCode` /
    /// `resendPhoneNumberCode` answer (schema 1.8.67, line 78). Stored in
    /// `Session::change_number_phone` / `change_number_timeout` when the
    /// pending purpose is `SendPhoneNumberCode` / `ResendPhoneNumberCode`.
    /// The `type` / `next_type` variants are not kept: this slice is
    /// backend-only and the UI half will parse them when it ships.
    AuthenticationCodeInfo {
        phone_number: String,
        timeout: i32,
    },
    /// `countries` (`getCountries` answer), stored in `Session::countries`.
    Countries {
        countries: Vec<crate::phone::Country>,
    },
}
