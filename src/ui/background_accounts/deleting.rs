//! Deleting account folders: removing an account, and the lock-screen
//! "Log out" that wipes every account's local data.
//!
//! A folder is deleted only once no TDLib client may still have it open:
//! its client confirmed `authorizationStateClosed` and no preparation
//! (which creates the folders) is still running for it. The folders are
//! recorded in `accounts.json` first (`quill::settings::pending_deletes`),
//! so a removal or log-out whose clients were still closing when the app
//! quit finishes on the next start, before any client opens. The deletion
//! itself runs on a worker: a TDLib folder with cached media can be large.

use super::closing::CloseWait;
use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::tr_with;
use quill::platform::SecretStore;
use quill::settings::{
    begin_remove_account, delete_account_folder, finish_pending_delete, mark_for_deletion,
};
use std::path::{Path, PathBuf};

/// Opens the keychain that holds the account database keys (tests use an
/// in-memory one). Called on workers.
pub(in crate::ui) type SecretsFactory = Arc<dyn Fn() -> Box<dyn SecretStore> + Send + Sync>;

pub(in crate::ui) fn live_secrets() -> SecretsFactory {
    Arc::new(quill::platform::live_secret_store)
}

/// How a deletion ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) enum Deletion {
    /// Every folder is gone, or `error` says why one is not (it stays
    /// recorded and the next start tries again).
    Done { error: Option<String> },
    /// A client's close can't be confirmed in this run: the folders stay
    /// recorded and the next start deletes them.
    Deferred,
}

/// Delete `key`'s folder and its database key. Never while a client of
/// `key` may still be open.
fn delete_account_data(
    root: &Path,
    key: &AccountKey,
    store: &dyn SecretStore,
) -> std::io::Result<()> {
    delete_account_folder(root, key)?;
    // Best effort: a locked keychain leaves an orphaned item, not data.
    // Without this the lowest-free-id recycling in `add_account` would
    // hand a later account the deleted account's stale key.
    let _ = store.delete(key);
    Ok(())
}

/// Folders an earlier run left for deletion (it quit before their
/// clients had closed), deleted on a worker before the first client opens.
pub(in crate::ui) struct Sweep {
    root: PathBuf,
    /// The recorded folders to delete, read once when the sweep began and
    /// already without the accounts this run still has a client or a
    /// deletion for. A removal after that is not the sweep's business.
    keys: Vec<AccountKey>,
    secrets: SecretsFactory,
}

impl Sweep {
    /// Every folder recorded for deletion under `root`: the run's first
    /// sweep, before any client exists.
    pub(in crate::ui) fn pending(root: PathBuf, secrets: SecretsFactory) -> Self {
        Self {
            keys: pending_deletes(&root),
            root,
            secrets,
        }
    }

    /// Returns the keys whose folders are gone.
    pub(in crate::ui) fn run(self) -> (PathBuf, Vec<AccountKey>) {
        let store = (self.secrets)();
        let mut deleted = Vec::new();
        for key in self.keys {
            if delete_account_data(&self.root, &key, store.as_ref()).is_ok() {
                deleted.push(key);
            }
        }
        (self.root, deleted)
    }
}

