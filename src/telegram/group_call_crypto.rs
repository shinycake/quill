//! A conference call's frame encryption through TDLib
//! (docs/decisions/tgcalls-conference-calls.md).
//!
//! tgcalls asks for every audio and video frame of a conference call to be
//! encrypted or decrypted synchronously, on its media threads (tdesktop
//! answers in place with tde2e's `call_encrypt` / `call_decrypt`). TDLib
//! keeps the call's key state to itself and offers only the asynchronous
//! `encryptGroupCallData` / `decryptGroupCallData`, which `td_execute`
//! refuses. So the media thread sends the request to the account's live
//! client with an `@extra` of its own (`quill-e2e:<n>`) and waits for the
//! answer. The process-wide receive loop ([`crate::telegram::hub`]) hands
//! any envelope carrying such an `@extra` to [`intercept`] first, so the
//! answer reaches the waiting thread without passing through the UI's
//! session, and the receive loop itself never waits on a call thread.

use crate::calls::engine::FrameCrypto;
use crate::telegram::ffi::TdJson;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const EXTRA_PREFIX: &str = "quill-e2e:";

/// How long a media thread waits for one frame's answer before it drops
/// the frame. TDLib answers within a millisecond when it is not busy; a
/// frame later than this is too late to play anyway, and the wait must
/// stay short because destroying the call waits for a frame in flight.
pub const FRAME_TIMEOUT: Duration = Duration::from_millis(250);

/// Where the requests go: the account's live TDLib client. Sends bypass
/// the connect driver's sender on purpose: the agent screen and the paid
/// chat guard screen user actions, and a call's frames are neither.
pub trait CryptoLink: Send + Sync {
    /// Hands `request` to TDLib; false when it could not go out.
    fn send(&self, request: &str) -> bool;
}

/// A live client's `td_send`, which is thread-safe.
pub struct TdCryptoLink {
    api: Arc<TdJson>,
    client_id: i32,
}

impl TdCryptoLink {
    pub fn new(api: Arc<TdJson>, client_id: i32) -> Self {
        Self { api, client_id }
    }
}

impl CryptoLink for TdCryptoLink {
    fn send(&self, request: &str) -> bool {
        self.api.send(self.client_id, request).is_ok()
    }
}

/// What a waiting media thread gets: the frame, or `None` for TDLib's
/// error (the call isn't joined yet, a frame that fails to decrypt).
type Answer = Option<Vec<u8>>;

static PENDING: Mutex<Option<HashMap<u64, SyncSender<Answer>>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// One conference call's encryption: tdesktop's `TdE2E::EncryptDecrypt`,
/// whose call id is set once the call is joined and cleared when it ends.
/// Until then every frame is dropped, as tdesktop's callback returns
/// nothing without a call id.
pub struct GroupCallCrypto {
    link: Arc<dyn CryptoLink>,
    /// TDLib's group call id; 0 while there is none.
    group_call_id: AtomicI32,
    timeout: Duration,
}

impl GroupCallCrypto {
    pub fn new(link: Arc<dyn CryptoLink>) -> Self {
        Self::with_timeout(link, FRAME_TIMEOUT)
    }

    pub fn with_timeout(link: Arc<dyn CryptoLink>, timeout: Duration) -> Self {
        Self {
            link,
            group_call_id: AtomicI32::new(0),
            timeout,
        }
    }

    /// The call's TDLib id, once `joinGroupCall` or `createGroupCall`
    /// answered with it (tdesktop `setCallId`).
    pub fn bind(&self, group_call_id: i32) {
        self.group_call_id.store(group_call_id, Ordering::SeqCst);
    }

    /// The call is over: frames still in flight are dropped (tdesktop
    /// `clearCallId`).
    pub fn unbind(&self) {
        self.group_call_id.store(0, Ordering::SeqCst);
    }

    pub fn group_call_id(&self) -> Option<i32> {
        match self.group_call_id.load(Ordering::SeqCst) {
            0 => None,
            id => Some(id),
        }
    }

    /// Sends `request` and waits for its answer, at most `timeout`.
    fn call(&self, mut request: Value) -> Answer {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        let object = request.as_object_mut()?;
        object.insert(
            "@extra".into(),
            Value::String(format!("{EXTRA_PREFIX}{id}")),
        );
        let (tx, rx) = mpsc::sync_channel(1);
        crate::locks::lock_or_recover(&PENDING)
            .get_or_insert_with(HashMap::new)
            .insert(id, tx);
        if !self.link.send(&request.to_string()) {
            forget(id);
            return None;
        }
        let answer = rx.recv_timeout(self.timeout);
        forget(id);
        answer.ok().flatten()
    }
}

impl FrameCrypto for GroupCallCrypto {
    fn encrypt(&self, frame: &[u8], unencrypted_prefix: i32) -> Option<Vec<u8>> {
        let group_call_id = self.group_call_id()?;
        // tdesktop encrypts both its instances on channel 0
        // (`tde2e_api::CallChannelId(0)`): the main data channel.
        self.call(json!({
            "@type": "encryptGroupCallData",
            "group_call_id": group_call_id,
            "data_channel": {"@type": "groupCallDataChannelMain"},
            "data": STANDARD.encode(frame),
            "unencrypted_prefix_size": unencrypted_prefix.max(0),
        }))
    }

    fn decrypt(&self, sender_user_id: i64, frame: &[u8]) -> Option<Vec<u8>> {
        let group_call_id = self.group_call_id()?;
        self.call(json!({
            "@type": "decryptGroupCallData",
            "group_call_id": group_call_id,
            "participant_id": {"@type": "messageSenderUser", "user_id": sender_user_id},
            "data_channel": {"@type": "groupCallDataChannelMain"},
            "data": STANDARD.encode(frame),
        }))
    }
}

fn forget(id: u64) {
    if let Some(pending) = crate::locks::lock_or_recover(&PENDING).as_mut() {
        pending.remove(&id);
    }
}

fn request_id(envelope: &Value) -> Option<u64> {
    envelope
        .get("@extra")?
        .as_str()?
        .strip_prefix(EXTRA_PREFIX)?
        .parse()
        .ok()
}

/// TDLib's `data` answer as bytes; anything else (an `error`) is `None`.
fn frame_of(envelope: &Value) -> Answer {
    if envelope.get("@type").and_then(Value::as_str) != Some("data") {
        return None;
    }
    STANDARD.decode(envelope.get("data")?.as_str()?).ok()
}

/// Take `json` when it answers a frame request. True means the envelope is
/// a frame's (delivered, or late after its media thread gave up) and must
/// not reach the UI.
pub fn intercept(json: &str) -> bool {
    if !json.contains(EXTRA_PREFIX) {
        return false;
    }
    let Ok(envelope) = serde_json::from_str::<Value>(json) else {
        return false;
    };
    let Some(id) = request_id(&envelope) else {
        return false;
    };
    let waiter = crate::locks::lock_or_recover(&PENDING)
        .as_mut()
        .and_then(|pending| pending.remove(&id));
    if let Some(tx) = waiter {
        let _ = tx.try_send(frame_of(&envelope));
    }
    true
}

#[cfg(test)]
#[path = "group_call_crypto_tests.rs"]
mod tests;
