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
        let proxy = &mut self.proxy;
        proxy
            .pings
            .retain(|id, _| *id == LINK_PING_ID || proxies.iter().any(|p| p.id == *id));
        proxy.list = Some(proxies);
        proxy.loading = false;
        proxy.stale = false;
        proxy.error = None;
    }

    /// `addProxy` / `editProxy` answer.
    pub(crate) fn apply_added_proxy(&mut self, pending: Option<&PendingRequest>) {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::MutateProxy) {
            self.proxy_mutation_succeeded();
        }
    }

    fn proxy_mutation_succeeded(&mut self) {
        self.proxy.mutating = false;
        self.proxy.stale = true;
        self.proxy.error = None;
    }

    /// `pingProxy` answer, in seconds.
    pub(crate) fn apply_proxy_ping(&mut self, pending: Option<&PendingRequest>, seconds: f64) {
        if let Some(RequestPurpose::PingProxy { proxy_id }) = pending.map(|p| p.purpose) {
            let ms = (seconds * 1000.0).round().clamp(0.0, f64::from(u32::MAX)) as u32;
            self.proxy.pings.insert(proxy_id, PingStatus::Available(ms));
        }
    }

    /// `ok` for the proxy purposes.
    pub(crate) fn apply_proxy_ok(&mut self, pending: Option<&PendingRequest>) {
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::MutateProxy) => self.proxy_mutation_succeeded(),
            Some(RequestPurpose::SetPreferIpv6 { on }) => self.proxy.prefer_ipv6 = on,
            _ => {}
        }
    }

    /// Error for the proxy purposes.
    pub(crate) fn apply_proxy_error(&mut self, purpose: RequestPurpose, err: &TdError) {
        match purpose {
            RequestPurpose::GetProxies => {
                self.proxy.loading = false;
                self.proxy.stale = false;
                self.proxy.error = Some(sessions_error_line("load the proxy list", err));
            }
            RequestPurpose::MutateProxy => {
                self.proxy.mutating = false;
                self.proxy.error = Some(sessions_error_line("change the proxy", err));
            }
            // A failed ping is the answer: the proxy is not available.
            RequestPurpose::PingProxy { proxy_id } => {
                self.proxy.pings.insert(proxy_id, PingStatus::Unavailable);
            }
            RequestPurpose::SetPreferIpv6 { .. } => {
                self.proxy.error = Some(sessions_error_line("change the IPv6 setting", err));
            }
            _ => {}
        }
    }
}
