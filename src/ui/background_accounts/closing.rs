//! Closing TDLib clients without freezing the window
//! (`quill::connect::ClosingSet`): a client on its way out is sent `close`
//! and parked in `BackgroundAccounts::closing`; the background poll (and
//! anyone waiting on a closed account) drains it a little per tick until
//! TDLib confirms `authorizationStateClosed`. Work that must not overlap
//! the close (opening the same database, deleting it) waits for it, the
//! way Telegram X postpones such work with `Tdlib.awaitClose`.
//!
//! On quit every client, the one on screen included, closes together
//! under one shared deadline (`QUIT_CLOSE_DEADLINE`).

use super::*;
use crate::ui::chat_window::ScopedApp;
use gpui_kit::AsyncApp;
use quill::connect::{EXPORT_STOP_HOLD, QUIT_CLOSE_DEADLINE, deadline_after};

/// How long a waiter waits before it gives up (about a minute): a close or
/// a preparation that never ends reads as `Never` then, so the work it
/// holds up is deferred to the next start instead of waiting for good.
const CLOSE_GIVE_UP: Duration = Duration::from_secs(60);

type CloseWaitFn = Box<dyn Fn(&QuillApp) -> CloseWait>;
type CloseThenFn = Box<dyn FnOnce(&mut QuillApp, bool, &mut Context<QuillApp>)>;

/// Work waiting on closing clients (`after_closing`). A waiter is looked at
/// when something it waits on changes (`settle_close_waiters`: the
/// background pass that drains the closing set, a deletion or a preparation
/// ending), and its own `cap` task is the one way out of a wait that never
/// ends. Dropping the waiter drops the cap.
#[derive(Default)]
pub(super) struct CloseWaiters {
    next: u64,
    held: Vec<CloseWaiter>,
}

struct CloseWaiter {
    id: u64,
    wait: CloseWaitFn,
    then: CloseThenFn,
    _cap: Task<()>,
}

impl CloseWaiters {
    fn next_id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    fn hold(&mut self, waiter: CloseWaiter) {
        self.held.push(waiter);
    }

    fn take_all(&mut self) -> Vec<CloseWaiter> {
        std::mem::take(&mut self.held)
    }

    /// The waiters that still wait go back before any added meanwhile.
    fn put_back(&mut self, mut waiting: Vec<CloseWaiter>) {
        waiting.append(&mut self.held);
        self.held = waiting;
    }

    fn take(&mut self, id: u64) -> Option<CloseWaiter> {
        let at = self.held.iter().position(|waiter| waiter.id == id)?;
        Some(self.held.remove(at))
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.held.len()
    }
}

/// What work waiting on closing clients should do now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum CloseWait {
    /// A client it depends on is still closing.
    Wait,
    /// Every client it depends on is closed: go ahead.
    Go,
    /// A client's close can no longer be confirmed in this run (its
    /// receive loop died, or quit cut it off): never go ahead.
    Never,
}

impl BackgroundAccounts {
    /// Send `close` to `live` and keep it until Closed. The caller makes
    /// sure something drains it (`QuillApp::close_later`).
    pub(super) fn push_closing(&mut self, key: AccountKey, live: LiveConnect, restart: bool) {
        // Only the background poll drains closing clients (`reap_closing`).
        live.bridge.wake_with(&quill::wake::BACKGROUND_UPDATES);
        self.closing.push(key, Box::new(live), restart);
    }

    pub(in crate::ui) fn live_clients_mut(&mut self) -> impl Iterator<Item = &mut LiveConnect> {
        self.clients
            .iter_mut()
            .filter_map(|client| client.live.as_mut())
    }

    /// Hand every running client to the closing set and forget them all.
    /// Never blocks.
    pub(super) fn close_all_later(&mut self) {
        let clients = std::mem::take(&mut *self.clients_mut());
        for client in clients {
            if let Some(live) = client.live {
                self.push_closing(client.key, live, false);
            }
        }
    }

    /// Quit (or the last-resort drop): close everything together, at most
    /// until `deadline`.
    pub(super) fn close_all_within(&mut self, deadline: Instant) {
        self.close_all_later();
        let _ = self.closing.close_all_within(deadline);
    }
}

impl QuillApp {
    /// Send `close` to `live` and keep draining it in the background until
    /// Closed. Never blocks.
    pub(in crate::ui) fn close_later(
        &mut self,
        key: AccountKey,
        live: LiveConnect,
        restart: bool,
        cx: &mut Context<Self>,
    ) {
        self.background.push_closing(key, live, restart);
        self.spawn_background_poll(cx);
    }

    /// Close the client that was on screen without waiting for it.
    /// `restart`: start it again in the background once it is closed.
    pub(in crate::ui) fn close_live_later(
        &mut self,
        live: LiveConnect,
        restart: bool,
        cx: &mut Context<Self>,
    ) {
        let key = live.driver.session.account.clone();
        self.close_later(key, live, restart, cx);
    }

    /// Close the client on screen, if any, without waiting for it. The
    /// screen's UI goes with it at the end of this update
    /// (`screen_leaves_later`): the callers have no window, and the
    /// client (logging out, or on a full disk) has nothing to say.
    pub(in crate::ui) fn close_screen_client(&mut self, cx: &mut Context<Self>) {
        self.screen_leaves_later(cx);
        if let Some(live) = self.take_live() {
            self.close_live_later(live, false, cx);
        }
    }

