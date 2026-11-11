//! `parity:notify-all-accounts`: every signed-in account stays connected,
//! like Telegram Desktop's `Main::Domain` keeps every account's session.
//!
//! The account on screen is `QuillApp::live`. Every other registered
//! account with a local database gets a TDLib client of its own here
//! (`quill::connect::start_background_live_connect`; all clients share the
//! one receive thread of `quill::telegram::hub`). A background client
//! receives updates and keeps a full `Session`, so it can raise
//! notifications (when Settings > Notifications > "All accounts" is on),
//! count its unread chats for the account list and the app badge, and be
//! switched to without a restart: `switch_account` hands the outgoing
//! client to the background and takes the incoming one from it.
//!
//! A background client never opens a chat or loads history. One that needs
//! a sign-in, or whose session ended, is closed and only its status stays:
//! signing in runs on screen.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::export_progress::take_ended_exports;
use super::selection_html::rich_html_note;
use super::status_note::StatusNote;
use super::wake_wait::{PassEnd, PollPace, backlog_hold, run_poll_loop};
use gpui_kit::*;
use quill::connect::{ClosingSet, ConnectBlocker, LiveConnect, start_background_live_connect};
use quill::diagnostics::DiscardSink;
use quill::ids::{AccountKey, ChatId};
use quill::lang::{keys, tr};
use quill::notify::NotificationAction;
use quill::notify_accounts::{
    AccountUnread, BackgroundStatus, ChatClick, NotifyDrain, background_status, drain_background,
    set_other_accounts_unread,
};
use quill::settings::{
    AccountPaths, active_account, list_accounts, pending_deletes, safe_app_root,
};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod account_row;
mod calls;
pub(super) mod chrome;
mod clicks;
pub(super) mod closing;
pub(in crate::ui) mod deleting;
mod prepare;
mod restart;
mod switch;

/// Clicks on notifications, each with the account it was shown for, queued
/// by the OS callback, Quill's notification windows and the Linux
/// notification threads. `drain_account_clicks` hands the ones for the
/// account on screen to `notify.notify_clicks` and runs the rest on their
/// own account.
pub(crate) type AccountClicks = Arc<Mutex<Vec<ChatClick>>>;

/// Most time one background poll spends applying one account's updates;
/// the rest waits for the next poll (a reconnect burst must not stall the
/// window).
const INGEST_BUDGET: Duration = Duration::from_millis(4);

/// The background poll's pace, slower than the screen's (the updates only
/// feed notifications and counters). Updates do not wait for the ceiling.
const POLL_PACE: PollPace = PollPace {
    gap: Duration::from_millis(40),
    ceiling: Duration::from_secs(2),
};

/// One account that is not on screen.
pub(super) struct BackgroundClient {
    pub(super) key: AccountKey,
    /// `None` once the client was closed (needs sign-in, signed out, or
    /// could not start).
    pub(super) live: Option<LiveConnect>,
    pub(super) status: BackgroundStatus,
    pub(super) unread: AccountUnread,
}

impl BackgroundClient {
    /// The client could not start, or died: the account is started again
    /// by the next pass. One that needs a sign-in, or was signed out, is
    /// not.
    pub(super) fn retryable(&self) -> bool {
        self.live.is_none() && matches!(self.status, BackgroundStatus::Failed(_))
    }
}

/// After a click on another account's notification: the chat to open
/// once the switch has made that account's session ready.
pub(super) struct PendingOpen {
    pub(super) account: AccountKey,
    pub(super) chat_id: ChatId,
    pub(super) action: NotificationAction,
    pub(super) until: Instant,
}

/// The folders of the accounts, as this run treats them: swept once at the
/// first connection, then wiped at most once by the lock-screen log out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LocalData {
    /// The folders an earlier run left for deletion are not swept yet.
    Fresh,
    Swept,
    /// The lock-screen log out is under way: no client starts until the
    /// local data is gone (the wipe starts the connection itself). A wipe
    /// that could not finish stays so for the run.
    Wiping,
}

