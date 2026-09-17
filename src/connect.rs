//! Live TDLib connect gate: credentials + tdjson → setTdlibParameters → auth updates.
//! Never logs api_hash, phone numbers, or codes.

use crate::composer::ComposerSnapshot;
use crate::credentials::TelegramCredentials;
use crate::diagnostics::DiagnosticSink;
use crate::ids::{AccountKey, ChatId, MessageId, RequestId};
use crate::lifecycle::{RestoreBlocker, plan_restore};
use crate::platform::{DatabaseKey, KeyDecision, SecretStore, load_or_create_key};
use crate::settings::{AccountPaths, default_app_root};
use crate::state::{RequestPurpose, Session, ShutdownPhase};
use crate::telegram::client::{LiveTdJson, OwnedEnvelope, ReceiveBridge};
use crate::telegram::envelope::AuthorizationState;
use crate::telegram::ffi::{LibraryOrigin, TdJsonError, resolve_tdjson_path};
use crate::telegram::requests::{
    SetTdlibParameters, check_authentication_code, check_authentication_password, close_request,
    get_authorization_state, get_chat_history, load_chats, send_text,
    set_authentication_phone_number,
};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How many chats to ask TDLib to load per `loadChats` page.
pub const MAIN_CHAT_LOAD_LIMIT: i32 = 100;
/// Page size for `getChatHistory`.
pub const HISTORY_PAGE_SIZE: i32 = 50;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectBlocker {
    MissingCredentials,
    MissingTdjson,
    MissingKeyAgainstExistingDb,
    LockedStore,
    StoreError,
    TdjsonLoad,
}

impl ConnectBlocker {
    pub fn user_message(&self) -> &'static str {
        match self {
            ConnectBlocker::MissingCredentials => {
                "set TELEGRAM_API_ID / TELEGRAM_API_HASH (or local .env)"
            }
            ConnectBlocker::MissingTdjson => {
                "tdjson not found — set QUILL_TDJSON_PATH to libtdjson, or bundle it next to the executable (see docs/native-bundle.md). Homebrew paths are never searched."
            }
            ConnectBlocker::MissingKeyAgainstExistingDb => {
                "database encryption key missing for an existing TDLib database — restore the key or remove the local database"
            }
            ConnectBlocker::LockedStore => "secret store is locked or unavailable",
            ConnectBlocker::StoreError => "secret store error",
            ConnectBlocker::TdjsonLoad => {
                "tdjson library found but failed to load (missing symbols or wrong arch)"
            }
        }
    }

    /// One-token label for `--connect-smoke` (no secrets).
    pub fn slug(&self) -> &'static str {
        match self {
            ConnectBlocker::MissingCredentials => "missing-credentials",
            ConnectBlocker::MissingTdjson => "missing-tdjson",
            ConnectBlocker::MissingKeyAgainstExistingDb => "missing-key",
            ConnectBlocker::LockedStore => "locked-store",
            ConnectBlocker::StoreError => "store-error",
            ConnectBlocker::TdjsonLoad => "tdjson-load",
        }
    }
}

impl From<RestoreBlocker> for ConnectBlocker {
    fn from(value: RestoreBlocker) -> Self {
        match value {
            RestoreBlocker::MissingCredentials => ConnectBlocker::MissingCredentials,
            RestoreBlocker::MissingKeyAgainstExistingDb => {
                ConnectBlocker::MissingKeyAgainstExistingDb
            }
            RestoreBlocker::LockedStore => ConnectBlocker::LockedStore,
            RestoreBlocker::StoreError => ConnectBlocker::StoreError,
        }
    }
}

/// Outcome of the pre-flight gate (no native load yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectGate {
    Blocked(ConnectBlocker),
    Ready { tdjson: LibraryOrigin },
}

/// Classify credentials + tdjson availability. Does not open a client.
pub fn evaluate_gate(credentials_present: bool) -> ConnectGate {
    if !credentials_present {
        return ConnectGate::Blocked(ConnectBlocker::MissingCredentials);
    }
    match resolve_tdjson_path() {
        Some(tdjson) => ConnectGate::Ready { tdjson },
        None => ConnectGate::Blocked(ConnectBlocker::MissingTdjson),
    }
}

