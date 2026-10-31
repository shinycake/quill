//! The driver's timed work, run once per poll by the UI loops (the account
//! on screen in `ui::notifications`, the others in
//! `ui::background_accounts`). None of it has timers of its own; the poll
//! loop already ticks.
use super::*;
use crate::net_watch::NetworkState;
use crate::state::{RequestPurpose, ShutdownPhase};
use crate::telegram::requests::{NetworkType, set_network_type};
use std::time::Instant;

/// Which account the poll is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickScope {
    /// The account on screen; `settings_open` while Settings is shown.
    OnScreen { settings_open: bool },
    /// A signed-in account in the background: only what keeps its
    /// requests and connection going.
    Background,
}

/// What the ticks did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TickOutcome {
    /// Something changed or went out.
    pub progressed: bool,
    /// What changed is on screen: redraw now.
    pub redraw_now: bool,
}

impl TickOutcome {
    fn progress(&mut self, ran: bool) {
        self.progressed |= ran;
    }

    fn redraw(&mut self, ran: bool) {
        self.progressed |= ran;
        self.redraw_now |= ran;
    }
}

/// What TDLib needs to hear after `seen`: nothing at the start, unless the
/// machine is offline. A proxy on the local network works with no route out.
pub(super) fn network_step(
    seen: Option<u64>,
    network: NetworkState,
    through_proxy: bool,
) -> Option<NetworkType> {
    let offline = !network.available && !through_proxy;
    match seen {
        Some(epoch) if epoch == network.epoch => None,
        None if !offline => None,
        _ if offline => Some(NetworkType::None),
        _ => Some(NetworkType::Other),
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Run every tick that is due for `scope`.
    pub fn poll_ticks(&mut self, scope: TickScope, now: Instant) -> TickOutcome {
        self.ticks_on(scope, now, crate::net_watch::latest())
    }

    pub(super) fn ticks_on(
        &mut self,
        scope: TickScope,
        now: Instant,
        network: NetworkState,
    ) -> TickOutcome {
        let mut outcome = TickOutcome::default();
        // Q1/R5: re-send rate-limited reads whose wait is over and sweep
        // requests whose answer never came.
        outcome.redraw(self.request_tick(now));
        // `parity:data-proxy-web`: carrier failures and hosting changes.
        outcome.redraw(self.carrier_tick());
        // Every client keeps its own connections.
        outcome.progress(self.network_tick(network));
        let TickScope::OnScreen { settings_open } = scope else {
            return outcome;
        };
        // User avatars the last frames showed without a file.
        outcome.progress(self.download_claimed_avatars());
        // Settings → My Grams: the balance probe and unknown names.
        outcome.progress(self.grams_tick(settings_open));
        // Bots named by a group's command list that the app does not know.
        outcome.progress(self.group_bot_users_tick());
        // Proxies: auto-switch.
        outcome.redraw(self.proxy_tick(crate::state::unix_ms_now()));
        // `parity:data-proxy-extras`: "Use system proxy settings".
        outcome.progress(
            self.system_proxy_tick(&crate::system_proxy::latest(self.wants_system_proxy())),
        );
        // Star reactions: commit the ones whose Undo window ended, and keep
        // the toast's countdown redrawing while one is waiting.
        outcome.redraw(
            self.paid_reaction_tick(crate::state::unix_ms_now())
                || self.session.payments.paid.has_pending(),
        );
        outcome
    }

    fn network_tick(&mut self, network: NetworkState) -> bool {
        if !self.parameters_sent
            || self.network_seen == Some(network.epoch)
            || !matches!(self.session.shutdown, ShutdownPhase::Running)
        {
            return false;
        }
        let seen = self.network_seen.replace(network.epoch);
        let proxy = &self.session.settings.proxy;
        let through_proxy =
            proxy.enabled().is_some() || proxy.system.iter().any(|entry| entry.is_enabled);
        let Some(kind) = network_step(seen, network, through_proxy) else {
            return false;
        };
        let extra = self.session.request(RequestPurpose::SetNetworkType, None);
        match self.sender.send_json(&set_network_type(extra, kind)) {
            Ok(()) => true,
            Err(_) => {
                self.session.requests.take(extra);
                false
            }
        }
    }

    /// Whether the OS proxy settings are followed right now: the user
    /// asked for it and no proxy of their own is on.
    fn wants_system_proxy(&self) -> bool {
        let proxy = &self.session.settings.proxy;
        proxy.prefs.use_system && proxy.enabled().is_none()
    }
}
