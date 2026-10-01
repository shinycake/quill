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
    EnterEmail,
    Register,
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
                // Slice A10: recovery runs inside Quill now — "Forgot
                // password?" emails a recovery code via
                // requestAuthenticationPasswordRecovery.
                "Enter your two-step verification password, or choose \"Forgot password?\" to get a recovery code by email.".into()
            } else {
                "Enter your two-step verification password.".into()
            },
            action: AuthAction::EnterPassword,
            blocking: true,
        },
        AuthorizationState::WaitOtherDeviceConfirmation { .. } => AuthView {
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
        AuthorizationState::WaitEmailAddress => AuthView {
            title: "Login email",
            body: "Enter the email address to receive your Telegram login code.".into(),
            action: AuthAction::EnterEmail,
            blocking: true,
        },
        AuthorizationState::WaitEmailCode { email_pattern, code_length } => AuthView {
            title: "Email verification code",
            body: match code_length {
                Some(len) => format!("Enter the {len}-character code sent to {email_pattern}."),
                None => format!("Enter the code sent to {email_pattern}."),
            },
            action: AuthAction::EnterCode,
            blocking: true,
        },
        AuthorizationState::WaitRegistration { .. } => AuthView {
            title: "Create Telegram account",
            body: "Enter your name and review any Telegram terms before creating your account.".into(),
            action: AuthAction::Register,
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
    use crate::ids::RequestId;
    use crate::state::{AuthRequestError, RequestPurpose, is_auth_submit};
    use crate::telegram::envelope::ErrorClass;
    use crate::telegram::requests::{
        recover_authentication_password, request_authentication_password_recovery,
    };

    #[test]
    fn unsupported_auth_does_not_auto_act() {
        for state in [
            AuthorizationState::WaitPremiumPurchase,
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

    #[test]
    fn qr_state_maps_to_wait_other_device_view() {
        let view = view_for(&AuthorizationState::WaitOtherDeviceConfirmation {
            link: "tg://login/?token=unit-test".into(),
        });
        assert_eq!(view.action, AuthAction::WaitOtherDevice);
        assert!(view.blocking);
        assert!(view.title.contains("Confirm"));
    }
    // Slice A10 (moved from waived src/state.rs / src/telegram/requests.rs:
    // tests must not grow waived files).
    #[test]
    fn recovery_request_and_recover_errors_are_classified() {
        // Both recovery purposes classify through the shared auth-error path;
        // messages stay honest and secret-free.
        for (purpose, class, message) in [
            (
                RequestPurpose::RequestAuthenticationPasswordRecovery,
                ErrorClass::Invalid,
                "couldn't send the recovery code",
            ),
            (
                RequestPurpose::RequestAuthenticationPasswordRecovery,
                ErrorClass::Flood,
                "too many recovery requests — wait and try again",
            ),
            (
                RequestPurpose::RecoverAuthenticationPassword,
                ErrorClass::Invalid,
                "recovery code not accepted",
            ),
            (
                RequestPurpose::RecoverAuthenticationPassword,
                ErrorClass::Flood,
                "too many recovery attempts — wait and try again",
            ),
        ] {
            let err = AuthRequestError {
                purpose,
                class,
                flood_wait_secs: None,
            };
            assert_eq!(err.user_message(), message);
            assert!(is_auth_submit(purpose));
        }
    }

    #[test]
    fn auth_flood_error_shows_retry_countdown() {
        // Slice parity:platform-flood-errors — a known FLOOD_WAIT becomes
        // "… try again in N seconds"; unknown stays the static line.
        let err = AuthRequestError {
            purpose: RequestPurpose::CheckAuthenticationCode,
            class: ErrorClass::Flood,
            flood_wait_secs: Some(30),
        };
        assert_eq!(
            err.user_message(),
            "too many code attempts — try again in 30 seconds"
        );
        let err = AuthRequestError {
            purpose: RequestPurpose::CheckAuthenticationCode,
            class: ErrorClass::Flood,
            flood_wait_secs: None,
        };
        assert_eq!(
            err.user_message(),
            "too many code attempts — wait and try again"
        );
    }

    #[test]
    fn request_authentication_password_recovery_shape() {
        let json = request_authentication_password_recovery(RequestId(7));
        assert!(json.contains("\"@type\":\"requestAuthenticationPasswordRecovery\""));
    }

    #[test]
    fn recover_authentication_password_shape() {
        let json = recover_authentication_password(RequestId(8), "unit-test-code");
        assert!(json.contains("\"@type\":\"recoverAuthenticationPassword\""));
        assert!(json.contains("\"recovery_code\":\"unit-test-code\""));
        assert!(json.contains("\"new_password\":\"\""));
        assert!(json.contains("\"new_hint\":\"\""));
    }
}