pub(crate) struct BackgroundAccounts {
    /// Every change goes through [`Self::clients_mut`], which publishes the
    /// combined unread when it ends: the only writer of the badge.
    clients: Vec<BackgroundClient>,
    /// Accounts whose client is wanted once its database key is read on a
    /// worker, and the preparations still running (`prepare_connect`
    /// creates the account's folders: nothing may delete those until they
    /// are done).
    prepare: prepare::PrepareSet,
    /// `start_connection` preparations still running on a worker.
    pub(super) screen_preparing: u32,
    /// Bumped by `stop_background_accounts`: starts in flight and the poll
    /// loop of an older generation end.
    generation: u64,
    /// The background poll; dropping it ends the loop.
    poll: Option<Task<()>>,
    pub(super) clicks: AccountClicks,
    pub(super) pending_open: Option<PendingOpen>,
    /// tdesktop `notifyFromAll`, read from the account registry.
    pub(super) notify_from_all: bool,
    /// A background notification asked for the taskbar flash / Dock bounce.
    pub(super) attention: bool,
    /// A call ringing on a background account (`calls`).
    ringing: Option<calls::BackgroundRing>,
    /// Clients sent `close` and drained a little per poll (`closing`):
    /// switching, signing out or logging out never waits on TDLib.
    pub(super) closing: ClosingSet,
    /// When a client that died or failed to start starts again.
    restarts: restart::Restarts,
    /// The quit observer that closes every client together.
    quit_close: Option<Subscription>,
    /// Accounts whose folder is being deleted (`deleting`): nothing opens
    /// them.
    pub(super) deleting: HashSet<AccountKey>,
    pub(super) local: LocalData,
    /// The keychain the account database keys live in.
    pub(super) secrets: deleting::SecretsFactory,
    /// Tests point account work at a temporary app root.
    #[cfg(test)]
    pub(super) test_root: Option<std::path::PathBuf>,
}

impl BackgroundAccounts {
    pub(super) fn new() -> Self {
        Self {
            clients: Vec::new(),
            prepare: prepare::PrepareSet::default(),
            screen_preparing: 0,
            generation: 0,
            poll: None,
            clicks: Arc::new(Mutex::new(Vec::new())),
            pending_open: None,
            notify_from_all: true,
            attention: false,
            ringing: None,
            closing: ClosingSet::new(),
            restarts: restart::Restarts::default(),
            quit_close: None,
            deleting: HashSet::new(),
            local: LocalData::Fresh,
            secrets: deleting::live_secrets(),
            #[cfg(test)]
            test_root: None,
        }
    }

    /// The lock-screen log out is under way.
    pub(super) fn wiping(&self) -> bool {
        self.local == LocalData::Wiping
    }

    /// The clients, running or not.
    pub(super) fn clients(&self) -> &[BackgroundClient] {
        &self.clients
    }

    /// The clients, to change: the combined unread is published when the
    /// returned guard goes.
    pub(super) fn clients_mut(&mut self) -> ClientsMut<'_> {
        ClientsMut(&mut self.clients)
    }

    pub(super) fn client(&self, key: &AccountKey) -> Option<&BackgroundClient> {
        self.clients.iter().find(|client| &client.key == key)
    }

    pub(super) fn client_mut(&mut self, key: &AccountKey) -> Option<&mut BackgroundClient> {
        self.clients.iter_mut().find(|client| &client.key == key)
    }

    pub(super) fn keys(&self) -> Vec<AccountKey> {
        self.clients
            .iter()
            .map(|client| client.key.clone())
            .collect()
    }

    fn has_work(&self) -> bool {
        self.prepare.any_wanted()
            || !self.closing.is_empty()
            || self.clients.iter().any(|client| client.live.is_some())
            || self.clients.iter().any(BackgroundClient::retryable)
    }

    /// How long until the soonest failed client may start again: `Some`
    /// while one waits, zero when one may start now.
    pub(super) fn next_restart(&self, now: Instant) -> Option<Duration> {
        self.clients
            .iter()
            .filter(|client| client.retryable())
            .map(|client| self.restarts.due_in(&client.key, now))
            .min()
    }

    /// The clients whose delay is over and that the pass still could not
    /// start wait the first delay again, so a due client never makes
    /// every poll pass run the start pass.
    fn put_off_unstarted(&mut self, now: Instant) {
        for client in self.clients.iter().filter(|client| client.retryable()) {
            if self.restarts.due_in(&client.key, now).is_zero() {
                self.restarts.put_off(&client.key, now);
            }
        }
    }

    /// Every background account's unread, combined (tdesktop
    /// `OtherAccountsUnreadState`).
    pub(super) fn total_unread(&self) -> AccountUnread {
        total_unread_of(&self.clients)
    }
}

