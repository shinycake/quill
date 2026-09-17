use crate::telegram::envelope::AuthorizationState;

/// Screen + permitted actions derived from `updateAuthorizationState`, not from the last click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthView {
    pub title: &'static str,
    pub body: String,
    pub action: AuthAction,
    pub blocking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthAction {
    ProvideParameters,
    EnterPhone,
    EnterCode,
    EnterPassword,
    WaitOtherDevice,
    UnsupportedHalt { reason: &'static str },
    Ready,
    LoggingOut,
    Closed,
    Closing,
}

pub fn view_for(state: &AuthorizationState) -> AuthView {
    match state {
        AuthorizationState::WaitTdlibParameters => AuthView {
            title: "Starting",
            body: "Quill needs local TDLib parameters (database path and API credentials)."
                .into(),
            action: AuthAction::ProvideParameters,
            blocking: true,
        },
        AuthorizationState::WaitPhoneNumber => AuthView {
            title: "Sign in",
            body: "Enter the phone number for this Telegram account.".into(),
            action: AuthAction::EnterPhone,
            blocking: true,
        },
        AuthorizationState::WaitCode { code_length } => AuthView {
            title: "Verification code",
            body: match code_length {
                Some(len) => format!("Enter the {len}-digit code from Telegram."),
                None => "Enter the verification code from Telegram.".into(),
            },
            action: AuthAction::EnterCode,
            blocking: true,
        },
        AuthorizationState::WaitPassword { has_recovery_email } => AuthView {
            title: "Two-step password",
            body: if *has_recovery_email {
                "Enter your two-step verification password. Recovery email is available in the official client.".into()
            } else {
                "Enter your two-step verification password.".into()
            },
            action: AuthAction::EnterPassword,
            blocking: true,
        },
        AuthorizationState::WaitOtherDeviceConfirmation => AuthView {
            title: "Confirm on another device",
            body: "Scan the QR code in a logged-in Telegram client. The QR payload is never logged."
                .into(),
            action: AuthAction::WaitOtherDevice,
            blocking: true,
        },
        AuthorizationState::WaitPremiumPurchase => AuthView {
            title: "Unsupported sign-in state",
            body: "Telegram asked for a Premium purchase to continue. Quill will not start a payment. Use an official client, then return.".into(),
            action: AuthAction::UnsupportedHalt {
                reason: "premium-purchase",
            },
            blocking: true,
        },
        AuthorizationState::WaitEmailAddress | AuthorizationState::WaitEmailCode => AuthView {
            title: "Unsupported sign-in state",
            body: "Email-based authorization is not implemented yet. Use phone or QR in a later build, or an official client.".into(),
            action: AuthAction::UnsupportedHalt {
                reason: "email-auth",
            },
            blocking: true,
        },
        AuthorizationState::WaitRegistration => AuthView {
            title: "Unsupported sign-in state",
            body: "Quill will not auto-register a new Telegram user or accept terms on your behalf. Finish registration in an official client.".into(),
            action: AuthAction::UnsupportedHalt {
                reason: "registration",
            },
            blocking: true,
        },
        AuthorizationState::Unknown(name) => AuthView {
            title: "Unsupported sign-in state",
            body: format!(
                "Authorization variant `{name}` is not supported. The client will not continue automatically."
            ),
            action: AuthAction::UnsupportedHalt {
                reason: "unknown-auth",
            },
            blocking: true,
        },
        AuthorizationState::Ready => AuthView {
            title: "Ready",
            body: "Signed in. Cloud chats only.".into(),
            action: AuthAction::Ready,
            blocking: false,
        },
        AuthorizationState::LoggingOut => AuthView {
            title: "Signing out",
            body: "Waiting for TDLib to finish logout.".into(),
            action: AuthAction::LoggingOut,
            blocking: true,
        },
        AuthorizationState::Closing => AuthView {
            title: "Closing",
            body: "TDLib is closing this client. Wait for the closed state before quitting the process.".into(),
            action: AuthAction::Closing,
            blocking: true,
        },
        AuthorizationState::Closed => AuthView {
            title: "Closed",
            body: "Session databases are closed. Create a new client to continue.".into(),
            action: AuthAction::Closed,
            blocking: true,
        },
    }
}

/// Live login is disabled until the owner supplies api_id/api_hash out of tree.
pub fn credentials_ready(api_id: Option<i32>, api_hash: Option<&str>) -> bool {
    matches!(api_id, Some(id) if id > 0)
        && matches!(api_hash, Some(hash) if !hash.is_empty() && hash != "YOUR_API_HASH")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premium_and_email_do_not_auto_act() {
        for state in [
            AuthorizationState::WaitPremiumPurchase,
            AuthorizationState::WaitEmailAddress,
            AuthorizationState::WaitRegistration,
            AuthorizationState::Unknown("authorizationStateWaitSomethingNew".into()),
        ] {
            let view = view_for(&state);
            assert!(matches!(view.action, AuthAction::UnsupportedHalt { .. }));
            assert!(view.blocking);
        }
    }

    #[test]
    fn sample_credentials_are_rejected() {
        assert!(!credentials_ready(Some(12345), Some("YOUR_API_HASH")));
        assert!(!credentials_ready(None, Some("abc")));
        assert!(credentials_ready(Some(1), Some("not-a-sample")));
    }
}
