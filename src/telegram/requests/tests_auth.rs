use crate::ids::{ChatId, RequestId};
use crate::telegram::requests::*;

/// Slice A2: the five 2FA request shapes against the pinned schema
/// (1.8.67): `getPasswordState` (:11426), `setPassword` (:11434),
/// `setRecoveryEmailAddress` (:11458),
/// `resendRecoveryEmailAddressCode` (:11464),
/// `cancelRecoveryEmailAddressVerification` (:11467). Passwords and
/// emails ride the JSON body, never `@extra` or logs.
#[test]
fn a2_password_request_shapes_match_1_8_67() {
    let v: serde_json::Value = serde_json::from_str(&get_password_state(RequestId(71))).unwrap();
    assert_eq!(v["@type"], "getPasswordState");
    assert_eq!(v["@extra"], "71");

    // Enable: empty old password, new password + hint + recovery email
    // in the same call (TGX MODE_NEW sends null old_password).
    let v: serde_json::Value = serde_json::from_str(&set_password(
        RequestId(72),
        "",
        "s3cret",
        "hint",
        Some("me@example.com"),
    ))
    .unwrap();
    assert_eq!(v["@type"], "setPassword");
    assert_eq!(v["old_password"], "");
    assert_eq!(v["new_password"], "s3cret");
    assert_eq!(v["new_hint"], "hint");
    assert_eq!(v["set_recovery_email_address"], true);
    assert_eq!(v["new_recovery_email_address"], "me@example.com");

    // Disable: empty new password, no recovery-email change.
    let v: serde_json::Value =
        serde_json::from_str(&set_password(RequestId(73), "s3cret", "", "", None)).unwrap();
    assert_eq!(v["@type"], "setPassword");
    assert_eq!(v["old_password"], "s3cret");
    assert_eq!(v["new_password"], "");
    assert_eq!(v["set_recovery_email_address"], false);
    assert_eq!(v["new_recovery_email_address"], "");

    let v: serde_json::Value = serde_json::from_str(&set_recovery_email_address(
        RequestId(74),
        "s3cret",
        "new@example.com",
    ))
    .unwrap();
    assert_eq!(v["@type"], "setRecoveryEmailAddress");
    assert_eq!(v["@extra"], "74");
    assert_eq!(v["password"], "s3cret");
    assert_eq!(v["new_recovery_email_address"], "new@example.com");

    let v: serde_json::Value =
        serde_json::from_str(&resend_recovery_email_address_code(RequestId(75))).unwrap();
    assert_eq!(v["@type"], "resendRecoveryEmailAddressCode");
    assert_eq!(v["@extra"], "75");

    let v: serde_json::Value =
        serde_json::from_str(&cancel_recovery_email_address_verification(RequestId(76))).unwrap();
    assert_eq!(v["@type"], "cancelRecoveryEmailAddressVerification");
    assert_eq!(v["@extra"], "76");
}

#[test]
fn a3_session_request_shapes_match_1_8_67() {
    // Slice A3: `getActiveSessions = Sessions;` (line 15102),
    // `terminateSession session_id:int64 = Ok;` (line 15105),
    // `terminateAllOtherSessions = Ok;` (line 15108).
    let v: serde_json::Value = serde_json::from_str(&get_active_sessions(RequestId(71))).unwrap();
    assert_eq!(v["@type"], "getActiveSessions");
    assert_eq!(v["@extra"], "71");

    let v: serde_json::Value =
        serde_json::from_str(&terminate_session(RequestId(72), 123456789)).unwrap();
    assert_eq!(v["@type"], "terminateSession");
    assert_eq!(v["@extra"], "72");
    assert_eq!(v["session_id"], 123456789);

    let v: serde_json::Value =
        serde_json::from_str(&terminate_all_other_sessions(RequestId(73))).unwrap();
    assert_eq!(v["@type"], "terminateAllOtherSessions");
    assert_eq!(v["@extra"], "73");
}

