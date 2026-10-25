//! Connect gate: live TDLib connection bootstrap and shutdown.
use super::closing::{Closable, CloseProgress, QUIT_CLOSE_DEADLINE};
use super::*;
use crate::credentials::TelegramCredentials;
use crate::data_settings::load_data_storage_prefs;
use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::ids::AccountKey;
use crate::platform::SecretStore;
use crate::settings::{
    load_badge_prefs, load_call_prefs, load_contact_prefs, load_language_prefs, load_media_prefs,
    load_preferences, safe_app_root,
};
use crate::state::Session;
use crate::telegram::client::{LiveTdJson, OwnedEnvelope, ReceiveBridge};
use crate::telegram::envelope::AuthorizationState;
use crate::telegram::ffi::TdJsonError;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long `--connect-smoke` waits for `authorizationStateClosed`, and how
/// long a closing client is drained in the background ([`CLOSE_GRACE`]).
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
    super::closing::begin_close(driver);
    let deadline = super::closing::deadline_after(timeout);
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
/// The app closes clients through a [`ClosingSet`] (never blocking).
/// Drop / [`LiveConnect::shutdown`] (CLI tools, last resort) send `close`, wait for
/// `authorizationStateClosed`, then remove the client's route from the
/// shared receive hub (`telegram::hub`). The hub, and with it `libtdjson`,
/// lives as long as the process: unloading the library while TDLib worker
/// threads were still running is what produced SIGSEGV (exit 139) after
/// `--connect-smoke`, and several accounts now share it.
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
        // A dead receive loop can never deliver Closed; do not wait for it.
        if self.bridge.stopped_unexpectedly() {
            self.bridge.shutdown();
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
    /// Last resort: the app hands clients to a [`ClosingSet`] first, which
    /// leaves them joined, so this returns at once. Bounded by the quit
    /// deadline, not the 5 s CLI wait.
    fn drop(&mut self) {
        self.shutdown(QUIT_CLOSE_DEADLINE);
    }
}

impl Closable for LiveConnect {
    fn begin_close(&mut self) {
        super::closing::begin_close_bridged(&mut self.driver, &self.bridge);
    }

    fn drain(&mut self, budget: Duration) -> CloseProgress {
        super::closing::drain_bridged(&mut self.driver, &self.bridge, budget)
    }