fn total_unread_of(clients: &[BackgroundClient]) -> AccountUnread {
    clients
        .iter()
        .fold(AccountUnread::default(), |sum, client| {
            sum.combine(client.unread)
        })
}

/// The clients to change. The tray, the app icon badge and the window
/// title add their combined unread to the account on screen, so it is
/// published when the change is over: nothing else writes it.
pub(super) struct ClientsMut<'a>(&'a mut Vec<BackgroundClient>);

impl std::ops::Deref for ClientsMut<'_> {
    type Target = Vec<BackgroundClient>;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl std::ops::DerefMut for ClientsMut<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl Drop for ClientsMut<'_> {
    fn drop(&mut self) {
        set_other_accounts_unread(total_unread_of(self.0));
    }
}

impl Drop for BackgroundAccounts {
    /// Quitting closes every background client (GPUI drops the window's
    /// views on quit); a TDLib client left open can crash the process on
    /// its way out. The quit observer (`closing::watch_quit_close`) has
    /// normally done it already; otherwise every client closes together
    /// under the one quit deadline.
    fn drop(&mut self) {
        self.close_all_within(quill::connect::deadline_after(
            quill::connect::QUIT_CLOSE_DEADLINE,
        ));
    }
}

impl QuillApp {
    /// Start a background client for every registered account that is not
    /// on screen and has a local database; forget clients of accounts that
    /// were removed or are on screen now. Idempotent: runs after every
    /// connect and switch.
    pub(super) fn ensure_background_accounts(&mut self, cx: &mut Context<Self>) {
        self.watch_quit_close(cx);
        if self.background.wiping() || self.disk.is_full() {
            return;
        }
        let Some(root) = self.client_accounts_root() else {
            return;
        };
        let Some(credentials) = self.credentials.clone() else {
            return;
        };
        self.background.notify_from_all = quill::settings::notify_from_all(&root);
        let active = active_account(&root);
        let accounts = list_accounts(&root);
        let pending = pending_deletes(&root);
        // Forget clients of accounts that were removed or are on screen
        // now; their clients close without blocking the window.
        let (keep, gone): (Vec<_>, Vec<_>) = std::mem::take(&mut *self.background.clients_mut())
            .into_iter()
            .partition(|client| {
                client.key != active && accounts.iter().any(|record| record.key == client.key)
            });
        *self.background.clients_mut() = keep;
        for client in gone {
            self.background.restarts.forget(&client.key);
            if let Some(live) = client.live {
                self.close_later(client.key, live, false, cx);
            }
        }
        // Under the lock the master key is cleared: a start would fail on
        // it and never be retried. The unlock starts them
        // (`resume_after_unlock`).
        if self.refused_while_locked() && !self.account.passcode.demo {
            self.spawn_background_poll(cx);
            return;
        }
        for record in accounts {
            let key = record.key;
            // A client still closing keeps its database until Closed; the
            // account starts again once it is (`reap_closing`). A folder
            // waiting to be deleted is never opened.
            // A client that could not start is tried again (it is not a
            // dead entry that blocks the account for the run): the next
            // pass replaces it. One that needs a sign-in stays as it is.
            let retry = self
                .background
                .client(&key)
                .is_some_and(BackgroundClient::retryable);
            if key == active
                || (self.background.client(&key).is_some() && !retry)
                || self.background.prepare.wanted(&key)
                || self.background.closing.may_be_open(&key)
                || self.background.deleting.contains(&key)
                || pending.contains(&key)
            {
                continue;
            }
            if retry {
                self.background
                    .clients_mut()
                    .retain(|client| client.key != key);
            }
            // Never signed in here: nothing to restore, nothing to start.
            if !AccountPaths::for_root(&root, &key).database_exists() {
                self.background.clients_mut().push(BackgroundClient {
                    key,
                    live: None,
                    status: BackgroundStatus::NeedsSignIn,
                    unread: AccountUnread::default(),
                });
                continue;
            }
            self.background.prepare.begin(&key);
            let generation = self.background.generation;
            let prepare_root = root.clone();
            let prepare_key = key.clone();
            let prepare_credentials = credentials.clone();
            let secrets = self.background.secrets.clone();
            // The database key comes from the OS keychain: off the UI thread.
            let prepared = cx.background_executor().spawn(async move {
                quill::connect::prepare_connect(
                    &prepare_root,
                    prepare_key,
                    secrets().as_ref(),
                    &prepare_credentials,
                )
            });
            let credentials = credentials.clone();
            let root = root.clone();
            // screen-scope: a background start belongs to the app's account list, not to the account on screen; its generation guard ends it on a restart.
            Self::spawn_app(cx, async move |this, cx| {
                let prepared = prepared.await;
                let _ = this.update(cx, |this, cx| {
                    let wanted = this.background.prepare.finished(&key);
                    // Stopped since: every start of that generation ends.
                    if generation != this.background.generation {
                        return;
                    }
                    // Removed since (it is no longer wanted), being wiped,
                    // switched to meanwhile, or still closing: a second
                    // client must never open the same database, nor one
                    // whose folder is being deleted.
                    if !wanted
                        || this.background.wiping()
                        || this.disk.is_full()
                        || this.background.deleting.contains(&key)
                        || active_account(&root) == key
                        || this.background.client(&key).is_some()
                        || this.background.closing.may_be_open(&key)
                    {
                        return;
                    }
                    // Prepared before a lock that cleared the key: it
                    // starts again at the unlock, not as a failed account.
                    if matches!(prepared, Err(ConnectBlocker::LockedStore))
                        && this.refused_while_locked()
                    {
                        return;
                    }
                    let started = prepared.and_then(|prepared| {
                        start_background_live_connect(credentials, prepared, Arc::new(DiscardSink))
                    });
                    this.finish_background_start(key, started, cx);
                });
            })
            .detach();
        }
        self.spawn_background_poll(cx);
    }

