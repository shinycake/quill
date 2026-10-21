//! Reducer half of the proxy settings (`parity:proxy-settings`): applies
//! `addedProxies` / `addedProxy` / `seconds` / `ok` / errors onto
//! `Session::proxy`. Mutations never touch the list optimistically: the
//! answer marks it stale and the driver refetches `getProxies`.
use super::*;
use crate::proxy::PingStatus;

/// Ping slot used by the "enable this proxy?" link box, whose proxy is
/// not in the list (TDLib ids are non-negative).
pub const LINK_PING_ID: i32 = -1;

impl Session {
    /// `getProxies` answer: replaces the list; pings of proxies that no
    /// longer exist are dropped.
    pub(crate) fn apply_added_proxies(
        &mut self,
        pending: Option<&PendingRequest>,
        proxies: Vec<crate::proxy::ProxyEntry>,
    ) {
        if pending.map(|p| p.purpose) != Some(RequestPurpose::GetProxies) {
            return;
        }
        let proxy = &mut self.settings.proxy;
        proxy
            .pings
            .retain(|id, _| *id == LINK_PING_ID || proxies.iter().any(|p| p.id == *id));
        // The system proxy's entries stay out of the list (never listed,
        // shared or rotated).
        let (system, list) = proxies
            .into_iter()
            .partition(crate::system_proxy::is_system_entry);
        proxy.system = system;
        let enabled = list.iter().find(|p| p.is_enabled).map(|p| p.id);
        if proxy.carrier_failed != enabled {
            proxy.carrier_failed = None;
        }
        proxy.list = Some(list);
        proxy.loading = false;
        proxy.stale = false;
        proxy.error = None;
    }

    /// `updateWebProxyCarrierOpened`: host it, or refuse it when this
    /// client has no carrier host (`crate::proxy_carrier`). A temporary
    /// carrier (ping / test) is refused and is not a failure of the saved
    /// proxy.
    pub(crate) fn apply_web_proxy_carrier(
        &mut self,
        carrier: crate::proxy_carrier::CarrierRequest,
    ) {
        use crate::proxy_carrier::OpenVerdict;
        let proxy = &mut self.settings.proxy;
        match proxy.carriers.opened(carrier) {
            OpenVerdict::Hosting { proxy_id } => {
                if proxy.carrier_failed == Some(proxy_id) {
                    proxy.carrier_failed = None;
                }
            }
            OpenVerdict::Refused { proxy_id } => proxy.carrier_failed = Some(proxy_id),
            OpenVerdict::Temporary => {}
        }
    }

    /// `updateWebProxyCarrierClosed`.
    pub(crate) fn apply_web_proxy_carrier_closed(&mut self, carrier_id: i32) {
        self.settings.proxy.carriers.closed(carrier_id);
    }

    /// A carrier's host failed it (or could not start it): TDLib hears
    /// why, and the proxy's row says "not available" until TDLib tries
    /// again.
    pub fn apply_web_proxy_carrier_failed(
        &mut self,
        carrier_id: i32,
        error: crate::proxy_carrier::CarrierError,
    ) {
        let proxy = &mut self.settings.proxy;
        if let Some(proxy_id) = proxy.carriers.failed(carrier_id, error) {
            proxy.carrier_failed = Some(proxy_id);
        }
    }

    /// `addProxy` / `editProxy` answer.
    pub(crate) fn apply_added_proxy(&mut self, pending: Option<&PendingRequest>) {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::MutateProxy) {
            self.proxy_mutation_succeeded();
        }
    }

    fn proxy_mutation_succeeded(&mut self) {
        self.settings.proxy.mutating = false;
        self.settings.proxy.stale = true;
        self.settings.proxy.error = None;
    }

    /// `pingProxy` answer, in seconds.
    pub(crate) fn apply_proxy_ping(&mut self, pending: Option<&PendingRequest>, seconds: f64) {
        if let Some(RequestPurpose::Settings(SettingsPurpose::PingProxy { proxy_id })) =
            pending.map(|p| p.purpose)
        {
            let ms = (seconds * 1000.0).round().clamp(0.0, f64::from(u32::MAX)) as u32;
            self.settings
                .proxy
                .pings
                .insert(proxy_id, PingStatus::Available(ms));
        }
    }

    /// `ok` for the proxy purposes.
    pub(crate) fn apply_proxy_ok(&mut self, pending: Option<&PendingRequest>) {
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::MutateProxy) => self.proxy_mutation_succeeded(),
            Some(RequestPurpose::Settings(SettingsPurpose::SetPreferIpv6 { on })) => {
                self.settings.proxy.prefer_ipv6 = on
            }
            _ => {}
        }
    }

    /// Error for the proxy purposes.
    pub(crate) fn apply_proxy_error(&mut self, purpose: RequestPurpose, err: &TdError) {
        match purpose {
            RequestPurpose::GetProxies => {
                self.settings.proxy.loading = false;
                self.settings.proxy.stale = false;
                self.settings.proxy.error = Some(sessions_error_line("load the proxy list", err));
            }
            RequestPurpose::MutateProxy => {
                self.settings.proxy.mutating = false;
                self.settings.proxy.error = Some(sessions_error_line("change the proxy", err));
            }
            // A failed ping is the answer: the proxy is not available.
            RequestPurpose::Settings(SettingsPurpose::PingProxy { proxy_id }) => {
                self.settings
                    .proxy
                    .pings
                    .insert(proxy_id, PingStatus::Unavailable);
            }
            RequestPurpose::Settings(SettingsPurpose::SetPreferIpv6 { .. }) => {
                self.settings.proxy.error =
                    Some(sessions_error_line("change the IPv6 setting", err));
            }
            _ => {}
        }
    }
}
