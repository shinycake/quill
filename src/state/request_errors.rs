//! Error lines for failed requests: a classified, user-facing line built
//! from a TDLib error's code and class, never its raw message.
use super::*;
use crate::lang::{keys, tr, tr_with};

/// Phase C1: classified error line for a failed call request. Only the
/// numeric code is shown — TDLib's `message` is never stored (it can
/// contain secrets). Code 4005000 (outgoing call missed because the
/// timeout expired) gets a plain-language line instead.
pub(crate) fn call_request_error_line(err: &TdError, action: &str) -> String {
    if err.code == 4005000 {
        tr_with(keys::Q_OPS_ERR_CALL_TIMED_OUT, &[("action", action)])
    } else {
        tr_with(
            keys::Q_OPS_ERR_CALL_CODE,
            &[("action", action), ("code", &err.code.to_string())],
        )
    }
}

/// Slice A2: honest one-line failure for a 2FA management request. The
/// classification comes from TDLib's actual numeric error code (400 =
/// wrong password / invalid input, 429 = flood-wait); the native message
/// is never stored (it can contain secrets — see `TdError`).
pub(crate) fn password_op_error_line(op: PasswordOp, err: &TdError) -> String {
    // Slice parity:platform-flood-errors — precomputed so the Flood arm
    // can stay `&str` like the other arms.
    let flood_detail = err.flood_line(tr(keys::Q_OPS_ERR_PW_FLOOD));
    let detail = match err.class {
        ErrorClass::Flood => flood_detail.as_str(),
        ErrorClass::Unauthorized => tr(keys::Q_OPS_ERR_UNAUTHORIZED),
        ErrorClass::Invalid => match op {
            PasswordOp::SetPassword | PasswordOp::DisablePassword => tr(keys::Q_OPS_ERR_PW_INVALID),
            PasswordOp::SetRecoveryEmail => tr(keys::Q_OPS_ERR_PW_EMAIL),
            PasswordOp::ResendCode => tr(keys::Q_OPS_ERR_CODE_RESEND),
            PasswordOp::AbortEmailSetup => tr(keys::Q_OPS_ERR_SETUP_ABORT),
            PasswordOp::CheckEmailCode
            | PasswordOp::RecoverPassword
            | PasswordOp::CheckLoginEmailCode => tr(keys::Q_OPS_ERR_CODE_WRONG),
            PasswordOp::RequestRecoveryCode => tr(keys::Q_OPS_ERR_NO_RECOVERY),
            PasswordOp::ResetPassword | PasswordOp::CancelPasswordReset => {
                tr(keys::Q_OPS_ERR_NO_RESET)
            }
            PasswordOp::SetLoginEmail => tr(keys::Q_OPS_ERR_EMAIL_REJECTED),
            PasswordOp::Fetch => tr(keys::Q_OPS_ERR_TRY_AGAIN),
        },
        _ => tr(keys::Q_OPS_ERR_REJECTED),
    };
    tr_with(
        keys::Q_OPS_ERR_COULD_NOT_DETAIL,
        &[("action", op.action_label()), ("detail", detail)],
    )
}

