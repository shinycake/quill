//! Connect gate: live TDLib connection bootstrap and shutdown.
use super::*;
use crate::accounts::AccountRegistry;
use crate::credentials::TelegramCredentials;
use crate::data_settings::load_data_storage_prefs;
use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::ids::AccountKey;
use crate::platform::SecretStore;
use crate::settings::{
    load_badge_prefs, load_call_prefs, load_contact_prefs, load_media_prefs, load_preferences,
    safe_app_root,
};
use crate::state::Session;
use crate::telegram::client::{LiveTdJson, OwnedEnvelope, ReceiveBridge};
use crate::telegram::envelope::AuthorizationState;
use crate::telegram::ffi::TdJsonError;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    account: &AccountKey,
) -> Result<LiveConnect, ConnectBlocker> {
    match evaluate_gate(true) {
        ConnectGate::Blocked(b) => return Err(b),
        ConnectGate::Ready { .. } => {}
    }
    let app_root = safe_app_root().ok_or(ConnectBlocker::LockedStore)?;
    let prepared = prepare_connect(&app_root, account.clone(), store, &credentials)?;
    let live = LiveTdJson::connect().map_err(|e| match e {
        TdJsonError::NotFound => ConnectBlocker::MissingTdjson,
        _ => ConnectBlocker::TdjsonLoad,
    })?;
    let sender = LiveSender::from_live(&live);
    let bridge = ReceiveBridge::spawn_live(live.api.clone(), diagnostics.clone());
    let session = Session::new(prepared.account.clone(), diagnostics.clone());
    // Phase C2i: local call prefs (confirm-before-calling, less-data)
    // are loaded once here; the UI saves them back on toggle.
    let mut session = session;
    session.call_prefs = load_call_prefs(&prepared.paths);
    // MED1: local media prefs (remember-media-grouping) load the same way.
    session.media_prefs = load_media_prefs(&prepared.paths);
    // Slice A6: local contacts prefs (sync toggle) load the same way.
    session.contact_prefs = load_contact_prefs(&prepared.paths);
    // Slice parity:chatlist-badge-settings: local badge-counter prefs
    // load the same way.
    session.badge_prefs = load_badge_prefs(&prepared.paths);
    // Parity slice: in-app notification sounds toggle (tdesktop "Play
    // sounds") loads the same way.
    session.inapp_sounds_enabled = load_preferences(&prepared.paths).inapp_sounds_enabled;
    // Slice S4: local per-network auto-download settings load the same
    // way (seeded from `getAutoDownloadSettingsPresets` on first open
    // when no file exists).
    session.data_storage = load_data_storage_prefs(&prepared.paths);
    let mut driver = ConnectDriver::new(session, sender, credentials, prepared);
    match crate::calls::engine::NtgcallsEngine::load() {
        Ok(engine) => {
            driver.set_call_engine(Box::new(engine));
            diagnostics.record(Diagnostic {
                category: "call",
                type_name: None,
                extra: None,
                seq: None,
                note: "call-engine-ready",
            });
        }
        Err(_) => diagnostics.record(Diagnostic {
            category: "call",
            type_name: None,
            extra: None,
            seq: None,
            note: "call-engine-unavailable-signaling-only",
        }),
    }
    driver.kickoff().map_err(|_| ConnectBlocker::TdjsonLoad)?;
    Ok(LiveConnect {
        driver,
        bridge,
        _live: live,
    })
}

/// Multi-account part 2: switch the live client to another known account.
///
/// The account must be in the registry — an unknown key is rejected with
/// `UnknownAccount` and the current client is left untouched. Otherwise the
/// old client is shut down (dropped) *before* the new one starts, the
/// registry's current account is persisted, and a fresh client boots under
/// the new key (a never-authorized key lands on the auth screens, which is
/// how accounts get added).
///
/// If persisting the registry fails, the switch still happened in memory and
/// the error is `StoreError` — the next launch will revert to the previously
/// persisted current account.
pub fn switch_live_account(
    live: &mut Option<LiveConnect>,
    account: &AccountKey,
    app_root: &Path,
    credentials: TelegramCredentials,
    store: &(impl SecretStore + ?Sized),
    diagnostics: Arc<dyn DiagnosticSink>,
) -> Result<(), ConnectBlocker> {
    let mut registry = AccountRegistry::load(app_root);
    if registry.get(account).is_none() {
        return Err(ConnectBlocker::UnknownAccount);
    }
    // Shut down the old client before the new one starts.
    *live = None;
    registry.set_current(account);
    let save_err = registry.save(app_root).err();
    *live = Some(start_live_connect(
        credentials,
        store,
        diagnostics,
        account,
    )?);
    if save_err.is_some() {
        return Err(ConnectBlocker::StoreError);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::MemorySink;
    use crate::platform::MemorySecretStore;

    fn test_credentials() -> TelegramCredentials {
        TelegramCredentials {
            api_id: 99,
            api_hash: "unit-test-hash-not-for-network".into(),
        }
    }

    fn tmp_root() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "quill-switch-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Multi-account part 2: switching to an unknown account is rejected
    /// before the current client is touched (no live TDLib needed — the
    /// rejection happens before any shutdown or connect).
    #[test]
    fn switch_to_unknown_account_rejected_and_client_untouched() {
        let dir = tmp_root();
        let store = MemorySecretStore::new();
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let mut live: Option<LiveConnect> = None;
        let ghost = AccountKey("ghost".to_string());
        let err = switch_live_account(&mut live, &ghost, &dir, test_credentials(), &store, sink)
            .unwrap_err();
        assert!(matches!(err, ConnectBlocker::UnknownAccount));
        assert!(live.is_none());
        // The failed switch wrote nothing.
        assert!(!dir.join("accounts.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
