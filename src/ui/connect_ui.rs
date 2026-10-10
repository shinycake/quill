//! connection status UI.

use quill::auth::view_for;
use quill::connect::{ConnectBlocker, ConnectGate, evaluate_gate, start_prepared_live_connect};
use quill::credentials::TelegramCredentials;
use quill::diagnostics::MemorySink;
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
        AuthorizationState::WaitTdlibParameters => "Connecting to Telegram…".into(),
        AuthorizationState::WaitPhoneNumber => "enter phone number".into(),
        AuthorizationState::WaitCode { .. } => "enter the verification code from Telegram".into(),
        AuthorizationState::WaitPassword { .. } => {
            "enter your two-step verification password".into()
        }
        AuthorizationState::Ready => "".into(),
        AuthorizationState::WaitOtherDeviceConfirmation { .. } => {
            "Confirm the sign-in on another device.".into()
        }
        AuthorizationState::LoggingOut => "signing out".into(),
        AuthorizationState::Closing => "Closing…".into(),
        AuthorizationState::Closed => "session closed".into(),
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
        self.connection.status_note = "Connecting to Telegram…".into();
        let connect = cx
            .background_executor()
            .spawn(async move { prepare_startup(credentials) });
        cx.spawn(async move |this, cx| {
            let prepared = connect.await;
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
