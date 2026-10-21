//! Connect driver: proxy settings (`parity:proxy-settings`). Every proxy
//! request may be sent before authorization (schema 1.8.67 :16202), so
//! the gate is "not shutting down", not "auth Ready".
use super::*;
use crate::calls::proxy::ProxyKind;
use crate::ids::RequestId;
use crate::proxy::{PingStatus, ProxyDraft, ProxyEntry, RotationAction};
use crate::proxy_sync::{ProxySyncStep, plan_proxy_sync};
use crate::settings::save_proxy_prefs;
use crate::state::SettingsPurpose;
use crate::state::{LINK_PING_ID, RequestPurpose, ShutdownPhase};
use crate::system_proxy::{Detected, SYSTEM_PROXY_COMMENT, SystemProxyStep, plan_system_proxy};
use crate::telegram::envelope::ConnectionState;
use crate::telegram::requests::{
    add_proxy, disable_proxy, edit_proxy, enable_proxy, get_proxies, ping_proxy, remove_proxy,
    report_web_proxy_carrier_error, set_prefer_ipv6,
};

impl<S: JsonSender> ConnectDriver<S> {
    fn proxy_path_active(&self) -> bool {
        matches!(self.session.shutdown, ShutdownPhase::Running)
    }

    /// `getProxies` — once per session when nothing is cached (the
    /// connection strip needs to know whether a proxy is enabled), and
    /// again whenever a mutation marked the list stale.
    pub fn maybe_fetch_proxies(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.proxy_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let proxy = &self.session.settings.proxy;
        let wanted = (proxy.list.is_none() && !proxy.fetch_attempted) || proxy.stale;
        if !wanted
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetProxies)
        {
            return Ok(None);
        }
        self.fetch_proxies().map(Some)
    }

    /// Explicit refresh (the dialog's open / Refresh).
    pub fn fetch_proxies(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.proxy_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::GetProxies, None);
        self.session.settings.proxy.loading = true;
        self.session.settings.proxy.fetch_attempted = true;
        self.session.settings.proxy.error = None;
        match self.sender.send_json(&get_proxies(extra)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.settings.proxy.loading = false;
                Err(err)
            }
        }
    }

    /// Refetch after a mutation marked the list stale (called from
    /// `ingest`, the `refresh_*_if_stale` pattern).
    pub fn refresh_proxies_if_stale(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        self.refuse_web_proxy_carriers();
        if !self.session.settings.proxy.stale {
            return Ok(None);
        }
        self.maybe_fetch_proxies()
    }

    /// Answer every pending `updateWebProxyCarrierOpened`: Quill cannot
    /// host the relay page, so TDLib is told to stop asking.
    fn refuse_web_proxy_carriers(&mut self) {
        let reports = std::mem::take(&mut self.session.settings.proxy.carrier_reports);
        for (carrier_id, temporary) in reports {
            let extra = self.session.request(
                RequestPurpose::Settings(SettingsPurpose::ReportWebProxyCarrier),
                None,
            );
            let sent = self.sender.send_json(&report_web_proxy_carrier_error(
                extra, carrier_id, temporary,
            ));
            if sent.is_err() {
                self.session.requests.take(extra);
            }
        }
    }

    fn send_proxy_mutation(
        &mut self,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.proxy_path_active() || self.session.settings.proxy.mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::MutateProxy, None);
        self.session.settings.proxy.mutating = true;
        self.session.settings.proxy.error = None;
        match self.sender.send_json(&build(extra)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.settings.proxy.mutating = false;
                Err(err)
            }
        }
    }

    /// `addProxy`; `enable` switches to it at once. The draft must have
    /// passed `ProxyDraft::validated`.
    pub fn add_proxy(
        &mut self,
        draft: &ProxyDraft,
        enable: bool,
        comment: &str,
    ) -> Result<RequestId, ConnectSendError> {
        // Quill cannot host a web proxy's carrier yet: it is saved, never
        // switched to (`crate::proxy_web`).
        let enable = enable && draft.kind != ProxyKind::Web;
        self.send_proxy_mutation(|extra| add_proxy(extra, draft, enable, comment))
    }

    /// `editProxy` of an existing entry.
    pub fn edit_proxy(
        &mut self,
        proxy_id: i32,
        draft: &ProxyDraft,
        enable: bool,
        comment: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if self.session.settings.proxy.find(proxy_id).is_none() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // TDLib turns the edited proxy back on when it is the active one
        // (`ConnectionCreator::add_proxy`), which would leave the user
        // offline behind a web proxy Quill cannot carry: refuse it.
        let edits_enabled = self
            .session
            .settings
            .proxy
            .find(proxy_id)
            .is_some_and(|p| p.is_enabled);
        if draft.kind == ProxyKind::Web && edits_enabled {
            return Err(ConnectSendError::InvalidRequest);
        }
        let enable = enable && draft.kind != ProxyKind::Web;
        self.session.settings.proxy.pings.remove(&proxy_id);
        self.send_proxy_mutation(|extra| edit_proxy(extra, proxy_id, draft, enable, comment))
    }

    /// `enableProxy` (TDLib keeps one enabled at a time).
    pub fn enable_proxy(&mut self, proxy_id: i32) -> Result<RequestId, ConnectSendError> {
        let enableable = self
            .session
            .settings
            .proxy
            .find(proxy_id)
            .is_some_and(|p| p.proxy.kind.can_be_enabled());
        if !enableable {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_proxy_mutation(|extra| enable_proxy(extra, proxy_id))
    }

    /// `disableProxy` — back to a direct connection.
    pub fn disable_proxy(&mut self) -> Result<RequestId, ConnectSendError> {
        self.send_proxy_mutation(disable_proxy)
    }

    pub fn remove_proxy(&mut self, proxy_id: i32) -> Result<RequestId, ConnectSendError> {
        if self.session.settings.proxy.find(proxy_id).is_none() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.settings.proxy.pings.remove(&proxy_id);
        self.send_proxy_mutation(|extra| remove_proxy(extra, proxy_id))
    }

    /// `pingProxy` for a list entry.
    pub fn ping_listed_proxy(&mut self, proxy_id: i32) -> Result<RequestId, ConnectSendError> {
        let draft = self
            .session
            .settings
            .proxy
            .find(proxy_id)
            .map(|p| p.proxy.clone())
            .ok_or(ConnectSendError::InvalidRequest)?;
        self.ping_draft(proxy_id, &draft)
    }

    /// `pingProxy` for the link-confirmation box's proxy (not in the
    /// list; result kept under [`LINK_PING_ID`]).
    pub fn ping_link_proxy(&mut self, draft: &ProxyDraft) -> Result<RequestId, ConnectSendError> {
        self.ping_draft(LINK_PING_ID, draft)
    }

    fn ping_draft(&mut self, slot: i32, draft: &ProxyDraft) -> Result<RequestId, ConnectSendError> {
        if !self.proxy_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::PingProxy { proxy_id: slot }),
            None,
        );
        self.session
            .settings
            .proxy
            .pings
            .insert(slot, PingStatus::Checking);
        match self.sender.send_json(&ping_proxy(extra, draft)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.settings.proxy.pings.remove(&slot);
                Err(err)
            }
        }
    }

    /// `setOption("prefer_ipv6")`.
    pub fn set_prefer_ipv6(&mut self, on: bool) -> Result<RequestId, ConnectSendError> {
        if !self.proxy_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::SetPreferIpv6 { on }),
            None,
        );
        self.session.settings.proxy.error = None;
        match self.sender.send_json(&set_prefer_ipv6(extra, on)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Persist the auto-switch preferences (`proxy_prefs.json`).
    pub fn save_proxy_prefs(&mut self) -> std::io::Result<()> {
        save_proxy_prefs(&self.paths, &self.session.settings.proxy.prefs)
    }

    /// Re-read the app-wide auto-switch settings: another account may have
    /// changed them while this one was in the background.
    pub fn reload_proxy_prefs(&mut self) {
        self.session.settings.proxy.prefs = crate::settings::load_proxy_prefs(&self.paths);
    }

    /// A background account's proxy work (`crate::proxy_sync`): its first
    /// `getProxies`, then one step toward the app's list (`target`, the
    /// list of the account on screen) and IPv6 choice. Each change marks
    /// the list stale and `ingest` refetches it, so the next call plans on
    /// the fresh list. No auto-switch here: the account on screen runs it
    /// and its pick is mirrored. Returns whether a request went out.
    pub fn sync_proxies_to(&mut self, target: &[ProxyEntry], prefer_ipv6: bool) -> bool {
        if !self.proxy_path_active() {
            return false;
        }
        if self.maybe_fetch_proxies().ok().flatten().is_some() {
            return true;
        }
        let proxy = &self.session.settings.proxy;
        if proxy.mutating
            || proxy.stale
            || proxy.loading
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetProxies)
        {
            return false;
        }
        let Some(current) = proxy.all_entries() else {
            return false;
        };
        let step = plan_proxy_sync(target, &current).into_iter().next();
        let ipv6_differs = proxy.prefer_ipv6 != prefer_ipv6
            && !self.session.requests.has_purpose(RequestPurpose::Settings(
                SettingsPurpose::SetPreferIpv6 { on: prefer_ipv6 },
            ));
        let mut sent = ipv6_differs && self.set_prefer_ipv6(prefer_ipv6).is_ok();
        sent |= match step {
            None => false,
            Some(ProxySyncStep::Add {
                proxy,
                comment,
                enable,
            }) => self.add_proxy(&proxy, enable, &comment).is_ok(),
            Some(ProxySyncStep::Remove { id }) => self.remove_proxy(id).is_ok(),
            Some(ProxySyncStep::Enable { id }) => self.enable_proxy(id).is_ok(),
            Some(ProxySyncStep::Disable) => self.disable_proxy().is_ok(),
        };
        sent
    }

    /// "Use system proxy settings" (`crate::system_proxy`): one step that
    /// brings TDLib's list in line with the mode and what the OS reports
    /// (`detected`, read off the UI thread). For the account on screen;
    /// background accounts mirror its list. Returns whether a request
    /// went out.
    pub fn system_proxy_tick(&mut self, detected: &Detected) -> bool {
        if !self.proxy_path_active() {
            return false;
        }
        let proxy = &self.session.settings.proxy;
        if proxy.list.is_none()
            || proxy.mutating
            || proxy.stale
            || proxy.loading
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetProxies)
        {
            return false;
        }
        let step = plan_system_proxy(
            proxy.prefs.use_system,
            proxy.enabled().is_some(),
            detected,
            &proxy.system,
        );
        // The last step finished (nothing is mutating): an error means
        // TDLib refused it. The user never asked for it, so the proxy box
        // doesn't show that error, and the step waits `RETRY` before it is
        // sent again.
        if let Some(sent) = self.system_proxy_sent.as_mut()
            && !sent.settled
        {
            sent.settled = true;
            sent.refused = self.session.settings.proxy.error.take().is_some();
        }
        let Some(step) = step else {
            self.system_proxy_sent = None;
            return false;
        };
        let now = std::time::Instant::now();
        if let Some(sent) = &self.system_proxy_sent
            && sent.refused
            && sent.step == step
            && now.saturating_duration_since(sent.at) < crate::system_proxy::RETRY
        {
            return false;
        }
        self.session.settings.proxy.error = None;
        self.system_proxy_sent = Some(crate::system_proxy::SentStep {
            step: step.clone(),
            at: now,
            refused: false,
            settled: false,
        });
        match step {
            SystemProxyStep::Add(draft) => {
                self.add_proxy(&draft, true, SYSTEM_PROXY_COMMENT).is_ok()
            }
            SystemProxyStep::Edit { id, proxy } => self
                .edit_proxy(id, &proxy, true, SYSTEM_PROXY_COMMENT)
                .is_ok(),
            SystemProxyStep::Enable { id } => self.enable_proxy(id).is_ok(),
            SystemProxyStep::Remove { id } => self.remove_proxy(id).is_ok(),
        }
    }

    /// Pick a proxy mode in the proxy box. "Disable proxy" and "Use system
    /// proxy settings" turn a custom proxy off (TDLib then connects
    /// directly until [`Self::system_proxy_tick`] enables the system
    /// one); "Use custom proxy" is the caller's (it enables a list entry).
    pub fn set_proxy_mode(&mut self, use_system: bool) -> Result<(), ConnectSendError> {
        self.session.settings.proxy.prefs.use_system = use_system;
        let _ = self.save_proxy_prefs();
        let proxy = &self.session.settings.proxy;
        let anything_on =
            proxy.enabled().is_some() || (!use_system && proxy.system.iter().any(|p| p.is_enabled));
        if anything_on {
            self.disable_proxy()?;
        }
        Ok(())
    }

    /// Per-poll proxy work: the first `getProxies`, then the auto-switch
    /// machine (tdesktop `ProxyRotationManager`). Returns whether the
    /// state changed in a way the UI should redraw for.
    pub fn proxy_tick(&mut self, now_ms: u64) -> bool {
        if !self.proxy_path_active() {
            return false;
        }
        let mut changed = self.maybe_fetch_proxies().ok().flatten().is_some();
        let connected = matches!(
            self.session.connection,
            ConnectionState::Ready | ConnectionState::Updating
        );
        match self.session.settings.proxy.rotation_step(now_ms, connected) {
            RotationAction::None => {}
            RotationAction::Ping(id) => {
                changed |= self.ping_listed_proxy(id).is_ok();
            }
            RotationAction::Switch(id) => {
                changed |= self.enable_proxy(id).is_ok();
            }
        }
        changed
    }
}
