//! connection status UI.
#![deny(clippy::indexing_slicing)]

use super::early_connect::First;
use crate::ui::status_note::StatusNote;
use futures_channel::oneshot;
use gpui_kit::Task;
use quill::auth::{StartupFailure, view_for};
use quill::connect::{
    ConnectBlocker, ConnectGate, LiveConnect, evaluate_gate, start_prepared_live_connect,
};
use quill::credentials::TelegramCredentials;
use quill::diagnostics::DiscardSink;
use quill::ids::AccountKey;
use quill::lang::{keys, tr};
use quill::platform::SecretStore;
use quill::telegram::envelope::AuthorizationState;
use std::sync::Arc;

/// Where the startup connection stands while there is no client to ask
/// (no secrets): one value, so the pane and the authorization card it draws
/// cannot disagree.
pub enum ConnectStage {
    /// Nothing is being connected: a client was opened (or refused for a
    /// reason with no card of its own), or a start waits for the unlock.
    Idle,
    /// The client is being prepared and opened by this task. It belongs to
    /// the stage: a restart, an adopted client or a wipe replaces the stage
    /// and the task ends with it.
    Connecting { _task: Task<()> },
    /// No API credentials: the synthetic pane.
    NeedCredentials,
    /// The disk is below the open floor: the full-disk screen says so, and
    /// a reading with room connects again (`reconnect_after_disk`).
    DiskRefused,
    /// The client could not start: the card says why, with Retry.
    Blocked(StartupFailure),
    /// An authorization state shown by the card: a screenshot demo's
    /// injected surface (no live TDLib), or the state a client left.
    Shown(AuthorizationState),
}

impl ConnectStage {
    /// The authorization state the card draws while no client or demo
    /// session answers for it.
    pub fn auth(&self) -> AuthorizationState {
        match self {
            Self::Idle | Self::Connecting { .. } | Self::DiskRefused => {
                AuthorizationState::WaitTdlibParameters
            }
            Self::NeedCredentials => AuthorizationState::WaitPhoneNumber,
            Self::Blocked(failure) => AuthorizationState::StartupFailed(failure.clone()),
            Self::Shown(auth) => auth.clone(),
        }
    }

    /// A connection being prepared must not open: its task ends. Any other
    /// stage stays as it is.
    pub(super) fn cancel_connecting(&mut self) {
        if matches!(self, Self::Connecting { .. }) {
            *self = Self::Idle;
        }
    }
}

/// The workers preparing the screen's connection (`prepare_startup` creates
/// the account's folders), each holding a claim while it runs: nothing
/// deletes a folder until they are all done, also when the connection that
/// asked for one was replaced meanwhile.
#[derive(Default)]
pub(super) struct ScreenWorkers {
    workers: Vec<std::sync::Weak<()>>,
    /// One per worker: it wakes the work that waits for the folders when
    /// its worker ends, however the worker ends.
    watches: Vec<Task<()>>,
}

/// A screen worker's hold on the account folders. It goes when the worker
/// ends or is dropped with its connection.
pub(super) struct ScreenClaim {
    // The claim is released before the end is told (fields drop in this
    // order), so whoever is woken finds the worker gone.
    _claim: Arc<()>,
    _ended: oneshot::Sender<()>,
}

impl ScreenWorkers {
    /// A worker starts: it holds the returned claim until it ends, and the
    /// receiver resolves once the claim is gone.
    pub(super) fn begin(&mut self) -> (ScreenClaim, oneshot::Receiver<()>) {
        self.workers.retain(|worker| worker.strong_count() > 0);
        if self.workers.is_empty() {
            self.watches.clear();
        }
        let claim = Arc::new(());
        self.workers.push(Arc::downgrade(&claim));
        let (ended, end) = oneshot::channel();
        (
            ScreenClaim {
                _claim: claim,
                _ended: ended,
            },
            end,
        )
    }

    pub(super) fn running(&self) -> bool {
        self.workers.iter().any(|worker| worker.strong_count() > 0)
    }
}

