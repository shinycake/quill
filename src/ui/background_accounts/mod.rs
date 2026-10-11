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

use super::app::QuillApp;
use gpui_kit::*;
use quill::connect::{CLIENT_CLOSE_TIMEOUT, LiveConnect, start_background_live_connect};
use quill::diagnostics::MemorySink;
use quill::ids::{AccountKey, ChatId};
use quill::notify::NotificationAction;
use quill::notify_accounts::{
    AccountUnread, BackgroundDrain, BackgroundStatus, background_status, drain_background,
    set_other_accounts_unread,
};
use quill::platform::live_secret_store;
use quill::settings::{AccountPaths, active_account, list_accounts, safe_app_root};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod account_row;
mod clicks;
mod switch;

/// Clicks on notifications of accounts that are not on screen, queued by
/// the OS callback and the Linux notification threads.
pub(crate) type AccountClicks = Arc<Mutex<Vec<(AccountKey, ChatId, NotificationAction)>>>;

/// Most time one background poll spends applying one account's updates;
/// the rest waits for the next poll (a reconnect burst must not stall the
/// window).
const INGEST_BUDGET: Duration = Duration::from_millis(4);

/// One account that is not on screen.
pub(super) struct BackgroundClient {
    pub(super) key: AccountKey,
    /// `None` once the client was closed (needs sign-in, signed out, or
    /// could not start).
    pub(super) live: Option<LiveConnect>,
    pub(super) status: BackgroundStatus,
    pub(super) unread: AccountUnread,
}

/// After a click on another account's notification: the chat to open
/// once the switch has made that account's session ready.
pub(super) struct PendingOpen {
    pub(super) account: AccountKey,
    pub(super) chat_id: ChatId,
    pub(super) action: NotificationAction,
    pub(super) until: Instant,
}

pub(crate) struct BackgroundAccounts {
    pub(super) clients: Vec<BackgroundClient>,
    /// Accounts whose database key is being read on a worker.
    starting: HashSet<AccountKey>,
    /// Bumped by `stop_background_accounts`: starts in flight and the poll
    /// loop of an older generation end.
    generation: u64,
    polling: bool,
    pub(super) clicks: AccountClicks,
    pub(super) pending_open: Option<PendingOpen>,
    /// tdesktop `notifyFromAll`, read from the account registry.
    pub(super) notify_from_all: bool,
    /// A background notification asked for the taskbar flash / Dock bounce.
    pub(super) attention: bool,
}

impl BackgroundAccounts {
    pub(super) fn new() -> Self {
        Self {
            clients: Vec::new(),
            starting: HashSet::new(),
            generation: 0,
            polling: false,
            clicks: Arc::new(Mutex::new(Vec::new())),
            pending_open: None,
            notify_from_all: true,
            attention: false,
        }
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
        !self.starting.is_empty() || self.clients.iter().any(|client| client.live.is_some())
    }

    /// Every background account's unread, combined (tdesktop
    /// `OtherAccountsUnreadState`).
    pub(super) fn total_unread(&self) -> AccountUnread {
        self.clients
            .iter()
            .fold(AccountUnread::default(), |sum, client| {
                sum.combine(client.unread)
            })
    }

    /// Ask every client to close at once; dropping them afterwards waits for
    /// all of them together instead of one after another.
    fn request_close_all(&mut self) {
        for live in self.clients.iter_mut().filter_map(|c| c.live.as_mut()) {
            let _ = live.driver.request_close();
        }
    }

    /// Close every client and forget them.
    fn close_all(&mut self) {
        self.request_close_all();
        for client in &mut self.clients {
            if let Some(live) = client.live.as_mut() {
                live.shutdown(CLIENT_CLOSE_TIMEOUT);
            }
        }
        self.clients.clear();
    }
}

impl Drop for BackgroundAccounts {
    /// Quitting closes every background client (GPUI drops the window's
    /// views on quit); a TDLib client left open can crash the process on
    /// its way out.
    fn drop(&mut self) {
        self.close_all();
    }
}

impl QuillApp {
    /// Start a background client for every registered account that is not
    /// on screen and has a local database; forget clients of accounts that
    /// were removed or are on screen now. Idempotent: runs after every
    /// connect and switch.
    pub(super) fn ensure_background_accounts(&mut self, cx: &mut Context<Self>) {
        if self.demo_session.is_some() {
            return;
        }
        let (Some(root), Some(credentials)) = (safe_app_root(), self.credentials.clone()) else {
            return;
        };
        self.background.notify_from_all = quill::settings::notify_from_all(&root);
        let active = active_account(&root);
        let accounts = list_accounts(&root);
        self.background.clients.retain(|client| {
            client.key != active && accounts.iter().any(|record| record.key == client.key)
        });
        for record in accounts {
            let key = record.key;
            if key == active
                || self.background.client(&key).is_some()
                || self.background.starting.contains(&key)
            {
                continue;
            }
            // Never signed in here: nothing to restore, nothing to start.
            if !AccountPaths::for_root(&root, &key).database_exists() {
                self.background.clients.push(BackgroundClient {
                    key,
                    live: None,
                    status: BackgroundStatus::NeedsSignIn,
                    unread: AccountUnread::default(),
                });
                continue;
            }
            self.background.starting.insert(key.clone());
            let generation = self.background.generation;
            let prepare_root = root.clone();
            let prepare_key = key.clone();
            let prepare_credentials = credentials.clone();
            // The database key comes from the OS keychain: off the UI thread.
            let prepared = cx.background_executor().spawn(async move {
                quill::connect::prepare_connect(
                    &prepare_root,
                    prepare_key,
                    live_secret_store().as_ref(),
                    &prepare_credentials,
                )
            });
            let credentials = credentials.clone();
            let root = root.clone();
            cx.spawn(async move |this, cx| {
                let prepared = prepared.await;
                let _ = this.update(cx, |this, cx| {
                    this.background.starting.remove(&key);
                    // Stopped since, or switched to meanwhile: a second client
                    // must never open the same database.
                    if generation != this.background.generation
                        || active_account(&root) == key
                        || this.background.client(&key).is_some()
                    {
                        return;
                    }
                    let started = prepared.and_then(|prepared| {
                        start_background_live_connect(
                            credentials,
                            prepared,
                            Arc::new(MemorySink::new()),
                        )
                    });
                    let (live, status) = match started {
                        Ok(live) => (Some(live), BackgroundStatus::Connecting),
                        Err(blocker) => (
                            None,
                            BackgroundStatus::Failed(blocker.user_message().to_string()),
                        ),
                    };
                    this.background.clients.push(BackgroundClient {
                        key,
                        live,
                        status,
                        unread: AccountUnread::default(),
                    });
                    this.spawn_background_poll(cx);
                    cx.notify();
                });
            })
            .detach();
        }
        self.spawn_background_poll(cx);
    }

