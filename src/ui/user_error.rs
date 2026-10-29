//! Plain sentences for failures, for the status toast. Telegram Desktop
//! shows the server's localized error or a short sentence, never a struct
//! dump; the raw error goes to the debug log instead.

use quill::connect::ConnectSendError;
use quill::lang::{Key, keys, tr, tr_with, trn_with};
use quill::telegram::envelope::{ErrorClass, TdError};

/// An error a screen can describe to the user.
pub(crate) trait UserError: std::fmt::Debug {
    /// The localized sentence for this error.
    fn user_text(&self) -> String;

    /// Whether [`Self::user_text`] only says "something went wrong", so a
    /// sentence naming the failed action does not repeat it after itself.
    fn is_generic(&self) -> bool {
        false
    }
}

impl UserError for ConnectSendError {
    fn user_text(&self) -> String {
        match self {
            Self::InvalidRequest => tr(keys::Q_NOTES_SOMETHING_WENT_WRONG).to_string(),
            Self::Native => tr(keys::Q_NOTES_NO_CONNECTION).to_string(),
            Self::CaptionTooLong { limit } => tr_with(
                keys::Q_NOTES_CAPTION_TOO_LONG,
                &[("limit", &limit.to_string())],
            ),
            Self::TextTooLong { limit } => tr_with(
                keys::Q_NOTES_TEXT_TOO_LONG,
                &[("limit", &limit.to_string())],
            ),
            Self::AgentDriving => tr(keys::Q_NOTES_AGENT_DRIVING).to_string(),
            Self::CallsUnavailable => tr(keys::Q_OPS_CALLS_ARE_UNAVAILABLE_THE_AUDIO).to_string(),
        }
    }

    fn is_generic(&self) -> bool {
        matches!(self, Self::InvalidRequest)
    }
}

impl UserError for quill::composer::PrepareError {
    fn user_text(&self) -> String {
        use quill::composer::PrepareError;
        match self {
            PrepareError::Video => tr(keys::Q_CHAT_COULD_NOT_READ_VIDEO_DURATION),
            PrepareError::VideoNote => tr(keys::Q_CHAT_VIDEO_NOTE_MUST_BE_A),
            PrepareError::TimedOut => tr(keys::Q_NOTES_PREPARING_FILES_TIMED_OUT),
            PrepareError::Unreadable => tr(keys::Q_NOTES_COULD_NOT_PREPARE_FILES),
        }
        .to_string()
    }

    fn is_generic(&self) -> bool {
        true
    }
}

impl UserError for TdError {
    fn is_generic(&self) -> bool {
        !self.is_flood()
            && !matches!(
                (self.class, self.code),
                (ErrorClass::NotFound | ErrorClass::Unauthorized, _)
                    | (_, 401 | 403 | 404 | 500..=599)
            )
    }

    fn user_text(&self) -> String {
        if self.is_flood() {
            return match self.flood_wait_secs {
                Some(secs) => trn_with(
                    keys::Q_NOTES_FLOOD_WAIT,
                    secs,
                    &[("count", &secs.to_string())],
                ),
                None => tr(keys::FLOOD_ERROR).to_string(),
            };
        }
        match (self.class, self.code) {
            (ErrorClass::NotFound, _) | (_, 404) => tr(keys::Q_NOTES_NOT_FOUND).to_string(),
            (ErrorClass::Unauthorized, _) | (_, 401 | 403) => {
                tr(keys::Q_NOTES_NOT_ALLOWED).to_string()
            }
            (_, 500..=599) => tr(keys::Q_NOTES_SERVER_ERROR).to_string(),
            _ => tr(keys::Q_NOTES_SOMETHING_WENT_WRONG).to_string(),
        }
    }
}

/// The sentence for `err`, with the raw error written to the debug log.
pub(crate) fn user_error(err: &impl UserError) -> String {
    quill::advanced::debug_mode::debug_log(&format!("error shown to the user: {err:?}"));
    err.user_text()
}

