//! S14: story-posting restriction notices — TGX-verbatim copy.
//!
//! TDLib detects per-chat story restrictions through `canPostStory`
//! (schema 1.8.67, `td_api.tl:13702`): premium / boost / limit failures
//! arrive as `CanPostStoryResult` variants (mapped by TDLib itself —
//! `StoryManager::get_can_post_story_result_object`), while chat-level
//! restrictions arrive as raw `error` payloads. The notices below are
//! TGX's verbatim strings (`ChatDisabledStory` / `ChatRestrictedStory`
//! / `ChatRestrictedStoryUntil`, TGX-Android `strings.xml`).
//!
//! The raw error message is classified at parse time in
//! `telegram::envelope::parse_error` — `TdError` deliberately drops the
//! message text (secret-scrubbing), so only the scrub-safe `ErrorClass`
//! variants below survive to the reducer. The story composer surfaces
//! the notice through its existing gating: `canPostStory` error →
//! `Session::story_post.check_error` → composer status line.

use crate::telegram::envelope::ErrorClass;

/// TGX `ChatDisabledStory`, verbatim.
pub const CHAT_DISABLED_STORY: &str = "Only admins can send stories in this group";

/// TGX `ChatRestrictedStory`, verbatim.
pub const CHAT_RESTRICTED_STORY: &str =
    "The admins of this group have restricted your ability to send stories.";

/// TGX `ChatRestrictedStoryUntil`, verbatim (`%1$s` = the restriction
/// end, formatted by the caller). The `canPostStory` error channel
/// carries no until-date, so the composer uses the plain restricted
/// notice there; this stays available for dated restriction signals
/// (e.g. `chatMemberStatusRestricted.restricted_until_date`).
pub fn chat_restricted_story_until(until: &str) -> String {
    format!("Admins have restricted you from sending stories in this group until {until}")
}

/// Classify a raw TDLib `error.message` into a scrub-safe `ErrorClass`.
/// Only story-posting restriction server strings are recognized —
/// anything else returns `None` and the caller falls back to the
/// code-based class, so every other flow is unchanged:
///
/// - `CHAT_ADMIN_REQUIRED`: documented 400 error for
///   `stories.canSendStory` / `stories.sendStory`
///   (core.telegram.org: "You must be an admin in this chat to do
///   this.") — stories are disabled for non-admins in the target chat.
/// - `USER_RESTRICTED`: documented 403 error for users restricted in
///   supergroups/channels.
///
/// The message itself is never stored.
pub fn classify_server_message(message: &str) -> Option<ErrorClass> {
    match message {
        "CHAT_ADMIN_REQUIRED" => Some(ErrorClass::StoryChatDisabled),
        "USER_RESTRICTED" => Some(ErrorClass::StoryUserRestricted),
        _ => None,
    }
}

/// The TGX-verbatim restriction notice for a classified error, or
/// `None` when the error is not a story-posting restriction.
pub fn notice_for_error_class(class: ErrorClass) -> Option<&'static str> {
    match class {
        ErrorClass::StoryChatDisabled => Some(CHAT_DISABLED_STORY),
        ErrorClass::StoryUserRestricted => Some(CHAT_RESTRICTED_STORY),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::envelope::{EnvelopePayload, parse_envelope};

    #[test]
    fn restriction_errors_classify_at_parse_time() {
        // End-to-end through the public envelope parser: the raw message
        // is dropped (secret-scrubbing) but the scrub-safe class survives.
        for (message, code, class, notice) in [
            (
                "CHAT_ADMIN_REQUIRED",
                400,
                ErrorClass::StoryChatDisabled,
                CHAT_DISABLED_STORY,
            ),
            (
                "USER_RESTRICTED",
                403,
                ErrorClass::StoryUserRestricted,
                CHAT_RESTRICTED_STORY,
            ),
        ] {
            let json = format!(r#"{{"@type":"error","code":{code},"message":"{message}"}}"#);
            let envelope = parse_envelope(&json).expect("parses");
            let EnvelopePayload::Error(err) = envelope.payload else {
                panic!("expected error payload");
            };
            assert_eq!(err.code, code);
            assert_eq!(err.class, class);
            assert_eq!(notice_for_error_class(err.class), Some(notice));
        }
    }

    #[test]
    fn non_restriction_errors_keep_code_based_class() {
        // Anything unrecognized falls back to the code mapping, so all
        // other flows render exactly what they did before S14.
        for (message, code, class) in [
            ("CHANNEL_INVALID", 400, ErrorClass::Invalid),
            ("SOME_UNKNOWN_THING", 403, ErrorClass::Other),
            ("FLOOD_WAIT_12", 429, ErrorClass::Flood),
        ] {
            let json = format!(r#"{{"@type":"error","code":{code},"message":"{message}"}}"#);
            let envelope = parse_envelope(&json).expect("parses");
            let EnvelopePayload::Error(err) = envelope.payload else {
                panic!("expected error payload");
            };
            assert_eq!(err.class, class, "{message}");
            assert_eq!(notice_for_error_class(err.class), None);
        }
    }

    #[test]
    fn until_notice_matches_tgx_format() {
        // TGX `ChatRestrictedStoryUntil`: "… until %1$s".
        assert_eq!(
            chat_restricted_story_until("Oct 3, 2026"),
            "Admins have restricted you from sending stories in this group until Oct 3, 2026"
        );
    }
}