    fn finish_background_start(
        &mut self,
        key: AccountKey,
        started: Result<LiveConnect, ConnectBlocker>,
        cx: &mut Context<Self>,
    ) {
        let (live, status) = match started {
            Ok(mut live) => {
                self.disk.seed(&mut live.driver);
                live.bridge.wake_with(&quill::wake::BACKGROUND_UPDATES);
                (Some(live), BackgroundStatus::Connecting)
            }
            Err(ConnectBlocker::DiskFull) => {
                self.disk.refused_start();
                return;
            }
            Err(_) => {
                self.background.restarts.failed(&key, Instant::now());
                (
                    None,
                    BackgroundStatus::Failed(tr(keys::Q_STARTUP_FAILED_TITLE).into()),
                )
            }
        };
        self.background.clients_mut().push(BackgroundClient {
            key,
            live,
            status,
            unread: AccountUnread::default(),
        });
        self.spawn_background_poll(cx);
        cx.notify();
    }

    /// Close one background client without waiting for it, and end a start
    /// in flight. Before its account is removed, wait with `after_closing`
    /// so TDLib is not writing while its folder is deleted.
    pub(super) fn stop_background_account(&mut self, key: &AccountKey, cx: &mut Context<Self>) {
        self.background.prepare.cancel(key);
        self.background.closing.cancel_restart(key);
        if let Some(index) = self
            .background
            .clients
            .iter()
            .position(|client| &client.key == key)
        {
            let client = self.background.clients_mut().remove(index);
            if let Some(live) = client.live {
                self.close_later(client.key, live, false, cx);
            }
        }
    }

