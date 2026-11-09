//! "Log out" on the lock screen: the forgotten-passcode way out. It wipes
//! every account's local data, as tdesktop's logout from the passcode box
//! resets the accounts, and only a wipe that really started lifts the lock.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use gpui_kit::*;
use quill::lang::{keys, tr};
use std::path::PathBuf;

impl QuillApp {
    /// The confirm of the lock screen's Log out.
    pub(crate) fn confirm_lock_logout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.passcode_root();
        self.confirm_lock_logout_in(root, window, cx);
    }

    /// [`Self::confirm_lock_logout`] for the data under `root`. Every
    /// client closes first so TDLib is not writing while the database
    /// folders are removed; the wipe and the fresh connection wait for the
    /// close without freezing the window, and a quit before then finishes
    /// the wipe on the next start.
    ///
    /// Quill stays locked unless the wipe began: with no data folder, or
    /// when the deletion record cannot be written, the data and the
    /// passcode both stay, and lifting the lock would show them without a
    /// passcode.
    pub(in crate::ui) fn confirm_lock_logout_in(
        &mut self,
        root: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.account.passcode.is_locked() {
            return;
        }
        // The screenshot demos have no data on disk: their log out only
        // lifts the lock.
        if !self.account.passcode.demo {
            let started = match root {
                Some(root) => self.begin_local_wipe(root, cx),
                None => Err(tr(keys::Q_PREFS_NO_APP_DATA_FOLDER_IS).to_string()),
            };
            if let Err(message) = started {
                self.account.passcode.logout_confirm = false;
                self.lock_failed(message, cx);
                return;
            }
        }
        self.account.passcode.enabled = false;
        quill::wake::PASSCODE.poke();
        self.account.passcode.system_unlock = false;
        self.account.passcode.system_unlock_key = None;
        self.account.passcode.logout_confirm = false;
        self.account.passcode.deferred_connect = false;
        // What waited for the unlock was the wiped accounts'.
        self.account.passcode.unlock_intents.drop_all();
        self.set_locked(false, cx);
        self.account
            .passcode
            .lock
            .input
            .update(cx, |input, cx| input.set_value("", window, cx));
        // The demo connects to nothing; with local data, the connection
        // starts once it is wiped (`begin_local_wipe`).
        cx.notify();
    }
}
