//! Connect gate: live TDLib connection bootstrap and shutdown.
use super::*;
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
) -> Result<LiveConnect, ConnectBlocker> {
    let app_root = safe_app_root().ok_or(ConnectBlocker::LockedStore)?;
    start_live_connect_for_account(
        credentials,
        store,
        diagnostics,
        crate::settings::active_account(&app_root),
    )
}

/// Connect as a specific account. This is the account-switching seam: the
/// account-switcher UI calls [`LiveConnect::shutdown`] on the current client
/// and then this with the new key (plus
/// [`crate::settings::set_active_account`] to persist it for next startup).
pub fn start_live_connect_for_account(
    credentials: TelegramCredentials,
    store: &(impl SecretStore + ?Sized),
    diagnostics: Arc<dyn DiagnosticSink>,
    account: AccountKey,
) -> Result<LiveConnect, ConnectBlocker> {
    match evaluate_gate(true) {
        ConnectGate::Blocked(b) => return Err(b),
        ConnectGate::Ready { .. } => {}
    }
    let app_root = safe_app_root().ok_or(ConnectBlocker::LockedStore)?;
    let prepared = prepare_connect(&app_root, account, store, &credentials)?;
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
