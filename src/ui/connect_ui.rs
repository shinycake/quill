//! connection status UI.
#![deny(clippy::indexing_slicing)]

use super::early_connect::First;
use crate::ui::status_note::StatusNote;
use quill::auth::view_for;
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

/// Startup connect classification for the status bar (no secrets).
#[derive(Clone, PartialEq, Eq)]
pub enum ConnectUiStatus {
    NeedCredentials,
    Blocked,
    /// Synthetic WaitPhoneNumber surface for screenshot proof (no live TDLib).
    DemoWaitPhone,
    /// Synthetic WaitCode surface for screenshot proof (no live TDLib).
    DemoWaitCode,
    /// Synthetic WaitPassword surface for screenshot proof (no live TDLib).
    DemoWaitPassword,
    /// Injected Ready + main chat list (no live Telegram).
    DemoReadyChats,
    Live,
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

impl super::app::QuillApp {
    /// Apply what TDLib answered while the window was being set up, so the
    /// first frame draws it instead of the one after the poll loop's
    /// first tick.
    pub fn poll_before_first_frame(&mut self, cx: &mut gpui_kit::Context<Self>) {
        self.poll_live(cx);
    }

    pub(super) fn start_connection(&mut self, cx: &mut gpui_kit::Context<Self>) {
        // The lock-screen log out is deleting the local data; it starts the
        // connection itself once that is gone.
        if self.background.wiping {
            return;
        }
        quill::startup_trace::mark("connect_start");
        self.connection.generation += 1;
        let generation = self.connection.generation;
        let credentials = self.credentials.clone();
        self.connection.status = ConnectUiStatus::Live;
        self.auth_ui.demo_state = AuthorizationState::WaitTdlibParameters;
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_MISC_CONNECTING_TO_TELEGRAM)));
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
                self.poll_live(cx);
                return;
            }
            Some(First::Preparing(early)) => Some(early),
            _ => None,
        };
        let root = self.accounts_root();
        let secrets = self.secrets();
        // Nothing deletes a folder while this preparation (which creates
        // the account's folders) still runs.
        self.background.screen_preparing += 1;
        let connect = cx.background_executor().spawn(async move {
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
        Self::spawn_main(cx, async move |this, cx| {
            let (swept, prepared) = connect.await;
            quill::startup_trace::mark("prepare_done");
            let _ = this.update(cx, |this, _| {
                this.background.screen_preparing =
                    this.background.screen_preparing.saturating_sub(1);
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
                        if generation != this.connection.generation {
                            return false;
                        }
                        if !safe {
                            // Its old client's close can't be confirmed in
                            // this run: never open the database again.
                            this.connection.set_note(StatusNote::silent(tr(
                                keys::Q_REST_CONNECTION_TO_TELEGRAM_WAS_CLOSED,
                            )));
                            cx.notify();
                            return false;
                        }
                        // A client still on screen closes first; when it is
                        // this account's, wait for that close too.
                        if let Some(old) = this.live.take() {
                            let same = Some(&old.driver.session.account) == account.as_ref();
                            this.close_live_later(old, false, cx);
                            if same {
                                return true;
                            }
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
        })
        .detach();
        cx.notify();
    }

    /// Open the prepared client on screen (its database is not in use).
    fn open_prepared(
        &mut self,
        prepared: Result<(TelegramCredentials, quill::connect::PreparedConnect), ConnectBlocker>,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        self.open_client(
            prepared.and_then(|(credentials, prepared)| {
                start_prepared_live_connect(credentials, prepared, Arc::new(DiscardSink))
            }),
            cx,
        );
    }

    /// Put the opened client on screen, or say why there is none.
    fn open_client(
        &mut self,
        result: Result<LiveConnect, ConnectBlocker>,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        match result {
            Ok(mut live) => {
                quill::startup_trace::mark("client_open");
                live.driver.set_stream_volume(self.playback.volume);
                self.live = Some(live);
            }
            Err(blocker) => match blocker.startup_failure() {
                Some(failure) => {
                    eprintln!("quill: no client: {}", blocker.user_message());
                    self.auth_ui.demo_state = AuthorizationState::StartupFailed(failure);
                    self.connection.status = ConnectUiStatus::Blocked;
                    self.connection.clear_note();
                }
                None => {
                    self.auth_ui.demo_state = AuthorizationState::WaitPhoneNumber;
                    self.connection.status = ConnectUiStatus::NeedCredentials;
                    self.connection
                        .set_note(StatusNote::error(blocker.user_message()));
                }
            },
        }
        self.reload_account_keybindings(cx);
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
    use quill::platform::MemorySecretStore;

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