impl super::app::QuillApp {
    /// A worker preparing the screen's connection starts. The work that
    /// waits for the account folders (`after_closing`) is looked at again
    /// when the claim is released, also when the connection that asked for
    /// the worker was replaced and no longer hears of its end.
    pub(super) fn begin_screen_worker(&mut self, cx: &mut gpui_kit::Context<Self>) -> ScreenClaim {
        let (claim, end) = self.background.screen_workers.begin();
        // screen-scope: the waiters it settles cover every account's folders, whichever account is on screen.
        let watch = Self::spawn_app(cx, async move |this, cx| {
            // Resolves when the claim is dropped, with or without a send.
            let _ = end.await;
            let _ = this.update(cx, |this, cx| this.settle_close_waiters(cx));
        });
        self.background.screen_workers.watches.push(watch);
        claim
    }
}

/// A first connection's preparation: the folders the startup sweep deleted
/// (under the app root) and the prepared client or why there is none.
pub(super) type Prepared = (
    Option<(std::path::PathBuf, Vec<AccountKey>)>,
    Result<(TelegramCredentials, quill::connect::PreparedConnect), ConnectBlocker>,
);

/// `swept`: folders the startup sweep just deleted (their records are
/// cleared on the UI thread afterwards).
pub(super) fn prepare_startup(
    credentials: Option<TelegramCredentials>,
    root: Option<std::path::PathBuf>,
    secrets: &dyn SecretStore,
    swept: &[AccountKey],
) -> Result<(TelegramCredentials, quill::connect::PreparedConnect), ConnectBlocker> {
    if let ConnectGate::Blocked(blocker) = evaluate_gate(credentials.is_some()) {
        return Err(blocker);
    }
    let credentials = credentials.ok_or(ConnectBlocker::MissingCredentials)?;
    let root = root.ok_or(ConnectBlocker::DataFolder)?;
    let active = quill::settings::active_account(&root);
    // A folder that could not be deleted yet is never opened.
    if quill::settings::pending_deletes(&root)
        .iter()
        .any(|key| key == &active && !swept.contains(key))
    {
        return Err(ConnectBlocker::DataFolder);
    }
    let prepared = quill::connect::prepare_connect(&root, active, secrets, &credentials)?;
    quill::launch_lines::open(&prepared.paths, &prepared.database_key);
    Ok((credentials, prepared))
}

pub(super) fn live_status_for(auth: &AuthorizationState) -> String {
    match auth {
        AuthorizationState::WaitTdlibParameters => tr(keys::Q_MISC_CONNECTING_TO_TELEGRAM).into(),
        AuthorizationState::WaitPhoneNumber => tr(keys::Q_MISC_ENTER_PHONE_NUMBER).into(),
        AuthorizationState::WaitCode { .. } => {
            tr(keys::Q_MISC_ENTER_THE_VERIFICATION_CODE_FROM).into()
        }
        AuthorizationState::WaitPassword { .. } => {
            tr(keys::Q_MISC_ENTER_YOUR_TWO_STEP_VERIFICATION).into()
        }
        AuthorizationState::Ready => "".into(),
        AuthorizationState::WaitOtherDeviceConfirmation { .. } => {
            tr(keys::Q_MISC_CONFIRM_THE_SIGN_IN_ON).into()
        }
        AuthorizationState::LoggingOut => tr(keys::Q_MISC_SIGNING_OUT).into(),
        AuthorizationState::Closing => tr(keys::Q_MISC_CLOSING).into(),
        AuthorizationState::Closed => tr(keys::Q_MISC_SESSION_CLOSED).into(),
        other => view_for(other).body,
    }
}

/// How long the first frame waits for the launch's first chat page once
/// TDLib is ready. The page came 9 to 15 ms after `authorizationStateReady`
/// in cold launches.
const LAUNCH_CHATS_WAIT: std::time::Duration = std::time::Duration::from_millis(50);

