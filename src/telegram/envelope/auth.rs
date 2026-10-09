use super::*;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TdError {
    pub code: i32,
    /// Error *class* only. The TDLib `message` field is not stored; it can
    /// contain phone numbers or other secrets.
    pub class: ErrorClass,
    /// Slice parity:platform-flood-errors — retry-after seconds parsed
    /// from a `FLOOD_WAIT_<n>` message in `parse_error` (the only place
    /// the raw message is still available). The message text itself is
    /// still dropped; only the numeric wait survives.
    pub flood_wait_secs: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    NotFound,
    Unauthorized,
    Flood,
    Invalid,
    /// S14: `canPostStory` failed with `CHAT_ADMIN_REQUIRED` — stories
    /// are disabled for non-admins in the target chat. Classified from
    /// the raw error message in `parse_error` (the only place it is
    /// still available); the message text itself is dropped.
    StoryChatDisabled,
    /// S14: `canPostStory` failed with `USER_RESTRICTED`.
    StoryUserRestricted,
    /// Slice msg-richtext-ai-tools: an AI compose/fix call failed with
    /// `AICOMPOSE_FLOOD_PREMIUM` (schema documents it on all five AI
    /// methods) — Telegram Premium is required for further requests.
    /// Classified from the raw error message in `parse_error` (the only
    /// place it is still available); the message text itself is dropped.
    AiComposeFloodPremium,
    StickersForbidden,
    GifsForbidden,
    /// `PHONE_NUMBER_BANNED` while signing in (tdesktop's banned-number box).
    PhoneBanned,
    /// `PHONE_NUMBER_INVALID` (tdesktop's `lng_bad_phone`).
    PhoneInvalid,
    /// `PHONE_NUMBER_FLOOD`: the account was deleted and re-created too often.
    PhoneFlood,
    /// `TASK_ALREADY_EXISTS`: a login-email reset is already pending.
    TaskAlreadyExists,
    Other,
}

impl TdError {
    pub fn send_permission_notice(&self) -> Option<&'static str> {
        match self.class {
            ErrorClass::StickersForbidden => {
                Some("You don't have permission to send stickers in this chat.")
            }
            ErrorClass::GifsForbidden => {
                Some("You don't have permission to send GIFs in this chat.")
            }
            _ => None,
        }
    }

    pub fn from_code(code: i32) -> Self {
        let class = match code {
            404 => ErrorClass::NotFound,
            401 => ErrorClass::Unauthorized,
            429 => ErrorClass::Flood,
            400 => ErrorClass::Invalid,
            _ => ErrorClass::Other,
        };
        Self {
            code,
            class,
            flood_wait_secs: None,
        }
    }

    /// Whether this is a rate-limit answer (code 429 / `FLOOD_WAIT`).
    pub fn is_flood(&self) -> bool {
        self.class == ErrorClass::Flood || self.code == 429
    }

    /// tdesktop's `lng_flood_error` for a user action that hit a rate
    /// limit: "Too many attempts. Try again in N seconds." (generic line
    /// when TDLib gave no wait).
    pub fn flood_notice(&self) -> Option<String> {
        if !self.is_flood() {
            return None;
        }
        Some(match self.flood_wait_secs {
            Some(1) => "Too many attempts. Try again in 1 second.".to_string(),
            Some(secs) => format!("Too many attempts. Try again in {secs} seconds."),
            None => "Too many attempts. Try again later.".to_string(),
        })
    }

    /// Slice parity:platform-flood-errors — the retry countdown line for
    /// a flood error: "try again in N seconds" when the wait is known,
    /// otherwise the honest generic line.
    pub fn flood_line(&self, generic: &str) -> String {
        match self.flood_wait_secs {
            Some(1) => "try again in 1 second".to_string(),
            Some(secs) => format!("try again in {secs} seconds"),
            None => generic.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationTerms {
    pub text: String,
    pub min_user_age: i32,
}

/// How TDLib delivers (or can next deliver) a login code
/// (`AuthenticationCodeType`; only the kind matters to the UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodeKind {
    /// A code type this build does not name.
    #[default]
    Other,
    /// A message in the user's other Telegram apps.
    TelegramMessage,
    Sms,
    Call,
    FlashCall,
    MissedCall,
    Fragment,
    Firebase,
}

/// `authenticationCodeInfo` reduced to what the code screen needs: how the
/// code was sent, how it can be sent next and after how many seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CodeDelivery {
    pub kind: CodeKind,
    pub next: Option<CodeKind>,
    pub timeout_secs: i32,
}