    fn stop_background_clients(&mut self, cx: &mut Context<Self>) {
        self.background.generation += 1;
        self.background.prepare.cancel_all();
        // The old loop ends with its task; the new generation's poll
        // drains the closing clients.
        self.background.poll = None;
        self.background.close_all_later();
        // The new generation's poll drains the closing clients.
        self.spawn_background_poll(cx);
    }

    /// Close every background client (local data is about to be wiped:
    /// wait with `after_closing` before deleting it). Never blocks.
    pub(super) fn stop_background_accounts(&mut self, cx: &mut Context<Self>) {
        self.stop_background_clients(cx);
        self.background.closing.cancel_restarts();
    }

    /// The background poll: its own loop, slower than the screen's (the
    /// updates only feed notifications and counters), ending when no
    /// background client runs.
    fn spawn_background_poll(&mut self, cx: &mut Context<Self>) {
        if self.background.poll.is_some() || !self.background.has_work() {
            return;
        }
        let generation = self.background.generation;
        // screen-scope: the poll loop drains the background accounts; its generation guard ends it on a restart.
        let poll = Self::spawn_app(cx, async move |this, cx| {
            let executor = cx.background_executor().clone();
            let hold = backlog_hold(&executor);
            run_poll_loop(
                executor,
                &quill::wake::BACKGROUND_UPDATES,
                POLL_PACE,
                hold,
                || {
                    this.update(cx, |this, cx| {
                        if generation != this.background.generation {
                            return None;
                        }
                        let work_left = this.poll_background(cx);
                        if this.background.has_work() {
                            Some(PassEnd {
                                work_left,
                                due: this.background.next_restart(Instant::now()),
                            })
                        } else {
                            this.background.poll = None;
                            None
                        }
                    })
                    .ok()
                    .flatten()
                },
            )
            .await;
        });
        self.background.poll = Some(poll);
    }

    /// A client that died or failed to start is started again once its
    /// delay is over (`restart`); nothing else re-runs the pass while the
    /// user stays on one account. One the pass cannot start now (the app is
    /// locked, or its old client is still closing) is looked at again after
    /// the first delay, not on every pass.
    pub(super) fn restart_due(&mut self, now: Instant, cx: &mut Context<Self>) {
        if self.background.next_restart(now) != Some(Duration::ZERO) {
            return;
        }
        self.ensure_background_accounts(cx);
        self.background.put_off_unstarted(now);
    }

    /// The app's proxy list and IPv6 choice: the account on screen's, once
    /// it is loaded and no change of it is in flight.
    fn app_proxy_list(&self) -> Option<(Vec<quill::proxy::ProxyEntry>, bool)> {
        let proxy = &self.live.as_ref()?.driver.session.settings.proxy;
        if proxy.busy() {
            return None;
        }
        Some((proxy.all_entries()?, proxy.prefer_ipv6))
    }

