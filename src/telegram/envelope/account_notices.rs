//! Batch 4 / 6: account-hygiene updates and answers — the unconfirmed
//! session ("new login") alert, server service notifications, terms of
//! service, email code info and the password-reset result.
use super::*;
use serde_json::Value;

/// `unconfirmedSession` (schema 1.8.67, line 9155): the first login the
/// user has not confirmed from another device. The update carries no
/// session id — `getActiveSessions` (`session.is_unconfirmed`) resolves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnconfirmedLogin {
    pub date: i32,
    pub device_model: String,
    pub location: String,
}

/// `termsOfService` (schema 1.8.67, line 180).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermsOfService {
    pub id: String,
    pub text: String,
    pub min_user_age: i32,
    pub show_popup: bool,
}

/// `resetPasswordResult*` (schema 1.8.67, lines 8613–8622).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetPasswordOutcome {
    /// The password was removed.
    Ok,
    /// The request is pending; the password can be reset after this date.
    Pending { reset_date: i32 },
    /// A recent reset was declined; retry after this date.
    Declined { retry_date: i32 },
}

pub(crate) fn parse_unconfirmed_session_update(value: &Value) -> EnvelopePayload {
    let session = value
        .get("session")
        .filter(|session| session.is_object())
        .map(|session| UnconfirmedLogin {
            date: json_i32(session.get("date"), 0),
            device_model: json_field_str(session, "device_model"),
            location: json_field_str(session, "location"),
        });
    EnvelopePayload::UpdateUnconfirmedSession {
        session,
        count: json_i32(value.get("unconfirmed_session_count"), 0).max(0),
    }
}

/// `updateServiceNotification type:string content:MessageContent`: the
/// popup shows the plain text (message text, or a media caption).
pub(crate) fn parse_service_notification(value: &Value) -> EnvelopePayload {
    let content = value.get("content");
    let text = content
        .and_then(|content| {
            content
                .get("text")
                .or_else(|| content.get("caption"))
                .and_then(|formatted| formatted.get("text"))
        })
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    EnvelopePayload::UpdateServiceNotification {
        kind: json_field_str(value, "type"),
        text,
    }
}

pub(crate) fn parse_terms_of_service(value: &Value) -> Option<TermsOfService> {
    let terms = value.get("terms_of_service")?;
    let text = terms.get("text")?.get("text")?.as_str()?.to_string();
    let id = value.get("terms_of_service_id")?.as_str()?.to_string();
    if id.is_empty() || text.is_empty() {
        return None;
    }
    Some(TermsOfService {
        id,
        text,
        min_user_age: json_i32(terms.get("min_user_age"), 0).max(0),
        show_popup: terms
            .get("show_popup")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub(crate) fn parse_reset_password_result(type_name: &str, value: &Value) -> EnvelopePayload {
    EnvelopePayload::ResetPasswordResult(match type_name {
        "resetPasswordResultPending" => ResetPasswordOutcome::Pending {
            reset_date: json_i32(value.get("pending_reset_date"), 0),
        },
        "resetPasswordResultDeclined" => ResetPasswordOutcome::Declined {
            retry_date: json_i32(value.get("retry_date"), 0),
        },
        _ => ResetPasswordOutcome::Ok,
    })
}

#[cfg(test)]
mod tests {
    use crate::telegram::envelope::{EnvelopePayload, ResetPasswordOutcome, parse_envelope};

    #[test]
    fn unconfirmed_session_update_parses() {
        let json = r#"{"@type":"updateUnconfirmedSession","session":{"@type":"unconfirmedSession","type":{"@type":"sessionTypeAndroid"},"date":1760000000,"device_model":"Pixel 9","location":"Berlin, Germany"},"unconfirmed_session_count":2}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateUnconfirmedSession { session, count } => {
                let session = session.expect("session");
                assert_eq!(session.device_model, "Pixel 9");
                assert_eq!(session.location, "Berlin, Germany");
                assert_eq!(session.date, 1_760_000_000);
                assert_eq!(count, 2);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn unconfirmed_session_cleared_when_none_left() {
        let json =
            r#"{"@type":"updateUnconfirmedSession","session":null,"unconfirmed_session_count":0}"#;
        match parse_envelope(json).unwrap().payload {
            EnvelopePayload::UpdateUnconfirmedSession { session, count } => {
                assert!(session.is_none());
                assert_eq!(count, 0);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn service_notification_keeps_type_and_text() {
        let json = r#"{"@type":"updateServiceNotification","type":"AUTH_KEY_DROP_DUPLICATE","content":{"@type":"messageText","text":{"@type":"formattedText","text":"Your session was terminated.","entities":[]}}}"#;
        match parse_envelope(json).unwrap().payload {
            EnvelopePayload::UpdateServiceNotification { kind, text } => {
                assert_eq!(kind, "AUTH_KEY_DROP_DUPLICATE");
                assert_eq!(text, "Your session was terminated.");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn service_notification_media_uses_caption() {
        let json = r#"{"@type":"updateServiceNotification","type":"","content":{"@type":"messagePhoto","caption":{"@type":"formattedText","text":"Look","entities":[]}}}"#;
        match parse_envelope(json).unwrap().payload {
            EnvelopePayload::UpdateServiceNotification { text, .. } => assert_eq!(text, "Look"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn terms_of_service_update_parses() {
        let json = r#"{"@type":"updateTermsOfService","terms_of_service_id":"tos-2026","terms_of_service":{"@type":"termsOfService","text":{"@type":"formattedText","text":"New terms.","entities":[]},"min_user_age":16,"show_popup":true}}"#;
        match parse_envelope(json).unwrap().payload {
            EnvelopePayload::UpdateTermsOfService { terms } => {
                assert_eq!(terms.id, "tos-2026");
                assert_eq!(terms.text, "New terms.");
                assert_eq!(terms.min_user_age, 16);
                assert!(terms.show_popup);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn terms_without_id_or_text_are_not_a_prompt() {
        let json = r#"{"@type":"updateTermsOfService","terms_of_service_id":"","terms_of_service":{"@type":"termsOfService","text":{"@type":"formattedText","text":"x","entities":[]},"min_user_age":0,"show_popup":false}}"#;
        assert!(parse_envelope(json).is_err());
    }

    #[test]
    fn reset_password_results_parse() {
        let ok = parse_envelope(r#"{"@type":"resetPasswordResultOk"}"#).unwrap();
        assert_eq!(
            ok.payload,
            EnvelopePayload::ResetPasswordResult(ResetPasswordOutcome::Ok)
        );
        let pending = parse_envelope(
            r#"{"@type":"resetPasswordResultPending","pending_reset_date":1760604800}"#,
        )
        .unwrap();
        assert_eq!(
            pending.payload,
            EnvelopePayload::ResetPasswordResult(ResetPasswordOutcome::Pending {
                reset_date: 1_760_604_800
            })
        );
        let declined =
            parse_envelope(r#"{"@type":"resetPasswordResultDeclined","retry_date":1760700000}"#)
                .unwrap();
        assert_eq!(
            declined.payload,
            EnvelopePayload::ResetPasswordResult(ResetPasswordOutcome::Declined {
                retry_date: 1_760_700_000
            })
        );
    }

    #[test]
    fn email_code_info_parses() {
        let json = r#"{"@type":"emailAddressAuthenticationCodeInfo","email_address_pattern":"i***@example.com","length":6}"#;
        match parse_envelope(json).unwrap().payload {
            EnvelopePayload::EmailCodeInfo { pattern, length } => {
                assert_eq!(pattern, "i***@example.com");
                assert_eq!(length, 6);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
