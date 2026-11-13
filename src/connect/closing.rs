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
//! Dropping the set never waits. On quit every client closes together under one shared deadline
//! ([`ClosingSet::close_all_within`], [`QUIT_CLOSE_DEADLINE`]). A deadline
//! only stops Quill listening: with `td_json_client` a client id is never
//! destroyed by its owner, so TDLib finishes its own close on its own
//! threads (and the shared hub keeps `libtdjson` loaded).

use super::*;
use crate::ids::AccountKey;
use crate::state::ShutdownPhase;
use crate::telegram::client::{OwnedEnvelope, ReceiveBridge};
use crate::telegram::envelope::AuthorizationState;
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

/// How long a `close` the sender refused waits before it is sent again.
pub const CLOSE_RETRY_BACKOFF: Duration = Duration::from_millis(250);

/// What one drain of a closing client found (a return value, never stored).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOutcome {
    /// Closed not seen yet; more may arrive.
    Pending,
    /// TDLib confirmed `authorizationStateClosed`.
    Closed,
    /// Closed can no longer arrive (the receive loop ended), so whether
    /// TDLib finished closing is unknown.
    Lost,
}

/// Most time a quit waits for stopped exports to leave their last requests,
/// inside [`QUIT_CLOSE_DEADLINE`]. A worker sees the stop within 100 ms,
/// and a copy into the export folder within one 1 MiB chunk
/// (`export::copy`).
pub const EXPORT_STOP_HOLD: Duration = Duration::from_millis(400);

/// A client that can be closed in steps.
pub trait Closable {
    /// Announce going offline and send `close` (called once).
    fn begin_close(&mut self);
    /// Apply what has arrived, without waiting for more and for at most
    /// `budget`.
    fn drain(&mut self, budget: Duration) -> CloseOutcome;
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
    driver.flush_export_requests();
    if matches!(driver.session.auth, AuthorizationState::Ready) {
        driver.hang_up_for_close();
        let _ = driver.set_online(false);
    }
    if driver.hang_up_pending() {
        driver.session.shutdown = ShutdownPhase::HoldingClose {
            until: deadline_after(HANG_UP_HOLD),
        };
    } else {
        send_close(driver);
    }
}

/// Send `close`. A close TDLib's client could not take (the send failed, the
/// session still runs) is sent again after a backoff ([`release_close`]):
/// the client would otherwise wait for a Closed nothing asked for.
fn send_close<S: JsonSender>(driver: &mut ConnectDriver<S>) {
    if let Err(error) = driver.request_close()
        && error != ConnectSendError::InvalidRequest
    {
        driver.session.shutdown = ShutdownPhase::CloseRefused {
            retry_at: deadline_after(CLOSE_RETRY_BACKOFF),
        };
    }
}

