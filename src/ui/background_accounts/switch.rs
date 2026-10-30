//! Switching accounts without restarting either client: the outgoing
//! account's client keeps running in the background and the incoming one,
//! when it already runs there, comes forward with its session as it is.

use super::*;
use crate::ui::connect_ui::{ConnectUiStatus, live_status_for};
use crate::ui::status_note::StatusNote;

impl QuillApp {
    /// Hand the account on screen to the background instead of closing it.
    /// Its open chat is closed in TDLib and it goes offline, as tdesktop's
    /// inactive accounts are. A client in the sign-in flow, or in a call
    /// (which would go on with no window to hang it up), is closed instead.
    pub(in crate::ui) fn retire_live_to_background(&mut self, cx: &mut Context<Self>) {
        self.leave_live_story_for_switch(cx);
        let Some(mut live) = self.live.take() else {
            return;
        };
        let key = live.driver.session.account.clone();
        let status = background_status(&live.driver.session.auth);
        let in_call = live.driver.session.calls.active_call.is_some()
            || live.driver.session.calls.active_group_call.is_some();
        self.background.clients.retain(|client| client.key != key);
        if !status.stops_client() && !in_call && !live.bridge.stopped_unexpectedly() {
            let _ = live.driver.leave_for_background();
            let _ = live.driver.set_online(false);
            let unread = AccountUnread::of(&live.driver.session);
            self.background.clients.push(BackgroundClient {
                key,
                live: Some(live),
                status,
                unread,
            });
        } else {
            // Closed without waiting. An account closed for a call starts
            // again in the background once Closed (`reap_closing`); one that
            // needs a sign-in keeps only its status until it is switched to.
            self.close_live_later(live, !status.stops_client(), cx);
            if status.stops_client() {
                self.background.clients.push(BackgroundClient {
                    key,
                    live: None,
                    status,
                    unread: AccountUnread::default(),
                });
            }
        }
        self.publish_background_unread();
    }

    /// Take `key`'s background client when it runs (signed in, or still
    /// restoring its session), so the switch can bring it forward. A dead
    /// one is closed instead: two clients must never open one database.
    pub(in crate::ui) fn take_background_live(
        &mut self,
        key: &AccountKey,
        cx: &mut Context<Self>,
    ) -> Option<LiveConnect> {
        self.background.starting.remove(key);
        let index = self
            .background
            .clients
            .iter()
            .position(|client| &client.key == key)?;
        let mut client = self.background.clients.remove(index);
        self.publish_background_unread();
        let usable = !client.status.stops_client()
            && client
                .live
                .as_ref()
                .is_some_and(|live| !live.bridge.stopped_unexpectedly());
        if usable {
            return client.live.take();
        }
        // Closed without waiting; `start_connection` opens the database
        // again only once it is (`closing::closed`).
        if let Some(live) = client.live.take() {
            self.close_later(client.key, live, false, cx);
        }
        None
    }

    /// Put a running client on screen: what `start_connection` does once
    /// its client exists, minus the restart. The chat list, folders and
    /// counters are there at once.
    pub(in crate::ui) fn adopt_live(&mut self, mut live: LiveConnect, cx: &mut Context<Self>) {
        self.connection.generation += 1;
        self.connection.status = ConnectUiStatus::Live;
        self.connection.presence.reset();
        let auth = live.driver.session.auth.clone();
        self.connection
            .set_note(StatusNote::silent(live_status_for(&auth)));
        self.auth_ui.demo_state = auth;
        live.attach_call_engine(&DiscardSink);
        // The auto-switch runs on screen; its settings are the app's.
        live.driver.reload_proxy_prefs();
        live.driver.set_stream_volume(self.playback.volume);
        self.live = Some(live);
        self.reload_account_keybindings(cx);
        self.spawn_poll_loop(cx);
        self.ensure_background_accounts(cx);
        cx.notify();
    }
}