/// Local paths + database key ready for `setTdlibParameters`.
#[derive(Debug)]
pub struct PreparedConnect {
    pub account: AccountKey,
    pub paths: AccountPaths,
    pub database_key: DatabaseKey,
    pub database_exists: bool,
}

/// Resolve account paths and DB key. Credentials must already be validated.
pub fn prepare_connect<S: SecretStore + ?Sized>(
    app_root: &Path,
    account: AccountKey,
    store: &S,
    credentials: &TelegramCredentials,
) -> Result<PreparedConnect, ConnectBlocker> {
    let plan = plan_restore(
        app_root,
        account.clone(),
        store,
        Some(credentials.api_id),
        Some(credentials.api_hash.as_str()),
    )
    .map_err(ConnectBlocker::from)?;
    let key =
        load_or_create_key(store, &plan.account, plan.database_exists).map_err(|e| match e {
            KeyDecision::MissingAgainstExistingDb => ConnectBlocker::MissingKeyAgainstExistingDb,
            KeyDecision::Locked => ConnectBlocker::LockedStore,
            KeyDecision::Store(_) => ConnectBlocker::StoreError,
        })?;
    std::fs::create_dir_all(&plan.paths.tdlib_database).map_err(|_| ConnectBlocker::StoreError)?;
    std::fs::create_dir_all(&plan.paths.tdlib_files).map_err(|_| ConnectBlocker::StoreError)?;
    Ok(PreparedConnect {
        account: plan.account,
        paths: plan.paths,
        database_key: key,
        database_exists: plan.database_exists,
    })
}

/// Build `setTdlibParameters` from loaded credentials + local paths/key.
/// The returned JSON includes `api_hash`; callers must not log it.
pub fn build_set_tdlib_parameters(
    credentials: &TelegramCredentials,
    paths: &AccountPaths,
    database_key: &DatabaseKey,
) -> SetTdlibParameters {
    SetTdlibParameters {
        use_test_dc: std::env::var("QUILL_USE_TEST_DC")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        database_directory: paths.tdlib_database.to_string_lossy().into_owned(),
        files_directory: paths.tdlib_files.to_string_lossy().into_owned(),
        database_encryption_key_b64: database_key.tdlib_base64(),
        api_id: credentials.api_id,
        api_hash: credentials.api_hash.clone(),
        device_model: "Desktop".into(),
        system_version: std::env::consts::OS.into(),
        application_version: env!("CARGO_PKG_VERSION").into(),
        system_language_code: "en".into(),
    }
}

/// Outbound JSON (live `td_send` or test recorder). Must not log request bodies.
pub trait JsonSender: Send {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectSendError {
    InvalidRequest,
    Native,
}

/// Records outbound JSON for unit/replay tests (no network).
#[derive(Default)]
pub struct RecordingSender {
    pub sent: Mutex<Vec<String>>,
}

impl RecordingSender {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.sent.lock().expect("recording sender").clone()
    }
}

impl JsonSender for RecordingSender {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        self.sent
            .lock()
            .expect("recording sender")
            .push(request.to_string());
        Ok(())
    }
}

impl JsonSender for Arc<RecordingSender> {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        (**self).send_json(request)
    }
}

/// Live `td_send` wrapper.
pub struct LiveSender {
    api: Arc<crate::telegram::ffi::TdJson>,
    client_id: i32,
}

impl LiveSender {
    pub fn from_live(live: &LiveTdJson) -> Self {
        Self {
            api: live.api.clone(),
            client_id: live.client_id,
        }
    }
}

impl JsonSender for LiveSender {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        self.api.send(self.client_id, request).map_err(|e| match e {
            TdJsonError::InvalidRequest => ConnectSendError::InvalidRequest,
            _ => ConnectSendError::Native,
        })
    }
}

/// Session + outbound sender that auto-replies to `WaitTdlibParameters`.
pub struct ConnectDriver<S: JsonSender> {
    pub session: Session,
    sender: S,
    credentials: TelegramCredentials,
    paths: AccountPaths,
    database_key: DatabaseKey,
    parameters_sent: bool,
}