/// `EmailAddressResetState` of the email-code step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmailResetState {
    /// The email cannot be reset.
    #[default]
    Unavailable,
    /// May be reset after `wait_period` seconds.
    Available { wait_period: i32 },
    /// A reset is already scheduled and completes in `reset_in` seconds.
    Pending { reset_in: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationState {
    WaitTdlibParameters,
    WaitPhoneNumber,
    WaitPremiumPurchase,
    WaitEmailAddress,
    WaitEmailCode {
        email_pattern: String,
        code_length: Option<i32>,
        reset: EmailResetState,
    },
    WaitCode {
        code_length: Option<i32>,
        delivery: CodeDelivery,
    },
    WaitOtherDeviceConfirmation {
        link: String,
    },
    WaitRegistration {
        terms: Option<RegistrationTerms>,
    },
    WaitPassword {
        has_recovery_email: bool,
    },
    Ready,
    LoggingOut,
    Closing,
    Closed,
    Unknown(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// No `updateConnectionState` yet: the indicator stays hidden rather
    /// than flashing "offline" at startup.
    Initial,
    WaitingForNetwork,
    ConnectingToProxy,
    Connecting,
    Updating,
    Ready,
    Unknown,
}

/// Slice A2: `passwordState` (TDLib 1.8.67, `schema/td_api.tl:273`):
/// `passwordState has_password:Bool password_hint:string
/// has_recovery_email_address:Bool has_passport_data:Bool
/// recovery_email_address_code_info:emailAddressAuthenticationCodeInfo
/// login_email_address_pattern:string pending_reset_date:int32 =
/// PasswordState;`
/// `recovery_email_address_code_info` is null unless a recovery-email
/// confirmation is pending (`emailAddressAuthenticationCodeInfo
/// email_address_pattern:string length:int32 = ...`, schema line 83).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordState {
    pub has_password: bool,
    pub password_hint: String,
    pub has_recovery_email_address: bool,
    pub has_passport_data: bool,
    /// Pattern of the pending recovery email (e.g. "i***@example.com"),
    /// `None` when no confirmation is pending.
    pub pending_email_pattern: Option<String>,
    /// Expected confirmation-code length (`length` of the code info).
    pub pending_email_code_length: i32,
    pub login_email_address_pattern: String,
    pub pending_reset_date: i32,
}

fn parse_code_kind(value: Option<&Value>) -> Option<CodeKind> {
    let ty = value.filter(|v| !v.is_null())?.get("@type")?.as_str()?;
    Some(match ty {
        "authenticationCodeTypeTelegramMessage" => CodeKind::TelegramMessage,
        "authenticationCodeTypeSms"
        | "authenticationCodeTypeSmsWord"
        | "authenticationCodeTypeSmsPhrase" => CodeKind::Sms,
        "authenticationCodeTypeCall" => CodeKind::Call,
        "authenticationCodeTypeFlashCall" => CodeKind::FlashCall,
        "authenticationCodeTypeMissedCall" => CodeKind::MissedCall,
        "authenticationCodeTypeFragment" => CodeKind::Fragment,
        "authenticationCodeTypeFirebaseAndroid" | "authenticationCodeTypeFirebaseIos" => {
            CodeKind::Firebase
        }
        _ => CodeKind::Other,
    })
}

pub(crate) fn parse_code_delivery(info: Option<&Value>) -> CodeDelivery {
    let Some(info) = info else {
        return CodeDelivery::default();
    };
    CodeDelivery {
        kind: parse_code_kind(info.get("type")).unwrap_or_default(),
        next: parse_code_kind(info.get("next_type")),
        timeout_secs: info
            .get("timeout")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, i64::from(i32::MAX)) as i32,
    }
}

