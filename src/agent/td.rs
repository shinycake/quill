//! Agent requests to TDLib, kept apart from the UI's.
//!
//! An agent request goes to the live client of an account with an `@extra`
//! of its own (`quill-agent:<n>`). The shared receive loop hands any
//! envelope carrying such an `@extra` to [`intercept`] before parsing, so the
//! answer reaches the waiting agent thread and never the UI's session.

use crate::ids::AccountKey;
use crate::telegram::client::LiveTdJson;
use crate::telegram::ffi::TdJson;
use crate::telegram::hub::TdHub;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const EXTRA_PREFIX: &str = "quill-agent:";
/// How long one agent request waits for TDLib.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// One account's live TDLib client.
#[derive(Clone)]
pub struct Client {
    api: Arc<TdJson>,
    hub: Arc<TdHub>,
    client_id: i32,
}

impl Client {
    pub fn id(&self) -> i32 {
        self.client_id
    }

    fn alive(&self) -> bool {
        self.hub.routes().is_registered(self.client_id)
    }
}

/// Why a TDLib call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TdError {
    /// TDLib answered with an `error` object.
    Td { code: i64, message: String },
    /// No answer within [`CALL_TIMEOUT`].
    Timeout,
    /// The request could not be handed to TDLib.
    Send,
}

static CLIENTS: Mutex<Vec<(AccountKey, Client)>> = Mutex::new(Vec::new());
static PENDING: Mutex<Option<HashMap<u64, SyncSender<Value>>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// Make `live` the client agents use for `account`. Called when the
/// account's client opens, before its first request.
pub fn register_client(account: &AccountKey, live: &LiveTdJson) {
    let client = Client {
        api: live.api.clone(),
        hub: live.hub.clone(),
        client_id: live.client_id,
    };
    let mut clients = crate::locks::lock_or_recover(&CLIENTS);
    clients.retain(|(key, old)| key != account && old.alive());
    clients.push((account.clone(), client));
}

/// The live client of `account`, while its route on the hub is open.
pub fn client_for(account: &AccountKey) -> Option<Client> {
    let clients = crate::locks::lock_or_recover(&CLIENTS);
    clients
        .iter()
        .find(|(key, client)| key == account && client.alive())
        .map(|(_, client)| client.clone())
}

/// Send `request` to `client` and wait for its answer.
pub fn call(client: &Client, request: Value) -> Result<Value, TdError> {
    call_waiting(client, request, CALL_TIMEOUT)
}

/// [`call`] with a timeout of its own, for slow requests (downloads).
pub fn call_waiting(
    client: &Client,
    mut request: Value,
    timeout: Duration,
) -> Result<Value, TdError> {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    let Some(object) = request.as_object_mut() else {
        return Err(TdError::Send);
    };
    object.insert(
        "@extra".into(),
        Value::String(format!("{EXTRA_PREFIX}{id}")),
    );
    let (tx, rx) = mpsc::sync_channel(1);
    crate::locks::lock_or_recover(&PENDING)
        .get_or_insert_with(HashMap::new)
        .insert(id, tx);
    if client
        .api
        .send(client.client_id, &request.to_string())
        .is_err()
    {
        forget(id);
        return Err(TdError::Send);
    }
    let answer = rx.recv_timeout(timeout);
    forget(id);
    let answer = answer.map_err(|_| TdError::Timeout)?;
    if answer.get("@type").and_then(Value::as_str) == Some("error") {
        return Err(TdError::Td {
            code: answer.get("code").and_then(Value::as_i64).unwrap_or(0),
            message: answer
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        });
    }
    Ok(answer)
}

fn forget(id: u64) {
    if let Some(pending) = crate::locks::lock_or_recover(&PENDING).as_mut() {
        pending.remove(&id);
    }
}

fn agent_request_id(envelope: &Value) -> Option<u64> {
    envelope
        .get("@extra")?
        .as_str()?
        .strip_prefix(EXTRA_PREFIX)?
        .parse()
        .ok()
}

/// Take `json` when it answers an agent request. True means the envelope is
/// the agent's (delivered, or late after its caller gave up) and must not
/// reach the UI.
pub fn intercept(json: &str) -> bool {
    if !json.contains(EXTRA_PREFIX) {
        return false;
    }
    let Ok(envelope) = serde_json::from_str::<Value>(json) else {
        return false;
    };
    let Some(id) = agent_request_id(&envelope) else {
        return false;
    };
    let waiter = crate::locks::lock_or_recover(&PENDING)
        .as_mut()
        .and_then(|pending| pending.remove(&id));
    if let Some(tx) = waiter {
        let _ = tx.try_send(envelope);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{PENDING, intercept};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::mpsc;

    #[test]
    fn agent_answers_are_taken_and_ui_envelopes_pass() {
        let (tx, rx) = mpsc::sync_channel(1);
        crate::locks::lock_or_recover(&PENDING)
            .get_or_insert_with(HashMap::new)
            .insert(9_000_001, tx);
        assert!(intercept(
            r#"{"@type":"ok","@extra":"quill-agent:9000001","@client_id":1}"#
        ));
        assert_eq!(
            rx.try_recv().ok(),
            Some(json!({"@type":"ok","@extra":"quill-agent:9000001","@client_id":1}))
        );
        // A late answer is still the agent's.
        assert!(intercept(
            r#"{"@type":"ok","@extra":"quill-agent:9000002"}"#
        ));
        // The UI's numeric extras and message text that mentions the prefix pass.
        assert!(!intercept(r#"{"@type":"ok","@extra":17}"#));
        assert!(!intercept(
            r#"{"@type":"updateNewMessage","message":{"content":{"text":{"text":"quill-agent:5"}}}}"#
        ));
    }
}
