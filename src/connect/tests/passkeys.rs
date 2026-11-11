//! Connect-driver tests: login passkeys (recorded JSON only; nothing is
//! removed live).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::AccountKey;
use crate::platform::MemorySecretStore;
use crate::state::Session;
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

const OPTIONS: &str = r#"{"publicKey":{"rp":{"id":"telegram.org","name":"Telegram"},"user":{"id":"AQ","name":"me","displayName":"Me"},"challenge":"3q2-7w","pubKeyCredParams":[{"type":"public-key","alg":-7}],"timeout":60000}}"#;

fn registration_for(options: &crate::webauthn::CreationOptions) -> crate::webauthn::Registration {
    crate::webauthn::Registration {
        credential_id: vec![1, 2],
        attestation_object: vec![0xA3],
        client_data_json: options.client_data_json(),
    }
}

#[test]
fn creating_a_passkey_fetches_options_then_adds_the_result() {
    if !crate::passkeys::webauthn_supported() {
        return;
    }
    let mut p = parts();
    listed(&mut p);
    assert!(p.0.begin_register().unwrap().is_some());
    assert!(p.0.passkey_parameters_pending());
    // A second click while the options are on their way sends nothing.
    assert!(p.0.begin_register().unwrap().is_none());
    answer(
        &mut p,
        "getPasskeyParameters",
        json!({"@type": "text", "text": OPTIONS}),
    );
    let options = p.0.session.settings.passkeys.take_starting().unwrap();
    assert_eq!(options.rp_id, "telegram.org");
    assert_eq!(options.challenge, vec![0xde, 0xad, 0xbe, 0xef]);
    // Within their timeout the same options are used again, no request.
    assert!(p.0.begin_register().unwrap().is_none());
    assert!(p.0.session.settings.passkeys.take_starting().is_some());
    assert_eq!(count(&p.1, "getPasskeyParameters"), 1);

    let registration = registration_for(&options);
    p.0.finish_register(&registration).unwrap();
    let sent = sent_request(&p.1, "addLoginPasskey");
    assert_eq!(sent["attestation_object"], "ow==");
    assert_eq!(
        sent["client_data"],
        r#"{"challenge":"3q2-7w","crossOrigin":false,"origin":"https://telegram.org","type":"webauthn.create"}"#
    );
    // One at a time.
    assert!(p.0.finish_register(&registration).is_err());
    answer(
        &mut p,
        "addLoginPasskey",
        json!({"@type": "passkey", "id": "k3", "name": "YubiKey 5 NFC",
               "addition_date": 1_790_600_000, "last_usage_date": 0,
               "software_icon_custom_emoji_id": "0"}),
    );
    let state = &p.0.session.settings.passkeys;
    assert_eq!(state.list.len(), 3);
    assert_eq!(state.list.last().map(|k| k.id.as_str()), Some("k3"));
    assert!(matches!(state.add, crate::passkeys::PasskeyAdd::Added));
}

#[test]
fn a_failed_registration_forgets_the_options() {
    if !crate::passkeys::webauthn_supported() {
        return;
    }
    let mut p = parts();
    listed(&mut p);
    p.0.begin_register().unwrap();
    answer(
        &mut p,
        "getPasskeyParameters",
        json!({"@type": "text", "text": OPTIONS}),
    );
    let options = p.0.session.settings.passkeys.take_starting().unwrap();
    p.0.finish_register(&registration_for(&options)).unwrap();
    answer(
        &mut p,
        "addLoginPasskey",
        json!({"@type": "error", "code": 400, "message": "PASSKEY_CREDENTIAL_NOT_FOUND"}),
    );
    let state = &p.0.session.settings.passkeys;
    assert!(matches!(state.add, crate::passkeys::PasskeyAdd::Idle));
    assert_eq!(state.list.len(), 2);
    assert!(state.error.is_some());
    // The next try asks for new options.
    p.0.begin_register().unwrap();
    assert_eq!(count(&p.1, "getPasskeyParameters"), 2);
    // Unreadable options: an error, no ceremony.
    answer(
        &mut p,
        "getPasskeyParameters",
        json!({"@type": "text", "text": "{}"}),
    );
    let state = &p.0.session.settings.passkeys;
    assert!(!state.add_busy());
    assert!(state.error.is_some());
}

/// Round 17 C finding 23: options that landed after the lock (and were never
/// started) do not start the next Create's ceremony: the lock drops them, and
/// the next Create asks again.
#[test]
fn the_lock_drops_unstarted_options() {
    if !crate::passkeys::webauthn_supported() {
        return;
    }
    let mut p = parts();
    listed(&mut p);
    assert!(p.0.begin_register().unwrap().is_some());
    answer(
        &mut p,
        "getPasskeyParameters",
        json!({"@type": "text", "text": OPTIONS}),
    );
    p.0.cancel_register();
    assert!(p.0.session.settings.passkeys.take_starting().is_none());
    assert!(p.0.begin_register().unwrap().is_some());
}

