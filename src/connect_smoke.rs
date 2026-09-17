//! Headless live-connect smoke: credentials + tdjson → WaitPhoneNumber (or a
//! clear blocker). No GPUI. Never prints api_hash, phone numbers, codes, or
//! passwords.

use crate::connect::{ConnectBlocker, ConnectDriver, JsonSender, LiveConnect, start_live_connect};
use crate::credentials::{self, TelegramCredentials};
use crate::platform::live_secret_store;
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::AuthorizationState;
use std::path::Path;
use std::time::{Duration, Instant};

/// Default wall-clock budget for `--connect-smoke`.
pub const CONNECT_SMOKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Redacted one-line outcome. Safe to print on a public CI log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmokeOutcome {
    OkWaitPhone,
    OkWaitCode,
    OkWaitPassword,
    OkReady,
    OkOtherDevice,
    Blocked(ConnectBlocker),
    BlockedUnsupported,
    BlockedClosed,
    Timeout,
    SendFailed,
}

impl SmokeOutcome {
    pub fn line(&self) -> String {
        match self {
            SmokeOutcome::OkWaitPhone => "SMOKE_OK wait-phone".into(),
            SmokeOutcome::OkWaitCode => "SMOKE_OK wait-code".into(),
            SmokeOutcome::OkWaitPassword => "SMOKE_OK wait-password".into(),
            SmokeOutcome::OkReady => "SMOKE_OK ready".into(),
            SmokeOutcome::OkOtherDevice => "SMOKE_OK other-device".into(),
            SmokeOutcome::Blocked(blocker) => format!("SMOKE_BLOCKED {}", blocker.slug()),
            SmokeOutcome::BlockedUnsupported => "SMOKE_BLOCKED unsupported".into(),
            SmokeOutcome::BlockedClosed => "SMOKE_BLOCKED closed".into(),
            SmokeOutcome::Timeout => "SMOKE_FAIL timeout".into(),
            SmokeOutcome::SendFailed => "SMOKE_FAIL send".into(),
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            SmokeOutcome::OkWaitPhone
            | SmokeOutcome::OkWaitCode
            | SmokeOutcome::OkWaitPassword
            | SmokeOutcome::OkReady
            | SmokeOutcome::OkOtherDevice => 0,
            _ => 1,
        }
    }
}

/// Terminal classification of an auth state. `None` means keep ingesting.
pub fn classify_smoke_auth(auth: &AuthorizationState) -> Option<SmokeOutcome> {
    match auth {
        AuthorizationState::WaitTdlibParameters => None,
        AuthorizationState::WaitPhoneNumber => Some(SmokeOutcome::OkWaitPhone),
        AuthorizationState::WaitCode { .. } => Some(SmokeOutcome::OkWaitCode),
        AuthorizationState::WaitPassword { .. } => Some(SmokeOutcome::OkWaitPassword),
        AuthorizationState::Ready => Some(SmokeOutcome::OkReady),
        AuthorizationState::WaitOtherDeviceConfirmation => Some(SmokeOutcome::OkOtherDevice),
        AuthorizationState::WaitPremiumPurchase
        | AuthorizationState::WaitEmailAddress
        | AuthorizationState::WaitEmailCode
        | AuthorizationState::WaitRegistration
        | AuthorizationState::Unknown(_) => Some(SmokeOutcome::BlockedUnsupported),
        AuthorizationState::Closing | AuthorizationState::Closed => {
            Some(SmokeOutcome::BlockedClosed)
        }
        AuthorizationState::LoggingOut => None,
    }
}

