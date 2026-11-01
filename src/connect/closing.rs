//! Closing TDLib clients without blocking the caller.
//!
//! TDLib's `close` flushes and closes the client's databases and ends with
//! `authorizationStateClosed`; the client must not be reopened, and its
//! folder must not be deleted, before that. Waiting for it on the UI thread
//! froze the window for up to [`CLIENT_CLOSE_TIMEOUT`] per account (account
//! switch, a background account that signed out, lock-screen log out).
//! Instead a closing client goes into a [`ClosingSet`]: `close` is sent at
//! once and the caller's existing poll drains it ([`ClosingSet::poll`],
//! never blocking) until Closed, like Telegram X postpones work with
//! `Tdlib.awaitClose` until the state arrives.
//!
//! Outside quit there is no deadline: a client stays in the set until TDLib
//! confirms Closed ([`CLOSE_GRACE`] only logs a slow one). A client whose
//! Closed can no longer be heard (the receive loop died), or that the quit
//! deadline cut off, stays "unconfirmed" for the rest of the process:
//! [`ClosingSet::may_be_open`] keeps saying its database may be in use.
//!
//! On quit every client closes together under one shared deadline
//! ([`ClosingSet::close_all_within`], [`QUIT_CLOSE_DEADLINE`]). A deadline
//! only stops Quill listening: with `td_json_client` a client id is never
//! destroyed by its owner, so TDLib finishes its own close on its own
//! threads (and the shared hub keeps `libtdjson` loaded).

use super::*;
use crate::ids::AccountKey;
use crate::telegram::client::{OwnedEnvelope, ReceiveBridge};
use crate::telegram::envelope::AuthorizationState;
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// A client still closing after this long is logged once. Not a deadline:
/// it keeps being drained until Closed.
pub const CLOSE_GRACE: Duration = CLIENT_CLOSE_TIMEOUT;

/// On quit, the one deadline every open client shares.
pub const QUIT_CLOSE_DEADLINE: Duration = Duration::from_millis(1500);

/// Most time one poll spends applying one closing client's updates.
pub const CLOSE_DRAIN_BUDGET: Duration = Duration::from_millis(4);

/// Most time `close` waits behind a call's hang-up, inside
/// [`QUIT_CLOSE_DEADLINE`]: `close` sent right behind `discardCall` or
/// `leaveGroupCall` can cut them off before they reach the server.
pub const HANG_UP_HOLD: Duration = Duration::from_millis(500);

/// Where one closing client stands after a drain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseProgress {
    /// Closed not seen yet; more may arrive.
    Pending,
    /// TDLib confirmed `authorizationStateClosed`.
    Closed,
    /// Closed can no longer arrive (the receive loop ended), so whether
    /// TDLib finished closing is unknown.
    Lost,
}

/// A client that can be closed in steps.
pub trait Closable {
    /// Announce going offline and send `close` (called once).
    fn begin_close(&mut self);
    /// Apply what has arrived, without waiting for more and for at most
    /// `budget`.
    fn drain(&mut self, budget: Duration) -> CloseProgress;
    /// Stop receiving (remove the route, join the receive thread). Called
    /// once, after Closed, Lost or the quit deadline.
    fn finish(&mut self);
}

/// Like tdesktop on quit: end the account's calls, announce going offline,
/// then `close` on the same ordered queue, held back until the hang-up
/// landed ([`release_close`]). A client that is Closed already gets nothing.
pub(crate) fn begin_close<S: JsonSender>(driver: &mut ConnectDriver<S>) {
    // `parity:data-proxy-web`: no hidden web view outlives its client.
    driver.shut_down_carriers();
    if matches!(driver.session.auth, AuthorizationState::Closed) {
        return;
    }
    if matches!(driver.session.auth, AuthorizationState::Ready) {
        driver.hang_up_for_close();
        let _ = driver.set_online(false);
    }
    if driver.hang_up_pending() {
        driver.close_held_until = Some(deadline_after(HANG_UP_HOLD));
    } else {
        let _ = driver.request_close();
    }
}

