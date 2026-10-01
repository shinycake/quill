use crate::ids::RequestId;
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
            "use_secret_chats": true,
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

/// Slice A3: `getActiveSessions = Sessions;` (TDLib 1.8.67,
/// `schema/td_api.tl:15102`): "Returns all active sessions of the current
/// user."
pub fn get_active_sessions(extra: RequestId) -> String {
    json!({
        "@type": "getActiveSessions",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice A3: `terminateSession session_id:int64 = Ok;` (TDLib 1.8.67,
/// `schema/td_api.tl:15105`): "Terminates a session of the current user."
pub fn terminate_session(extra: RequestId, session_id: i64) -> String {
    json!({
        "@type": "terminateSession",
        "@extra": extra.as_extra(),
        "session_id": session_id,
    })
    .to_string()
}

/// Slice A3: `terminateAllOtherSessions = Ok;` (TDLib 1.8.67,
/// `schema/td_api.tl:15108`): "Terminates all other sessions of the
/// current user."
pub fn terminate_all_other_sessions(extra: RequestId) -> String {
    json!({
        "@type": "terminateAllOtherSessions",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice A4: `toggleSessionCanAcceptCalls session_id:int64
/// can_accept_calls:Bool = Ok;` (TDLib 1.8.67, `schema/td_api.tl:15114`):
/// per-session toggle — the session accepts (or rejects) incoming calls.
/// TGX applies it directly (no confirmation, `EditSessionController`);
/// the toggled value is reflected from the authoritative `ok`.
pub fn toggle_session_can_accept_calls(
    extra: RequestId,
    session_id: i64,
    can_accept_calls: bool,
) -> String {
    json!({
        "@type": "toggleSessionCanAcceptCalls",
        "@extra": extra.as_extra(),
        "session_id": session_id,
        "can_accept_calls": can_accept_calls,
    })
    .to_string()
}

/// Slice A4: `getConnectedWebsites = ConnectedWebsites;` (TDLib 1.8.67,
/// `schema/td_api.tl:15124`): "Returns all website where the current
/// user used Telegram to log in" (TGX `SettingsWebsitesController` /
/// `WebSessionsTitle` "Logged In with Telegram").
pub fn get_connected_websites(extra: RequestId) -> String {
    json!({
        "@type": "getConnectedWebsites",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice A4: `disconnectWebsite website_id:int64 = Ok;` (TDLib 1.8.67,
/// `schema/td_api.tl:15127`): "Disconnects website from the current
/// user's Telegram account" (TGX `TerminateWebSessionQuestion`
/// "Disconnect %1$s?").
pub fn disconnect_website(extra: RequestId, website_id: i64) -> String {
    json!({
        "@type": "disconnectWebsite",
        "@extra": extra.as_extra(),
        "website_id": website_id,
    })
    .to_string()
}

/// Slice A4: `disconnectAllWebsites = Ok;` (TDLib 1.8.67,
/// `schema/td_api.tl:15130`): "Disconnects all websites from the current
/// user's Telegram account" (TGX `DisconnectAllWebsitesHint` "Are you
/// sure you want to disconnect all websites?").
pub fn disconnect_all_websites(extra: RequestId) -> String {
    json!({
        "@type": "disconnectAllWebsites",
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

/// `resendAuthenticationCode` for the auth flow (NOT `resendPhoneNumberCode`,
/// which belongs to the phone-number-verification flow). Reason is the
/// user-initiated one; TDLib enforces its own server-side cooldown (429 on
/// too-early resend), so no local countdown is invented.
pub fn resend_authentication_code(extra: RequestId) -> String {
    json!({
        "@type": "resendAuthenticationCode",
        "@extra": extra.as_extra(),
        "reason": { "@type": "resendCodeReasonUserRequest" },
    })
    .to_string()
}

/// Slice A10: `requestAuthenticationPasswordRecovery` — asks Telegram to
/// email a 2FA recovery code. Works only in `authorizationStateWaitPassword`
/// (schema 1.8.67, `schema/td_api.tl:11381`).
pub fn request_authentication_password_recovery(extra: RequestId) -> String {
    json!({
        "@type": "requestAuthenticationPasswordRecovery",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice A10: `recoverAuthenticationPassword`. `code` is the emailed
/// recovery code; new password and hint are empty — recovery removes 2FA
/// and the user re-enables it from Settings (slice A2). `checkAuthenticationPasswordRecoveryCode`
/// is skipped: this call validates the code itself, so the extra
/// round-trip adds nothing. Callers must not log `code`.
pub fn recover_authentication_password(extra: RequestId, code: &str) -> String {
    json!({
        "@type": "recoverAuthenticationPassword",
        "@extra": extra.as_extra(),
        "recovery_code": code,
        "new_password": "",
        "new_hint": "",
    })
    .to_string()
}

/// `requestQrCodeAuthentication`. `other_user_ids` is empty: this client has
/// no other logged-in user to hint at.
pub fn request_qr_code_authentication(extra: RequestId) -> String {
    json!({
        "@type": "requestQrCodeAuthentication",
        "@extra": extra.as_extra(),
        "other_user_ids": [],
    })
    .to_string()
}

/// Slice A2: `getPasswordState` (TDLib 1.8.67, `schema/td_api.tl:11426`):
/// "Returns the current state of 2-step verification".
pub fn get_password_state(extra: RequestId) -> String {
    json!({
        "@type": "getPasswordState",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice A2: `setPassword` (TDLib 1.8.67, `schema/td_api.tl:11434`):
/// `setPassword old_password:string new_password:string new_hint:string
/// set_recovery_email_address:Bool new_recovery_email_address:string =
/// PasswordState;`
/// "Changes the 2-step verification password for the current user. If a
/// new recovery email address is specified, then the change will not be
/// applied until the new recovery email address is confirmed".
/// Empty `new_password` removes the password; `old_password` is empty
/// when enabling for the first time (TGX `PasswordController` sends null
/// in MODE_NEW). Callers must not log `old_password`/`new_password`.
pub fn set_password(
    extra: RequestId,
    old_password: &str,
    new_password: &str,
    new_hint: &str,
    new_recovery_email_address: Option<&str>,
) -> String {
    json!({
        "@type": "setPassword",
        "@extra": extra.as_extra(),
        "old_password": old_password,
        "new_password": new_password,
        "new_hint": new_hint,
        "set_recovery_email_address": new_recovery_email_address.is_some(),
        "new_recovery_email_address": new_recovery_email_address.unwrap_or(""),
    })
    .to_string()
}

/// Slice A2: `setRecoveryEmailAddress` (TDLib 1.8.67,
/// `schema/td_api.tl:11458`): "Changes the 2-step verification recovery
/// email address of the user. If a new recovery email address is
/// specified, then the change will not be applied until the new recovery
/// email address is confirmed." Callers must not log `password`.
pub fn set_recovery_email_address(
    extra: RequestId,
    password: &str,
    new_recovery_email_address: &str,
) -> String {
    json!({
        "@type": "setRecoveryEmailAddress",
        "@extra": extra.as_extra(),
        "password": password,
        "new_recovery_email_address": new_recovery_email_address,
    })
    .to_string()
}

/// Slice A2: `resendRecoveryEmailAddressCode` (TDLib 1.8.67,
/// `schema/td_api.tl:11464`): "Resends the 2-step verification recovery
/// email address verification code". TDLib enforces its own server-side
/// cooldown (429 on too-early resend), so no local countdown is invented.
pub fn resend_recovery_email_address_code(extra: RequestId) -> String {
    json!({
        "@type": "resendRecoveryEmailAddressCode",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice A2: `cancelRecoveryEmailAddressVerification` (TDLib 1.8.67,
/// `schema/td_api.tl:11467`): "Cancels verification of the 2-step
/// verification recovery email address".
pub fn cancel_recovery_email_address_verification(extra: RequestId) -> String {
    json!({
        "@type": "cancelRecoveryEmailAddressVerification",
        "@extra": extra.as_extra(),
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

/// Slice A7: `deleteAccount reason:string password:string = Ok;` (TDLib
/// 1.8.67, `schema/td_api.tl:15675`): "Deletes the account of the current
/// user, deleting all information associated with the user from the
/// server."
pub fn delete_account(extra: RequestId, reason: &str, password: &str) -> String {
    json!({
        "@type": "deleteAccount",
        "@extra": extra.as_extra(),
        "reason": reason,
        "password": password,
    })
    .to_string()
}

/// Slice A7: `getAccountTtl = AccountTtl;` (TDLib 1.8.67,
/// `schema/td_api.tl:15669`).
pub fn get_account_ttl(extra: RequestId) -> String {
    json!({
        "@type": "getAccountTtl",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice A7: `setAccountTtl ttl:accountTtl = Ok;` (TDLib 1.8.67,
/// `schema/td_api.tl:15666`): "Changes the period of inactivity after
/// which the account of the current user will automatically be deleted."
pub fn set_account_ttl(extra: RequestId, days: i32) -> String {
    json!({
        "@type": "setAccountTtl",
        "@extra": extra.as_extra(),
        "ttl": { "@type": "accountTtl", "days": days },
    })
    .to_string()
}

/// Slice A8: `sendPhoneNumberCode phone_number:string
/// settings:phoneNumberAuthenticationSettings type:PhoneNumberCodeType =
/// AuthenticationCodeInfo;` (TDLib 1.8.67, `schema/td_api.tl:14877`):
/// "Sends a code to the specified phone number. Aborts previous phone
/// number verification if there was one." Sent while authorized with
/// `phoneNumberCodeTypeChange` (schema 1.8.67, `schema/td_api.tl:10384`) —
/// the while-authorized change-number flow, NOT the auth flow. Callers
/// must not log `phone_number`.
pub fn send_phone_number_code(extra: RequestId, phone_number: &str) -> String {
    json!({
        "@type": "sendPhoneNumberCode",
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
        },
        "type": { "@type": "phoneNumberCodeTypeChange" },
    })
    .to_string()
}

/// Slice A8: `resendPhoneNumberCode reason:ResendCodeReason =
/// AuthenticationCodeInfo;` (TDLib 1.8.67, `schema/td_api.tl:14888`).
/// "Works only if the previously received authenticationCodeInfo
/// next_code_type was not null and the server-specified timeout has
/// passed." A manual resend is a user request.
pub fn resend_phone_number_code(extra: RequestId) -> String {
    json!({
        "@type": "resendPhoneNumberCode",
        "@extra": extra.as_extra(),
        "reason": { "@type": "resendCodeReasonUserRequest" },
    })
    .to_string()
}

/// Slice A8: `checkPhoneNumberCode code:string = Ok;` (TDLib 1.8.67,
/// `schema/td_api.tl:14891`): "Checks the authentication code and
/// completes the request for which the code was sent if appropriate."
/// Callers must not log `code`.
pub fn check_phone_number_code(extra: RequestId, code: &str) -> String {
    json!({
        "@type": "checkPhoneNumberCode",
        "@extra": extra.as_extra(),
        "code": code,
    })
    .to_string()
}

/// TDLib email login; values ride request JSON only.
pub fn set_authentication_email_address(extra: RequestId, email: &str) -> String {
    json!({"@type":"setAuthenticationEmailAddress","@extra":extra.as_extra(),"email_address":email})
        .to_string()
}
pub fn check_authentication_email_code(extra: RequestId, code: &str) -> String {
    json!({"@type":"checkAuthenticationEmailCode","@extra":extra.as_extra(),"code":{"@type":"emailAddressAuthenticationCode","code":code}}).to_string()
}

pub fn register_user(
    extra: RequestId,
    first: &str,
    last: &str,
    disable_notification: bool,
) -> String {
    json!({"@type":"registerUser","@extra":extra.as_extra(),"first_name":first,"last_name":last,"disable_notification":disable_notification}).to_string()
}
