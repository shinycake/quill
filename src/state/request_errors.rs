//! Error lines for failed requests: a classified, user-facing line built
//! from a TDLib error's code and class, never its raw message.
use super::*;

/// Phase C1: classified error line for a failed call request. Only the
/// numeric code is shown — TDLib's `message` is never stored (it can
/// contain secrets). Code 4005000 (outgoing call missed because the
/// timeout expired) gets a plain-language line instead.
pub(crate) fn call_request_error_line(err: &TdError, action: &str) -> String {
    if err.code == 4005000 {
        format!("{action}: no answer — the call timed out")
    } else {
        format!("{action} (error {})", err.code)
    }
}

/// Slice A2: honest one-line failure for a 2FA management request. The
/// classification comes from TDLib's actual numeric error code (400 =
/// wrong password / invalid input, 429 = flood-wait); the native message
/// is never stored (it can contain secrets — see `TdError`).
pub(crate) fn password_op_error_line(op: PasswordOp, err: &TdError) -> String {
    // Slice parity:platform-flood-errors — precomputed so the Flood arm
    // can stay `&str` like the other arms.
    let flood_detail = err.flood_line("too many attempts — wait and try again");
    let detail = match err.class {
        ErrorClass::Flood => flood_detail.as_str(),
        ErrorClass::Unauthorized => "session is no longer authorized",
        ErrorClass::Invalid => match op {
            PasswordOp::SetPassword | PasswordOp::DisablePassword => {
                "wrong password or invalid input"
            }
            PasswordOp::SetRecoveryEmail => "wrong password, or the email was rejected",
            PasswordOp::ResendCode => "the code can't be resent yet",
            PasswordOp::AbortEmailSetup => "the pending setup can't be aborted",
            PasswordOp::CheckEmailCode
            | PasswordOp::RecoverPassword
            | PasswordOp::CheckLoginEmailCode => "the code is wrong or has expired",
            PasswordOp::RequestRecoveryCode => "no recovery email is available",
            PasswordOp::ResetPassword | PasswordOp::CancelPasswordReset => {
                "no reset is possible right now"
            }
            PasswordOp::SetLoginEmail => "the email address was rejected",
            PasswordOp::Fetch => "try again",
        },
        _ => "Telegram rejected the request",
    };
    format!("Could not {}: {detail}", op.action_label())
}

/// Slice A3: honest one-line failure for a sessions fetch or terminate.
/// The classification comes from TDLib's actual numeric error code
/// (400 = invalid/refused — e.g. terminating a session TDLib won't let
/// go, 429 = flood-wait); the native message is never stored (it can
/// contain secrets — see `TdError`).
pub(crate) fn sessions_error_line(action: &str, err: &TdError) -> String {
    // Slice parity:platform-flood-errors — precomputed so the Flood arm
    // can stay `&str` like the other arms.
    let flood_detail = err.flood_line("too many requests — wait and try again");
    let detail = match err.class {
        ErrorClass::Flood => flood_detail.as_str(),
        ErrorClass::Unauthorized => "session is no longer authorized",
        // The sign-in classes are 400s: same wording the plain code gave.
        ErrorClass::Invalid
        | ErrorClass::PhoneBanned
        | ErrorClass::PhoneInvalid
        | ErrorClass::PhoneFlood
        | ErrorClass::TaskAlreadyExists => "Telegram refused the request",
        ErrorClass::NotFound => "no longer exists",
        // S14: mirror the Invalid / Other arms — the code-based class
        // these errors had before classification.
        ErrorClass::StoryChatDisabled => "Telegram refused the request",
        ErrorClass::StoryUserRestricted => {
            return format!("Could not {action} (error {})", err.code);
        }
        // Slice msg-richtext-ai-tools: AI errors never reach the sessions
        // screen, so AiComposeFloodPremium shares the generic code-based
        // line (kept as an explicit arm: the match must stay exhaustive).
        ErrorClass::AiComposeFloodPremium
        | ErrorClass::StickersForbidden
        | ErrorClass::GifsForbidden
        | ErrorClass::CollectibleNotFound
        | ErrorClass::Other => {
            return format!("Could not {action} (error {})", err.code);
        }
        // Group-call classes keep the line their plain code gave.
        ErrorClass::GroupCallRejoin | ErrorClass::GroupCallLeft | ErrorClass::StreamTimeTooBig => {
            return sessions_error_line(action, &TdError::from_code(err.code));
        }
    };
    format!("Could not {action}: {detail}")
}

/// Phase 9.3: one-line `TdError` reason for the story composer. Never
/// includes the native TDLib message (it can contain secrets — see
/// `TdError`).
pub(crate) fn error_reason(err: &TdError) -> String {
    match err.class {
        ErrorClass::NotFound | ErrorClass::CollectibleNotFound => "not found".to_string(),
        ErrorClass::Unauthorized => "not authorized".to_string(),
        ErrorClass::Flood => err.flood_line("too many requests — try again later"),
        ErrorClass::Invalid
        | ErrorClass::PhoneBanned
        | ErrorClass::PhoneInvalid
        | ErrorClass::PhoneFlood
        | ErrorClass::TaskAlreadyExists => "invalid request".to_string(),
        // S14: classified story-restriction errors keep the exact text
        // the code-based class produced before (400 → Invalid,
        // 403 → Other), so non-story flows render byte-identical text.
        ErrorClass::StoryChatDisabled => "invalid request".to_string(),
        ErrorClass::StoryUserRestricted => format!("error {}", err.code),
        // Slice msg-richtext-ai-tools: the documented AI flood error —
        // the AI arm in `apply_error` phrases this for the user; this
        // is the fallback for any generic path.
        ErrorClass::AiComposeFloodPremium => "AI request limit reached".to_string(),
        // Group-call classes keep the text their plain code gave.
        ErrorClass::GroupCallRejoin | ErrorClass::GroupCallLeft | ErrorClass::StreamTimeTooBig => {
            error_reason(&TdError::from_code(err.code))
        }
        ErrorClass::StickersForbidden | ErrorClass::GifsForbidden => err
            .send_permission_notice()
            .unwrap_or("You don't have permission to send this in this chat.")
            .to_string(),
        ErrorClass::Other => format!("error {}", err.code),
    }
}
