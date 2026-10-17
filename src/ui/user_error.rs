//! Plain sentences for failures, for the status toast. Telegram Desktop
//! shows the server's localized error or a short sentence, never a struct
//! dump; the raw error goes to the debug log instead.

use quill::connect::ConnectSendError;
use quill::lang::{keys, tr, tr_with, trn_with};
use quill::telegram::envelope::{ErrorClass, TdError};

/// An error a screen can describe to the user.
pub(crate) trait UserError: std::fmt::Debug {
    /// The localized sentence for this error.
    fn user_text(&self) -> String;
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
        }
    }
}

impl UserError for TdError {
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
    fn no_error_text_is_a_debug_dump() {
        for text in [
            ConnectSendError::Native.user_text(),
            TdError::from_code(500).user_text(),
        ] {
            assert!(!text.contains("Td {") && !text.contains("code:"), "{text}");
        }
    }
}