impl<S: JsonSender> ConnectDriver<S> {
    pub fn new(
        session: Session,
        sender: S,
        credentials: TelegramCredentials,
        prepared: PreparedConnect,
    ) -> Self {
        Self {
            session,
            sender,
            credentials,
            paths: prepared.paths,
            database_key: prepared.database_key,
            parameters_sent: false,
        }
    }

    pub fn parameters_sent(&self) -> bool {
        self.parameters_sent
    }

    /// Kick the JSON client so authorization updates start flowing.
    pub fn kickoff(&mut self) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::GetAuthorizationState, None);
        self.sender.send_json(&get_authorization_state(extra))?;
        Ok(extra)
    }

    pub fn ingest(&mut self, owned: OwnedEnvelope) -> Result<(), ConnectSendError> {
        self.session.apply(owned);
        self.maybe_send_parameters()?;
        self.maybe_load_main_chats()?;
        Ok(())
    }

    fn maybe_send_parameters(&mut self) -> Result<(), ConnectSendError> {
        if self.parameters_sent {
            return Ok(());
        }
        if !matches!(self.session.auth, AuthorizationState::WaitTdlibParameters) {
            return Ok(());
        }
        let extra = self.session.request(RequestPurpose::SetParameters, None);
        let params = build_set_tdlib_parameters(&self.credentials, &self.paths, &self.database_key);
        // Contains api_hash — do not log `json`.
        let json = params.to_json(extra);
        self.sender.send_json(&json)?;
        self.parameters_sent = true;
        Ok(())
    }

    fn chats_path_active(&self) -> bool {
        matches!(self.session.auth, AuthorizationState::Ready)
            && matches!(self.session.shutdown, ShutdownPhase::Running)
    }

    /// After Ready, keep paging `loadChats` until TDLib returns 404 (exhausted).
    pub fn maybe_load_main_chats(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(None);
        }
        if self.session.chats_exhausted {
            return Ok(None);
        }
        if self.session.requests.has_purpose(RequestPurpose::LoadChats) {
            return Ok(None);
        }
        let extra = self.session.request(RequestPurpose::LoadChats, None);
        self.sender
            .send_json(&load_chats(extra, MAIN_CHAT_LOAD_LIMIT))?;
        Ok(Some(extra))
    }

    /// Select a chat and request history. Returns `None` if history is already complete.
    pub fn select_chat(&mut self, chat_id: ChatId) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.open_chat(chat_id);
        if !self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported())
        {
            return Ok(None);
        }
        self.fetch_history()
    }

    /// Load another page of history for the open chat (`from_message_id` = oldest, or 0).
    pub fn fetch_history(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if self
            .session
            .histories
            .get(&chat_id.0)
            .is_some_and(|h| h.loaded_complete)
        {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetHistory, chat_id)
        {
            return Ok(None);
        }
        let from = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|h| h.oldest_id())
            .unwrap_or(MessageId(0));
        let extra = self
            .session
            .request(RequestPurpose::GetHistory, Some(chat_id));
        self.sender.send_json(&get_chat_history(
            extra,
            chat_id,
            from,
            0,
            HISTORY_PAGE_SIZE,
            false,
        ))?;
        Ok(Some(extra))
    }

    /// Send `sendMessage` for a snapshot frozen at composer submit.
    /// `snapshot.text` is not logged.
    pub fn send_text_snapshot(
        &mut self,
        snapshot: &ComposerSnapshot,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if snapshot.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let chat_id = snapshot.chat_id();
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SendText, Some(chat_id));
        // Contains message text — do not log `json`.
        self.sender
            .send_json(&send_text(extra, chat_id, snapshot.text.trim()))?;
        Ok(extra)
    }

    /// Send `setAuthenticationPhoneNumber` when auth is WaitPhoneNumber.
    /// Phone value is never stored on the session or diagnostics.
    pub fn submit_phone(&mut self, phone: &str) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitPhoneNumber) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let phone = phone.trim();
        if phone.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self.session.request(RequestPurpose::SetPhoneNumber, None);
        self.sender
            .send_json(&set_authentication_phone_number(extra, phone))?;
        Ok(extra)
    }

    /// Send `checkAuthenticationCode` when auth is WaitCode.
    /// The code is never stored on the session or diagnostics.
    pub fn submit_code(&mut self, code: &str) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitCode { .. }) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let code = code.trim();
        if code.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self
            .session
            .request(RequestPurpose::CheckAuthenticationCode, None);
        self.sender
            .send_json(&check_authentication_code(extra, code))?;
        Ok(extra)
    }

    /// Send `checkAuthenticationPassword` when auth is WaitPassword.
    /// The password is never stored on the session or diagnostics. Not trimmed
    /// (leading/trailing spaces can be significant).
    pub fn submit_password(&mut self, password: &str) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitPassword { .. }) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if password.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self
            .session
            .request(RequestPurpose::CheckAuthenticationPassword, None);
        self.sender
            .send_json(&check_authentication_password(extra, password))?;
        Ok(extra)
    }

    /// Send `close` (not `logOut`). Callers must keep receiving until Closed.
    pub fn request_close(&mut self) -> Result<RequestId, ConnectSendError> {
        if matches!(
            self.session.auth,
            AuthorizationState::Closed | AuthorizationState::Closing
        ) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.begin_close();
        let extra = self.session.request(RequestPurpose::Close, None);
        self.sender.send_json(&close_request(extra))?;
        Ok(extra)
    }
}

