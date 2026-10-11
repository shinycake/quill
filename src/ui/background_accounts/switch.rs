//! Switching accounts without restarting either client: the outgoing
//! account's client keeps running in the background and the incoming one,
//! when it already runs there, comes forward with its session as it is.

use super::*;
use crate::ui::connect_ui::{ConnectUiStatus, live_status_for};

impl QuillApp {
    /// Hand the account on screen to the background instead of closing it.
    /// Its open chat is closed in TDLib and it goes offline, as tdesktop's
    /// inactive accounts are. A client in the sign-in flow, or in a call
    /// (which would go on with no window to hang it up), is closed instead.
    pub(in crate::ui) fn retire_live_to_background(&mut self) {
        let Some(mut live) = self.live.take() else {
            return;
        };
        let key = live.driver.session.account.clone();
        let status = background_status(&live.driver.session.auth);
        let in_call = live.driver.session.calls.active_call.is_some()
            || live.driver.session.calls.active_group_call.is_some();
        self.background.clients.retain(|client| client.key != key);
        if !status.stops_client() && !in_call && !live.bridge.stopped_unexpectedly() {
            let _ = live.driver.deselect_chat();
            let _ = live.driver.set_online(false);
            let unread = AccountUnread::of(&live.driver.session);
            self.background.clients.push(BackgroundClient {
                key,
                live: Some(live),
                status,
                unread,
            });
        } else {
            live.shutdown(CLIENT_CLOSE_TIMEOUT);
            // An account closed for a call starts again in the background
            // (`ensure_background_accounts`); one that needs a sign-in keeps
            // only its status until it is switched to.
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
    pub(in crate::ui) fn take_background_live(&mut self, key: &AccountKey) -> Option<LiveConnect> {
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
        if let Some(live) = client.live.as_mut() {
            live.shutdown(CLIENT_CLOSE_TIMEOUT);
        }
        None
    }

    /// Put a running client on screen: what `start_connection` does once
    /// its client exists, minus the restart. The chat list, folders and
    /// counters are there at once.
    pub(in crate::ui) fn adopt_live(&mut self, mut live: LiveConnect, cx: &mut Context<Self>) {
        self.connection.generation += 1;
        self.connection.status = ConnectUiStatus::Live;
        self.connection.lost = false;
        self.connection.presence.reset();
        let auth = live.driver.session.auth.clone();
        self.connection.status_note = live_status_for(&auth);
        self.auth_ui.demo_state = auth;
        live.attach_call_engine(&MemorySink::new());
        self.live = Some(live);
        self.reload_account_keybindings(cx);
        self.spawn_poll_loop(cx);
        self.ensure_background_accounts(cx);
        cx.notify();
    }
}