pub(crate) fn release_close<S: JsonSender>(driver: &mut ConnectDriver<S>, now: Instant) {
    match driver.session.shutdown {
        ShutdownPhase::HoldingClose { until } if now >= until || !driver.hang_up_pending() => {
            driver.session.shutdown = ShutdownPhase::Active;
            send_close(driver);
        }
        ShutdownPhase::CloseRefused { retry_at } if now >= retry_at => {
            driver.session.shutdown = ShutdownPhase::Active;
            send_close(driver);
        }
        _ => {}
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
/// loop that died is [`CloseOutcome::Lost`] once what it queued is
/// drained, never Closed: TDLib may still be closing the database.
pub(crate) fn drain_bridged<S: JsonSender>(
    driver: &mut ConnectDriver<S>,
    bridge: &ReceiveBridge,
    budget: Duration,
) -> CloseOutcome {
    if matches!(driver.session.auth, AuthorizationState::Closed) {
        return CloseOutcome::Closed;
    }
    if bridge.is_joined() {
        return CloseOutcome::Lost;
    }
    // Read before draining: what was queued before the loop died is still
    // applied, and a dry channel then means nothing more can come.
    let stopped = bridge.stopped_unexpectedly();
    let drained = drain_until_closed(driver, || bridge.next_timeout(Duration::ZERO), budget);
    release_close(driver, Instant::now());
    match drained {
        Drained::Closed => CloseOutcome::Closed,
        Drained::Dry if stopped => CloseOutcome::Lost,
        Drained::Dry | Drained::Budget => CloseOutcome::Pending,
    }
}

/// How long a closing client has waited: since when, and whether the slow
/// close was logged (once).
#[derive(Clone, Copy)]
enum CloseWait {
    Since(Instant),
    SlowLogged,
}

/// One client on its way out.
pub struct ClosingClient {
    pub key: AccountKey,
    client: Box<dyn Closable>,
    wait: CloseWait,
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

/// Where one account's close stands in the set: its client still being
/// drained, or finished without a confirmed Closed (its database may still
/// be closing, for the rest of the process). An account is in one stage.
enum Slot {
    Closing(ClosingClient),
    Unconfirmed(AccountKey),
}

impl Slot {
    fn key(&self) -> &AccountKey {
        match self {
            Self::Closing(entry) => &entry.key,
            Self::Unconfirmed(key) => key,
        }
    }
}

/// Clients sent `close` and not yet finished.
#[derive(Default)]
pub struct ClosingSet {
    slots: Vec<Slot>,
}

impl ClosingSet {
    pub fn new() -> Self {
        Self::default()
    }

    fn closing(&self) -> impl Iterator<Item = &ClosingClient> {
        self.slots.iter().filter_map(|slot| match slot {
            Slot::Closing(entry) => Some(entry),
            Slot::Unconfirmed(_) => None,
        })
    }

    fn closing_mut(&mut self) -> impl Iterator<Item = &mut ClosingClient> {
        self.slots.iter_mut().filter_map(|slot| match slot {
            Slot::Closing(entry) => Some(entry),
            Slot::Unconfirmed(_) => None,
        })
    }

    /// `key` finished without a confirmed Closed.
    fn mark_unconfirmed(&mut self, key: AccountKey) {
        if !self
            .slots
            .iter()
            .any(|slot| matches!(slot, Slot::Unconfirmed(held) if *held == key))
        {
            self.slots.push(Slot::Unconfirmed(key));
        }
    }

    /// Send `close` to `client` and keep it until Closed. Never blocks.
    pub fn push(&mut self, key: AccountKey, mut client: Box<dyn Closable>, restart: bool) {
        client.begin_close();
        self.slots.push(Slot::Closing(ClosingClient {
            key,
            client,
            wait: CloseWait::Since(Instant::now()),
            restart,
        }));
    }

    /// No client is being drained.
    pub fn is_empty(&self) -> bool {
        self.closing().next().is_none()
    }

    pub fn len(&self) -> usize {
        self.closing().count()
    }

    /// `key`'s client is still being drained.
    pub fn contains(&self, key: &AccountKey) -> bool {
        self.closing().any(|entry| &entry.key == key)
    }

    /// `key`'s database may still be open in TDLib: it must be neither
    /// opened nor deleted.
    pub fn may_be_open(&self, key: &AccountKey) -> bool {
        self.slots.iter().any(|slot| slot.key() == key)
    }

    /// `key` finished without a confirmed Closed: it stays
    /// [`Self::may_be_open`] until the process ends.
    pub fn unconfirmed(&self, key: &AccountKey) -> bool {
        self.slots
            .iter()
            .any(|slot| matches!(slot, Slot::Unconfirmed(held) if held == key))
    }

    /// Every account whose database may still be open.
    pub fn keys(&self) -> Vec<AccountKey> {
        let mut keys: Vec<AccountKey> = self
            .slots
            .iter()
            .filter_map(|slot| match slot {
                Slot::Unconfirmed(key) => Some(key.clone()),
                Slot::Closing(_) => None,
            })
            .collect();
        for entry in self.closing() {
            if !keys.contains(&entry.key) {
                keys.push(entry.key.clone());
            }
        }
        keys
    }

    /// Take over `other`'s clients and its unconfirmed accounts.
    pub fn absorb(&mut self, mut other: ClosingSet) {
        for slot in std::mem::take(&mut other.slots) {
            match slot {
                Slot::Closing(entry) => self.slots.push(Slot::Closing(entry)),
                Slot::Unconfirmed(key) => self.mark_unconfirmed(key),
            }
        }
    }

    /// Nothing restarts any more (local data is about to be wiped).
    pub fn cancel_restarts(&mut self) {
        for entry in self.closing_mut() {
            entry.restart = false;
        }
    }

    pub fn restart_all(&mut self) {
        for entry in self.closing_mut() {
            entry.restart = true;
        }
    }

    /// `key` does not restart (it is being removed).
    pub fn cancel_restart(&mut self, key: &AccountKey) {
        for entry in self.closing_mut() {
            if &entry.key == key {
                entry.restart = false;
            }
        }
    }

    /// Drain every closing client once without waiting; finish and drop
    /// the ones that reached Closed or can no longer reach it.
    pub fn poll(&mut self, now: Instant) -> Vec<ClosedClient> {
        let mut done = Vec::new();
        for slot in std::mem::take(&mut self.slots) {
            let Slot::Closing(mut entry) = slot else {
                self.slots.push(slot);
                continue;
            };
            let confirmed = match entry.client.drain(CLOSE_DRAIN_BUDGET) {
                CloseOutcome::Closed => true,
                CloseOutcome::Lost => false,
                CloseOutcome::Pending => {
                    if let CloseWait::Since(since) = entry.wait
                        && now.saturating_duration_since(since) >= CLOSE_GRACE
                    {
                        entry.wait = CloseWait::SlowLogged;
                        eprintln!(
                            "quill: TDLib has not closed account {} after {:?}; still waiting",
                            entry.key, CLOSE_GRACE
                        );
                    }
                    self.slots.push(Slot::Closing(entry));
                    continue;
                }
            };
            entry.client.finish();
            if !confirmed {
                self.mark_unconfirmed(entry.key.clone());
            }
            done.push(ClosedClient {
                key: entry.key.clone(),
                restart: entry.restart,
                confirmed,
            });
        }
        done
    }

    /// Stop listening to every client still closing, without waiting: they
    /// stay unconfirmed. Returns how many there were.
    fn abandon_closing(&mut self) -> usize {
        let mut abandoned = 0;
        for slot in std::mem::take(&mut self.slots) {
            match slot {
                Slot::Closing(mut entry) => {
                    entry.client.finish();
                    self.mark_unconfirmed(entry.key);
                    abandoned += 1;
                }
                Slot::Unconfirmed(key) => self.mark_unconfirmed(key),
            }
        }
        abandoned
    }

    /// Quit: wait for every client together until all are Closed or
    /// `deadline` passes, then finish them all. Blocks at most until
    /// `deadline`, however many clients there are. Returns how many
    /// finished without a confirmed Closed. The only place the set waits.
    pub fn close_all_within(&mut self, deadline: Instant) -> usize {
        let mut unconfirmed = 0;
        loop {
            let now = Instant::now();
            unconfirmed += self
                .poll(now)
                .iter()
                .filter(|closed| !closed.confirmed)
                .count();
            if self.is_empty() {
                return unconfirmed;
            }
            if now >= deadline {
                return unconfirmed + self.abandon_closing();
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
    /// Dropping never waits on the thread that drops: the quit paths call
    /// [`ClosingSet::close_all_within`] (the one wait, under one shared
    /// deadline). What arrived is applied, and a client still closing is
    /// let go at once: TDLib finishes its own close on its own threads.
    fn drop(&mut self) {
        if !self.is_empty() {
            let _ = self.poll(Instant::now());
            let _ = self.abandon_closing();
        }
    }
}
