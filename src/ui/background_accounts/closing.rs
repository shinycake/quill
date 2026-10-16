//! Closing TDLib clients without freezing the window
//! (`quill::connect::ClosingSet`): a client on its way out is sent `close`
//! and parked in `BackgroundAccounts::closing`; the background poll (and
//! anyone waiting on a closed account) drains it a little per tick until
//! `authorizationStateClosed` or its deadline. Work that must not overlap
//! the close (opening the same database, deleting it) waits for it, the
//! way Telegram X postpones such work with `Tdlib.awaitClose`.
//!
//! On quit every client, the one on screen included, closes together
//! under one shared deadline (`QUIT_CLOSE_DEADLINE`).

use super::*;
use gpui_kit::{AsyncApp, WeakEntity};
use quill::connect::{CLOSE_GRACE, ClosingSet, QUIT_CLOSE_DEADLINE, deadline_after};

/// How often a waiter checks the closing set again.
const CLOSE_POLL: Duration = Duration::from_millis(16);

impl BackgroundAccounts {
    /// Send `close` to `live` and keep draining it in the background.
    pub(super) fn close_later(&mut self, key: AccountKey, live: LiveConnect, restart: bool) {
        self.closing
            .push(key, Box::new(live), restart, deadline_after(CLOSE_GRACE));
    }

    /// Hand every running client to the closing set and forget them all.
    /// Never blocks.
    pub(super) fn close_all_later(&mut self) {
        for client in std::mem::take(&mut self.clients) {
            if let Some(live) = client.live {
                self.close_later(client.key, live, false);
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
    /// Close the client that was on screen without waiting for it.
    /// `restart`: start it again in the background once it is closed.
    pub(in crate::ui) fn close_live_later(&mut self, live: LiveConnect, restart: bool) {
        let key = live.driver.session.account.clone();
        self.background.close_later(key, live, restart);
    }

    /// Drain the closing clients once. Accounts closed only for a call
    /// start again in the background. Returns whether any finished.
    pub(super) fn reap_closing(&mut self, cx: &mut Context<Self>) -> bool {
        let done = self.background.closing.poll(Instant::now());
        if done.iter().any(|closed| closed.restart) {
            self.ensure_background_accounts(cx);
        }
        !done.is_empty()
    }

    /// Run `then` once `ready` holds for the closing set (checked, and
    /// `then` run, in the same update, so nothing can slip in between).
    /// Drives the drain itself, so it works with no background poll.
    pub(in crate::ui) fn after_closing(
        &mut self,
        cx: &mut Context<Self>,
        ready: impl Fn(&ClosingSet) -> bool + 'static,
        then: impl FnOnce(&mut QuillApp, &mut Context<QuillApp>) + 'static,
    ) {
        let mut then = Some(then);
        cx.spawn(async move |this, cx| {
            loop {
                let finished = this.update(cx, |this, cx| {
                    this.reap_closing(cx);
                    if !ready(&this.background.closing) {
                        return false;
                    }
                    if let Some(then) = then.take() {
                        then(this, cx);
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

    /// `key` runs again (on screen, in the background, or starting): its
    /// folder must stay.
    pub(in crate::ui) fn account_in_use(&self, root: &std::path::Path, key: &AccountKey) -> bool {
        active_account(root) == *key
            || self
                .live
                .as_ref()
                .is_some_and(|live| &live.driver.session.account == key)
            || self
                .background
                .client(key)
                .is_some_and(|client| client.live.is_some())
            || self.background.starting.contains(key)
            || self.background.closing.contains(key)
    }

    /// Close everything for quit: the client on screen and every
    /// background one, together, at most until `deadline`.
    pub(in crate::ui) fn close_all_for_quit(&mut self, deadline: Instant) {
        if let Some(live) = self.live.take() {
            self.close_live_later(live, false);
        }
        self.background.close_all_within(deadline);
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
pub(in crate::ui) async fn closed(
    this: &WeakEntity<QuillApp>,
    key: &AccountKey,
    cx: &mut AsyncApp,
) {
    loop {
        let closing = this
            .update(cx, |this, cx| {
                this.reap_closing(cx);
                this.background.closing.contains(key)
            })
            .unwrap_or(false);
        if !closing {
            return;
        }
        cx.background_executor().timer(CLOSE_POLL).await;
    }
}