    /// Close one background client (before its account is removed, so
    /// TDLib is not writing while its folder is deleted).
    pub(super) fn stop_background_account(&mut self, key: &AccountKey) {
        self.background.starting.remove(key);
        if let Some(index) = self
            .background
            .clients
            .iter()
            .position(|client| &client.key == key)
        {
            let mut client = self.background.clients.remove(index);
            if let Some(live) = client.live.as_mut() {
                live.shutdown(CLIENT_CLOSE_TIMEOUT);
            }
        }
        self.publish_background_unread();
    }

    /// Close every background client (local data is about to be wiped).
    pub(super) fn stop_background_accounts(&mut self) {
        self.background.generation += 1;
        self.background.starting.clear();
        self.background.polling = false;
        self.background.close_all();
        self.publish_background_unread();
    }

    fn publish_background_unread(&self) {
        set_other_accounts_unread(self.background.total_unread());
    }

    /// The background poll: its own loop, slower than the screen's (the
    /// updates only feed notifications and counters), ending when no
    /// background client runs.
    fn spawn_background_poll(&mut self, cx: &mut Context<Self>) {
        if self.background.polling || !self.background.has_work() {
            return;
        }
        self.background.polling = true;
        let generation = self.background.generation;
        cx.spawn(async move |this, cx| {
            const BUSY: Duration = Duration::from_millis(40);
            const IDLE_MAX: Duration = Duration::from_millis(400);
            let mut delay = Duration::from_millis(100);
            loop {
                cx.background_executor().timer(delay).await;
                let step = this
                    .update(cx, |this, cx| {
                        if generation != this.background.generation {
                            return None;
                        }
                        let busy = this.poll_background(cx);
                        if this.background.has_work() {
                            Some(busy)
                        } else {
                            this.background.polling = false;
                            None
                        }
                    })
                    .ok()
                    .flatten();
                match step {
                    None => break,
                    Some(true) => delay = BUSY,
                    Some(false) => delay = (delay * 2).clamp(BUSY, IDLE_MAX),
                }
            }
        })
        .detach();
    }

    /// Apply what every background client received, then show its
    /// notifications. Returns whether anything arrived.
    fn poll_background(&mut self, cx: &mut Context<Self>) -> bool {
        let notify_from_all = self.background.notify_from_all;
        let app_active = self.frame.window_active.get();
        let now = Instant::now();
        let mut busy = false;
        let mut changed = false;
        let mut drained: Vec<(AccountKey, BackgroundDrain)> = Vec::new();
        for client in &mut self.background.clients {
            let Some(live) = client.live.as_mut() else {
                continue;
            };
            live.driver.session.app_active = app_active;
            let started = Instant::now();
            let mut ingested = false;
            while let Some(owned) = live.bridge.next_timeout(Duration::ZERO) {
                let _ = live.driver.ingest(owned);
                ingested = true;
                if started.elapsed() >= INGEST_BUDGET {
                    break;
                }
            }
            ingested |= live.driver.request_tick(now);
            busy |= ingested;
            let mut status = background_status(&live.driver.session.auth);
            if live.bridge.stopped_unexpectedly() {
                status = BackgroundStatus::Failed("Connection to Telegram was closed".into());
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
            if status == BackgroundStatus::Ready && client.status != BackgroundStatus::Ready {
                // tdesktop `Updates::updateOnline`: only the account in an
                // active window is online.
                let _ = live.driver.set_online(false);
            }
            if status != client.status {
                client.status = status.clone();
                changed = true;
            }
            if status.stops_client()
                && let Some(mut live) = client.live.take()
            {
                live.shutdown(CLIENT_CLOSE_TIMEOUT);
                client.unread = AccountUnread::default();
                changed = true;
            }
        }
        if changed {
            self.publish_background_unread();
        }
        for (key, drain) in drained {
            self.deliver_background(&key, drain, cx);
        }
        self.drain_account_clicks(cx);
        if changed {
            cx.notify();
        }
        busy
    }
}

#[cfg(all(test, feature = "demo-capture"))]
mod tests;
