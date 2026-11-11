//! Generated-once classification of every `AuthPurpose` variant.
use super::RequestKind;
use crate::state::AuthPurpose;

impl AuthPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::SetPhoneNumber
            | Self::CheckAuthenticationCode
            | Self::SetAuthenticationEmail
            | Self::RegisterUser
            | Self::CheckAuthenticationEmailCode
            | Self::CheckAuthenticationPassword
            | Self::ResendAuthenticationCode
            | Self::RequestAuthenticationPasswordRecovery
            | Self::RecoverAuthenticationPassword
            | Self::RequestQrCodeAuthentication
            | Self::ResetAuthenticationEmail => RequestKind::Auth,
            Self::GetAuthorizationState | Self::GetCountries | Self::GetCountryCode => {
                RequestKind::Lookup
            }
            Self::SetParameters
            | Self::SetNetworkType
            | Self::PasswordStateOp { .. }
            | Self::SendPhoneNumberCode
            | Self::ResendPhoneNumberCode
            | Self::CheckPhoneNumberCode
            | Self::Close
            | Self::LogOut => RequestKind::UserAction,
        }
    }
}