/// How long Drop / `--connect-smoke` waits for `authorizationStateClosed`.
pub const CLIENT_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Send `close` and ingest until Closed. Does not join a receive thread or
/// unload tdjson. Returns whether Closed was observed.
pub fn wait_closed<S: JsonSender>(
    driver: &mut ConnectDriver<S>,
    mut recv: impl FnMut(Duration) -> Option<OwnedEnvelope>,
    timeout: Duration,
) -> bool {
    if matches!(driver.session.auth, AuthorizationState::Closed) {
        return true;
    }
    let _ = driver.request_close();
    let deadline = Instant::now() + timeout;
    loop {
        if matches!(driver.session.auth, AuthorizationState::Closed) {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        let slice = deadline
            .saturating_duration_since(now)
            .min(Duration::from_millis(50));
        if let Some(owned) = recv(slice) {
            let _ = driver.ingest(owned);
        }
    }
}

/// Open LiveTdJson + receive bridge when gate + restore succeed.
///
/// Drop / [`LiveConnect::shutdown`] send `close`, wait for
/// `authorizationStateClosed`, then join the receive thread **before**
/// `libtdjson` is unloaded. Unloading while TDLib worker threads are still
/// running is what produced SIGSEGV (exit 139) after `--connect-smoke`.
///
/// Field order: `bridge` is dropped before `_live` (declaration order) so
/// `td_receive` is not in-flight during `dlclose`.
pub struct LiveConnect {
    pub driver: ConnectDriver<LiveSender>,
    pub bridge: ReceiveBridge,
    _live: LiveTdJson,
}

impl LiveConnect {
    /// Close the TDLib client and join the receive thread. Safe to call twice.
    /// Does not panic; a timeout still joins the thread so Drop can unload.
    pub fn shutdown(&mut self, wait: Duration) {
        if self.bridge.is_joined() {
            return;
        }
        let _ = wait_closed(
            &mut self.driver,
            |timeout| self.bridge.next_timeout(timeout),
            wait,
        );
        self.bridge.shutdown();
    }
}

impl Drop for LiveConnect {
    fn drop(&mut self) {
        self.shutdown(CLIENT_CLOSE_TIMEOUT);
    }
}

pub fn start_live_connect(
    credentials: TelegramCredentials,
    store: &(impl SecretStore + ?Sized),
    diagnostics: Arc<dyn DiagnosticSink>,
) -> Result<LiveConnect, ConnectBlocker> {
    match evaluate_gate(true) {
        ConnectGate::Blocked(b) => return Err(b),
        ConnectGate::Ready { .. } => {}
    }
    let app_root = default_app_root();
    let prepared = prepare_connect(&app_root, AccountKey::primary(), store, &credentials)?;
    let live = LiveTdJson::connect().map_err(|e| match e {
        TdJsonError::NotFound => ConnectBlocker::MissingTdjson,
        _ => ConnectBlocker::TdjsonLoad,
    })?;
    let sender = LiveSender::from_live(&live);
    let bridge = ReceiveBridge::spawn_live(live.api.clone(), diagnostics.clone());
    let session = Session::new(prepared.account.clone(), diagnostics);
    let mut driver = ConnectDriver::new(session, sender, credentials, prepared);
    driver.kickoff().map_err(|_| ConnectBlocker::TdjsonLoad)?;
    Ok(LiveConnect {
        driver,
        bridge,
        _live: live,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::MemorySink;
    use crate::platform::MemorySecretStore;
    use crate::telegram::client::copy_and_parse;
    use serde_json::Value;
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::AtomicU64;

    static TDJSON_ENV_LOCK: StdMutex<()> = StdMutex::new(());

    fn test_credentials() -> TelegramCredentials {
        TelegramCredentials {
            api_id: 99,
            api_hash: "unit-test-hash-not-for-network".into(),
        }
    }

    fn prepared_tmp(store: &MemorySecretStore) -> (std::path::PathBuf, PreparedConnect) {
        let dir = std::env::temp_dir().join(format!(
            "quill-connect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let prepared = prepare_connect(&dir, AccountKey::primary(), store, &test_credentials())
            .expect("prepare");
        (dir, prepared)
    }

    #[test]
    fn gate_blocks_without_credentials() {
        assert_eq!(
            evaluate_gate(false),
            ConnectGate::Blocked(ConnectBlocker::MissingCredentials)
        );
    }

    #[test]
    fn gate_blocks_without_tdjson_when_credentials_present() {
        let _lock = TDJSON_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var("QUILL_TDJSON_PATH").ok();
        // SAFETY: exclusive lock; restored below.
        unsafe { std::env::remove_var("QUILL_TDJSON_PATH") };
        let gate = evaluate_gate(true);
        if let Some(v) = previous {
            unsafe { std::env::set_var("QUILL_TDJSON_PATH", v) };
        }
        assert_eq!(gate, ConnectGate::Blocked(ConnectBlocker::MissingTdjson));
        assert!(
            ConnectBlocker::MissingTdjson
                .user_message()
                .contains("QUILL_TDJSON_PATH")
        );
        assert!(
            ConnectBlocker::MissingTdjson
                .user_message()
                .contains("native-bundle")
        );
    }

    #[test]
    fn set_tdlib_parameters_shape_includes_hash_but_debug_redacts_credentials() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let creds = test_credentials();
        let params = build_set_tdlib_parameters(&creds, &prepared.paths, &prepared.database_key);
        let json = params.to_json(RequestId(7));
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setTdlibParameters");
        assert_eq!(v["@extra"], "7");
        assert_eq!(v["api_id"], 99);
        assert_eq!(v["api_hash"], "unit-test-hash-not-for-network");
        assert_eq!(v["use_secret_chats"], false);
        assert_eq!(v["use_file_database"], true);
        assert!(v["database_directory"].as_str().unwrap().contains("tdlib"));
        assert!(!v["database_encryption_key"].as_str().unwrap().is_empty());
        let debug = format!("{creds:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("unit-test-hash"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_sends_parameters_then_reaches_wait_phone_via_injection() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let kick = driver.kickoff().unwrap();
        assert_eq!(kick.0, 1);

        let seq = AtomicU64::new(0);
        let wait_params = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitTdlibParameters"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_params).unwrap();
        assert!(driver.parameters_sent());

        let sent = recorder.snapshot();
        assert_eq!(sent.len(), 2); // getAuthorizationState + setTdlibParameters
        assert!(sent[0].contains("getAuthorizationState"));
        assert!(sent[1].contains("setTdlibParameters"));
        assert!(sent[1].contains("\"api_id\":99"));
        assert!(sent[1].contains("unit-test-hash-not-for-network"));
        assert!(!sink.rendered().contains("unit-test-hash"));

        let ok = copy_and_parse(r#"{"@type":"ok","@extra":"2"}"#, &seq, &dyn_sink).unwrap();
        driver.ingest(ok).unwrap();
        let wait_phone = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_phone).unwrap();
        assert!(matches!(
            driver.session.auth,
            AuthorizationState::WaitPhoneNumber
        ));
        assert_eq!(
            driver.session.auth_view.action,
            crate::auth::AuthAction::EnterPhone
        );

        let phone_extra = driver.submit_phone("+15551212").unwrap();
        let sent = recorder.snapshot();
        let phone_json = sent.last().unwrap();
        assert!(phone_json.contains("setAuthenticationPhoneNumber"));
        assert!(phone_json.contains(&format!("\"@extra\":\"{}\"", phone_extra.0)));
        assert!(phone_json.contains("+15551212"));
        assert!(!sink.rendered().contains("+15551212"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_submits_code_and_password_only_in_matching_states() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

        assert_eq!(
            driver.submit_code("12345"),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(
            driver.submit_password("secret"),
            Err(ConnectSendError::InvalidRequest)
        );

        let seq = AtomicU64::new(0);
        let wait_code = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitCode","code_info":{"@type":"authenticationCodeInfo","type":{"@type":"authenticationCodeTypeSms","length":5}}}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_code).unwrap();
        assert!(matches!(
            driver.session.auth,
            AuthorizationState::WaitCode {
                code_length: Some(5)
            }
        ));
        assert_eq!(
            driver.submit_password("secret"),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(
            driver.submit_code("  "),
            Err(ConnectSendError::InvalidRequest)
        );
        let code_extra = driver.submit_code("  12345 ").unwrap();
        let sent = recorder.snapshot();
        let code_json = sent.last().unwrap();
        assert!(code_json.contains("checkAuthenticationCode"));
        assert!(code_json.contains(&format!("\"@extra\":\"{}\"", code_extra.0)));
        assert!(code_json.contains("\"code\":\"12345\""));
        assert!(!sink.rendered().contains("12345"));

        let wait_password = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPassword","password_hint":"CANARY_HINT","has_recovery_email_address":true}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_password).unwrap();
        assert_eq!(
            driver.submit_code("12345"),
            Err(ConnectSendError::InvalidRequest)
        );
        let pw_extra = driver.submit_password(" unit-pw ").unwrap();
        let sent = recorder.snapshot();
        let pw_json = sent.last().unwrap();
        assert!(pw_json.contains("checkAuthenticationPassword"));
        assert!(pw_json.contains(&format!("\"@extra\":\"{}\"", pw_extra.0)));
        // Password is not trimmed.
        assert!(pw_json.contains("\"password\":\" unit-pw \""));
        assert!(!sink.rendered().contains("unit-pw"));
        assert!(!sink.rendered().contains("CANARY_HINT"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wait_closed_sends_close_and_reaches_closed_via_injection() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

        let seq = AtomicU64::new(0);
        let wait_phone = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_phone).unwrap();

        let envelopes = vec![
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosing"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosed"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        ];
        let mut iter = envelopes.into_iter();
        assert!(wait_closed(
            &mut driver,
            |_| iter.next(),
            Duration::from_secs(2)
        ));
        assert!(matches!(driver.session.auth, AuthorizationState::Closed));
        let sent = recorder.snapshot();
        let close_json = sent.last().expect("close request");
        assert!(close_json.contains("\"@type\":\"close\""));
        assert!(!sink.rendered().contains("unit-test-hash"));
        // Second call is a no-op once Closed (no extra send).
        let before = sent.len();
        assert!(wait_closed(&mut driver, |_| None, Duration::ZERO));
        assert_eq!(recorder.snapshot().len(), before);
        drop(driver);
        drop(recorder);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_tdjson_message_is_actionable() {
        let msg = ConnectBlocker::MissingTdjson.user_message();
        assert!(msg.contains("QUILL_TDJSON_PATH"));
        assert!(msg.contains("never searched"));
    }

    #[test]
    fn driver_loads_chats_after_ready_then_send_text() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

        assert_eq!(
            driver.select_chat(ChatId(1)),
            Err(ConnectSendError::InvalidRequest)
        );

        let seq = AtomicU64::new(0);
        let ready = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(ready).unwrap();
        assert!(matches!(driver.session.auth, AuthorizationState::Ready));

        let sent = recorder.snapshot();
        let load = sent.last().expect("loadChats after Ready");
        assert!(load.contains("\"@type\":\"loadChats\""));
        assert!(load.contains("chatListMain"));
        assert!(load.contains(&format!("\"limit\":{MAIN_CHAT_LOAD_LIMIT}")));
        let load_extra = driver
            .session
            .requests
            .has_purpose(RequestPurpose::LoadChats);
        assert!(load_extra);

        let new_chat = copy_and_parse(
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(new_chat).unwrap();
        let position = copy_and_parse(
            r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"12","is_pinned":false}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(position).unwrap();
        assert_eq!(driver.session.ordered_chats()[0].id.0, 7);

        // In-flight loadChats: ingest of unrelated updates must not send another page.
        let loads_before = recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .count();
        assert_eq!(loads_before, 1);

        let extra = driver
            .session
            .requests
            .has_purpose(RequestPurpose::LoadChats);
        assert!(extra);
        let load_ok = copy_and_parse(r#"{"@type":"ok","@extra":"1"}"#, &seq, &dyn_sink).unwrap();
        driver.ingest(load_ok).unwrap();
        // @extra "1" may not match the loadChats extra (kickoff wasn't called, first request is LoadChats extra 1).
        // If it matched, a second loadChats is sent until 404.
        let loads_after_ok = recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .count();
        assert!(loads_after_ok >= 1);

        // Exhaust with the current pending extra if any.
        if driver
            .session
            .requests
            .has_purpose(RequestPurpose::LoadChats)
        {
            // Find the extra from the last loadChats JSON.
            let last_load = recorder
                .snapshot()
                .into_iter()
                .rev()
                .find(|j| j.contains("loadChats"))
                .unwrap();
            let v: Value = serde_json::from_str(&last_load).unwrap();
            let extra = v["@extra"].as_str().unwrap();
            let err404 = copy_and_parse(
                &format!(
                    r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{extra}"}}"#
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap();
            driver.ingest(err404).unwrap();
        }
        assert!(driver.session.chats_exhausted);
        let loads_done = recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .count();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            recorder
                .snapshot()
                .iter()
                .filter(|j| j.contains("loadChats"))
                .count(),
            loads_done
        );
        assert!(!sink.rendered().contains("Not Found"));

        let history_extra = driver.select_chat(ChatId(7)).unwrap().expect("history");
        let sent = recorder.snapshot();
        let history_json = sent.last().unwrap();
        assert!(history_json.contains("getChatHistory"));
        assert!(history_json.contains("\"chat_id\":7"));
        assert!(history_json.contains(&format!("\"@extra\":\"{}\"", history_extra.0)));

        let messages = copy_and_parse(
            &format!(
                r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":11,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}]}}"#,
                history_extra.0
            ),
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(messages).unwrap();
        assert!(
            driver
                .session
                .histories
                .get(&7)
                .unwrap()
                .messages
                .contains_key(&11)
        );

        let snap = crate::composer::ComposerSnapshot::capture(
            ChatId(7),
            driver.session.view_generation,
            "CANARY_SEND_ping",
        );
        let send_extra = driver.send_text_snapshot(&snap).unwrap();
        let sent = recorder.snapshot();
        let send_json = sent.last().unwrap();
        assert!(send_json.contains("sendMessage"));
        assert!(send_json.contains("\"topic_id\":null"));
        assert!(send_json.contains("CANARY_SEND_ping"));
        assert!(send_json.contains(&format!("\"@extra\":\"{}\"", send_extra.0)));
        assert!(!sink.rendered().contains("CANARY_SEND"));

        let gated = copy_and_parse(
            r#"{"@type":"updateNewChat","chat":{"id":8,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":8,"is_channel":true},"unread_count":0}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(gated).unwrap();
        let gated_snap = crate::composer::ComposerSnapshot::capture(
            ChatId(8),
            driver.session.view_generation,
            "nope",
        );
        assert_eq!(
            driver.send_text_snapshot(&gated_snap),
            Err(ConnectSendError::InvalidRequest)
        );
        assert!(!sink.rendered().contains("CANARY_SEND"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn send_text_rejected_when_empty_or_no_open_chat() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let snap = crate::composer::ComposerSnapshot::capture(
            ChatId(1),
            driver.session.view_generation,
            "   ",
        );
        assert_eq!(
            driver.send_text_snapshot(&snap),
            Err(ConnectSendError::InvalidRequest)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