impl super::app::QuillApp {
    /// Apply what TDLib answered while the window was being set up, so the
    /// first frame draws it instead of the one after the poll loop's
    /// first tick.
    ///
    /// On macOS, when TDLib is ready but its first chat page is not in, the
    /// page gets a short wait first: when it misses the first frame, the run
    /// loop's first pass loads Writing Tools (280 to 370 ms on a cold
    /// launch) before the poll loop can draw it
    /// (docs/decisions/launch-warm-waits.md).
    pub fn poll_before_first_frame(&mut self, cx: &mut gpui_kit::Context<Self>) {
        if cfg!(target_os = "macos") && self.live.is_some() {
            use quill::telegram::hub::{LAUNCH_CHATS, LaunchWait};
            quill::startup_trace::mark("chats_wait");
            quill::startup_trace::mark(match LAUNCH_CHATS.wait(LAUNCH_CHATS_WAIT) {
                LaunchWait::Came => "chats_came",
                LaunchWait::Capped => "chats_capped",
                LaunchWait::Skipped => "chats_skipped",
            });
        }
        self.poll_pass(cx);
    }

    pub(super) fn start_connection(&mut self, cx: &mut gpui_kit::Context<Self>) {
        // The lock-screen log out is deleting the local data; it starts the
        // connection itself once that is gone.
        if self.background.wiping() {
            return;
        }
        // Under the lock the master key is cleared, so the preparation
        // would fail on it (`KeyLocked`): the connection waits for the
        // unlock (`resume_after_unlock`), as at a locked start.
        if self.connect_waits_for_unlock() {
            return;
        }
        quill::startup_trace::mark("connect_start");
        self.connect.generation += 1;
        let credentials = self.credentials.clone();
        // A connection already being made ends with its stage.
        self.auth_demo.stage = ConnectStage::Idle;
        self.set_note(StatusNote::silent(tr(keys::Q_MISC_CONNECTING_TO_TELEGRAM)));
        // The first connection of the run first deletes folders an earlier
        // run left for deletion (it quit before their clients had closed).
        let sweep = self.startup_sweep();
        // The run's first connection was begun in `main`.
        let early = match super::early_connect::take() {
            Some(First::Opened { swept, client }) if self.live.is_none() => {
                quill::startup_trace::mark("prepare_done");
                self.finish_sweep(&swept.0, &swept.1);
                self.open_client(*client, cx);
                // TDLib has been opening its database while the window was
                // built: apply what it has answered (and send what that
                // asks for) before the first frame.
                self.poll_pass(cx);
                return;
            }
            Some(First::Preparing(early)) => Some(early),
            _ => None,
        };
        let root = self.accounts_root();
        let secrets = self.secrets();
        // Nothing deletes a folder while this preparation (which creates
        // the account's folders) still runs.
        let claim = self.begin_screen_worker(cx);
        let connect = cx.background_executor().spawn(async move {
            let _claim = claim;
            if let Some(early) = early
                && let Ok(Some(prepared)) = early.await
            {
                return prepared;
            }
            let swept = sweep.map(|sweep| sweep.run());
            let done: &[AccountKey] = swept.as_ref().map_or(&[], |(_, keys)| keys.as_slice());
            let prepared = prepare_startup(credentials, root, secrets().as_ref(), done);
            (swept, prepared)
        });
        // screen-scope: connecting puts the prepared client on the screen, replacing what is there; the stage holds the task, so a restart or an adopted client ends it.
        let task = Self::spawn_app(cx, async move |this, cx| {
            let (swept, prepared) = connect.await;
            quill::startup_trace::mark("prepare_done");
            // A deletion that waits for this preparation is woken by the
            // worker's claim (`begin_screen_worker`).
            let _ = this.update(cx, |this, _| {
                if let Some((root, deleted)) = &swept {
                    this.finish_sweep(root, deleted);
                }
            });
            let account = prepared.as_ref().ok().map(|(_, p)| p.account.clone());
            let mut prepared = Some(prepared);
            loop {
                // The account's previous client may still be closing.
                let safe = match &account {
                    Some(account) => {
                        super::background_accounts::closing::closed(&this, account, cx).await
                    }
                    None => true,
                };
                let again = this
                    .update(cx, |this, cx| {
                        if !safe {
                            // Its old client's close can't be confirmed in
                            // this run: never open the database again, and
                            // say so instead of staying on Connecting.
                            this.auth_demo.stage =
                                ConnectStage::Blocked(StartupFailure::CloseUnconfirmed);
                            this.connection.clear_note();
                            cx.notify();
                            return false;
                        }
                        // A client still on screen closes first, and its
                        // screen's goodbyes (deferred to the end of this
                        // update) run before the new client is put there:
                        // the loop comes round again for the open, which
                        // also waits for that close when it is this
                        // account's.
                        if this.live.is_some() {
                            this.screen_leaves_later(cx);
                        }
                        if let Some(old) = this.take_live() {
                            this.close_live_later(old, false, cx);
                            return true;
                        }
                        if this.goodbyes_due() {
                            return true;
                        }
                        let Some(prepared) = prepared.take() else {
                            return false;
                        };
                        this.open_prepared(prepared, cx);
                        false
                    })
                    .unwrap_or(false);
                if !again {
                    break;
                }
            }
        });
        self.auth_demo.stage = ConnectStage::Connecting { _task: task };
        cx.notify();
    }