#[test]
fn a8_change_number_request_shapes_match_1_8_67() {
    // Slice A8: `sendPhoneNumberCode phone_number:string
    // settings:phoneNumberAuthenticationSettings
    // type:PhoneNumberCodeType = AuthenticationCodeInfo;` (line
    // 14877), `resendPhoneNumberCode reason:ResendCodeReason =
    // AuthenticationCodeInfo;` (line 14888),
    // `checkPhoneNumberCode code:string = Ok;` (line 14891),
    // `phoneNumberCodeTypeChange = PhoneNumberCodeType;` (line 10384).
    let v: serde_json::Value =
        serde_json::from_str(&send_phone_number_code(RequestId(77), "+15550199")).unwrap();
    assert_eq!(v["@type"], "sendPhoneNumberCode");
    assert_eq!(v["@extra"], "77");
    assert_eq!(v["phone_number"], "+15550199");
    assert_eq!(v["settings"]["@type"], "phoneNumberAuthenticationSettings");
    assert_eq!(v["type"]["@type"], "phoneNumberCodeTypeChange");

    let v: serde_json::Value =
        serde_json::from_str(&resend_phone_number_code(RequestId(78))).unwrap();
    assert_eq!(v["@type"], "resendPhoneNumberCode");
    assert_eq!(v["@extra"], "78");
    assert_eq!(v["reason"]["@type"], "resendCodeReasonUserRequest");

    let v: serde_json::Value =
        serde_json::from_str(&check_phone_number_code(RequestId(79), "12345")).unwrap();
    assert_eq!(v["@type"], "checkPhoneNumberCode");
    assert_eq!(v["@extra"], "79");
    assert_eq!(v["code"], "12345");
}

#[test]
fn a7_account_lifecycle_request_shapes_match_1_8_67() {
    // Slice A7: `deleteAccount reason:string password:string = Ok;`
    // (line 15675), `getAccountTtl = AccountTtl;` (line 15669),
    // `setAccountTtl ttl:accountTtl = Ok;` (line 15666).
    let v: serde_json::Value =
        serde_json::from_str(&delete_account(RequestId(74), "switching", "s3cret")).unwrap();
    assert_eq!(v["@type"], "deleteAccount");
    assert_eq!(v["@extra"], "74");
    assert_eq!(v["reason"], "switching");
    assert_eq!(v["password"], "s3cret");

    let v: serde_json::Value = serde_json::from_str(&get_account_ttl(RequestId(75))).unwrap();
    assert_eq!(v["@type"], "getAccountTtl");
    assert_eq!(v["@extra"], "75");

    let v: serde_json::Value = serde_json::from_str(&set_account_ttl(RequestId(76), 90)).unwrap();
    assert_eq!(v["@type"], "setAccountTtl");
    assert_eq!(v["@extra"], "76");
    assert_eq!(v["ttl"]["@type"], "accountTtl");
    assert_eq!(v["ttl"]["days"], 90);
}

#[test]
fn a12_set_profile_accent_color_request_shape_matches_1_8_67() {
    // Slice A12: `setProfileAccentColor profile_accent_color_id:int32
    // profile_background_custom_emoji_id:int64 = Ok;` (line 14820).
    let v: serde_json::Value =
        serde_json::from_str(&set_profile_accent_color(RequestId(77), 3, 0)).unwrap();
    assert_eq!(v["@type"], "setProfileAccentColor");
    assert_eq!(v["@extra"], "77");
    assert_eq!(v["profile_accent_color_id"], 3);
    assert_eq!(v["profile_background_custom_emoji_id"], 0);

    let v: serde_json::Value =
        serde_json::from_str(&set_profile_accent_color(RequestId(78), -1, 536_870_912)).unwrap();
    assert_eq!(v["profile_accent_color_id"], -1);
    assert_eq!(v["profile_background_custom_emoji_id"], 536_870_912);
}

