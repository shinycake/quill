//! `--passkey-probe`: a one-off, read-only check of whether the server
//! hands WebAuthn creation options to this `api_id`. Connects with the
//! signed-in account, sends `getPasskeyParameters` once and prints a
//! redacted summary (`rp.id`, the algorithms, which user fields are
//! present). Creates nothing, stores nothing.
#![deny(clippy::indexing_slicing)]

use crate::connect::{CLIENT_CLOSE_TIMEOUT, JsonSender, start_live_connect};
use crate::connect_smoke::{SmokeOutcome, drive_until_terminal};
use crate::credentials;
use crate::ids::RequestId;
use crate::platform::live_secret_store;
use crate::telegram::envelope::CommonPayload;
use crate::telegram::envelope::EnvelopePayload;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const EXTRA: u64 = 0x5051_0001;

fn summary(text: &str) -> String {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return format!("PROBE_OK unparsed len={}", text.len());
    };
    let pk = v.get("publicKey").unwrap_or(&v);
    let rp = pk
        .pointer("/rp/id")
        .or_else(|| pk.get("rpId"))
        .and_then(Value::as_str)
        .unwrap_or("?");
    let algs: Vec<i64> = pk
        .get("pubKeyCredParams")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|p| p.get("alg")?.as_i64()).collect())
        .unwrap_or_default();
    let user_keys: Vec<String> = pk
        .get("user")
        .and_then(Value::as_object)
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    let top: Vec<String> = pk
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    format!("PROBE_OK rp.id={rp} algs={algs:?} user_fields={user_keys:?} keys={top:?}")
}

pub fn cli_exit_code(login: bool) -> i32 {
    let (Some(creds), Ok(_)) = (credentials::load(), std::env::var("QUILL_TDJSON_PATH")) else {
        println!("PROBE_BLOCKED setup");
        return 1;
    };
    let sink = std::sync::Arc::new(crate::diagnostics::MemorySink::new());
    let started = if login {
        // A throwaway, signed-out TDLib database: no account, no keychain.
        let dir = std::env::temp_dir().join(format!("quill-passkey-probe-{}", std::process::id()));
        let store = crate::platform::MemorySecretStore::default();
        crate::connect::prepare_connect(&dir, crate::ids::AccountKey::primary(), &store, &creds)
            .and_then(|prepared| {
                crate::connect::start_prepared_live_connect(creds.clone(), prepared, sink)
            })
    } else {
        start_live_connect(creds, live_secret_store().as_ref(), sink)
    };
    let mut live = match started {
        Ok(live) => live,
        Err(blocker) => {
            println!("PROBE_BLOCKED {}", blocker.slug());
            return 1;
        }
    };
    let outcome = drive_until_terminal(
        &mut live.driver,
        |wait| live.bridge.next_timeout(wait),
        Duration::from_secs(60),
    );
    let mut code = 1;
    let (want, method) = if login {
        (
            SmokeOutcome::OkWaitPhone,
            "getAuthenticationPasskeyParameters",
        )
    } else {
        (SmokeOutcome::OkReady, "getPasskeyParameters")
    };
    if outcome != want {
        println!("PROBE_NOT_READY {}", outcome.line());
    } else {
        let request = json!({"@type": method, "@extra": EXTRA}).to_string();
        if live.driver.sender.send_json(&request).is_err() {
            println!("PROBE_FAIL send");
        } else {
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut answered = false;
            while Instant::now() < deadline {
                let Some(owned) = live.bridge.next_timeout(Duration::from_millis(200)) else {
                    continue;
                };
                if owned.envelope.extra != Some(RequestId(EXTRA)) {
                    continue;
                }
                answered = true;
                match &owned.envelope.payload {
                    EnvelopePayload::Common(CommonPayload::Text { text }) => {
                        println!("{}", summary(text));
                        code = 0;
                    }
                    EnvelopePayload::Error(err) => println!("PROBE_ERROR {err:?}"),
                    other => println!("PROBE_OTHER {other:?}"),
                }
                break;
            }
            if !answered {
                println!("PROBE_FAIL timeout");
            }
        }
    }
    live.shutdown(CLIENT_CLOSE_TIMEOUT);
    code
}
