//! connection status UI.
#![deny(clippy::indexing_slicing)]

use quill::auth::view_for;
use quill::connect::{ConnectBlocker, ConnectGate, evaluate_gate, start_prepared_live_connect};
use quill::credentials::TelegramCredentials;
use quill::diagnostics::MemorySink;
use quill::lang::{keys, tr};
use quill::platform::live_secret_store;
use quill::telegram::envelope::AuthorizationState;
use std::sync::Arc;

/// Startup connect classification for the status bar (no secrets).
#[derive(Clone, PartialEq, Eq)]
pub enum ConnectUiStatus {
    NeedCredentials,
    NeedTdjson,
    RestoreBlocked(&'static str),
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

fn prepare_startup(
    credentials: Option<TelegramCredentials>,
) -> Result<(TelegramCredentials, quill::connect::PreparedConnect), ConnectBlocker> {
    if let ConnectGate::Blocked(blocker) = evaluate_gate(credentials.is_some()) {
        return Err(blocker);
    }
    let credentials = credentials.ok_or(ConnectBlocker::MissingCredentials)?;
    let root = quill::settings::safe_app_root().ok_or(ConnectBlocker::LockedStore)?;
    let prepared = quill::connect::prepare_connect(
        &root,
        quill::settings::active_account(&root),
        live_secret_store().as_ref(),
        &credentials,
    )?;
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
    pub(super) fn start_connection(&mut self, cx: &mut gpui_kit::Context<Self>) {
        self.connection.generation += 1;
        let generation = self.connection.generation;
        let credentials = self.credentials.clone();
        self.connection.status = ConnectUiStatus::Live;
        self.auth_ui.demo_state = AuthorizationState::WaitTdlibParameters;
        self.connection.status_note = tr(keys::Q_MISC_CONNECTING_TO_TELEGRAM).into();
        let connect = cx
            .background_executor()
            .spawn(async move { prepare_startup(credentials) });
        cx.spawn(async move |this, cx| {
            let prepared = connect.await;
            // The account's previous client may still be closing.
            if let Ok((_, prepared)) = &prepared {
                super::background_accounts::closing::closed(&this, &prepared.account, cx).await;
            }
            let _ = this.update(cx, |this, cx| {
                if generation != this.connection.generation {
                    return;
                }
                let result = prepared.and_then(|(credentials, prepared)| {
                    start_prepared_live_connect(credentials, prepared, Arc::new(MemorySink::new()))
                });
                match result {
                    Ok(live) => this.live = Some(live),
                    Err(blocker) => {
                        this.auth_ui.demo_state = AuthorizationState::WaitPhoneNumber;
                        this.connection.status = match blocker {
                            ConnectBlocker::MissingCredentials => ConnectUiStatus::NeedCredentials,
                            ConnectBlocker::MissingTdjson => ConnectUiStatus::NeedTdjson,
                            _ => ConnectUiStatus::RestoreBlocked(blocker.user_message()),
                        };
                        this.connection.status_note = blocker.user_message().into();
                    }
                }
                this.reload_account_keybindings(cx);
                if this.live.is_some() {
                    this.spawn_poll_loop(cx);
                    // `parity:notify-all-accounts`: the other accounts
                    // connect too, after the one on screen.
                    this.ensure_background_accounts(cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn preparation_without_credentials_does_not_open_keychain_or_a_client() {
        assert!(matches!(
            super::prepare_startup(None),
            Err(quill::connect::ConnectBlocker::MissingCredentials)
        ));
    }
}
