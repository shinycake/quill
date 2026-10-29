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
//!
//! While a call is bound, TDLib's `td_requests` log tag is raised out of
//! reach ([`CryptoLink::hide_request_log`]): at debug mode's verbosity it
//! would write every request and answer, the call's plaintext frames
//! among them, to `tdlib.log`. Each call's round trips are counted and
//! summed up in the calls log when it ends.

use crate::calls::engine::FrameCrypto;
use crate::telegram::ffi::TdJson;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

    /// Keeps TDLib's request log from writing frame requests and answers
    /// (`hide`), or lets it again. Called once per call each way.
    fn hide_request_log(&self, _hide: bool) {}
}

/// TDLib's tag for "Receive request" / "Sending result for request" lines
/// (`VLOG(td_requests)` in td/telegram/Td.cpp; INFO, 3, by default, so
/// debug mode's verbosity 4 writes them).
pub const REQUEST_LOG_TAG: &str = "td_requests";
/// The tag's level while a call is bound: a line is written only when the
/// global verbosity reaches its tag's level, and 1024 is TDLib's highest.
pub const HIDDEN_REQUEST_LOG_LEVEL: i32 = 1024;

/// How many calls keep the request log hidden, and the tag's level to put
/// back once none does. TDLib's log tags are process-wide, so every
/// account's calls share one.
#[derive(Default)]
pub struct RequestLogGate {
    state: Mutex<(usize, Option<i32>)>,
}

impl RequestLogGate {
    pub const fn new() -> Self {
        Self {
            state: Mutex::new((0, None)),
        }
    }

    /// A call hides the log: the first one reads the tag's level and sets
    /// the hidden one.
    pub fn hide(&self, read: impl FnOnce() -> Option<i32>, set: impl FnOnce(i32)) {
        let mut state = crate::locks::lock_or_recover(&self.state);
        state.0 += 1;
        if state.0 == 1 {
            state.1 = read();
            set(HIDDEN_REQUEST_LOG_LEVEL);
        }
    }

    /// A call is done: the last one puts the level back (TDLib's default
    /// when it could not be read).
    pub fn show(&self, set: impl FnOnce(i32)) {
        let mut state = crate::locks::lock_or_recover(&self.state);
        if state.0 == 0 {
            return;
        }
        state.0 -= 1;
        if state.0 == 0 {
            set(state.1.take().unwrap_or(DEFAULT_REQUEST_LOG_LEVEL));
        }
    }
}

/// `td_requests`' level in TDLib (`VERBOSITY_NAME(INFO)`).
const DEFAULT_REQUEST_LOG_LEVEL: i32 = 3;

static REQUEST_LOG: RequestLogGate = RequestLogGate::new();

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

impl TdCryptoLink {
    fn set_request_log_level(&self, level: i32) {
        let request = json!({
            "@type": "setLogTagVerbosityLevel",
            "tag": REQUEST_LOG_TAG,
            "new_verbosity_level": level,
        });
        let ok = matches!(
            self.api.execute(&request.to_string()),
            Ok(Some(body)) if body.contains("\"ok\"")
        );
        if !ok {
            eprintln!("quill: TDLib refused the {REQUEST_LOG_TAG} log level {level}");
        }
    }

    fn request_log_level(&self) -> Option<i32> {
        let request = json!({"@type": "getLogTagVerbosityLevel", "tag": REQUEST_LOG_TAG});
        let body = self.api.execute(&request.to_string()).ok()??;
        let level = serde_json::from_str::<Value>(&body).ok()?;
        i32::try_from(level.get("verbosity_level")?.as_i64()?).ok()
    }
}

impl CryptoLink for TdCryptoLink {
    fn send(&self, request: &str) -> bool {
        self.api.send(self.client_id, request).is_ok()
    }

    fn hide_request_log(&self, hide: bool) {
        if hide {
            REQUEST_LOG.hide(
                || self.request_log_level(),
                |level| self.set_request_log_level(level),
            );
        } else {
            REQUEST_LOG.show(|level| self.set_request_log_level(level));
        }
    }
}