/// "Couldn't <action>." from the lang key `action`, followed by the mapped
/// cause (flood wait, not allowed, not found, server) when there is one, so
/// the user always sees which action failed.
pub(crate) fn user_error_for(action: Key, err: &impl UserError) -> String {
    failed_action(tr(action).to_string(), err)
}

/// [`user_error_for`] for an action named at run time (`what`, a localized
/// verb phrase such as "create story album").
pub(crate) fn user_error_doing(what: &str, err: &impl UserError) -> String {
    failed_action(tr_with(keys::Q_NOTES_COULDNT_DO, &[("what", what)]), err)
}

fn failed_action(sentence: String, err: &impl UserError) -> String {
    quill::advanced::debug_mode::debug_log(&format!("error shown to the user: {err:?}"));
    if err.is_generic() {
        sentence
    } else {
        format!("{sentence} {}", err.user_text())
    }
}

#[cfg(test)]
mod tests {
    use super::UserError;
    use quill::connect::ConnectSendError;
    use quill::lang::{keys, tr};
    use quill::telegram::envelope::TdError;

    #[test]
    fn send_errors_become_plain_sentences() {
        assert_eq!(
            ConnectSendError::Native.user_text(),
            tr(keys::Q_NOTES_NO_CONNECTION)
        );
        assert_eq!(
            ConnectSendError::InvalidRequest.user_text(),
            tr(keys::Q_NOTES_SOMETHING_WENT_WRONG)
        );
        let long = ConnectSendError::CaptionTooLong { limit: 1024 }.user_text();
        assert!(long.contains("1024"), "{long}");
    }

    #[test]
    fn rpc_errors_map_by_code_and_flood_wait() {
        let mut flood = TdError::from_code(429);
        flood.flood_wait_secs = Some(12);
        assert!(flood.user_text().contains("12"), "{}", flood.user_text());
        assert_eq!(TdError::from_code(429).user_text(), tr(keys::FLOOD_ERROR));
        assert_eq!(
            TdError::from_code(404).user_text(),
            tr(keys::Q_NOTES_NOT_FOUND)
        );
        assert_eq!(
            TdError::from_code(401).user_text(),
            tr(keys::Q_NOTES_NOT_ALLOWED)
        );
        assert_eq!(
            TdError::from_code(502).user_text(),
            tr(keys::Q_NOTES_SERVER_ERROR)
        );
        assert_eq!(
            TdError::from_code(400).user_text(),
            tr(keys::Q_NOTES_SOMETHING_WENT_WRONG)
        );
    }

    #[test]
    fn a_failed_action_is_named_and_gets_the_cause_when_known() {
        use super::{user_error_doing, user_error_for};
        let generic = user_error_for(keys::Q_NOTES_FAILED_LOG_OUT, &TdError::from_code(400));
        assert_eq!(generic, tr(keys::Q_NOTES_FAILED_LOG_OUT));
        let mut flood = TdError::from_code(429);
        flood.flood_wait_secs = Some(12);
        let text = user_error_for(keys::Q_NOTES_FAILED_LOG_OUT, &flood);
        assert!(text.starts_with(tr(keys::Q_NOTES_FAILED_LOG_OUT)), "{text}");
        assert!(text.contains("12"), "{text}");
        let denied = user_error_for(keys::Q_NOTES_FAILED_BLOCK, &TdError::from_code(403));
        assert!(denied.ends_with(tr(keys::Q_NOTES_NOT_ALLOWED)), "{denied}");
        let doing = user_error_doing("pin stories", &ConnectSendError::InvalidRequest);
        assert_eq!(doing, "Couldn't pin stories.");
    }

    #[test]
    fn no_error_text_is_a_debug_dump() {
        for text in [
            ConnectSendError::Native.user_text(),
            TdError::from_code(500).user_text(),
        ] {
            assert!(!text.contains("Td {") && !text.contains("code:"), "{text}");
        }
    }
}