    fn finish(&mut self) {
        self.bridge.shutdown();
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
    start_prepared_live_connect(credentials, prepared, diagnostics)
}

/// Build the native client after account preparation. UI callers can resolve
/// the database key on a worker without moving the thread-bound call engine.
pub fn start_prepared_live_connect(
    credentials: TelegramCredentials,
    prepared: PreparedConnect,
    diagnostics: Arc<dyn DiagnosticSink>,
) -> Result<LiveConnect, ConnectBlocker> {
    let mut live = open_prepared_client(credentials, prepared, diagnostics.clone())?;
    live.attach_call_engine(diagnostics.as_ref());
    live.driver
        .kickoff()
        .map_err(|_| ConnectBlocker::TdjsonLoad)?;
    Ok(live)
}

/// A client for an account that is not on screen (tdesktop keeps every
/// account's session connected): the same session restore, but no call
/// engine until the account is switched to ([`LiveConnect::attach_call_engine`]).
pub fn start_background_live_connect(
    credentials: TelegramCredentials,
    prepared: PreparedConnect,
    diagnostics: Arc<dyn DiagnosticSink>,
) -> Result<LiveConnect, ConnectBlocker> {
    let mut live = open_prepared_client(credentials, prepared, diagnostics)?;
    live.driver
        .kickoff()
        .map_err(|_| ConnectBlocker::TdjsonLoad)?;
    Ok(live)
}

impl LiveConnect {
    /// Load and install the native call engine (once; a client that already
    /// has one keeps it). Without the library calls run signaling-only.
    pub fn attach_call_engine(&mut self, diagnostics: &dyn DiagnosticSink) {
        if self.driver.has_call_engine() {
            return;
        }
        match crate::calls::engine::NtgcallsEngine::load() {
            Ok(engine) => {
                self.driver.set_call_engine(Box::new(engine));
                self.driver.start_engine_for_tracked_call();
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
    }
}

/// The client, its route on the shared receive hub and a session with the
/// account's local prefs; nothing is sent yet.
fn open_prepared_client(
    credentials: TelegramCredentials,
    prepared: PreparedConnect,
    diagnostics: Arc<dyn DiagnosticSink>,
) -> Result<LiveConnect, ConnectBlocker> {
    let live = LiveTdJson::connect(diagnostics.clone()).map_err(|e| match e {
        TdJsonError::NotFound => ConnectBlocker::MissingTdjson,
        TdJsonError::VersionMismatch { .. } => ConnectBlocker::TdjsonVersion,
        _ => ConnectBlocker::TdjsonLoad,
    })?;
    let sender = LiveSender::from_live(&live);
    crate::agent::td::register_client(&prepared.account, &live);
    // Routed before the first request, so no envelope can arrive unrouted.
    let bridge = live.bridge();
    let session = Session::new(prepared.account.clone(), diagnostics.clone());
    // Phase C2i: local call prefs (confirm-before-calling, less-data)
    // are loaded once here; the UI saves them back on toggle.
    let mut session = session;
    session.calls.prefs = load_call_prefs(&prepared.paths);
    session.settings.proxy.prefs = crate::settings::load_proxy_prefs(&prepared.paths);
    // MED1: local media prefs (remember-media-grouping) load the same way.
    session.settings.media_prefs = load_media_prefs(&prepared.paths);
    // Slice A6: local contacts prefs (sync toggle) load the same way.
    session.settings.contact_prefs = load_contact_prefs(&prepared.paths);
    // Slice parity:chatlist-badge-settings: local badge-counter prefs
    // load the same way.
    session.settings.badge_prefs = load_badge_prefs(&prepared.paths);
    // Parity slice: in-app notification sounds toggle (tdesktop "Play
    // sounds") loads the same way.
    let prefs = load_preferences(&prepared.paths);
    session.settings.inapp_sounds_enabled = prefs.inapp_sounds_enabled;
    session.settings.desktop_notifications = prefs.desktop_notifications;
    session.settings.notify_view = prefs.notify_view;
    session.settings.notify_photo_previews = prefs.notify_photo_previews;
    // Slice parity:settings-language: the app language tag load the same
    // way (defaults to "en" when unset).
    session.settings.language_prefs = load_language_prefs(&prepared.paths);
    session.settings.device_prefs = crate::settings::load_device_prefs(&prepared.paths);
    // Slice S4: local per-network auto-download settings load the same
    // way (seeded from `getAutoDownloadSettingsPresets` on first open
    // when no file exists).
    session.settings.data_storage = load_data_storage_prefs(&prepared.paths);
    // `parity:data-proxy-web`: web proxy carriers run in the web view
    // helper next to Quill's executable, with this account's web view
    // files under its own folder.
    let carrier_config = crate::proxy_carrier::helper::HelperConfig {
        program: crate::proxy_carrier::helper::helper_program(),
        data_dir: Some(prepared.paths.root.join("webview-carrier")),
        probe_cache: crate::settings::safe_app_root()
            .map(|root| root.join("webview-carrier-probe")),
        relay: sender.relay_sink(),
        capability: crate::proxy_carrier::helper::Capability::app(),
    };
    let mut driver = ConnectDriver::new(session, sender, credentials, prepared);
    driver.set_carrier_host(Box::new(
        crate::proxy_carrier::helper::HelperCarrierHost::new(carrier_config),
    ));
    Ok(LiveConnect {
        driver,
        bridge,
        _live: live,
    })
}
