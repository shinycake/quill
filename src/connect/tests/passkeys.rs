//! Connect-driver tests: login passkeys (recorded JSON only; nothing is
//! removed live).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;
type Parts = (
    ConnectDriver<Recorder>,
    Recorder,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

fn parts() -> Parts {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    (driver, recorder, sink, seq)
}

fn feed(p: &mut Parts, json: Value) {
    let (driver, _, sink, seq) = p;
    driver
        .ingest(copy_and_parse(&json.to_string(), seq, sink).unwrap())
        .unwrap();
}

fn count(recorder: &RecordingSender, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| serde_json::from_str::<Value>(json).is_ok_and(|v| v["@type"] == type_name))
        .count()
}

fn answer(p: &mut Parts, request: &str, mut body: Value) {
    let sent = sent_request(&p.1, request);
    body["@extra"] = sent["@extra"].clone();
    feed(p, body);
}

fn two_passkeys() -> Value {
    json!({"@type": "passkeys", "passkeys": [
        {"@type": "passkey", "id": "k1", "name": "iCloud Keychain",
         "addition_date": 1_790_000_000, "last_usage_date": 1_790_500_000,
         "software_icon_custom_emoji_id": "5368324170671202286"},
        {"@type": "passkey", "id": "k2", "name": "YubiKey 5C",
         "addition_date": 1_790_100_000, "last_usage_date": 0,
         "software_icon_custom_emoji_id": "0"}
    ]})
}

fn listed(p: &mut Parts) {
    p.0.request_passkeys().unwrap();
    answer(p, "getLoginPasskeys", two_passkeys());
}

#[test]
fn lists_the_passkeys_once_while_fresh() {
    let mut p = parts();
    assert!(p.0.request_passkeys().unwrap().is_some());
    // The application config comes along for `settings_display_passkeys`.
    assert_eq!(count(&p.1, "getApplicationConfig"), 1);
    // A second look while the first is on its way sends nothing.
    assert!(p.0.request_passkeys().unwrap().is_none());
    answer(&mut p, "getLoginPasskeys", two_passkeys());
    let state = &p.0.session.settings.passkeys;
    assert!(state.known);
    assert_eq!(state.list.len(), 2);
    assert_eq!(state.row_label().text(), "2");
    // Fresh for 5 seconds: no new request.
    assert!(p.0.request_passkeys().unwrap().is_none());
    assert_eq!(count(&p.1, "getLoginPasskeys"), 1);
    // The software icon is asked for once.
    let icons = sent_request(&p.1, "getCustomEmojiStickers");
    assert_eq!(icons["custom_emoji_ids"], json!(["5368324170671202286"]));
}

#[test]
fn the_app_config_switch_reaches_the_row() {
    let mut p = parts();
    listed(&mut p);
    assert!(!p.0.session.settings.passkeys.row_shown());
    answer(
        &mut p,
        "getApplicationConfig",
        json!({"@type": "jsonValueObject", "members": [
            {"@type": "jsonObjectMember", "key": "settings_display_passkeys",
             "value": {"@type": "jsonValueBoolean", "value": true}}]}),
    );
    assert!(p.0.session.settings.passkeys.row_shown());
}

#[test]
fn a_removal_waits_for_the_server_then_refetches() {
    let mut p = parts();
    listed(&mut p);
    p.0.remove_passkey("k2").unwrap();
    let sent = sent_request(&p.1, "removeLoginPasskey");
    assert_eq!(sent["passkey_id"], "k2");
    // Nothing leaves the list before the answer, and one removal at a time.
    assert_eq!(p.0.session.settings.passkeys.list.len(), 2);
    assert!(p.0.remove_passkey("k1").is_err());
    answer(&mut p, "removeLoginPasskey", json!({"@type": "ok"}));
    let state = &p.0.session.settings.passkeys;
    assert!(state.find("k2").is_none());
    assert!(state.removing.is_none());
    assert!(!state.known);
    // The same ingest asked for the list again.
    assert_eq!(count(&p.1, "getLoginPasskeys"), 2);
    answer(
        &mut p,
        "getLoginPasskeys",
        json!({"@type": "passkeys", "passkeys": [
            {"@type": "passkey", "id": "k1", "name": "iCloud Keychain",
             "addition_date": 1_790_000_000}]}),
    );
    let state = &p.0.session.settings.passkeys;
    assert!(state.known);
    assert_eq!(state.row_label().text(), "iCloud Keychain");
}

#[test]
fn unknown_passkeys_are_not_sent() {
    let mut p = parts();
    listed(&mut p);
    assert!(p.0.remove_passkey("nope").is_err());
    assert_eq!(count(&p.1, "removeLoginPasskey"), 0);
}

#[test]
fn failures_are_shown_and_keep_the_list() {
    let mut p = parts();
    listed(&mut p);
    p.0.remove_passkey("k1").unwrap();
    answer(
        &mut p,
        "removeLoginPasskey",
        json!({"@type": "error", "code": 400, "message": "PASSKEY_INVALID"}),
    );
    let state = &p.0.session.settings.passkeys;
    assert_eq!(state.list.len(), 2);
    assert!(state.removing.is_none());
    assert_eq!(
        state.error.as_deref(),
        Some("Could not delete the passkey: Telegram refused the request")
    );
    // No refetch after a failed removal.
    assert_eq!(count(&p.1, "getLoginPasskeys"), 1);

    let mut p = parts();
    p.0.request_passkeys().unwrap();
    answer(
        &mut p,
        "getLoginPasskeys",
        json!({"@type": "error", "code": 500, "message": "INTERNAL"}),
    );
    let state = &p.0.session.settings.passkeys;
    assert!(!state.known);
    assert_eq!(state.row_label().text(), "Loading...");
    assert_eq!(
        state.error.as_deref(),
        Some("Could not load the passkeys (error 500)")
    );
    // The failure does not loop: no retry on the next ingest.
    feed(
        &mut p,
        json!({"@type": "updateOption", "name": "x", "value": {"@type": "optionValueEmpty"}}),
    );
    assert_eq!(count(&p.1, "getLoginPasskeys"), 1);
}