/// A client that takes no frame requests: a test sender's, where no call
/// carries media.
pub struct RefusingLink;

impl CryptoLink for RefusingLink {
    fn send(&self, _request: &str) -> bool {
        false
    }
}

/// What a waiting media thread gets: the frame, or `None` for TDLib's
/// error (the call isn't joined yet, a frame that fails to decrypt).
type Answer = Option<Vec<u8>>;

static PENDING: Mutex<Option<HashMap<u64, SyncSender<Answer>>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// A call's round trips to TDLib, for the calls log.
#[derive(Default)]
struct Stats {
    round_trips: AtomicU64,
    /// No answer within the timeout.
    timeouts: AtomicU64,
    /// TDLib answered with an error, or the request did not go out.
    errors: AtomicU64,
    /// Answered round trips' summed and longest latency.
    total_micros: AtomicU64,
    max_micros: AtomicU64,
}

/// What [`Stats`] add up to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CryptoStats {
    pub round_trips: u64,
    pub timeouts: u64,
    pub errors: u64,
    pub avg_micros: u64,
    pub max_micros: u64,
}

impl Stats {
    fn note(&self, outcome: Outcome, latency: Duration) {
        self.round_trips.fetch_add(1, Ordering::Relaxed);
        match outcome {
            Outcome::Answered => {
                let micros = u64::try_from(latency.as_micros()).unwrap_or(u64::MAX);
                self.total_micros.fetch_add(micros, Ordering::Relaxed);
                self.max_micros.fetch_max(micros, Ordering::Relaxed);
            }
            Outcome::TimedOut => {
                self.timeouts.fetch_add(1, Ordering::Relaxed);
            }
            Outcome::Failed => {
                self.errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// The totals so far, and counting starts over.
    fn take(&self) -> CryptoStats {
        let round_trips = self.round_trips.swap(0, Ordering::Relaxed);
        let timeouts = self.timeouts.swap(0, Ordering::Relaxed);
        let errors = self.errors.swap(0, Ordering::Relaxed);
        let total = self.total_micros.swap(0, Ordering::Relaxed);
        let answered = round_trips.saturating_sub(timeouts + errors);
        CryptoStats {
            round_trips,
            timeouts,
            errors,
            avg_micros: total.checked_div(answered).unwrap_or(0),
            max_micros: self.max_micros.swap(0, Ordering::Relaxed),
        }
    }
}

impl CryptoStats {
    /// The calls log line for call `group_call_id`.
    pub fn log_line(&self, group_call_id: i32) -> String {
        format!(
            "[call] group {group_call_id} e2e: {} round trips, {} timeouts, {} errors, avg {} us, max {} us",
            self.round_trips, self.timeouts, self.errors, self.avg_micros, self.max_micros
        )
    }
}

#[derive(Clone, Copy)]
enum Outcome {
    Answered,
    TimedOut,
    Failed,
}

/// One conference call's encryption: tdesktop's `TdE2E::EncryptDecrypt`,
/// whose call id is set once the call is joined and cleared when it ends.
/// Until then every frame is dropped, as tdesktop's callback returns
/// nothing without a call id.
pub struct GroupCallCrypto {
    link: Arc<dyn CryptoLink>,
    /// TDLib's group call id; 0 while there is none.
    group_call_id: AtomicI32,
    timeout: Duration,
    /// This call's requests waiting for TDLib, so unbinding wakes them.
    in_flight: Mutex<HashSet<u64>>,
    /// The request log is hidden for this call.
    hiding_log: AtomicBool,
    stats: Stats,
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
            in_flight: Mutex::new(HashSet::new()),
            hiding_log: AtomicBool::new(false),
            stats: Stats::default(),
        }
    }

    /// The call's TDLib id, once `joinGroupCall` or `createGroupCall`
    /// answered with it (tdesktop `setCallId`). TDLib's request log stops
    /// writing frames first.
    pub fn bind(&self, group_call_id: i32) {
        if !self.hiding_log.swap(true, Ordering::SeqCst) {
            self.link.hide_request_log(true);
        }
        self.group_call_id.store(group_call_id, Ordering::SeqCst);
    }

    /// The call is over (tdesktop `clearCallId`): frames still waiting on
    /// TDLib are dropped at once, later ones without asking, and the
    /// call's round trips go to the calls log.
    pub fn unbind(&self) {
        let was = self.group_call_id.swap(0, Ordering::SeqCst);
        let waiting: Vec<u64> = crate::locks::lock_or_recover(&self.in_flight)
            .drain()
            .collect();
        if let Some(pending) = crate::locks::lock_or_recover(&PENDING).as_mut() {
            for id in waiting {
                // Dropping the sender wakes its media thread.
                pending.remove(&id);
            }
        }
        if self.hiding_log.swap(false, Ordering::SeqCst) {
            self.link.hide_request_log(false);
        }
        if was != 0 {
            let stats = self.stats.take();
            if stats.round_trips > 0 {
                crate::advanced::debug_mode::call_log(&stats.log_line(was));
            }
        }
    }

    /// The round trips since the last summary, for tests.
    #[cfg(test)]
    fn take_stats(&self) -> CryptoStats {
        self.stats.take()
    }

    pub fn group_call_id(&self) -> Option<i32> {
        match self.group_call_id.load(Ordering::SeqCst) {
            0 => None,
            id => Some(id),
        }
    }

    /// Sends `request` and waits for its answer, at most `timeout`, or
    /// until the call is unbound.
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
        crate::locks::lock_or_recover(&self.in_flight).insert(id);
        // Unbound meanwhile: `unbind` may have swept before the insert.
        if self.group_call_id().is_none() {
            self.forget(id);
            return None;
        }
        let started = Instant::now();
        if !self.link.send(&request.to_string()) {
            self.forget(id);
            self.stats.note(Outcome::Failed, Duration::ZERO);
            return None;
        }
        let answer = rx.recv_timeout(self.timeout);
        let latency = started.elapsed();
        self.forget(id);
        let (outcome, frame) = match answer {
            Ok(Some(frame)) => (Outcome::Answered, Some(frame)),
            Ok(None) => (Outcome::Failed, None),
            Err(RecvTimeoutError::Timeout) => (Outcome::TimedOut, None),
            // Unbound while waiting: the call is over, not slow.
            Err(RecvTimeoutError::Disconnected) => return None,
        };
        self.stats.note(outcome, latency);
        frame
    }