    /// Open the prepared client on screen (its database is not in use).
    fn open_prepared(
        &mut self,
        prepared: Result<(TelegramCredentials, quill::connect::PreparedConnect), ConnectBlocker>,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let opened = if self.disk.is_full() {
            Err(ConnectBlocker::DiskFull)
        } else {
            prepared.and_then(|(credentials, prepared)| {
                start_prepared_live_connect(credentials, prepared, Arc::new(DiscardSink))
            })
        };
        self.open_client(opened, cx);
    }

    /// Put the opened client on screen, or say why there is none.
    pub(super) fn open_client(
        &mut self,
        result: Result<LiveConnect, ConnectBlocker>,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        // Whatever is left of the connection that asked ends here: the
        // arms below set the card for a refusal.
        self.auth_demo.stage = ConnectStage::Idle;
        match result {
            Ok(mut live) => {
                quill::startup_trace::mark("client_open");
                live.driver.set_stream_volume(self.players.volume());
                self.disk.seed(&mut live.driver);
                self.put_on_screen(live);
            }
            Err(ConnectBlocker::DiskFull) => {
                self.refuse_screen_open(cx);
                self.set_note(StatusNote::silent(""));
            }
            // Prepared before the lock, refused by the key it cleared: open
            // again after the unlock instead of a "key locked" card.
            Err(ConnectBlocker::LockedStore) if self.connect_waits_for_unlock() => {}
            Err(blocker) => match blocker.startup_failure() {
                Some(failure) => {
                    eprintln!("quill: no client: {}", blocker.user_message());
                    self.auth_demo.stage = ConnectStage::Blocked(failure);
                    self.connection.clear_note();
                }
                None => {
                    self.auth_demo.stage = ConnectStage::NeedCredentials;
                    self.set_note(StatusNote::error(blocker.user_message()));
                }
            },
        }
        self.reload_account_keybindings(cx);
        self.reload_account_wallpapers(cx);
        if self.live.is_some() {
            self.spawn_poll_loop(cx);
            // `parity:notify-all-accounts`: the other accounts
            // connect too, after the one on screen.
            self.ensure_background_accounts(cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{ConnectStage, ScreenWorkers};
    use gpui_kit::Task;
    use quill::platform::MemorySecretStore;

    #[test]
    fn a_worker_holds_the_claim_until_it_ends() {
        let mut workers = ScreenWorkers::default();
        assert!(!workers.running());
        let (first, mut first_end) = workers.begin();
        let (second, _second_end) = workers.begin();
        assert_eq!(first_end.try_recv(), Ok(None), "the first still runs");
        drop(first);
        assert!(first_end.try_recv().is_err(), "its end is told");
        assert!(workers.running(), "the second still runs");
        drop(second);
        assert!(!workers.running());
    }

    #[test]
    fn cancelling_ends_only_a_connection_in_flight() {
        let mut stage = ConnectStage::Connecting {
            _task: Task::ready(()),
        };
        stage.cancel_connecting();
        assert!(matches!(stage, ConnectStage::Idle));
        let mut stage = ConnectStage::NeedCredentials;
        stage.cancel_connecting();
        assert!(matches!(stage, ConnectStage::NeedCredentials));
    }

    #[test]
    fn preparation_without_credentials_does_not_open_keychain_or_a_client() {
        assert!(matches!(
            super::prepare_startup(None, None, &MemorySecretStore::new(), &[]),
            Err(quill::connect::ConnectBlocker::MissingCredentials)
        ));
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "connect_ui_tests.rs"]
mod failure_card_tests;
