use super::*;
use serde_json::Value;

/// Slice A3: one `session` from a `getActiveSessions` answer
/// (`session id:int64 is_current:Bool is_password_pending:Bool
/// is_unconfirmed:Bool can_accept_secret_chats:Bool can_accept_calls:Bool
/// device_type:SessionDeviceType api_id:int32 application_name:string
/// application_version:string is_official_application:Bool device_model:string
/// platform:string system_version:string log_in_date:int32
/// last_active_date:int32 ip_address:string location:string = Session;`,
/// schema 1.8.67, line 9144). Only the fields the sessions list renders
/// are kept; `is_unconfirmed`/`can_accept_*`/`device_type`/`log_in_date`
/// are out of this slice (per-session toggles are A4).
/// `is_password_pending` marks an incomplete login attempt — TGX
/// (`Tdlib.java` `SessionsInfo`) treats exactly these as the
/// "Incomplete Login Attempts" section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSession {
    pub id: i64,
    pub is_current: bool,
    pub is_password_pending: bool,
    /// Slice A4: `can_accept_secret_chats` (schema 1.8.67, line 9144)
    /// — the `toggleSessionCanAcceptSecretChats` target state.
    pub can_accept_secret_chats: bool,
    /// Slice A4: `can_accept_calls` (schema 1.8.67, line 9144) — the
    /// `toggleSessionCanAcceptCalls` target state.
    pub can_accept_calls: bool,
    pub device_model: String,
    pub application_name: String,
    pub application_version: String,
    pub platform: String,
    pub system_version: String,
    pub last_active_date: i32,
    pub ip_address: String,
    pub location: String,
}

/// Slice A4: one `connectedWebsite` from a `getConnectedWebsites` answer
/// (`connectedWebsite id:int64 domain_name:string bot_user_id:int53
/// browser:string platform:string log_in_date:int32
/// last_active_date:int32 ip_address:string location:string =
/// ConnectedWebsite;`, schema 1.8.67, line 9168). All fields are parsed;
/// resolving `bot_user_id` to a username needs the users cache (backlog,
/// see DECISIONS.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedWebsite {
    pub id: i64,
    pub domain_name: String,
    pub bot_user_id: i64,
    pub browser: String,
    pub platform: String,
    pub log_in_date: i32,
    pub last_active_date: i32,
    pub ip_address: String,
    pub location: String,
}

/// Slice A3: one `session` from a `sessions` answer (schema 1.8.67, line
/// 9144). The `@type` guard keeps a malformed entry from poisoning the
/// list; only the session id is required (`id:int64` is the terminate
/// key).
pub(crate) fn parse_session(value: &Value) -> Option<ParsedSession> {
    if value.get("@type").and_then(Value::as_str) != Some("session") {
        return None;
    }
    let id = json_i64_field(value.get("id"), 0);
    if id == 0 {
        return None;
    }
    let str_field = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let bool_field = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
    Some(ParsedSession {
        id,
        is_current: bool_field("is_current"),
        is_password_pending: bool_field("is_password_pending"),
        can_accept_secret_chats: bool_field("can_accept_secret_chats"),
        can_accept_calls: bool_field("can_accept_calls"),
        device_model: str_field("device_model"),
        application_name: str_field("application_name"),
        application_version: str_field("application_version"),
        platform: str_field("platform"),
        system_version: str_field("system_version"),
        last_active_date: json_i32(value.get("last_active_date"), 0),
        ip_address: str_field("ip_address"),
        location: str_field("location"),
    })
}

/// Slice A4: one `connectedWebsite` from a `connectedWebsites` answer.
/// A zero id is not a website (schema 1.8.67, line 9168), like A3's
/// session parse.
pub(crate) fn parse_website(value: &Value) -> Option<ParsedWebsite> {
    if value.get("@type").and_then(Value::as_str) != Some("connectedWebsite") {
        return None;
    }
    let id = json_i64_field(value.get("id"), 0);
    if id == 0 {
        return None;
    }
    let str_field = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Some(ParsedWebsite {
        id,
        domain_name: str_field("domain_name"),
        bot_user_id: json_i64_field(value.get("bot_user_id"), 0),
        browser: str_field("browser"),
        platform: str_field("platform"),
        log_in_date: json_i32(value.get("log_in_date"), 0),
        last_active_date: json_i32(value.get("last_active_date"), 0),
        ip_address: str_field("ip_address"),
        location: str_field("location"),
    })
}