/// Cancel before `getPasskeyParameters` answers: the late options start
/// no ceremony for the box that was closed.
#[test]
fn options_after_a_cancel_start_nothing() {
    if !crate::passkeys::webauthn_supported() {
        return;
    }
    let mut p = parts();
    listed(&mut p);
    assert!(p.0.begin_register().unwrap().is_some());
    p.0.cancel_register();
    answer(
        &mut p,
        "getPasskeyParameters",
        json!({"@type": "text", "text": OPTIONS}),
    );
    assert!(p.0.session.settings.passkeys.take_starting().is_none());
    assert!(!p.0.session.settings.passkeys.add_busy());
}

/// An `addLoginPasskey` answer without an id ends the Sending stage.
#[test]
fn an_unreadable_add_answer_ends_the_registration() {
    if !crate::passkeys::webauthn_supported() {
        return;
    }
    let mut p = parts();
    listed(&mut p);
    p.0.begin_register().unwrap();
    answer(
        &mut p,
        "getPasskeyParameters",
        json!({"@type": "text", "text": OPTIONS}),
    );
    let options = p.0.session.settings.passkeys.take_starting().unwrap();
    p.0.finish_register(&registration_for(&options)).unwrap();
    answer(&mut p, "addLoginPasskey", json!({"@type": "passkey"}));
    let state = &p.0.session.settings.passkeys;
    assert!(!state.add_busy());
    assert!(state.error.is_some());
    assert!(p.0.begin_register().is_ok());
}

/// Fails every send while `down` is set.
#[derive(Default)]
struct FlakySender {
    down: std::sync::atomic::AtomicBool,
    sent: std::sync::Mutex<Vec<String>>,
}

impl JsonSender for Arc<FlakySender> {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        use std::sync::atomic::Ordering;
        if self.down.load(Ordering::SeqCst) {
            return Err(ConnectSendError::Native);
        }
        if let Ok(mut sent) = self.sent.lock() {
            sent.push(request.to_string());
        }
        Ok(())
    }
}

fn flaky_driver() -> (ConnectDriver<Arc<FlakySender>>, Arc<FlakySender>) {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let sender = Arc::new(FlakySender::default());
    let session = Session::new(AccountKey::primary(), sink.clone());
    let mut driver = ConnectDriver::new(session, sender.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    let ready = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#;
    driver
        .ingest(copy_and_parse(ready, &seq, &sink).unwrap())
        .unwrap();
    (driver, sender)
}

/// A removal marks the list stale; if the refetch cannot be sent the list
/// stays stale with the throttle open, so the next look asks again.
#[test]
fn a_failed_refetch_send_keeps_the_list_stale() {
    use std::sync::atomic::Ordering;
    let (mut driver, sender) = flaky_driver();
    driver.session.settings.passkeys.finish_remove("gone");
    sender.down.store(true, Ordering::SeqCst);
    assert!(driver.request_passkeys().is_err());
    let state = &driver.session.settings.passkeys;
    assert!(state.stale);
    assert!(state.requested_at.is_none());
    sender.down.store(false, Ordering::SeqCst);
    assert!(driver.request_passkeys().unwrap().is_some());
}

/// The list asked before a removal answers after it: the removed passkey
/// does not come back, and the list is asked for again.
#[test]
fn a_list_in_flight_during_a_removal_is_asked_again() {
    let mut p = parts();
    listed(&mut p);
    p.0.session.settings.passkeys.requested_at = None;
    p.0.request_passkeys().unwrap();
    let old = sent_request(&p.1, "getLoginPasskeys");
    p.0.remove_passkey("k2").unwrap();
    answer(&mut p, "removeLoginPasskey", json!({"@type": "ok"}));
    // The stale list answers (still with k2), then is ignored.
    let mut body = two_passkeys();
    body["@extra"] = old["@extra"].clone();
    feed(&mut p, body);
    assert!(p.0.session.settings.passkeys.find("k2").is_none());
    assert!(p.0.session.settings.passkeys.stale);
    assert_eq!(count(&p.1, "getLoginPasskeys"), 3);
}

/// A passkey added while the ticker was ended by the lock leaves "added"
/// waiting; the next Create must not close its own box with it.
#[test]
fn a_stale_added_does_not_close_the_next_create() {
    if !crate::passkeys::webauthn_supported() {
        return;
    }
    let mut p = parts();
    listed(&mut p);
    p.0.begin_register().unwrap();
    answer(
        &mut p,
        "getPasskeyParameters",
        json!({"@type": "text", "text": OPTIONS}),
    );
    let options = p.0.session.settings.passkeys.take_starting().unwrap();
    p.0.finish_register(&registration_for(&options)).unwrap();
    answer(
        &mut p,
        "addLoginPasskey",
        json!({"@type": "passkey", "id": "k9", "name": "Key"}),
    );
    assert!(matches!(
        p.0.session.settings.passkeys.add,
        crate::passkeys::PasskeyAdd::Added
    ));
    // The next Create starts over; the old "added" is gone.
    p.0.begin_register().unwrap();
    assert!(!p.0.session.settings.passkeys.take_added());
    assert!(p.0.passkey_parameters_pending());
}