    fn forget(&self, id: u64) {
        crate::locks::lock_or_recover(&self.in_flight).remove(&id);
        forget(id);
    }
}

impl Drop for GroupCallCrypto {
    fn drop(&mut self) {
        if self.hiding_log.swap(false, Ordering::SeqCst) {
            self.link.hide_request_log(false);
        }
    }
}

impl FrameCrypto for GroupCallCrypto {
    // Both directions, both instances (the call's and its presentation's)
    // use the main data channel, as tdesktop's `EncryptDecrypt::callback`
    // does: it passes `tde2e_api::CallChannelId(0)` to `call_encrypt` and
    // `call_decrypt` alike, for its screencast instance too. TDLib maps
    // `groupCallDataChannelMain` (and a null channel) to channel 0 and
    // `groupCallDataChannelScreenSharing` to channel 1
    // (GroupCallManager::encrypt_group_call_data / decrypt_group_call_data).
    // A shared screen sent on channel 1 would be decrypted by tdesktop on
    // channel 0, and tgcalls doesn't say which instance's stream an
    // incoming frame belongs to, so channel 1 is never used.
    fn encrypt(&self, frame: &[u8], unencrypted_prefix: i32) -> Option<Vec<u8>> {
        let group_call_id = self.group_call_id()?;
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
