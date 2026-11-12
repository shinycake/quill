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

/// The line of a fetch whose request could not even be sent (the client is
/// closing or not connected): the same shape as a refusal, so the page draws
/// it with its Retry instead of a spinner that nothing will end.
pub(crate) fn send_failed_line(action: &str) -> String {
    call_request_error_line(&TdError::from_code(LOST_REQUEST_ERROR_CODE), action)
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

impl Session {
    /// Settle a request that will never be answered (swept after its TTL,
    /// dropped because the flood-retry re-send failed, or abandoned with its
    /// account) by feeding it through the same per-domain failure handling a
    /// TDLib error uses. Every purpose settles in this one place: spinners
    /// and "loading more" flags clear, the history window offers Retry.
    /// The request must already be out of the registry.
    pub(crate) fn settle_lost_request(&mut self, pending: PendingRequest) {
        let extra = pending.id;
        self.drop_admin_followups_of_lost(extra);
        let seq = self.last_seq;
        let error = TdError::from_code(LOST_REQUEST_ERROR_CODE);
        // An export's worker waits on its link for this answer: without it
        // the link stays busy and no further export request goes out.
        self.settings.fail_lost_export(pending.purpose, extra);
        self.acks.settle_lost(extra);
        self.settle_lost_instant_view(&pending);
        self.apply_payload_in_asking_view(
            EnvelopePayload::Error(error),
            Some(&pending),
            Some(extra),
            seq,
        );
    }

    /// A lost `getWebPageInstantView` is a refusal like any other: the link
    /// opens in the browser instead of staying in the asks for good (the
    /// answer path does the same in `take_instant_view_fallback`).
    fn settle_lost_instant_view(&mut self, pending: &PendingRequest) {
        if pending.purpose != RequestPurpose::GetWebPageInstantView {
            return;
        }
        let Some(ask) = self.messages.instant_view_requests.remove(&pending.id) else {
            return;
        };
        let window = pending.asked_in.chat();
        if window.is_none_or(|chat| self.window_chats.contains(&chat)) {
            self.messages
                .instant_view_fallbacks
                .push(InstantViewFallback {
                    window,
                    url: ask.url,
                    from_agent: ask.from_agent,
                });
        }
    }
}