#[test]
fn a4_session_toggle_and_websites_request_shapes_match_1_8_67() {
    // Slice A4: `toggleSessionCanAcceptCalls session_id:int64
    // can_accept_calls:Bool = Ok;` (line 15114),
    // `getConnectedWebsites = ConnectedWebsites;` (line 15124),
    // `disconnectWebsite website_id:int64 = Ok;` (line 15127),
    // `disconnectAllWebsites = Ok;` (line 15130).
    let v: serde_json::Value = serde_json::from_str(&toggle_session_can_accept_calls(
        RequestId(81),
        123456789,
        true,
    ))
    .unwrap();
    assert_eq!(v["@type"], "toggleSessionCanAcceptCalls");
    assert_eq!(v["@extra"], "81");
    assert_eq!(v["session_id"], 123456789);
    assert_eq!(v["can_accept_calls"], true);

    let v: serde_json::Value =
        serde_json::from_str(&get_connected_websites(RequestId(82))).unwrap();
    assert_eq!(v["@type"], "getConnectedWebsites");
    assert_eq!(v["@extra"], "82");

    let v: serde_json::Value =
        serde_json::from_str(&disconnect_website(RequestId(83), 987654321)).unwrap();
    assert_eq!(v["@type"], "disconnectWebsite");
    assert_eq!(v["@extra"], "83");
    assert_eq!(v["website_id"], 987654321);

    let v: serde_json::Value =
        serde_json::from_str(&disconnect_all_websites(RequestId(84))).unwrap();
    assert_eq!(v["@type"], "disconnectAllWebsites");
    assert_eq!(v["@extra"], "84");
}

#[test]
fn toggle_session_can_accept_secret_chats_shape_matches_1_8_67() {
    // Phase S1: `toggleSessionCanAcceptSecretChats session_id:int64
    // can_accept_secret_chats:Bool = Ok` (schema 1.8.67, line 15117);
    // int64 serializes as a JSON number, like `terminateSession`.
    let json = toggle_session_can_accept_secret_chats(RequestId(31), 123456789, true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "toggleSessionCanAcceptSecretChats");
    assert_eq!(v["@extra"], "31");
    assert_eq!(v["session_id"], 123456789);
    assert_eq!(v["can_accept_secret_chats"], true);

    let off = toggle_session_can_accept_secret_chats(RequestId(32), 123456789, false);
    let v: serde_json::Value = serde_json::from_str(&off).unwrap();
    assert_eq!(v["can_accept_secret_chats"], false);
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

#[test]
fn resend_authentication_code_shape() {
    let json = resend_authentication_code(RequestId(6));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "resendAuthenticationCode");
    assert_eq!(v["@extra"], "6");
    assert_eq!(v["reason"]["@type"], "resendCodeReasonUserRequest");
}

#[test]
fn request_qr_code_authentication_shape() {
    let json = request_qr_code_authentication(RequestId(7));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "requestQrCodeAuthentication");
    assert_eq!(v["@extra"], "7");
    assert_eq!(v["other_user_ids"], serde_json::json!([]));
}

#[test]
fn a5_set_name_shape_matches_1_8_67() {
    // A5: `setName first_name:string last_name:string = Ok;`
    // (schema 1.8.67, line 14823).
    let json = set_name(RequestId(1), "Ada", "Lovelace");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setName");
    assert_eq!(v["first_name"], "Ada");
    assert_eq!(v["last_name"], "Lovelace");
}

#[test]
fn a5_set_bio_shape_matches_1_8_67() {
    // A5: `setBio bio:string = Ok;` (schema 1.8.67, line 14826).
    let json = set_bio(RequestId(2), "hello");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setBio");
    assert_eq!(v["bio"], "hello");
}

#[test]
fn a5_set_username_shape_matches_1_8_67() {
    // A5: `setUsername username:string = Ok;` (schema 1.8.67,
    // line 14830).
    let json = set_username(RequestId(3), "adalove");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setUsername");
    assert_eq!(v["username"], "adalove");
}

#[test]
fn a5_check_chat_username_shape_matches_1_8_67() {
    // A5: `checkChatUsername chat_id:int53 username:string =
    // CheckChatUsernameResult;` (schema 1.8.67, line 11677).
    let json = check_chat_username(RequestId(4), ChatId(777), "adalove");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "checkChatUsername");
    assert_eq!(v["chat_id"], 777);
    assert_eq!(v["username"], "adalove");
}

#[test]
fn a5_reorder_active_usernames_shape_matches_1_8_67() {
    // A5: `reorderActiveUsernames usernames:vector<string> = Ok;`
    // (schema 1.8.67, line 14838).
    let json = reorder_active_usernames(RequestId(5), &["b".to_string(), "a".to_string()]);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "reorderActiveUsernames");
    assert_eq!(v["usernames"], serde_json::json!(["b", "a"]));
}

