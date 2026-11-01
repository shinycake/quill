//! The driver's timed work, run once per poll by the UI loops (the account
//! on screen in `ui::notifications`, the others in
//! `ui::background_accounts`). None of it has timers of its own; the poll
//! loop already ticks.
use super::*;
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

impl<S: JsonSender> ConnectDriver<S> {
    /// Run every tick that is due for `scope`.
    pub fn poll_ticks(&mut self, scope: TickScope, now: Instant) -> TickOutcome {
        let mut outcome = TickOutcome::default();
        // Q1/R5: re-send rate-limited reads whose wait is over and sweep
        // requests whose answer never came.
        outcome.redraw(self.request_tick(now));
        // `parity:data-proxy-web`: carrier failures and hosting changes.
        outcome.redraw(self.carrier_tick());
        self.pump_exports();
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

    /// Whether the OS proxy settings are followed right now: the user
    /// asked for it and no proxy of their own is on.
    fn wants_system_proxy(&self) -> bool {
        let proxy = &self.session.settings.proxy;
        proxy.prefs.use_system && proxy.enabled().is_none()
    }
}