/// `--connect-smoke` requires owner credentials and an explicit `QUILL_TDJSON_PATH`
/// file. Does not start a client.
pub fn smoke_preflight(
    credentials: Option<&TelegramCredentials>,
    tdjson_path: Option<&str>,
) -> Result<(), ConnectBlocker> {
    if credentials.is_none() {
        return Err(ConnectBlocker::MissingCredentials);
    }
    let Some(path) = tdjson_path.map(str::trim).filter(|s| !s.is_empty()) else {
        return Err(ConnectBlocker::MissingTdjson);
    };
    if !Path::new(path).is_file() {
        return Err(ConnectBlocker::MissingTdjson);
    }
    Ok(())
}

/// Ingest until WaitPhoneNumber (or another terminal auth state / send failure /
/// timeout). Used by the live CLI and by unit tests with a recording sender.
pub fn drive_until_terminal<S: JsonSender>(
    driver: &mut ConnectDriver<S>,
    mut recv: impl FnMut(Duration) -> Option<OwnedEnvelope>,
    timeout: Duration,
) -> SmokeOutcome {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(outcome) = classify_smoke_auth(&driver.session.auth) {
            return outcome;
        }
        let now = Instant::now();
        if now >= deadline {
            return SmokeOutcome::Timeout;
        }
        let slice = deadline
            .saturating_duration_since(now)
            .min(Duration::from_millis(200));
        if let Some(owned) = recv(slice)
            && driver.ingest(owned).is_err()
        {
            return SmokeOutcome::SendFailed;
        }
    }
}

/// Load credentials + `QUILL_TDJSON_PATH`, open a live client, ingest until a
/// terminal smoke outcome.
pub fn run_connect_smoke(timeout: Duration) -> SmokeOutcome {
    let credentials = credentials::load();
    let tdjson_path = std::env::var("QUILL_TDJSON_PATH").ok();
    if let Err(blocker) = smoke_preflight(credentials.as_ref(), tdjson_path.as_deref()) {
        return SmokeOutcome::Blocked(blocker);
    }
    let credentials = credentials.expect("preflight required credentials");
    let store = live_secret_store();
    let sink = std::sync::Arc::new(crate::diagnostics::MemorySink::new());
    let mut live: LiveConnect = match start_live_connect(credentials, store.as_ref(), sink) {
        Ok(live) => live,
        Err(blocker) => return SmokeOutcome::Blocked(blocker),
    };
    drive_until_terminal(
        &mut live.driver,
        |wait| live.bridge.next_timeout(wait),
        timeout,
    )
}