    /// Apply what every background client received, then show its
    /// notifications. Returns whether a budget left updates queued.
    fn poll_background(&mut self, cx: &mut Context<Self>) -> bool {
        let notify_from_all = self.background.notify_from_all;
        // A background account's chats are never the open one, so Quill is
        // never looking at them, focused or not. tdesktop's
        // `System::computeSkipState` has no window-focus test: only the open
        // chat of the active window drops its notifications
        // (`clearFromHistory`), so a message for another account while Quill
        // has the focus still rings. (`looked_at` is for the account on screen.)
        let app_active = false;
        let now = Instant::now();
        let mut backlog = false;
        let mut changed = false;
        let mut drained: Vec<(AccountKey, NotifyDrain)> = Vec::new();
        let proxy_target = self.app_proxy_list();
        // Clients on their way out: one non-blocking drain each.
        self.reap_closing(cx);
        let mut stopped: Vec<(AccountKey, LiveConnect)> = Vec::new();
        let mut signed_out: Vec<AccountKey> = Vec::new();
        let mut export_notes: Vec<StatusNote> = Vec::new();
        let mut ready: Vec<AccountKey> = Vec::new();
        let mut failed: Vec<AccountKey> = Vec::new();
        let mut clients = self.background.clients_mut();
        for client in clients.iter_mut() {
            let Some(live) = client.live.as_mut() else {
                continue;
            };
            live.driver.session.app_active = app_active;
            let started = Instant::now();
            let mut ingested = false;
            live.driver.begin_batch();
            while let Some(owned) = live.bridge.next_timeout(Duration::ZERO) {
                let _ = live.driver.ingest(owned);
                ingested = true;
                if started.elapsed() >= INGEST_BUDGET {
                    backlog = true;
                    break;
                }
            }
            live.driver.end_offscreen_batch();
            ingested |= live
                .driver
                .poll_ticks(
                    quill::connect::TickScope::Background,
                    now,
                    quill::net_watch::latest(),
                )
                .progressed;
            // One proxy list for the app (`quill::proxy_sync`; tdesktop keeps
            // proxies app-wide): follow the account on screen, whose
            // auto-switch picks the proxy for everyone.
            if let Some((list, prefer_ipv6)) = &proxy_target {
                ingested |= live.driver.sync_proxies_to(list, *prefer_ipv6);
            }
            export_notes.extend(take_ended_exports(&mut live.driver.session));
            export_notes.extend(
                live.driver
                    .take_finished_rich_html_exports()
                    .iter()
                    .filter_map(rich_html_note),
            );
            let mut status = background_status(&live.driver.session.auth);
            if live.bridge.stopped_unexpectedly() {
                status = BackgroundStatus::Failed(
                    tr(keys::Q_REST_CONNECTION_TO_TELEGRAM_WAS_CLOSED).into(),
                );
            }
            if ingested {
                let unread = AccountUnread::of(&live.driver.session);
                changed |= unread != client.unread;
                client.unread = unread;
                drained.push((
                    client.key.clone(),
                    drain_background(&mut live.driver.session, notify_from_all),
                ));
            }
            if status == BackgroundStatus::Ready {
                ready.push(client.key.clone());
            }
            if status == BackgroundStatus::Ready && client.status != BackgroundStatus::Ready {
                // tdesktop `Updates::updateOnline`: only the account in an
                // active window is online.
                let _ = live.driver.set_online(false);
            }
            if status != client.status {
                client.status = status.clone();
                changed = true;
            }
            if matches!(
                status,
                BackgroundStatus::SignedOut | BackgroundStatus::NeedsSignIn
            ) {
                signed_out.push(client.key.clone());
            }
            if status.stops_client()
                && let Some(live) = client.live.take()
            {
                if matches!(status, BackgroundStatus::Failed(_)) {
                    failed.push(client.key.clone());
                }
                stopped.push((client.key.clone(), live));
                client.unread = AccountUnread::default();
                changed = true;
            }
        }
        drop(clients);
        for key in &ready {
            self.background.restarts.forget(key);
        }
        for key in &failed {
            self.background.restarts.failed(key, now);
        }
        for (key, live) in stopped {
            self.background.push_closing(key, live, false);
        }
        // Logged out in the background: its next sends go with it.
        for key in signed_out {
            self.forget_account_sends(&key);
            super::notifications::forget_cached_icons();
        }
        for (key, drain) in drained {
            self.deliver_background(&key, drain, cx);
        }
        for note in export_notes {
            self.set_note(note);
            changed = true;
        }
        self.drain_account_clicks(cx);
        self.sync_background_calls(cx);
        self.restart_due(now, cx);
        if changed {
            cx.notify();
        }
        backlog
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod closing_tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod deleting_tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod disk_tests;
