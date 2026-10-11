//! B13 driver tests: new-chat privacy, gift settings, the inactive-session
//! TTL, the 18+ option, network usage and the password check. TDLib JSON is
//! recorded from the 1.8.67 schema shapes.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::platform::MemorySecretStore;
use crate::privacy::{GiftSettings, NewChatPrivacyState};
use crate::state::PasswordCheck;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Fixture = (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

fn fixture() -> Fixture {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    (dir, driver, recorder, sink, seq)
}

fn cleanup(f: Fixture) {
    let (dir, driver, ..) = f;
    drop(driver);
    std::fs::remove_dir_all(dir).unwrap();
}

fn ingest(f: &mut Fixture, json: &str) {
    let owned = copy_and_parse(json, &f.4, &f.3).unwrap();
    f.1.ingest(owned).unwrap();
}

fn last_request(f: &Fixture) -> Value {
    serde_json::from_str(f.2.snapshot().last().unwrap()).unwrap()
}

fn extra_of(request: &Value) -> String {
    request["@extra"].as_str().unwrap().to_string()
}

#[test]
fn new_chat_privacy_round_trip_keeps_the_paid_price() {
    let mut f = fixture();
    // Writing before the answer would reset the paid-message price.
    assert_invalid(f.1.set_new_chat_privacy(false));
    f.1.fetch_new_chat_privacy().unwrap();
    assert_eq!(
        f.1.session.settings.privacy_data.new_chat,
        Some(NewChatPrivacyState::Loading)
    );
    let request = last_request(&f);
    assert_eq!(request["@type"], "getNewChatPrivacySettings");
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"newChatPrivacySettings","allow_new_chats_from_unknown_users":true,"incoming_paid_message_star_count":"25","@extra":"{}"}}"#,
            extra_of(&request)
        ),
    );
    assert!(matches!(
        f.1.session.settings.privacy_data.new_chat,
        Some(NewChatPrivacyState::Ready(s))
            if s.allow_from_unknown && s.incoming_paid_message_star_count == 25
    ));
    f.1.set_new_chat_privacy(false).unwrap();
    let set = last_request(&f);
    assert_eq!(set["@type"], "setNewChatPrivacySettings");
    assert_eq!(set["settings"]["allow_new_chats_from_unknown_users"], false);
    assert_eq!(set["settings"]["incoming_paid_message_star_count"], 25);
    assert!(matches!(
        f.1.session.settings.privacy_data.new_chat,
        Some(NewChatPrivacyState::Ready(s)) if !s.allow_from_unknown
    ));
    // A refusal rolls the optimistic choice back and says so.
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"error","code":400,"message":"PREMIUM_ACCOUNT_REQUIRED","@extra":"{}"}}"#,
            extra_of(&set)
        ),
    );
    assert!(matches!(
        f.1.session.settings.privacy_data.new_chat,
        Some(NewChatPrivacyState::Ready(s)) if s.allow_from_unknown
    ));
    assert!(f.1.session.settings.privacy_data.error.is_some());
    cleanup(f);
}

#[test]
fn gift_settings_are_sent_and_applied_to_the_own_full_info() {
    let mut f = fixture();
    ingest(
        &mut f,
        r#"{"@type":"updateOption","name":"my_id","value":{"@type":"optionValueInteger","value":"60"}}"#,
    );
    assert!(f.1.session.my_gift_settings().is_none());
    ingest(
        &mut f,
        r#"{"@type":"updateUserFullInfo","user_id":60,"user_full_info":{"@type":"userFullInfo","gift_settings":{"@type":"giftSettings","show_gift_button":true,"accepted_gift_types":{"@type":"acceptedGiftTypes","unlimited_gifts":true,"limited_gifts":false,"upgraded_gifts":true,"gifts_from_channels":true,"premium_subscription":false}}}}"#,
    );
    let current = f.1.session.my_gift_settings().expect("parsed");
    assert!(current.show_gift_button && !current.limited_gifts && !current.premium_subscription);
    let next = GiftSettings {
        limited_gifts: true,
        ..current
    };
    f.1.set_gift_settings(next).unwrap();
    let request = last_request(&f);
    assert_eq!(request["@type"], "setGiftSettings");
    assert_eq!(request["settings"]["show_gift_button"], true);
    let accepted = &request["settings"]["accepted_gift_types"];
    assert_eq!(accepted["limited_gifts"], true);
    assert_eq!(accepted["premium_subscription"], false);
    assert_eq!(f.1.session.my_gift_settings(), Some(next));
    cleanup(f);
}

