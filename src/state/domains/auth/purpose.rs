//! Request purposes for sign-in, registration and the TDLib session lifecycle.
use crate::state::request_purpose::flat_purposes;
use crate::state::*;

/// In-flight requests for sign-in, registration and the TDLib session lifecycle; wrapped as
/// [`RequestPurpose::Auth`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthPurpose {
    GetAuthorizationState,
    SetParameters,
    SetPhoneNumber,
    CheckAuthenticationCode,
    SetAuthenticationEmail,
    RegisterUser,
    CheckAuthenticationEmailCode,
    CheckAuthenticationPassword,
    /// Slice A1: `resendAuthenticationCode` from the code-entry screen.
    ResendAuthenticationCode,
    /// Slice A10: `requestAuthenticationPasswordRecovery` from the
    /// password screen ("Forgot password?"). Resend re-issues this call —
    /// TDLib enforces the server-side cooldown, no local countdown.
    RequestAuthenticationPasswordRecovery,
    /// Slice A10: `recoverAuthenticationPassword` with the emailed code.
    RecoverAuthenticationPassword,
    /// Slice A1: `requestQrCodeAuthentication` from the phone screen.
    RequestQrCodeAuthentication,
    /// `resetAuthenticationEmailAddress` on the email-code step.
    ResetAuthenticationEmail,
    /// `getCountries` for the sign-in country picker.
    GetCountries,
    /// `getCountryCode`: the default country guess for the phone screen.
    GetCountryCode,
    /// Slice A2: a 2FA management request (`getPasswordState` /
    /// `setPassword` / `setRecoveryEmailAddress` /
    /// `resendRecoveryEmailAddressCode` /
    /// `cancelRecoveryEmailAddressVerification`). All answer
    /// `passwordState`, stored in `Session::password_state`; `op`
    /// classifies the honest error line.
    PasswordStateOp {
        op: PasswordOp,
    },
    /// Slice A8: `sendPhoneNumberCode` with `phoneNumberCodeTypeChange`.
    /// Response is `authenticationCodeInfo`, stored in
    /// `Session::change_number_phone` / `change_number_timeout`.
    SendPhoneNumberCode,
    /// Slice A8: `resendPhoneNumberCode`. Response is
    /// `authenticationCodeInfo`; same storage as `SendPhoneNumberCode`.
    ResendPhoneNumberCode,
    /// Slice A8: `checkPhoneNumberCode`. Response is `ok`; the server
    /// completed the number change, so the own user's `phone_number` is
    /// updated to the confirmed `Session::change_number_phone` target
    /// (not an optimistic guess — the `ok` confirms the change of
    /// exactly the number the code was sent to).
    CheckPhoneNumberCode,
    Close,
    LogOut,
}

flat_purposes!(Auth(AuthPurpose) {
    GetAuthorizationState,
    SetParameters,
    SetPhoneNumber,
    CheckAuthenticationCode,
    SetAuthenticationEmail,
    RegisterUser,
    CheckAuthenticationEmailCode,
    CheckAuthenticationPassword,
    ResendAuthenticationCode,
    RequestAuthenticationPasswordRecovery,
    RecoverAuthenticationPassword,
    RequestQrCodeAuthentication,
    ResetAuthenticationEmail,
    GetCountries,
    GetCountryCode,
    SendPhoneNumberCode,
    ResendPhoneNumberCode,
    CheckPhoneNumberCode,
    Close,
    LogOut,
});