fn parse_email_reset(state: Option<&Value>) -> EmailResetState {
    let Some(state) = state.filter(|v| !v.is_null()) else {
        return EmailResetState::Unavailable;
    };
    let secs = |key: &str| {
        state
            .get(key)
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, i64::from(i32::MAX)) as i32
    };
    match state.get("@type").and_then(Value::as_str) {
        Some("emailAddressResetStateAvailable") => EmailResetState::Available {
            wait_period: secs("wait_period"),
        },
        Some("emailAddressResetStatePending") => EmailResetState::Pending {
            reset_in: secs("reset_in"),
        },
        _ => EmailResetState::Unavailable,
    }
}

pub(crate) fn parse_auth(value: &Value) -> AuthorizationState {
    let ty = value.get("@type").and_then(Value::as_str).unwrap_or("");
    match ty {
        "authorizationStateWaitTdlibParameters" => AuthorizationState::WaitTdlibParameters,
        "authorizationStateWaitPhoneNumber" => AuthorizationState::WaitPhoneNumber,
        "authorizationStateWaitPremiumPurchase" => AuthorizationState::WaitPremiumPurchase,
        "authorizationStateWaitEmailAddress" => AuthorizationState::WaitEmailAddress,
        "authorizationStateWaitEmailCode" => AuthorizationState::WaitEmailCode {
            email_pattern: value
                .get("code_info")
                .map(|info| json_field_str(info, "email_address_pattern"))
                .unwrap_or_default(),
            code_length: value
                .get("code_info")
                .and_then(|info| info.get("length"))
                .and_then(Value::as_i64)
                .and_then(|n| i32::try_from(n).ok())
                .filter(|n| *n > 0),
            reset: parse_email_reset(value.get("email_address_reset_state")),
        },
        "authorizationStateWaitCode" => AuthorizationState::WaitCode {
            code_length: value
                .get("code_info")
                .and_then(|info| info.get("type"))
                .and_then(|ty| ty.get("length"))
                .and_then(Value::as_i64)
                .map(|n| n as i32),
            delivery: parse_code_delivery(value.get("code_info")),
        },
        "authorizationStateWaitOtherDeviceConfirmation" => {
            // The `link` is the QR payload (a tg://login token). It is
            // carried through to the UI for QR rendering and never logged.
            AuthorizationState::WaitOtherDeviceConfirmation {
                link: json_field_str(value, "link"),
            }
        }
        "authorizationStateWaitRegistration" => {
            let terms = value
                .get("terms_of_service")
                .filter(|terms| !terms.is_null());
            let parsed = terms.and_then(|terms| {
                let text = terms.get("text")?.get("text")?.as_str()?;
                let age = i32::try_from(terms.get("min_user_age")?.as_i64()?).ok()?;
                if terms.get("@type")?.as_str()? != "termsOfService" || text.is_empty() || age < 0 {
                    return None;
                }
                Some(RegistrationTerms {
                    text: text.into(),
                    min_user_age: age,
                })
            });
            if terms.is_some() && parsed.is_none() {
                AuthorizationState::Unknown("invalid-registration-terms".into())
            } else {
                AuthorizationState::WaitRegistration { terms: parsed }
            }
        }
        "authorizationStateWaitPassword" => AuthorizationState::WaitPassword {
            has_recovery_email: value
                .get("has_recovery_email_address")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        "authorizationStateReady" => AuthorizationState::Ready,
        "authorizationStateLoggingOut" => AuthorizationState::LoggingOut,
        "authorizationStateClosing" => AuthorizationState::Closing,
        "authorizationStateClosed" => AuthorizationState::Closed,
        other => AuthorizationState::Unknown(other.to_string()),
    }
}

pub(crate) fn parse_connection(value: Option<&Value>) -> ConnectionState {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("connectionStateWaitingForNetwork") => ConnectionState::WaitingForNetwork,
        Some("connectionStateConnectingToProxy") => ConnectionState::ConnectingToProxy,
        Some("connectionStateConnecting") => ConnectionState::Connecting,
        Some("connectionStateUpdating") => ConnectionState::Updating,
        Some("connectionStateReady") => ConnectionState::Ready,
        _ => ConnectionState::Unknown,
    }
}

