use crate::ids::{ChatId, MessageId, RequestId};
use crate::pins::{TDLIB_CMAKE_VERSION, TDLIB_GIT_COMMIT};
use serde_json::{Value, json};

pub struct SetTdlibParameters {
    pub use_test_dc: bool,
    pub database_directory: String,
    pub files_directory: String,
    pub database_encryption_key_b64: String,
    pub api_id: i32,
    pub api_hash: String,
    pub device_model: String,
    pub system_version: String,
    pub application_version: String,
    pub system_language_code: String,
}

impl SetTdlibParameters {
    pub fn to_json(&self, extra: RequestId) -> String {
        json!({
            "@type": "setTdlibParameters",
            "@extra": extra.as_extra(),
            "use_test_dc": self.use_test_dc,
            "database_directory": self.database_directory,
            "files_directory": self.files_directory,
            "database_encryption_key": self.database_encryption_key_b64,
            "use_file_database": true,
            "use_chat_info_database": true,
            "use_message_database": true,
            "use_secret_chats": false,
            "api_id": self.api_id,
            "api_hash": self.api_hash,
            "system_language_code": self.system_language_code,
            "device_model": self.device_model,
            "system_version": self.system_version,
            "application_version": self.application_version,
        })
        .to_string()
    }
}

pub fn get_authorization_state(extra: RequestId) -> String {
    json!({
        "@type": "getAuthorizationState",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `setAuthenticationPhoneNumber`. Callers must not log `phone_number`.
pub fn set_authentication_phone_number(extra: RequestId, phone_number: &str) -> String {
    json!({
        "@type": "setAuthenticationPhoneNumber",
        "@extra": extra.as_extra(),
        "phone_number": phone_number,
        "settings": {
            "@type": "phoneNumberAuthenticationSettings",
            "allow_flash_call": false,
            "allow_missed_call": false,
            "is_current_phone_number": false,
            "has_unknown_phone_number": false,
            "allow_sms_retriever_api": false,
            "firebase_authentication_settings": Value::Null,
            "authentication_tokens": []
        }
    })
    .to_string()
}

/// `checkAuthenticationCode`. Callers must not log `code`.
pub fn check_authentication_code(extra: RequestId, code: &str) -> String {
    json!({
        "@type": "checkAuthenticationCode",
        "@extra": extra.as_extra(),
        "code": code,
    })
    .to_string()
}

/// `checkAuthenticationPassword`. Callers must not log `password`.
pub fn check_authentication_password(extra: RequestId, password: &str) -> String {
    json!({
        "@type": "checkAuthenticationPassword",
        "@extra": extra.as_extra(),
        "password": password,
    })
    .to_string()
}

pub fn close_request(extra: RequestId) -> String {
    json!({
        "@type": "close",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

pub fn log_out(extra: RequestId) -> String {
    json!({
        "@type": "logOut",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

pub fn load_chats(extra: RequestId, limit: i32) -> String {
    json!({
        "@type": "loadChats",
        "@extra": extra.as_extra(),
        "chat_list": { "@type": "chatListMain" },
        "limit": limit,
    })
    .to_string()
}

pub fn get_chat_history(
    extra: RequestId,
    chat_id: ChatId,
    from_message_id: MessageId,
    offset: i32,
    limit: i32,
    only_local: bool,
) -> String {
    json!({
        "@type": "getChatHistory",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "from_message_id": from_message_id.0,
        "offset": offset,
        "limit": limit,
        "only_local": only_local,
    })
    .to_string()
}

/// `sendMessage` for the pinned 1.8.67 schema: typed `topic_id`, not `message_thread_id`.
pub fn send_text(extra: RequestId, chat_id: ChatId, text: &str) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "reply_to": Value::Null,
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageText",
            "text": {
                "@type": "formattedText",
                "text": text,
                "entities": []
            },
            "link_preview_options": Value::Null,
            "clear_draft": true
        }
    })
    .to_string()
}

pub fn runtime_version_request() -> String {
    json!({
        "@type": "getOption",
        "name": "version",
    })
    .to_string()
}

pub fn expected_runtime_label() -> String {
    format!("{TDLIB_CMAKE_VERSION} ({TDLIB_GIT_COMMIT})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::RequestId;

    #[test]
    fn send_text_includes_topic_id_null() {
        let json = send_text(RequestId(9), ChatId(1), "hi");
        assert!(json.contains("\"topic_id\":null"));
        assert!(!json.contains("message_thread_id"));
        assert!(json.contains("\"@extra\":\"9\""));
    }

    #[test]
    fn extra_is_decimal_string_not_float() {
        let json = get_authorization_state(RequestId(9007199254740993));
        assert!(json.contains("\"@extra\":\"9007199254740993\""));
        assert!(!json.contains("\"@extra\":9007199254740993"));
    }

    #[test]
    fn set_phone_shape_does_not_use_message_thread_id() {
        let json = set_authentication_phone_number(RequestId(3), "+10001112222");
        assert!(json.contains("\"@type\":\"setAuthenticationPhoneNumber\""));
        assert!(json.contains("\"phone_number\":\"+10001112222\""));
        assert!(json.contains("phoneNumberAuthenticationSettings"));
        assert!(json.contains("\"@extra\":\"3\""));
    }

    #[test]
    fn check_authentication_code_shape() {
        let json = check_authentication_code(RequestId(4), "12345");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "checkAuthenticationCode");
        assert_eq!(v["@extra"], "4");
        assert_eq!(v["code"], "12345");
    }

    #[test]
    fn check_authentication_password_shape() {
        let json = check_authentication_password(RequestId(5), "unit-test-password");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "checkAuthenticationPassword");
        assert_eq!(v["@extra"], "5");
        assert_eq!(v["password"], "unit-test-password");
    }
}