/// Parse args, run smoke, print one redacted line, return the process exit code.
pub fn cli_exit_code() -> i32 {
    let outcome = run_connect_smoke(CONNECT_SMOKE_TIMEOUT);
    println!("{}", outcome.line());
    outcome.exit_code()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connect::{PreparedConnect, RecordingSender};
    use crate::diagnostics::{DiagnosticSink, MemorySink};
    use crate::ids::AccountKey;
    use crate::platform::MemorySecretStore;
    use crate::state::Session;
    use crate::telegram::client::copy_and_parse;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    fn test_credentials() -> TelegramCredentials {
        TelegramCredentials {
            api_id: 99,
            api_hash: "unit-test-hash-not-for-network".into(),
        }
    }

    fn prepared_tmp(store: &MemorySecretStore) -> (std::path::PathBuf, PreparedConnect) {
        let dir = std::env::temp_dir().join(format!(
            "quill-smoke-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let prepared = crate::connect::prepare_connect(
            &dir,
            AccountKey::primary(),
            store,
            &test_credentials(),
        )
        .expect("prepare");
        (dir, prepared)
    }

    #[test]
    fn preflight_blocks_without_credentials() {
        assert_eq!(
            smoke_preflight(None, Some("/tmp/libtdjson.so")),
            Err(ConnectBlocker::MissingCredentials)
        );
    }

    #[test]
    fn preflight_requires_tdjson_path_file() {
        let creds = test_credentials();
        assert_eq!(
            smoke_preflight(Some(&creds), None),
            Err(ConnectBlocker::MissingTdjson)
        );
        assert_eq!(
            smoke_preflight(Some(&creds), Some("")),
            Err(ConnectBlocker::MissingTdjson)
        );
        assert_eq!(
            smoke_preflight(Some(&creds), Some("/no/such/libtdjson.so")),
            Err(ConnectBlocker::MissingTdjson)
        );
        let file = std::env::temp_dir().join(format!("quill-smoke-tdjson-{}", std::process::id()));
        std::fs::write(&file, b"not-a-real-library").unwrap();
        assert_eq!(
            smoke_preflight(Some(&creds), Some(file.to_str().unwrap())),
            Ok(())
        );
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn outcome_lines_are_secret_free() {
        let hash = "unit-test-hash-not-for-network";
        for outcome in [
            SmokeOutcome::OkWaitPhone,
            SmokeOutcome::OkWaitCode,
            SmokeOutcome::OkWaitPassword,
            SmokeOutcome::OkReady,
            SmokeOutcome::Blocked(ConnectBlocker::MissingTdjson),
            SmokeOutcome::Blocked(ConnectBlocker::MissingCredentials),
            SmokeOutcome::BlockedUnsupported,
            SmokeOutcome::Timeout,
            SmokeOutcome::SendFailed,
        ] {
            let line = outcome.line();
            assert!(!line.contains(hash));
            assert!(!line.contains('+'));
            assert!(
                line.starts_with("SMOKE_OK ")
                    || line.starts_with("SMOKE_BLOCKED ")
                    || line.starts_with("SMOKE_FAIL ")
            );
        }
        assert_eq!(SmokeOutcome::OkWaitPhone.line(), "SMOKE_OK wait-phone");
        assert_eq!(
            SmokeOutcome::Blocked(ConnectBlocker::MissingTdjson).line(),
            "SMOKE_BLOCKED missing-tdjson"
        );
        assert_eq!(SmokeOutcome::OkWaitPhone.exit_code(), 0);
        assert_eq!(SmokeOutcome::Timeout.exit_code(), 1);
        assert_eq!(
            SmokeOutcome::Blocked(ConnectBlocker::MissingTdjson).exit_code(),
            1
        );
    }

    #[test]
    fn classify_wait_phone_and_unsupported() {
        assert_eq!(
            classify_smoke_auth(&AuthorizationState::WaitPhoneNumber),
            Some(SmokeOutcome::OkWaitPhone)
        );
        assert_eq!(
            classify_smoke_auth(&AuthorizationState::WaitTdlibParameters),
            None
        );
        assert_eq!(
            classify_smoke_auth(&AuthorizationState::WaitPremiumPurchase),
            Some(SmokeOutcome::BlockedUnsupported)
        );
        assert_eq!(
            classify_smoke_auth(&AuthorizationState::Closed),
            Some(SmokeOutcome::BlockedClosed)
        );
    }

    #[test]
    fn drive_reaches_wait_phone_via_injection() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        driver.kickoff().unwrap();

        let seq = AtomicU64::new(0);
        let envelopes = vec![
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitTdlibParameters"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
            copy_and_parse(r#"{"@type":"ok","@extra":"2"}"#, &seq, &dyn_sink).unwrap(),
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        ];
        let mut iter = envelopes.into_iter();
        let outcome = drive_until_terminal(&mut driver, |_| iter.next(), Duration::from_secs(2));
        assert_eq!(outcome, SmokeOutcome::OkWaitPhone);
        assert_eq!(outcome.line(), "SMOKE_OK wait-phone");
        assert!(!sink.rendered().contains("unit-test-hash"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drive_times_out_when_stuck_on_parameters() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink);
        let mut driver = ConnectDriver::new(session, recorder, test_credentials(), prepared);
        let outcome = drive_until_terminal(&mut driver, |_| None, Duration::ZERO);
        assert_eq!(outcome, SmokeOutcome::Timeout);
        assert_eq!(outcome.line(), "SMOKE_FAIL timeout");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