/// Retry-after seconds from a TDLib flood message: `FLOOD_WAIT_<n>`
/// (login flow) or `Too Many Requests: retry after <n>` (code 429 on any
/// other request).
pub(crate) fn parse_flood_wait_secs(message: &str) -> Option<u64> {
    message
        .strip_prefix("FLOOD_WAIT_")
        .or_else(|| message.strip_prefix("Too Many Requests: retry after "))
        .and_then(|n| n.trim().parse::<u64>().ok())
}

pub(crate) fn parse_error(value: Option<&Value>) -> TdError {
    let code = value
        .and_then(|v| v.get("code"))
        .and_then(Value::as_i64)
        .unwrap_or(0) as i32;
    // S14: story-posting restriction errors are classified here — the
    // only place the raw message is still available (`TdError` drops it
    // for secret-scrubbing). Anything unrecognized falls back to the
    // code-based class, so every other flow is unchanged.
    // Slice parity:platform-flood-errors — same story for `FLOOD_WAIT_<n>`:
    // extract the retry-after seconds before the message is dropped, so
    // error lines can show a real countdown. Only the number survives.
    let flood_wait_secs = value
        .and_then(|v| v.get("message"))
        .and_then(Value::as_str)
        .and_then(parse_flood_wait_secs);
    if let Some(class) = value
        .and_then(|v| v.get("message"))
        .and_then(Value::as_str)
        .and_then(|message| {
            crate::story_restriction::classify_server_message(message)
                .or_else(|| crate::auth::classify_sign_in_message(message))
        })
    {
        return TdError {
            code,
            class,
            flood_wait_secs,
        };
    }
    // Slice msg-richtext-ai-tools: the documented AI flood error
    // (`AICOMPOSE_FLOOD_PREMIUM`) is a known constant, safe to match —
    // it names a product state, not a secret.
    if value
        .and_then(|v| v.get("message"))
        .and_then(Value::as_str)
        .is_some_and(|message| message == "AICOMPOSE_FLOOD_PREMIUM")
    {
        return TdError {
            code,
            class: ErrorClass::AiComposeFloodPremium,
            flood_wait_secs,
        };
    }
    let permission = match value.and_then(|v| v.get("message")).and_then(Value::as_str) {
        Some("Not enough rights to send stickers to the chat" | "CHAT_SEND_STICKERS_FORBIDDEN") => {
            Some(ErrorClass::StickersForbidden)
        }
        Some("Not enough rights to send animations to the chat" | "CHAT_SEND_GIFS_FORBIDDEN") => {
            Some(ErrorClass::GifsForbidden)
        }
        _ => None,
    };
    let mut err = TdError::from_code(code);
    if let Some(class) = permission {
        err.class = class;
    }
    err.flood_wait_secs = flood_wait_secs;
    err
}

#[cfg(test)]
mod tests {
    use super::*;

    // Slice parity:platform-flood-errors — `parse_error` extracts the
    // retry-after seconds from `FLOOD_WAIT_<n>` before the message is
    // dropped for secret-scrubbing.
    #[test]
    fn parse_error_extracts_flood_wait_seconds() {
        let value: Value =
            serde_json::from_str(r#"{"@type":"error","code":429,"message":"FLOOD_WAIT_30"}"#)
                .unwrap();
        let err = parse_error(Some(&value));
        assert_eq!(err.code, 429);
        assert_eq!(err.class, ErrorClass::Flood);
        assert_eq!(err.flood_wait_secs, Some(30));
        assert_eq!(
            err.flood_line("too many requests — wait and try again"),
            "try again in 30 seconds"
        );
    }

    #[test]
    fn parse_error_without_flood_wait_has_no_countdown() {
        let value: Value =
            serde_json::from_str(r#"{"@type":"error","code":429,"message":"FLOOD"}"#).unwrap();
        let err = parse_error(Some(&value));
        assert_eq!(err.class, ErrorClass::Flood);
        assert_eq!(err.flood_wait_secs, None);
        assert_eq!(
            err.flood_line("too many requests — wait and try again"),
            "too many requests — wait and try again"
        );
    }
}
