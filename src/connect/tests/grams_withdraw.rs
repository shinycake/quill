//! Collect on the Grams screen: the password box, then
//! `getGramWithdrawalUrl` (recorded JSON only; nothing live, nothing is
//! withdrawn). No withdrawal request goes out before Submit with a typed
//! password, and the password is never kept.
use super::super::*;
use super::support::{prepared_tmp, ready_driver, sent_request};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::gift_manage::ExportError;
use crate::platform::MemorySecretStore;
use crate::premium_hub::ton::GramWithdraw;
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

const PASSWORD: &str = "CANARY-gram-pw-9021";
const URL: &str = "https://fragment.com/ads/withdraw?token=abc";

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

/// The Grams screen with `available` nano-Grams to withdraw.
fn open_screen(p: &mut Parts, available: &str) {
    p.0.open_grams(false).unwrap();
    let extra = sent_request(&p.1, "getGramRevenueStatistics")["@extra"].clone();
    feed(
        p,
        json!({"@type": "gramRevenueStatistics", "@extra": extra,
               "revenue_by_day_graph": {"@type": "statisticalGraphError", "error_message": ""},
               "status": {"@type": "gramRevenueStatus", "total_amount": available,
                          "balance_amount": available, "available_amount": available,
                          "withdrawal_enabled": true},
               "usd_rate": 0.00000035}),
    );
}

fn password_state(p: &mut Parts, has_password: bool) {
    let extra = sent_request(&p.1, "getPasswordState")["@extra"].clone();
    feed(
        p,
        json!({"@type": "passwordState", "@extra": extra,
            "has_password": has_password, "password_hint": "", "has_recovery_email_address": false,
            "has_passport_data": false, "recovery_email_address_code_info": null,
            "login_email_address_pattern": "", "pending_reset_date": 0}),
    );
}

fn withdraw(p: &Parts) -> Option<GramWithdraw> {
    p.0.session.payments.hub.grams.withdraw.clone()
}

fn assert_no_password_kept(p: &Parts) {
    let state = format!("{:?}", p.0.session.payments.hub.grams);
    assert!(!state.contains(PASSWORD));
}

#[test]
fn collect_opens_the_password_box_and_sends_no_withdrawal() {
    let mut p = parts();
    // Not before the screen is open with an available amount.
    assert!(p.0.start_gram_withdrawal().is_err());
    open_screen(&mut p, "0");
    assert!(p.0.start_gram_withdrawal().is_err());
    assert!(withdraw(&p).is_none());

    let mut p = parts();
    open_screen(&mut p, "15000000000");
    p.0.start_gram_withdrawal().unwrap();
    // tdesktop `cloudPassword().reload()`: only the password state goes
    // out; the withdrawal waits for the password.
    assert_eq!(count(&p.1, "getPasswordState"), 1);
    assert_eq!(count(&p.1, "getGramWithdrawalUrl"), 0);
    assert_eq!(withdraw(&p), Some(GramWithdraw::idle(None)));
    // A second click changes nothing.
    p.0.start_gram_withdrawal().unwrap();
    assert_eq!(count(&p.1, "getPasswordState"), 1);
    // The state has a password: the box stays.
    password_state(&mut p, true);
    assert_eq!(withdraw(&p), Some(GramWithdraw::idle(None)));
    assert_eq!(count(&p.1, "getGramWithdrawalUrl"), 0);
}

#[test]
fn no_password_set_opens_the_security_check() {
    // The reload answers "no password": the idle box becomes the check.
    let mut p = parts();
    open_screen(&mut p, "15000000000");
    p.0.start_gram_withdrawal().unwrap();
    password_state(&mut p, false);
    assert_eq!(
        withdraw(&p),
        Some(GramWithdraw::SecurityCheck { later: false })
    );
    assert!(p.0.submit_gram_withdrawal(PASSWORD).is_err());

    // Already known: the check opens straight away.
    p.0.cancel_gram_withdrawal();
    p.0.start_gram_withdrawal().unwrap();
    assert_eq!(
        withdraw(&p),
        Some(GramWithdraw::SecurityCheck { later: false })
    );
    assert_eq!(count(&p.1, "getGramWithdrawalUrl"), 0);
}