    /// Close every background client, keeping each one's restart.
    pub(in crate::ui) fn close_background_clients_later(&mut self, cx: &mut Context<Self>) {
        self.stop_background_clients(cx);
        self.background.closing.restart_all();
    }

    /// Drain the closing clients once. Accounts closed only for a call
    /// start again in the background.
    pub(super) fn reap_closing(&mut self, cx: &mut Context<Self>) {
        let done = self.background.closing.poll(Instant::now());
        if done.iter().any(|closed| closed.restart && closed.confirmed) {
            self.ensure_background_accounts(cx);
        }
    }

    /// Run `then` once `wait` says go (`then(.., true)`) or never
    /// (`then(.., false)`, also when the wait lasts about a minute). The
    /// check and `then` happen in the same update, so nothing can slip in
    /// between. The waiter is looked at when what it waits on changes
    /// (`settle_close_waiters`), not on a clock; this makes sure the pass
    /// that drains the closing clients runs.
    pub(in crate::ui) fn after_closing(
        &mut self,
        cx: &mut Context<Self>,
        wait: impl Fn(&QuillApp) -> CloseWait + 'static,
        then: impl FnOnce(&mut QuillApp, bool, &mut Context<QuillApp>) + 'static,
    ) {
        let id = self.background.close_waiters.next_id();
        // screen-scope: the wait covers the closing clients of every account, and its follow-up acts on whatever is on screen then.
        let cap = Self::spawn_app(cx, async move |this, cx| {
            cx.background_executor().timer(CLOSE_GIVE_UP).await;
            let _ = this.update(cx, |this, cx| {
                if let Some(waiter) = this.background.close_waiters.take(id) {
                    (waiter.then)(this, false, cx);
                }
            });
        });
        self.background.close_waiters.hold(CloseWaiter {
            id,
            wait: Box::new(wait),
            then: Box::new(then),
            _cap: cap,
        });
        self.spawn_background_poll(cx);
        // After the caller's update, which may not expect `then` yet.
        let this = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = this.update(cx, |this, cx| this.settle_close_waiters(cx));
        });
    }

    /// Run the waiters whose wait is over. Called when a closing client
    /// finished, a deletion or a preparation ended.
    pub(in crate::ui) fn settle_close_waiters(&mut self, cx: &mut Context<Self>) {
        let mut waiting = Vec::new();
        for waiter in self.background.close_waiters.take_all() {
            match (waiter.wait)(self) {
                CloseWait::Wait => waiting.push(waiter),
                CloseWait::Go => (waiter.then)(self, true, cx),
                CloseWait::Never => (waiter.then)(self, false, cx),
            }
        }
        self.background.close_waiters.put_back(waiting);
    }

    /// Whether `key`'s database can be opened now: not while its old
    /// client may still be closing or its folder is being deleted.
    pub(in crate::ui) fn open_wait(&self, key: &AccountKey) -> CloseWait {
        if self.background.closing.unconfirmed(key) {
            CloseWait::Never
        } else if self.background.closing.contains(key) || self.background.deleting.contains(key) {
            CloseWait::Wait
        } else {
            CloseWait::Go
        }
    }

    /// Close everything for quit: the client on screen and every
    /// background one, together, at most until `deadline`. Folders waiting
    /// for a close stay recorded and are deleted on the next start.
    pub(in crate::ui) fn close_all_for_quit(&mut self, deadline: Instant) {
        self.stop_exports_and_wait(deadline_after(EXPORT_STOP_HOLD).min(deadline));
        if let Some(live) = self.live.take() {
            let key = live.driver.session.account.clone();
            self.background.push_closing(key, live, false);
        }
        self.background.close_all_within(deadline);
        quill::calls::engine::await_call_teardown(deadline);
    }

    /// The window's app is dropped without the quit observer having run:
    /// the same shared deadline, never one wait per client.
    pub(in crate::ui) fn close_clients_on_drop(&mut self) {
        self.close_all_for_quit(deadline_after(QUIT_CLOSE_DEADLINE));
    }

    /// Close every client when the app quits (once per app).
    pub(super) fn watch_quit_close(&mut self, cx: &mut Context<Self>) {
        if self.background.quit_close.is_some() {
            return;
        }
        self.background.quit_close = Some(cx.on_app_quit(|this, _| {
            this.close_all_for_quit(deadline_after(QUIT_CLOSE_DEADLINE));
            async {}
        }));
    }
}

/// Wait (without blocking the window) until `key`'s old client finished
/// closing, so a new client never opens a database that is still closing.
/// False: that can't be confirmed in this run, so never open it.
pub(in crate::ui) async fn closed(this: &ScopedApp, key: &AccountKey, cx: &mut AsyncApp) -> bool {
    let (answer, answered) = futures_channel::oneshot::channel();
    let key = key.clone();
    let asked = this.update(cx, |this, cx| {
        this.after_closing(
            cx,
            move |app| app.open_wait(&key),
            move |_, go, _| {
                let _ = answer.send(go);
            },
        );
    });
    if asked.is_err() {
        return false;
    }
    answered.await.unwrap_or(false)
}