#[test]
fn inactive_session_ttl_comes_from_sessions_and_is_set_optimistically() {
    let mut f = fixture();
    assert_invalid(f.1.set_inactive_session_ttl(0));
    assert_invalid(f.1.set_inactive_session_ttl(367));
    f.1.maybe_fetch_active_sessions().unwrap();
    let fetch = last_request(&f);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"sessions","sessions":[],"inactive_session_ttl_days":180,"@extra":"{}"}}"#,
            extra_of(&fetch)
        ),
    );
    assert_eq!(
        f.1.session.settings.privacy_data.inactive_session_ttl_days,
        Some(180)
    );
    f.1.set_inactive_session_ttl(30).unwrap();
    let set = last_request(&f);
    assert_eq!(set["@type"], "setInactiveSessionTtl");
    assert_eq!(set["inactive_session_ttl_days"], 30);
    assert_eq!(
        f.1.session.settings.privacy_data.inactive_session_ttl_days,
        Some(30)
    );
    // A refusal drops the optimistic value and reports it.
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"error","code":400,"message":"TTL_INVALID","@extra":"{}"}}"#,
            extra_of(&set)
        ),
    );
    assert_eq!(
        f.1.session.settings.privacy_data.inactive_session_ttl_days,
        None
    );
    assert!(f.1.session.settings.privacy_data.error.is_some());
    cleanup(f);
}

#[test]
fn sensitive_content_switch_follows_the_options_only() {
    let mut f = fixture();
    // The account may not turn it on: nothing is sent.
    assert_invalid(f.1.set_ignore_sensitive_content(true));
    ingest(
        &mut f,
        r#"{"@type":"updateOption","name":"can_ignore_sensitive_content_restrictions","value":{"@type":"optionValueBoolean","value":true}}"#,
    );
    ingest(
        &mut f,
        r#"{"@type":"updateOption","name":"ignore_sensitive_content_restrictions","value":{"@type":"optionValueBoolean","value":false}}"#,
    );
    f.1.set_ignore_sensitive_content(true).unwrap();
    let request = last_request(&f);
    assert_eq!(request["@type"], "setOption");
    assert_eq!(request["name"], "ignore_sensitive_content_restrictions");
    assert_eq!(request["value"]["value"], true);
    // Not applied until TDLib says so.
    assert_eq!(
        f.1.session.settings.privacy_data.ignore_sensitive,
        Some(false)
    );
    ingest(
        &mut f,
        r#"{"@type":"updateOption","name":"ignore_sensitive_content_restrictions","value":{"@type":"optionValueBoolean","value":true}}"#,
    );
    assert_eq!(
        f.1.session.settings.privacy_data.ignore_sensitive,
        Some(true)
    );
    cleanup(f);
}

#[test]
fn network_usage_fetch_and_reset() {
    let mut f = fixture();
    f.1.fetch_network_statistics().unwrap();
    let request = last_request(&f);
    assert_eq!(request["@type"], "getNetworkStatistics");
    assert_eq!(request["only_current"], false);
    assert!(f.1.session.settings.privacy_data.network_loading);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"networkStatistics","since_date":1700000000,"entries":[{{"@type":"networkStatisticsEntryFile","file_type":{{"@type":"fileTypePhoto"}},"network_type":{{"@type":"networkTypeWiFi"}},"sent_bytes":5,"received_bytes":95}}],"@extra":"{}"}}"#,
            extra_of(&request)
        ),
    );
    let usage =
        f.1.session
            .settings
            .privacy_data
            .network_usage
            .clone()
            .unwrap();
    assert_eq!(usage.grand_total().total(), 100);
    assert!(!f.1.session.settings.privacy_data.network_loading);
    // Reset sends the request and asks for the fresh start date.
    f.1.reset_network_statistics().unwrap();
    let sent: Vec<String> =
        f.2.snapshot()
            .iter()
            .map(|j| serde_json::from_str::<Value>(j).unwrap()["@type"].to_string())
            .collect();
    assert!(sent.contains(&"\"resetNetworkStatistics\"".to_string()));
    assert_eq!(sent.last().unwrap(), "\"getNetworkStatistics\"");
    assert!(f.1.session.settings.privacy_data.network_usage.is_none());
    cleanup(f);
}

