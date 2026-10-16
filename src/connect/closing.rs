//! Closing TDLib clients without blocking the caller.
//!
//! TDLib's `close` flushes and closes the client's databases and ends with
//! `authorizationStateClosed`; the client must not be reopened on the same
//! database before that. Waiting for it on the UI thread froze the window
//! for up to [`CLIENT_CLOSE_TIMEOUT`] per account (account switch, a
//! background account that signed out, lock-screen log out). Instead a
//! closing client goes into a [`ClosingSet`]: `close` is sent at once and
//! the caller's existing poll drains it ([`ClosingSet::poll`], never
//! blocking) until Closed or a deadline, like Telegram X postpones work
//! with `Tdlib.awaitClose` until the state arrives.
//!
//! On quit every client closes together under one shared deadline
//! ([`ClosingSet::close_all_within`], [`QUIT_CLOSE_DEADLINE`]).
//!
//! A deadline only stops Quill listening: with `td_json_client` a client id
//! is never destroyed by its owner, so TDLib finishes its own close on its
//! own threads (and the shared hub keeps `libtdjson` loaded).

use super::*;
use crate::ids::AccountKey;
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::AuthorizationState;
use std::time::{Duration, Instant};

/// How long a closing client is drained before Quill stops listening.
/// Not a wait: nothing blocks on it.
pub const CLOSE_GRACE: Duration = CLIENT_CLOSE_TIMEOUT;

/// On quit, the one deadline every open client shares.
pub const QUIT_CLOSE_DEADLINE: Duration = Duration::from_millis(1500);

/// Most time one poll spends applying one closing client's updates.
pub const CLOSE_DRAIN_BUDGET: Duration = Duration::from_millis(4);

/// A client that can be closed in steps.
pub trait Closable {
    /// Announce going offline and send `close` (called once).
    fn begin_close(&mut self);
    /// Apply what has arrived, without waiting for more and for at most
    /// `budget`. True once Closed was seen or nothing more can arrive.
    fn drain_closed(&mut self, budget: Duration) -> bool;
    /// Stop receiving (remove the route, join the receive thread). Called
    /// once, after Closed or the deadline.
    fn finish(&mut self);
}

/// Like tdesktop on quit: announce going offline, then `close` on the same
/// ordered queue. A client that is Closed already gets nothing.
pub(crate) fn begin_close<S: JsonSender>(driver: &mut ConnectDriver<S>) {
    if matches!(driver.session.auth, AuthorizationState::Closed) {
        return;
    }
    if matches!(driver.session.auth, AuthorizationState::Ready) {
        let _ = driver.set_online(false);
    }
    let _ = driver.request_close();
}

/// Ingest what `recv` hands back until Closed, `recv` runs dry or `budget`
/// is spent. Returns whether Closed was observed.
pub(crate) fn drain_until_closed<S: JsonSender>(
    driver: &mut ConnectDriver<S>,
    mut recv: impl FnMut() -> Option<OwnedEnvelope>,
    budget: Duration,
) -> bool {
    let started = Instant::now();
    loop {
        if matches!(driver.session.auth, AuthorizationState::Closed) {
            return true;
        }
        if started.elapsed() >= budget {
            return false;
        }
        let Some(owned) = recv() else {
            return false;
        };
        let _ = driver.ingest(owned);
    }
}

/// One client on its way out.
pub struct ClosingClient {
    pub key: AccountKey,
    client: Box<dyn Closable>,
    deadline: Instant,
    /// Start the account again in the background once closed (it was
    /// closed only because of a call, not because it signed out).
    pub restart: bool,
}

/// What [`ClosingSet::poll`] finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedClient {
    pub key: AccountKey,
    pub restart: bool,
    /// The deadline passed before Closed arrived.
    pub timed_out: bool,
}

/// Clients sent `close` and not yet finished.
#[derive(Default)]
pub struct ClosingSet {
    entries: Vec<ClosingClient>,
}

impl ClosingSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Send `close` to `client` and keep it until Closed or `deadline`.
    /// Never blocks.
    pub fn push(
        &mut self,
        key: AccountKey,
        mut client: Box<dyn Closable>,
        restart: bool,
        deadline: Instant,
    ) {
        client.begin_close();
        self.entries.push(ClosingClient {
            key,
            client,
            deadline,
            restart,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `key`'s client is still closing: its database must not be opened.
    pub fn contains(&self, key: &AccountKey) -> bool {
        self.entries.iter().any(|entry| &entry.key == key)
    }

    /// Nothing restarts any more (local data is about to be wiped).
    pub fn cancel_restarts(&mut self) {
        for entry in &mut self.entries {
            entry.restart = false;
        }
    }

    /// Drain every closing client once without waiting; finish and drop
    /// the ones that reached Closed or passed their deadline.
    pub fn poll(&mut self, now: Instant) -> Vec<ClosedClient> {
        let mut done = Vec::new();
        self.entries.retain_mut(|entry| {
            let closed = entry.client.drain_closed(CLOSE_DRAIN_BUDGET);
            let timed_out = !closed && now >= entry.deadline;
            if !closed && !timed_out {
                return true;
            }
            entry.client.finish();
            done.push(ClosedClient {
                key: entry.key.clone(),
                restart: entry.restart,
                timed_out,
            });
            false
        });
        done
    }

    /// Quit: wait for every client together until all are Closed or
    /// `deadline` passes, then finish them all. Blocks at most until
    /// `deadline`, however many clients there are. Returns how many timed
    /// out.
    pub fn close_all_within(&mut self, deadline: Instant) -> usize {
        for entry in &mut self.entries {
            entry.deadline = entry.deadline.min(deadline);
        }
        let mut timed_out = 0;
        loop {
            let now = Instant::now();
            timed_out += self
                .poll(now)
                .iter()
                .filter(|closed| closed.timed_out)
                .count();
            if self.entries.is_empty() {
                return timed_out;
            }
            let left = deadline.saturating_duration_since(Instant::now());
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