/// Slice A3: honest one-line failure for a sessions fetch or terminate.
/// The classification comes from TDLib's actual numeric error code
/// (400 = invalid/refused — e.g. terminating a session TDLib won't let
/// go, 429 = flood-wait); the native message is never stored (it can
/// contain secrets — see `TdError`).
pub(crate) fn sessions_error_line(action: &str, err: &TdError) -> String {
    // Slice parity:platform-flood-errors — precomputed so the Flood arm
    // can stay `&str` like the other arms.
    let flood_detail = err.flood_line(tr(keys::Q_OPS_ERR_REQ_FLOOD));
    let detail = match err.class {
        ErrorClass::Flood => flood_detail.as_str(),
        ErrorClass::Unauthorized => tr(keys::Q_OPS_ERR_UNAUTHORIZED),
        // The sign-in classes are 400s: same wording the plain code gave.
        ErrorClass::Invalid
        | ErrorClass::PhoneBanned
        | ErrorClass::PhoneInvalid
        | ErrorClass::PhoneFlood
        | ErrorClass::TaskAlreadyExists => tr(keys::Q_OPS_ERR_REFUSED),
        ErrorClass::NotFound => tr(keys::Q_OPS_ERR_GONE),
        // S14: mirror the Invalid / Other arms — the code-based class
        // these errors had before classification.
        ErrorClass::StoryChatDisabled => tr(keys::Q_OPS_ERR_REFUSED),
        ErrorClass::StoryUserRestricted => {
            return tr_with(
                keys::Q_OPS_ERR_COULD_NOT_CODE,
                &[("action", action), ("code", &err.code.to_string())],
            );
        }
        // Slice msg-richtext-ai-tools: AI errors never reach the sessions
        // screen, so AiComposeFloodPremium shares the generic code-based
        // line (kept as an explicit arm: the match must stay exhaustive).
        ErrorClass::AiComposeFloodPremium
        | ErrorClass::InputTextTooLong
        | ErrorClass::StylesSavedTooMany
        | ErrorClass::StyleInvalid
        | ErrorClass::StickersForbidden
        | ErrorClass::GifsForbidden
        | ErrorClass::CollectibleNotFound
        | ErrorClass::Other => {
            return tr_with(
                keys::Q_OPS_ERR_COULD_NOT_CODE,
                &[("action", action), ("code", &err.code.to_string())],
            );
        }
        // Group-call classes keep the line their plain code gave.
        ErrorClass::GroupCallRejoin | ErrorClass::GroupCallLeft | ErrorClass::StreamTimeTooBig => {
            return sessions_error_line(action, &TdError::from_code(err.code));
        }
    };
    tr_with(
        keys::Q_OPS_ERR_COULD_NOT_DETAIL,
        &[("action", action), ("detail", detail)],
    )
}

/// Phase 9.3: one-line `TdError` reason for the story composer. Never
/// includes the native TDLib message (it can contain secrets — see
/// `TdError`).
pub(crate) fn error_reason(err: &TdError) -> String {
    match err.class {
        ErrorClass::NotFound | ErrorClass::CollectibleNotFound => {
            tr(keys::Q_OPS_ERR_NOT_FOUND).to_string()
        }
        ErrorClass::Unauthorized => tr(keys::Q_OPS_ERR_NOT_AUTHORIZED).to_string(),
        ErrorClass::Flood => err.flood_line(tr(keys::Q_OPS_ERR_REQ_FLOOD_LATER)),
        ErrorClass::Invalid
        | ErrorClass::PhoneBanned
        | ErrorClass::PhoneInvalid
        | ErrorClass::PhoneFlood
        | ErrorClass::TaskAlreadyExists => tr(keys::Q_OPS_ERR_INVALID_REQUEST).to_string(),
        // S14: classified story-restriction errors keep the exact text
        // the code-based class produced before (400 → Invalid,
        // 403 → Other), so non-story flows render byte-identical text.
        ErrorClass::StoryChatDisabled => tr(keys::Q_OPS_ERR_INVALID_REQUEST).to_string(),
        ErrorClass::StoryUserRestricted => tr_with(
            keys::Q_OPS_ERR_ERROR_CODE,
            &[("code", &err.code.to_string())],
        ),
        // Slice msg-richtext-ai-tools: the documented AI flood error —
        // the AI arm in `apply_error` phrases this for the user; this
        // is the fallback for any generic path.
        ErrorClass::AiComposeFloodPremium => tr(keys::Q_OPS_ERR_AI_LIMIT).to_string(),
        ErrorClass::GroupCallRejoin
        | ErrorClass::GroupCallLeft
        | ErrorClass::StreamTimeTooBig
        | ErrorClass::InputTextTooLong
        | ErrorClass::StylesSavedTooMany
        | ErrorClass::StyleInvalid => error_reason(&TdError::from_code(err.code)),
        ErrorClass::StickersForbidden | ErrorClass::GifsForbidden => err
            .send_permission_notice()
            .unwrap_or_else(|| tr(keys::Q_OPS_ERR_NO_SEND_PERMISSION))
            .to_string(),
        ErrorClass::Other => tr_with(
            keys::Q_OPS_ERR_ERROR_CODE,
            &[("code", &err.code.to_string())],
        ),
    }
}
