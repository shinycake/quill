//! Reducer half of the proxy settings (`parity:proxy-settings`): applies
//! `addedProxies` / `addedProxy` / `seconds` / `ok` / errors onto
//! `Session::proxy`. Mutations never touch the list optimistically: the
//! answer marks it stale and the driver refetches `getProxies`.
use super::*;
use crate::proxy::PingStatus;

impl Session {
    /// `getProxies` answer: replaces the list; pings of proxies that no
    /// longer exist are dropped.
    pub(crate) fn apply_added_proxies(
        &mut self,
        pending: Option<&PendingRequest>,
        proxies: Vec<crate::proxy::ProxyEntry>,
    ) {
        let Some(pending) = pending.filter(|p| p.purpose == RequestPurpose::GetProxies) else {
            return;
        };
        let proxy = &mut self.settings.proxy;
        // An older fetch (sent before a change, or replaced) says nothing
        // about the list as it is now.
        if !proxy.ops.fetch_answered(pending.id) {
            return;
        }
        proxy
            .pings
            .retain(|id, _| proxies.iter().any(|p| p.id == *id));
        // The system proxy's entries stay out of the list (never listed,
        // shared or rotated).
        let (system, list) = proxies
            .into_iter()
            .partition(crate::system_proxy::is_system_entry);
        proxy.system = system;
        proxy.list = Some(list);
    }

    /// `updateWebProxyCarrierOpened`: host it, or refuse it when this
    /// client has no carrier host (`crate::proxy_carrier`). A temporary
    /// carrier (ping / test) is refused and is not a failure of the saved
    /// proxy.
    pub(crate) fn apply_web_proxy_carrier(
        &mut self,
        carrier: crate::proxy_carrier::CarrierRequest,
    ) {
        // A refused carrier (no host) shows through `can_enable`, a failed
        // one through `Carriers::failed_for`: the row has no copy to keep.
        let _ = self.settings.proxy.carriers.opened(carrier);
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
        let _ = self.settings.proxy.carriers.failed(carrier_id, error);
    }

    /// Whether this client can host carriers, as the host now says (a
    /// capability check finishing, or a carrier finding out it cannot
    /// run). Once it cannot, every new carrier is refused and web proxies
    /// are not switched to. Returns whether it changed.
    pub fn apply_carrier_hosting(&mut self, hosting: bool) -> bool {
        let carriers = &mut self.settings.proxy.carriers;
        let changed = carriers.hosting != hosting;
        carriers.hosting = hosting;
        changed
    }

    /// `addProxy` / `editProxy` answer.
    pub(crate) fn apply_added_proxy(&mut self, pending: Option<&PendingRequest>) {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::MutateProxy) {
            self.proxy_mutation_succeeded(pending);
        }
    }

    fn proxy_mutation_succeeded(&mut self, pending: Option<&PendingRequest>) {
        let proxy = &mut self.settings.proxy;
        let system = pending.is_some_and(|p| proxy.system_step.answered(p.id, false));
        // Only the change in flight unlocks the list (an answer of an older
        // one leaves the lock to its own), and a list fetch still out may
        // predate it: its answer is not the one to wait for any more, and
        // the refetch asks again.
        proxy.ops.change_succeeded(pending.map(|p| p.id));
        // The box's error line is the user's: the system step's success
        // does not wipe the error of the user's own failed change.
        if !system {
            proxy.error = None;
        }
    }

    /// Whether `pending` is the ping its slot waits for: the slot shows
    /// `Checking` under this very request (a slot cleared since, for an edit
    /// or a newer ping, waits for nothing).
    fn take_awaited_ping(&mut self, pending: Option<&PendingRequest>) -> Option<i32> {
        let pending = pending?;
        let RequestPurpose::Settings(SettingsPurpose::PingProxy { proxy_id }) = pending.purpose
        else {
            return None;
        };
        (self.settings.proxy.pings.get(&proxy_id) == Some(&PingStatus::Checking(pending.id)))
            .then_some(proxy_id)
    }

    /// The link box's check answered: only the request its `Checking`
    /// stage carries settles it.
    fn settle_link_check(&mut self, pending: Option<&PendingRequest>, status: PingStatus) {
        let Some(pending) = pending else { return };
        self.settings.proxy.settle_link(pending.id, status);
    }

    /// `pingProxy` answer, in seconds.
    pub(crate) fn apply_proxy_ping(&mut self, pending: Option<&PendingRequest>, seconds: f64) {
        let ms = (seconds * 1000.0).round().clamp(0.0, f64::from(u32::MAX)) as u32;
        if pending
            .is_some_and(|p| p.purpose == RequestPurpose::Settings(SettingsPurpose::PingLinkProxy))
        {
            self.settle_link_check(pending, PingStatus::Available(ms));
            return;
        }
        if let Some(proxy_id) = self.take_awaited_ping(pending) {
            self.settings
                .proxy
                .pings
                .insert(proxy_id, PingStatus::Available(ms));
        }
    }

    /// `ok` for the proxy purposes.
    pub(crate) fn apply_proxy_ok(&mut self, pending: Option<&PendingRequest>) {
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::MutateProxy) => self.proxy_mutation_succeeded(pending),
            Some(RequestPurpose::Settings(SettingsPurpose::SetPreferIpv6 { on })) => {
                self.settings.proxy.prefer_ipv6 = on;
                self.settings.proxy.ops.ipv6_ended(pending.map(|p| p.id));
            }
            _ => {}
        }
    }

    /// Error for the proxy purposes.
    pub(crate) fn apply_proxy_error(
        &mut self,
        purpose: RequestPurpose,
        pending: Option<&PendingRequest>,
        err: &TdError,
    ) {
        match purpose {
            RequestPurpose::GetProxies => {
                if pending.is_some_and(|p| self.settings.proxy.ops.fetch_failed(p.id)) {
                    self.settings.proxy.error =
                        Some(sessions_error_line("load the proxy list", err));
                }
            }
            RequestPurpose::MutateProxy => {
                self.settings.proxy.ops.change_failed(pending.map(|p| p.id));
                // TDLib refusing the system step is not the user's error:
                // the box does not show it, and the tick reads it here.
                let system =
                    pending.is_some_and(|p| self.settings.proxy.system_step.answered(p.id, true));
                if !system {
                    self.settings.proxy.error = Some(sessions_error_line("change the proxy", err));
                }
            }
            // A failed ping is the answer: the proxy is not available.
            RequestPurpose::Settings(SettingsPurpose::PingLinkProxy) => {
                self.settle_link_check(pending, PingStatus::Unavailable);
            }
            RequestPurpose::Settings(SettingsPurpose::PingProxy { .. }) => {
                if let Some(proxy_id) = self.take_awaited_ping(pending) {
                    self.settings
                        .proxy
                        .pings
                        .insert(proxy_id, PingStatus::Unavailable);
                }
            }
            RequestPurpose::Settings(SettingsPurpose::SetPreferIpv6 { .. }) => {
                self.settings.proxy.ops.ipv6_ended(pending.map(|p| p.id));
                self.settings.proxy.error =
                    Some(sessions_error_line("change the IPv6 setting", err));
            }
            _ => {}
        }
    }
}
