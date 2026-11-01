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
use quill::connect::{QUIT_CLOSE_DEADLINE, deadline_after};

/// How often a waiter checks the closing set again.
const CLOSE_POLL: Duration = Duration::from_millis(16);

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
        self.closing.push(key, Box::new(live), restart);
    }

    /// Hand every running client to the closing set and forget them all.
    /// Never blocks.
    pub(super) fn close_all_later(&mut self) {
        for client in std::mem::take(&mut self.clients) {
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

    /// Close the client on screen, if any, without waiting for it.
    pub(in crate::ui) fn close_screen_client(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.take() {
            self.close_live_later(live, false, cx);
        }
    }

    /// Drain the closing clients once. Accounts closed only for a call
    /// start again in the background. Returns whether any finished.
    pub(super) fn reap_closing(&mut self, cx: &mut Context<Self>) -> bool {
        let done = self.background.closing.poll(Instant::now());
        if done.iter().any(|closed| closed.restart && closed.confirmed) {
            self.ensure_background_accounts(cx);
        }
        !done.is_empty()
    }

    /// Run `then` once `wait` says go (`then(.., true)`) or never
    /// (`then(.., false)`). The check and `then` happen in the same update,
    /// so nothing can slip in between. Drives the drain itself, so it works
    /// with no background poll.
    pub(in crate::ui) fn after_closing(
        &mut self,
        cx: &mut Context<Self>,
        wait: impl Fn(&QuillApp) -> CloseWait + 'static,
        then: impl FnOnce(&mut QuillApp, bool, &mut Context<QuillApp>) + 'static,
    ) {
        let mut then = Some(then);
        Self::spawn_main(cx, async move |this, cx| {
            loop {
                let finished = this.update(cx, |this, cx| {
                    this.reap_closing(cx);
                    let go = match wait(this) {
                        CloseWait::Wait => return false,
                        CloseWait::Go => true,
                        CloseWait::Never => false,
                    };
                    if let Some(then) = then.take() {
                        then(this, go, cx);
                    }
                    true
                });
                match finished {
                    Ok(false) => cx.background_executor().timer(CLOSE_POLL).await,
                    Ok(true) | Err(_) => break,
                }
            }
        })
        .detach();
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
    loop {
        let wait = this
            .update(cx, |this, cx| {
                this.reap_closing(cx);
                this.open_wait(key)
            })
            .unwrap_or(CloseWait::Never);
        match wait {
            CloseWait::Go => return true,
            CloseWait::Never => return false,
            CloseWait::Wait => cx.background_executor().timer(CLOSE_POLL).await,
        }
    }
}
