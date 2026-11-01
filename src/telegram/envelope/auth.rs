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
    /// A folder request failed on a limit (`FILTER_INCLUDE_TOO_MUCH`,
    /// `CHATLISTS_TOO_MUCH`, ...): which one the message named. Only this
    /// classification survives, never the text.
    pub limit_hint: Option<crate::folder_limits::LimitHint>,
    /// The request ran into a Premium limit other than folders (pins,
    /// caption length, joined communities, public links): which one the
    /// message named. Only this classification survives, never the text.
    pub premium_limit: Option<crate::premium_limits::PremiumLimitKind>,
    /// A revenue withdrawal or statistics request failed for a reason the
    /// screen reacts to (`PASSWORD_MISSING`, `AMOUNT_TOO_SMALL`, ...).
    /// Only this classification survives, never the text.
    pub earn_hint: Option<crate::earn::EarnErrorHint>,
    /// The server's error type when the message is one: an upper-case
    /// constant such as `PASSKEY_CREDENTIAL_NOT_FOUND`. Screens that show
    /// it as Telegram Desktop does (the passkey sign-in) read it. Free text,
    /// which can carry secrets, never lands here.
    pub server_type: Option<String>,
    /// TDLib's words about a database or file it could not open, with the
    /// quoted path cut out ([`crate::auth::database_error_detail`]). The
    /// startup card shows it under the reason.
    pub database_detail: Option<String>,
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
    /// `INPUT_TEXT_TOO_LONG`: the text sent to `composeTextWithAi` /
    /// `fixTextWithAi` is over the server's limit.
    InputTextTooLong,
    /// `TONES_SAVED_TOO_MANY`: no more custom text composition styles can
    /// be added (`createTextCompositionStyle`, `addTextCompositionStyle`).
    StylesSavedTooMany,
    /// `AICOMPOSE_TONE_SLUG_INVALID`, `AICOMPOSE_TONE_INVALID` or
    /// `TONE_NOT_FOUND`: the text composition style was deleted or its
    /// name is bad (tdesktop api_compose_with_ai.cpp).
    StyleInvalid,
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
    /// `COLLECTIBLE_NOT_FOUND`: the tapped username is not a collectible
    /// one (tdesktop then copies its link instead).
    CollectibleNotFound,
    /// `GROUPCALL_JOIN_MISSING` / `GROUPCALL_FORBIDDEN`: a group call
    /// request needs the call rejoined (tdesktop's `startRejoin`).
    GroupCallRejoin,
    /// `GROUPCALL_INVALID`: the call is gone; TDLib treats it as having
    /// left the call (`GroupCallManager::on_group_call_left`).
    GroupCallLeft,
    /// `TIME_TOO_BIG`: a stream segment is not on the server yet.
    StreamTimeTooBig,
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
            limit_hint: None,
            premium_limit: None,
            earn_hint: None,
            server_type: None,
            database_detail: None,
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
            Some(secs) => crate::lang::trn(crate::lang::keys::Q_OPS_ERR_FLOOD_SECONDS, secs),
            None => crate::lang::tr(crate::lang::keys::Q_OPS_ERR_FLOOD_LATER).to_string(),
        })
    }

    /// Slice parity:platform-flood-errors — the retry countdown line for
    /// a flood error: "try again in N seconds" when the wait is known,
    /// otherwise the honest generic line.
    pub fn flood_line(&self, generic: &str) -> String {
        match self.flood_wait_secs {
            Some(secs) => crate::lang::trn(crate::lang::keys::Q_OPS_ERR_TRY_AGAIN_SECONDS, secs),
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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CodeDelivery {
    pub kind: CodeKind,
    pub next: Option<CodeKind>,
    pub timeout_secs: i32,
    /// Per-type fields of the current `type` (call pattern, Fragment URL).
    pub detail: CodeDetail,
}

/// The type-specific payload of `authenticationCodeType*` that the code
/// screen shows or opens. Empty strings mean "not supplied".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CodeDetail {
    /// `FlashCall.pattern` or `MissedCall.phone_number_prefix`.
    pub call_number: String,
    /// `MissedCall.length`: digits to type, excluding the prefix.
    pub missed_digits: Option<i32>,
    /// `Fragment.url`; only a plain https URL is kept.
    pub url: String,
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
    /// `authorizationStateWaitPremiumPurchase`. Quill cannot take an
    /// in-store purchase, so only what the explainer shows is kept.
    WaitPremiumPurchase {
        premium_day_count: i32,
        support_email_address: String,
        support_email_subject: String,
    },
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
    /// Quill's own state, never sent by TDLib: this client cannot start.
    StartupFailed(crate::auth::StartupFailure),
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

fn parse_code_detail(ty: Option<&Value>) -> CodeDetail {
    let Some(ty) = ty.filter(|v| !v.is_null()) else {
        return CodeDetail::default();
    };
    let kind = ty.get("@type").and_then(Value::as_str).unwrap_or_default();
    let mut detail = CodeDetail::default();
    match kind {
        "authenticationCodeTypeFlashCall" => {
            detail.call_number = json_field_str(ty, "pattern");
        }
        "authenticationCodeTypeMissedCall" => {
            detail.call_number = json_field_str(ty, "phone_number_prefix");
            detail.missed_digits = ty
                .get("length")
                .and_then(Value::as_i64)
                .and_then(|n| i32::try_from(n).ok())
                .filter(|n| *n > 0);
        }
        "authenticationCodeTypeFragment" => {
            let url = json_field_str(ty, "url");
            // The URL comes from the server and is opened in the browser:
            // accept https only.
            if url.starts_with("https://") && !url.chars().any(char::is_whitespace) {
                detail.url = url;
            }
        }
        _ => {}
    }
    detail
}

pub(crate) fn parse_code_delivery(info: Option<&Value>) -> CodeDelivery {
    let Some(info) = info else {
        return CodeDelivery::default();
    };
    CodeDelivery {
        detail: parse_code_detail(info.get("type")),
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
        "authorizationStateWaitPremiumPurchase" => AuthorizationState::WaitPremiumPurchase {
            premium_day_count: value
                .get("premium_day_count")
                .and_then(Value::as_i64)
                .map_or(0, |n| n.clamp(0, i64::from(i32::MAX)) as i32),
            support_email_address: json_field_str(value, "support_email_address"),
            support_email_subject: json_field_str(value, "support_email_subject"),
        },
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
                .map(|n| n.sat_i32()),
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

/// `message` when it is a server error type: `[A-Z][A-Z0-9_]*`, at most 64
/// bytes. Phone numbers, codes and free text never match.
fn server_error_type(message: &str) -> Option<String> {
    let mut bytes = message.bytes();
    let first = bytes.next()?;
    (first.is_ascii_uppercase()
        && message.len() <= 64
        && bytes.all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'))
    .then(|| message.to_string())
}

pub(crate) fn parse_error(value: Option<&Value>) -> TdError {
    let code = value
        .and_then(|v| v.get("code"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .sat_i32();
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
    let earn_hint = value
        .and_then(|v| v.get("message"))
        .and_then(Value::as_str)
        .and_then(crate::earn::error_hint);
    let server_type = value
        .and_then(|v| v.get("message"))
        .and_then(Value::as_str)
        .and_then(server_error_type);
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
            limit_hint: None,
            premium_limit: None,
            earn_hint,
            server_type,
            database_detail: None,
        };
    }
    let ai_class = match value.and_then(|v| v.get("message")).and_then(Value::as_str) {
        Some("AICOMPOSE_FLOOD_PREMIUM") => Some(ErrorClass::AiComposeFloodPremium),
        Some("INPUT_TEXT_TOO_LONG") => Some(ErrorClass::InputTextTooLong),
        Some("TONES_SAVED_TOO_MANY") => Some(ErrorClass::StylesSavedTooMany),
        Some("AICOMPOSE_TONE_SLUG_INVALID" | "AICOMPOSE_TONE_INVALID" | "TONE_NOT_FOUND") => {
            Some(ErrorClass::StyleInvalid)
        }
        _ => None,
    };
    if let Some(class) = ai_class {
        return TdError {
            code,
            class,
            flood_wait_secs,
            limit_hint: None,
            premium_limit: None,
            earn_hint,
            server_type,
            database_detail: None,
        };
    }
    if value
        .and_then(|v| v.get("message"))
        .and_then(Value::as_str)
        .is_some_and(|message| message == "COLLECTIBLE_NOT_FOUND")
    {
        return TdError {
            code,
            class: ErrorClass::CollectibleNotFound,
            flood_wait_secs,
            limit_hint: None,
            premium_limit: None,
            earn_hint,
            server_type,
            database_detail: None,
        };
    }
    let permission = match value.and_then(|v| v.get("message")).and_then(Value::as_str) {
        Some("Not enough rights to send stickers to the chat" | "CHAT_SEND_STICKERS_FORBIDDEN") => {
            Some(ErrorClass::StickersForbidden)
        }
        Some("Not enough rights to send animations to the chat" | "CHAT_SEND_GIFS_FORBIDDEN") => {
            Some(ErrorClass::GifsForbidden)
        }
        // Stream segments: known server constants, safe to match.
        Some("GROUPCALL_JOIN_MISSING" | "GROUPCALL_FORBIDDEN") => Some(ErrorClass::GroupCallRejoin),
        Some("GROUPCALL_INVALID") => Some(ErrorClass::GroupCallLeft),
        Some("TIME_TOO_BIG") => Some(ErrorClass::StreamTimeTooBig),
        _ => None,
    };
    let mut err = TdError::from_code(code);
    if let Some(class) = permission {
        err.class = class;
    }
    err.flood_wait_secs = flood_wait_secs;
    err.earn_hint = earn_hint;
    err.database_detail = value
        .and_then(|v| v.get("message"))
        .and_then(Value::as_str)
        .and_then(crate::auth::database_error_detail);
    err.server_type = server_type;
    err.limit_hint = value
        .and_then(|v| v.get("message"))
        .and_then(Value::as_str)
        .and_then(crate::folder_limits::limit_hint);
    err.premium_limit = value
        .and_then(|v| v.get("message"))
        .and_then(Value::as_str)
        .and_then(crate::premium_limits::kind_for_message);
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
    fn parse_error_keeps_only_server_error_types() {
        let parse = |message: &str| {
            let value = serde_json::json!({"@type": "error", "code": 400, "message": message});
            parse_error(Some(&value)).server_type
        };
        assert_eq!(
            parse("PASSKEY_CREDENTIAL_NOT_FOUND").as_deref(),
            Some("PASSKEY_CREDENTIAL_NOT_FOUND")
        );
        assert_eq!(parse("FLOOD_WAIT_30").as_deref(), Some("FLOOD_WAIT_30"));
        assert_eq!(parse("+15551234567"), None);
        assert_eq!(parse("123456"), None);
        assert_eq!(parse("Invalid code 12345"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse(&"A".repeat(65)), None);
    }

    #[test]
    fn parse_error_classifies_ai_compose_errors() {
        for (message, class) in [
            ("AICOMPOSE_FLOOD_PREMIUM", ErrorClass::AiComposeFloodPremium),
            ("INPUT_TEXT_TOO_LONG", ErrorClass::InputTextTooLong),
            ("TONES_SAVED_TOO_MANY", ErrorClass::StylesSavedTooMany),
            ("AICOMPOSE_TONE_SLUG_INVALID", ErrorClass::StyleInvalid),
            ("AICOMPOSE_TONE_INVALID", ErrorClass::StyleInvalid),
            ("TONE_NOT_FOUND", ErrorClass::StyleInvalid),
        ] {
            let value = serde_json::json!({"@type": "error", "code": 400, "message": message});
            let err = parse_error(Some(&value));
            assert_eq!(err.class, class, "{message}");
            assert_eq!(err.server_type.as_deref(), Some(message));
        }
    }

    #[test]
    fn parse_error_classifies_stream_segment_errors() {
        for (message, class) in [
            ("GROUPCALL_JOIN_MISSING", ErrorClass::GroupCallRejoin),
            ("GROUPCALL_FORBIDDEN", ErrorClass::GroupCallRejoin),
            ("GROUPCALL_INVALID", ErrorClass::GroupCallLeft),
            ("TIME_TOO_BIG", ErrorClass::StreamTimeTooBig),
        ] {
            let value = serde_json::json!({"@type": "error", "code": 400, "message": message});
            assert_eq!(parse_error(Some(&value)).class, class, "{message}");
        }
    }

    #[test]
    fn parse_error_keeps_only_the_premium_limit() {
        use crate::premium_limits::PremiumLimitKind;
        let value: Value =
            serde_json::from_str(r#"{"@type":"error","code":400,"message":"CHANNELS_TOO_MUCH"}"#)
                .unwrap();
        let err = parse_error(Some(&value));
        assert_eq!(err.premium_limit, Some(PremiumLimitKind::Channels));
        assert_eq!(err.limit_hint, None);
    }

    #[test]
    fn parse_error_keeps_only_the_limit_hint() {
        use crate::folder_limits::LimitHint;
        let value: Value = serde_json::from_str(
            r#"{"@type":"error","code":400,"message":"FILTER_INCLUDE_TOO_MUCH"}"#,
        )
        .unwrap();
        let err = parse_error(Some(&value));
        assert_eq!(err.limit_hint, Some(LimitHint::Include));
        let value: Value =
            serde_json::from_str(r#"{"@type":"error","code":400,"message":"PHONE_CODE_EMPTY"}"#)
                .unwrap();
        assert_eq!(parse_error(Some(&value)).limit_hint, None);
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