#[test]
fn password_check_distinguishes_wrong_from_right_and_dismisses() {
    let mut f = fixture();
    assert_invalid(f.1.check_remembered_password(""));
    ingest(
        &mut f,
        r#"{"@type":"updateSuggestedActions","added_actions":[{"@type":"suggestedActionCheckPassword"}],"removed_actions":[]}"#,
    );
    assert!(f.1.session.settings.privacy_data.check_password_suggested);

    f.1.check_remembered_password("wrong").unwrap();
    let request = last_request(&f);
    assert_eq!(request["@type"], "getRecoveryEmailAddress");
    assert_eq!(request["password"], "wrong");
    assert_eq!(
        f.1.session.settings.privacy_data.password_check,
        PasswordCheck::Checking
    );
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"error","code":400,"message":"PASSWORD_HASH_INVALID","@extra":"{}"}}"#,
            extra_of(&request)
        ),
    );
    assert_eq!(
        f.1.session.settings.privacy_data.password_check,
        PasswordCheck::Wrong
    );

    f.1.check_remembered_password("right").unwrap();
    let request = last_request(&f);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"recoveryEmailAddress","recovery_email_address":"a@b.c","@extra":"{}"}}"#,
            extra_of(&request)
        ),
    );
    assert_eq!(
        f.1.session.settings.privacy_data.password_check,
        PasswordCheck::Remembered
    );
    // The card stays for the finish step; Done hides the suggestion.
    assert!(f.1.session.settings.privacy_data.check_password_suggested);
    f.1.hide_check_password_suggestion().unwrap();
    let hide = last_request(&f);
    assert_eq!(hide["@type"], "hideSuggestedAction");
    assert_eq!(hide["action"]["@type"], "suggestedActionCheckPassword");
    assert!(!f.1.session.settings.privacy_data.check_password_suggested);
    // The server may also withdraw it from another device.
    ingest(
        &mut f,
        r#"{"@type":"updateSuggestedActions","added_actions":[{"@type":"suggestedActionCheckPassword"}],"removed_actions":[]}"#,
    );
    ingest(
        &mut f,
        r#"{"@type":"updateSuggestedActions","added_actions":[],"removed_actions":[{"@type":"suggestedActionCheckPassword"}]}"#,
    );
    assert!(!f.1.session.settings.privacy_data.check_password_suggested);
    cleanup(f);
}

#[test]
fn suggestions_follow_updates_and_hide_with_rollback() {
    use crate::chatlist_suggestions::{ACTION_PHOTO, Suggestion};
    let mut f = fixture();
    ingest(
        &mut f,
        r#"{"@type":"updateSuggestedActions","added_actions":[{"@type":"suggestedActionSetProfilePhoto"},{"@type":"suggestedActionSetBirthdate"}],"removed_actions":[]}"#,
    );
    ingest(
        &mut f,
        r#"{"@type":"updateContactCloseBirthdays","close_birthday_users":[{"@type":"closeBirthdayUser","user_id":9,"birthdate":{"@type":"birthdate","day":14,"month":3,"year":1990}}]}"#,
    );
    let facts = &f.1.session.chat_list.suggestions;
    assert!(facts.actions.contains(ACTION_PHOTO));
    assert_eq!(facts.close_birthdays.len(), 1);
    assert_eq!(facts.close_birthdays[0].year, Some(1990));

    // Hiding goes out as `hideSuggestedAction` and drops the action now.
    f.1.hide_suggestion(&Suggestion::SetProfilePhoto).unwrap();
    let request = last_request(&f);
    assert_eq!(request["@type"], "hideSuggestedAction");
    assert_eq!(request["action"]["@type"], "suggestedActionSetProfilePhoto");
    assert!(
        !f.1.session
            .chat_list
            .suggestions
            .actions
            .contains(ACTION_PHOTO)
    );
    // A refusal puts it back and says so.
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"NO"}}"#,
            extra_of(&request)
        ),
    );
    assert!(
        f.1.session
            .chat_list
            .suggestions
            .actions
            .contains(ACTION_PHOTO)
    );
    assert_eq!(
        f.1.session.chats_state.chat_action_error.as_deref(),
        Some("could not hide the suggestion (error 400)")
    );

    // Birthdays have their own request.
    f.1.hide_suggestion(&Suggestion::Birthdays(vec![9]))
        .unwrap();
    assert_eq!(last_request(&f)["@type"], "hideContactCloseBirthdays");
    assert!(f.1.session.chat_list.suggestions.birthdays_hidden);
    // A fresh list from TDLib shows them again.
    ingest(
        &mut f,
        r#"{"@type":"updateContactCloseBirthdays","close_birthday_users":[]}"#,
    );
    assert!(!f.1.session.chat_list.suggestions.birthdays_hidden);
    cleanup(f);
}