#[test]
fn the_url_is_asked_only_from_the_password_box() {
    let mut p = parts();
    open_screen(&mut p, "15000000000");
    // Not before Collect, not with an empty password.
    assert!(p.0.submit_gram_withdrawal(PASSWORD).is_err());
    p.0.start_gram_withdrawal().unwrap();
    assert!(p.0.submit_gram_withdrawal("").is_err());
    assert_eq!(count(&p.1, "getGramWithdrawalUrl"), 0);

    let extra = p.0.submit_gram_withdrawal(PASSWORD).unwrap();
    let sent = sent_request(&p.1, "getGramWithdrawalUrl");
    assert_eq!(
        sent,
        json!({"@type": "getGramWithdrawalUrl", "@extra": extra.as_extra(), "password": PASSWORD})
    );
    assert!(withdraw(&p).is_some_and(|w| w.sending()));
    // A second Submit while the first is out sends nothing.
    assert!(p.0.submit_gram_withdrawal(PASSWORD).is_err());
    assert_eq!(count(&p.1, "getGramWithdrawalUrl"), 1);
    assert_no_password_kept(&p);

    feed(
        &mut p,
        json!({"@type": "httpUrl", "@extra": extra.as_extra(), "url": URL}),
    );
    let grams = &p.0.session.payments.hub.grams;
    assert!(grams.withdraw.is_none());
    assert_eq!(grams.withdraw_url.as_deref(), Some(URL));
}

#[test]
fn a_url_after_cancel_is_dropped() {
    let mut p = parts();
    open_screen(&mut p, "15000000000");
    p.0.start_gram_withdrawal().unwrap();
    let extra = p.0.submit_gram_withdrawal(PASSWORD).unwrap();
    p.0.cancel_gram_withdrawal();
    feed(
        &mut p,
        json!({"@type": "httpUrl", "@extra": extra.as_extra(), "url": URL}),
    );
    assert!(p.0.session.payments.hub.grams.withdraw_url.is_none());
}

fn fail(message: &str, code: i32) -> Parts {
    let mut p = parts();
    open_screen(&mut p, "15000000000");
    p.0.start_gram_withdrawal().unwrap();
    let extra = p.0.submit_gram_withdrawal(PASSWORD).unwrap();
    feed(
        &mut p,
        json!({"@type": "error", "@extra": extra.as_extra(), "code": code, "message": message}),
    );
    assert!(p.0.session.payments.hub.grams.withdraw_url.is_none());
    assert_no_password_kept(&p);
    p
}

#[test]
fn errors_land_in_the_password_box() {
    let field = |error| Some(GramWithdraw::idle(Some(error)));
    assert_eq!(
        withdraw(&fail("PASSWORD_HASH_INVALID", 400)),
        field(ExportError::WrongPassword)
    );
    assert_eq!(
        withdraw(&fail("SRP_PASSWORD_CHANGED", 400)),
        field(ExportError::WrongPassword)
    );
    assert_eq!(
        withdraw(&fail("SRP_ID_INVALID", 400)),
        field(ExportError::ServerError)
    );
    assert_eq!(
        withdraw(&fail("Too Many Requests: retry after 30", 429)),
        field(ExportError::Flood)
    );
    assert_eq!(
        withdraw(&fail("PASSWORD_MISSING", 400)),
        Some(GramWithdraw::SecurityCheck { later: false })
    );
    assert_eq!(
        withdraw(&fail("PASSWORD_TOO_FRESH_86400", 400)),
        Some(GramWithdraw::SecurityCheck { later: true })
    );
    // Anything else stays visible under the field, ready for a retry.
    let other = withdraw(&fail("WITHDRAWAL_UNAVAILABLE", 400));
    assert!(
        matches!(
            &other,
            Some(GramWithdraw::Password {
                sending: false,
                error: Some(ExportError::Other(reason)),
            }) if !reason.is_empty()
        ),
        "{other:?}"
    );
    // tdesktop: AMOUNT_TOO_SMALL closes the box with a toast.
    let small = fail("AMOUNT_TOO_SMALL", 400);
    let grams = &small.0.session.payments.hub.grams;
    assert!(grams.withdraw.is_none() && grams.withdraw_too_small);
}

#[test]
fn closing_the_screen_refuses_the_request() {
    let mut p = parts();
    open_screen(&mut p, "15000000000");
    p.0.start_gram_withdrawal().unwrap();
    p.0.session.payments.hub.grams.open = false;
    assert!(p.0.submit_gram_withdrawal(PASSWORD).is_err());
    assert_eq!(count(&p.1, "getGramWithdrawalUrl"), 0);
}