pub(crate) fn release_close<S: JsonSender>(driver: &mut ConnectDriver<S>, now: Instant) {
    let Some(until) = driver.close_held_until else {
        return;
    };
    if now >= until || !driver.hang_up_pending() {
        driver.close_held_until = None;
        let _ = driver.request_close();
    }
}

/// How [`drain_until_closed`] stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Drained {
    Closed,
    /// Nothing more had arrived.
    Dry,
    /// The budget ran out with more possibly queued.
    Budget,
}

/// Ingest what `recv` hands back until Closed, `recv` runs dry or `budget`
/// is spent.
pub(crate) fn drain_until_closed<S: JsonSender>(
    driver: &mut ConnectDriver<S>,
    mut recv: impl FnMut() -> Option<OwnedEnvelope>,
    budget: Duration,
) -> Drained {
    let started = Instant::now();
    loop {
        if matches!(driver.session.auth, AuthorizationState::Closed) {
            return Drained::Closed;
        }
        if started.elapsed() >= budget {
            return Drained::Budget;
        }
        let Some(owned) = recv() else {
            return Drained::Dry;
        };
        let _ = driver.ingest(owned);
    }
}

/// [`Closable::begin_close`] for a driver fed by a receive bridge (what
/// [`LiveConnect`] is). A bridge already shut down was closed before.
pub(crate) fn begin_close_bridged<S: JsonSender>(
    driver: &mut ConnectDriver<S>,
    bridge: &ReceiveBridge,
) {
    if bridge.is_joined() {
        return;
    }
    begin_close(driver);
}

/// [`Closable::drain`] for a driver fed by a receive bridge. A receive
/// loop that died is [`CloseProgress::Lost`] once what it queued is
/// drained, never Closed: TDLib may still be closing the database.
pub(crate) fn drain_bridged<S: JsonSender>(
    driver: &mut ConnectDriver<S>,
    bridge: &ReceiveBridge,
    budget: Duration,
) -> CloseProgress {
    if matches!(driver.session.auth, AuthorizationState::Closed) {
        return CloseProgress::Closed;
    }
    if bridge.is_joined() {
        return CloseProgress::Lost;
    }
    // Read before draining: what was queued before the loop died is still
    // applied, and a dry channel then means nothing more can come.
    let stopped = bridge.stopped_unexpectedly();
    let drained = drain_until_closed(driver, || bridge.next_timeout(Duration::ZERO), budget);
    release_close(driver, Instant::now());
    match drained {
        Drained::Closed => CloseProgress::Closed,
        Drained::Dry if stopped => CloseProgress::Lost,
        Drained::Dry | Drained::Budget => CloseProgress::Pending,
    }
}

/// One client on its way out.
pub struct ClosingClient {
    pub key: AccountKey,
    client: Box<dyn Closable>,
    since: Instant,
    /// The slow close was logged.
    warned: bool,
    /// Start the account again in the background once closed (it was
    /// closed only because of a call, not because it signed out).
    pub restart: bool,
}

/// What [`ClosingSet::poll`] finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedClient {
    pub key: AccountKey,
    pub restart: bool,
    /// TDLib confirmed Closed. False: the receive loop died first, so the
    /// account stays [`ClosingSet::may_be_open`].
    pub confirmed: bool,
}

/// Clients sent `close` and not yet finished.
#[derive(Default)]
pub struct ClosingSet {
    entries: Vec<ClosingClient>,
    /// Finished without a confirmed Closed: their databases may still be
    /// closing, for the rest of the process.
    unconfirmed: HashSet<AccountKey>,
}