impl QuillApp {
    /// The app root account work uses (tests point it at a temp dir).
    pub(in crate::ui) fn accounts_root(&self) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(root) = &self.background.test_root {
            return Some(root.clone());
        }
        quill::settings::safe_app_root()
    }

    /// The app root clients open accounts under: none in a demo.
    pub(super) fn client_accounts_root(&self) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(root) = &self.background.test_root {
            return Some(root.clone());
        }
        if self.demo_session.is_some() {
            return None;
        }
        quill::settings::safe_app_root()
    }

    /// The keychain factory, for a preparation on a worker.
    pub(in crate::ui) fn secrets(&self) -> SecretsFactory {
        self.background.secrets.clone()
    }

    /// Once per run, at the first connection: the folders an earlier run
    /// left for deletion.
    pub(in crate::ui) fn startup_sweep(&mut self) -> Option<Sweep> {
        if self.background.swept {
            return None;
        }
        self.background.swept = true;
        let root = self.client_accounts_root()?;
        let mut skip: HashSet<AccountKey> = self.background.deleting.clone();
        skip.extend(self.background.closing.keys());
        skip.extend(self.background.starting.iter().cloned());
        skip.extend(self.background.preparing.keys().cloned());
        skip.extend(self.background.clients.iter().map(|c| c.key.clone()));
        skip.extend(
            self.live
                .iter()
                .map(|live| live.driver.session.account.clone()),
        );
        let keys = pending_deletes(&root)
            .into_iter()
            .filter(|key| !skip.contains(key))
            .collect();
        Some(Sweep {
            root,
            keys,
            secrets: self.secrets(),
        })
    }

    /// The sweep deleted these folders: forget their records (on the UI
    /// thread, like every other `accounts.json` write).
    pub(in crate::ui) fn finish_sweep(&mut self, root: &Path, deleted: &[AccountKey]) {
        for key in deleted {
            let _ = finish_pending_delete(root, key);
        }
    }

    /// Remove `key` (never the account on screen): it leaves the account
    /// list at once, its client closes without blocking the window, and
    /// its folder and keychain item go once TDLib has closed it.
    pub(in crate::ui) fn remove_account_in(
        &mut self,
        root: PathBuf,
        key: &AccountKey,
        cx: &mut Context<Self>,
    ) {
        self.account.accounts.remove_confirm = None;
        // A second click on Remove while the first is still closing.
        if self.background.deleting.contains(key)
            || self
                .live
                .as_ref()
                .is_some_and(|live| &live.driver.session.account == key)
        {
            cx.notify();
            return;
        }
        match begin_remove_account(&root, key) {
            Ok(true) => self.account.accounts.error = None,
            Ok(false) => {
                cx.notify();
                return;
            }
            Err(e) => {
                self.account.accounts.error = Some(tr_with(
                    keys::Q_OPS_COULD_NOT_REMOVE_THE_ACCOUNT,
                    &[("e", &e.to_string())],
                ));
                cx.notify();
                return;
            }
        }
        // Its Picture-in-Picture plays one of its files.
        self.video_pip_account_ended(key, cx);
        self.stop_background_account(key, cx);
        self.delete_after_close(
            root,
            vec![key.clone()],
            |this, deletion, _| {
                if let Deletion::Done { error: Some(e) } = deletion {
                    this.account.accounts.error = Some(tr_with(
                        keys::Q_OPS_COULD_NOT_REMOVE_THE_ACCOUNT,
                        &[("e", &e)],
                    ));
                }
            },
            cx,
        );
        cx.notify();
    }

    /// The lock-screen log out: close every client and delete every
    /// account's local data once they have closed, then connect afresh.
    /// The passcode goes at once, and the folders are recorded first, so a
    /// quit before the clients have closed still logs out.
    pub(in crate::ui) fn begin_local_wipe(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.background.wiping {
            return;
        }
        let accounts: Vec<AccountKey> = list_accounts(&root).into_iter().map(|r| r.key).collect();
        if let Err(e) = mark_for_deletion(&root, &accounts) {
            // Without the record, a quit before the clients close would
            // keep the data but lose the passcode: keep both.
            self.connection.set_note(StatusNote::error(tr_with(
                keys::Q_REST_COULD_NOT_LOG_OUT_E,
                &[("e", &e.to_string())],
            )));
            cx.notify();
            return;
        }
        quill::passcode::forget_passcode(&root, &quill::passcode::global_unlock());
        self.background.wiping = true;
        // A connection being prepared must not open now.
        self.connection.generation += 1;
        self.close_screen_client(cx);
        self.stop_background_accounts(cx);
        self.delete_after_close(
            root,
            accounts,
            |this, deletion, cx| match deletion {
                Deletion::Done { .. } => {
                    this.background.wiping = false;
                    this.start_connection(cx);
                }
                // Nothing opens in this run; the next start deletes the
                // folders before connecting.
                Deletion::Deferred => {
                    this.connection.set_note(StatusNote::silent(tr(
                        keys::Q_REST_RESTART_QUILL_TO_FINISH_LOGGING_OUT,
                    )));
                }
            },
            cx,
        );
    }

    /// Delete `keys`' folders (already recorded in `accounts.json`) once
    /// no client may have them open, on a worker; then `done`.
    pub(in crate::ui) fn delete_after_close(
        &mut self,
        root: PathBuf,
        keys: Vec<AccountKey>,
        done: impl FnOnce(&mut QuillApp, Deletion, &mut Context<QuillApp>) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.background.deleting.extend(keys.iter().cloned());
        for key in &keys {
            self.forget_account_sends(key);
        }
        super::super::notifications::forget_cached_icons();
        let waiting = keys.clone();
        self.after_closing(
            cx,
            move |this| this.deletion_wait(&waiting),
            move |this, go, cx| {
                if !go {
                    // Stays in `deleting`: nothing opens it in this run.
                    done(this, Deletion::Deferred, cx);
                    cx.notify();
                    return;
                }
                let secrets = this.secrets();
                let worker_keys = keys.clone();
                let deletion = cx.background_executor().spawn(async move {
                    let store = secrets();
                    let mut deleted = Vec::new();
                    let mut error = None;
                    for key in worker_keys {
                        match delete_account_data(&root, &key, store.as_ref()) {
                            Ok(()) => deleted.push(key),
                            Err(e) => error = Some(e.to_string()),
                        }
                    }
                    (root, deleted, error)
                });
                cx.spawn(async move |this, cx| {
                    let (root, deleted, error) = deletion.await;
                    let _ = this.update(cx, |this, cx| {
                        // A folder that failed stays recorded: the next
                        // start tries again, and nothing opens it before.
                        for key in &deleted {
                            let _ = finish_pending_delete(&root, key);
                        }
                        for key in &keys {
                            this.background.deleting.remove(key);
                        }
                        done(this, Deletion::Done { error }, cx);
                        cx.notify();
                    });
                })
                .detach();
            },
        );
    }

    /// Whether `keys`' folders can be deleted now: every client that had
    /// them open confirmed Closed, and no preparation for them still runs.
    fn deletion_wait(&self, keys: &[AccountKey]) -> CloseWait {
        let background = &self.background;
        if keys.iter().any(|key| background.closing.unconfirmed(key)) {
            return CloseWait::Never;
        }
        let open = |key: &AccountKey| {
            background.closing.contains(key)
                || background.preparing(key)
                || background
                    .client(key)
                    .is_some_and(|client| client.live.is_some())
                || self
                    .live
                    .as_ref()
                    .is_some_and(|live| &live.driver.session.account == key)
        };
        if background.screen_preparing > 0 || keys.iter().any(open) {
            CloseWait::Wait
        } else {
            CloseWait::Go
        }
    }
}
