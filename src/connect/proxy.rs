//! Connect driver: proxy settings (`parity:proxy-settings`). Every proxy
//! request may be sent before authorization (schema 1.8.67 :16202), so
//! the gate is "not shutting down", not "auth Ready".
use super::*;
use crate::ids::RequestId;
use crate::proxy::{PingStatus, ProxyDraft, RotationAction};
use crate::settings::save_proxy_prefs;
use crate::state::{LINK_PING_ID, RequestPurpose, ShutdownPhase};
use crate::telegram::envelope::ConnectionState;
use crate::telegram::requests::{
    add_proxy, disable_proxy, edit_proxy, enable_proxy, get_proxies, ping_proxy, remove_proxy,
    set_prefer_ipv6,
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
        let proxy = &self.session.proxy;
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
        self.session.proxy.loading = true;
        self.session.proxy.fetch_attempted = true;
        self.session.proxy.error = None;
        match self.sender.send_json(&get_proxies(extra)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.proxy.loading = false;
                Err(err)
            }
        }
    }

    /// Refetch after a mutation marked the list stale (called from
    /// `ingest`, the `refresh_*_if_stale` pattern).
    pub fn refresh_proxies_if_stale(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.proxy.stale {
            return Ok(None);
        }
        self.maybe_fetch_proxies()
    }

    fn send_proxy_mutation(
        &mut self,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.proxy_path_active() || self.session.proxy.mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::MutateProxy, None);
        self.session.proxy.mutating = true;
        self.session.proxy.error = None;
        match self.sender.send_json(&build(extra)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.proxy.mutating = false;
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
        if self.session.proxy.find(proxy_id).is_none() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.proxy.pings.remove(&proxy_id);
        self.send_proxy_mutation(|extra| edit_proxy(extra, proxy_id, draft, enable, comment))
    }

    /// `enableProxy` (TDLib keeps one enabled at a time).
    pub fn enable_proxy(&mut self, proxy_id: i32) -> Result<RequestId, ConnectSendError> {
        if self.session.proxy.find(proxy_id).is_none() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_proxy_mutation(|extra| enable_proxy(extra, proxy_id))
    }

    /// `disableProxy` — back to a direct connection.
    pub fn disable_proxy(&mut self) -> Result<RequestId, ConnectSendError> {
        self.send_proxy_mutation(disable_proxy)
    }

    pub fn remove_proxy(&mut self, proxy_id: i32) -> Result<RequestId, ConnectSendError> {
        if self.session.proxy.find(proxy_id).is_none() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.proxy.pings.remove(&proxy_id);
        self.send_proxy_mutation(|extra| remove_proxy(extra, proxy_id))
    }

    /// `pingProxy` for a list entry.
    pub fn ping_listed_proxy(&mut self, proxy_id: i32) -> Result<RequestId, ConnectSendError> {
        let draft = self
            .session
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
        let extra = self
            .session
            .request(RequestPurpose::PingProxy { proxy_id: slot }, None);
        self.session.proxy.pings.insert(slot, PingStatus::Checking);
        match self.sender.send_json(&ping_proxy(extra, draft)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.proxy.pings.remove(&slot);
                Err(err)
            }
        }
    }

    /// `setOption("prefer_ipv6")`.
    pub fn set_prefer_ipv6(&mut self, on: bool) -> Result<RequestId, ConnectSendError> {
        if !self.proxy_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetPreferIpv6 { on }, None);
        self.session.proxy.error = None;
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
        save_proxy_prefs(&self.paths, &self.session.proxy.prefs)
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
        match self.session.proxy.rotation_step(now_ms, connected) {
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