#[test]
fn a5_toggle_username_is_active_shape_matches_1_8_67() {
    // A5: `toggleUsernameIsActive username:string is_active:Bool =
    // Ok;` (schema 1.8.67, line 14835).
    let json = toggle_username_is_active(RequestId(6), "adalove", false);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "toggleUsernameIsActive");
    assert_eq!(v["username"], "adalove");
    assert_eq!(v["is_active"], false);
}

#[test]
fn a5_set_profile_photo_shape_matches_1_8_67() {
    // A5: `setProfilePhoto photo:InputChatPhoto is_public:Bool = Ok;`
    // (schema 1.8.67, line 14803) with
    // `inputChatPhotoStatic photo:InputFile` (line 1042).
    let json = set_profile_photo(RequestId(7), "/tmp/me.png", true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setProfilePhoto");
    assert_eq!(v["photo"]["@type"], "inputChatPhotoStatic");
    assert_eq!(v["photo"]["photo"]["@type"], "inputFileLocal");
    assert_eq!(v["photo"]["photo"]["path"], "/tmp/me.png");
    assert_eq!(v["is_public"], true);
}

#[test]
fn a5_delete_profile_photo_shape_matches_1_8_67() {
    // A5: `deleteProfilePhoto profile_photo_id:int64 = Ok;`
    // (schema 1.8.67, line 14806).
    let json = delete_profile_photo(RequestId(8), 12345);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "deleteProfilePhoto");
    assert_eq!(v["profile_photo_id"], 12345);
}

/// Batch 6: the recovery / reset / login-email request shapes against
/// schema 1.8.67 (`checkRecoveryEmailAddressCode` :11461,
/// `requestPasswordRecovery` :11470, `recoverPassword` :11479,
/// `resetPassword` :11482, `cancelPasswordReset` :11485,
/// `setLoginEmailAddress` :11443, `resendLoginEmailAddressCode` :11446,
/// `checkLoginEmailAddressCode` :11449).
#[test]
fn b6_recovery_reset_and_login_email_shapes_match_1_8_67() {
    let parse = |json: String| serde_json::from_str::<serde_json::Value>(&json).unwrap();
    let v = parse(check_recovery_email_address_code(RequestId(1), "123456"));
    assert_eq!(v["@type"], "checkRecoveryEmailAddressCode");
    assert_eq!(v["code"], "123456");
    let v = parse(request_password_recovery(RequestId(2)));
    assert_eq!(v["@type"], "requestPasswordRecovery");
    let v = parse(recover_password(RequestId(3), "654321", "new", "hint"));
    assert_eq!(v["@type"], "recoverPassword");
    assert_eq!(v["recovery_code"], "654321");
    assert_eq!(v["new_password"], "new");
    assert_eq!(v["new_hint"], "hint");
    assert_eq!(parse(reset_password(RequestId(4)))["@type"], "resetPassword");
    assert_eq!(
        parse(cancel_password_reset(RequestId(5)))["@type"],
        "cancelPasswordReset"
    );
    let v = parse(set_login_email_address(RequestId(6), "me@example.com"));
    assert_eq!(v["@type"], "setLoginEmailAddress");
    assert_eq!(v["new_login_email_address"], "me@example.com");
    assert_eq!(
        parse(resend_login_email_address_code(RequestId(7)))["@type"],
        "resendLoginEmailAddressCode"
    );
    let v = parse(check_login_email_address_code(RequestId(8), "12345"));
    assert_eq!(v["@type"], "checkLoginEmailAddressCode");
    assert_eq!(v["code"]["@type"], "emailAddressAuthenticationCode");
    assert_eq!(v["code"]["code"], "12345");
}

/// Batch 4: `confirmSession` (:15111) and `acceptTermsOfService` (:16143).
#[test]
fn b4_confirm_session_and_accept_terms_shapes_match_1_8_67() {
    let parse = |json: String| serde_json::from_str::<serde_json::Value>(&json).unwrap();
    let v = parse(confirm_session(RequestId(1), 77));
    assert_eq!(v["@type"], "confirmSession");
    assert_eq!(v["session_id"], 77);
    let v = parse(accept_terms_of_service(RequestId(2), "tos-1"));
    assert_eq!(v["@type"], "acceptTermsOfService");
    assert_eq!(v["terms_of_service_id"], "tos-1");
}