impl ClosingSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Send `close` to `client` and keep it until Closed. Never blocks.
    pub fn push(&mut self, key: AccountKey, mut client: Box<dyn Closable>, restart: bool) {
        client.begin_close();
        self.entries.push(ClosingClient {
            key,
            client,
            since: Instant::now(),
            warned: false,
            restart,
        });
    }

    /// No client is being drained.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `key`'s client is still being drained.
    pub fn contains(&self, key: &AccountKey) -> bool {
        self.entries.iter().any(|entry| &entry.key == key)
    }

    /// `key`'s database may still be open in TDLib: it must be neither
    /// opened nor deleted.
    pub fn may_be_open(&self, key: &AccountKey) -> bool {
        self.contains(key) || self.unconfirmed.contains(key)
    }

    /// `key` finished without a confirmed Closed: it stays
    /// [`Self::may_be_open`] until the process ends.
    pub fn unconfirmed(&self, key: &AccountKey) -> bool {
        self.unconfirmed.contains(key)
    }

    /// Every account whose database may still be open.
    pub fn keys(&self) -> Vec<AccountKey> {
        let mut keys: Vec<AccountKey> = self.unconfirmed.iter().cloned().collect();
        for entry in &self.entries {
            if !keys.contains(&entry.key) {
                keys.push(entry.key.clone());
            }
        }
        keys
    }

    /// Nothing restarts any more (local data is about to be wiped).
    pub fn cancel_restarts(&mut self) {
        for entry in &mut self.entries {
            entry.restart = false;
        }
    }

    pub fn restart_all(&mut self) {
        for entry in &mut self.entries {
            entry.restart = true;
        }
    }

    /// `key` does not restart (it is being removed).
    pub fn cancel_restart(&mut self, key: &AccountKey) {
        for entry in &mut self.entries {
            if &entry.key == key {
                entry.restart = false;
            }
        }
    }

    /// Drain every closing client once without waiting; finish and drop
    /// the ones that reached Closed or can no longer reach it.
    pub fn poll(&mut self, now: Instant) -> Vec<ClosedClient> {
        let mut done = Vec::new();
        let unconfirmed = &mut self.unconfirmed;
        self.entries.retain_mut(|entry| {
            let confirmed = match entry.client.drain(CLOSE_DRAIN_BUDGET) {
                CloseProgress::Closed => true,
                CloseProgress::Lost => false,
                CloseProgress::Pending => {
                    if !entry.warned && now.saturating_duration_since(entry.since) >= CLOSE_GRACE {
                        entry.warned = true;
                        eprintln!(
                            "quill: TDLib has not closed account {} after {:?}; still waiting",
                            entry.key, CLOSE_GRACE
                        );
                    }
                    return true;
                }
            };
            entry.client.finish();
            if !confirmed {
                unconfirmed.insert(entry.key.clone());
            }
            done.push(ClosedClient {
                key: entry.key.clone(),
                restart: entry.restart,
                confirmed,
            });
            false
        });
        done
    }

    /// Quit: wait for every client together until all are Closed or
    /// `deadline` passes, then finish them all. Blocks at most until
    /// `deadline`, however many clients there are. Returns how many
    /// finished without a confirmed Closed.
    pub fn close_all_within(&mut self, deadline: Instant) -> usize {
        let mut unconfirmed = 0;
        loop {
            let now = Instant::now();
            unconfirmed += self
                .poll(now)
                .iter()
                .filter(|closed| !closed.confirmed)
                .count();
            if self.entries.is_empty() {
                return unconfirmed;
            }
            if now >= deadline {
                for mut entry in std::mem::take(&mut self.entries) {
                    entry.client.finish();
                    self.unconfirmed.insert(entry.key);
                    unconfirmed += 1;
                }
                return unconfirmed;
            }
            let left = deadline.saturating_duration_since(now);
            std::thread::sleep(left.min(Duration::from_millis(5)));
        }
    }
}

/// `now + after`, saturating instead of panicking on overflow.
pub fn deadline_after(after: Duration) -> Instant {
    let now = Instant::now();
    now.checked_add(after).unwrap_or(now)
}

impl Drop for ClosingSet {
    /// Last resort (the quit hook normally emptied it): the same shared
    /// deadline, never one wait per client.
    fn drop(&mut self) {
        if !self.entries.is_empty() {
            let _ = self.close_all_within(deadline_after(QUIT_CLOSE_DEADLINE));
        }
    }
}
