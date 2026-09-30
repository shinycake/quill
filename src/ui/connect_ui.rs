//! connection status UI.

use quill::auth::view_for;
use quill::connect::{ConnectBlocker, ConnectGate, LiveConnect, evaluate_gate, start_live_connect};
use quill::credentials::TelegramCredentials;
use quill::diagnostics::{DiagnosticSink, MemorySink};
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
    /// Slice A1: synthetic QR-login surface for screenshot proof (no live TDLib).
    DemoWaitQr,
    /// Injected Ready + main chat list (no live Telegram).
    DemoReadyChats,
    Live,
}

pub(super) fn bootstrap_connect(
    credentials: Option<TelegramCredentials>,
) -> (
    ConnectUiStatus,
    Option<LiveConnect>,
    String,
    AuthorizationState,
) {
    let auth_demo = AuthorizationState::WaitPhoneNumber;
    match evaluate_gate(credentials.is_some()) {
        ConnectGate::Blocked(ConnectBlocker::MissingCredentials) => (
            ConnectUiStatus::NeedCredentials,
            None,
            ConnectBlocker::MissingCredentials.user_message().into(),
            auth_demo,
        ),
        ConnectGate::Blocked(ConnectBlocker::MissingTdjson) => (
            ConnectUiStatus::NeedTdjson,
            None,
            ConnectBlocker::MissingTdjson.user_message().into(),
            auth_demo,
        ),
        ConnectGate::Blocked(other) => (
            ConnectUiStatus::RestoreBlocked(other.user_message()),
            None,
            other.user_message().into(),
            auth_demo,
        ),
        ConnectGate::Ready { .. } => {
            let Some(credentials) = credentials else {
                return (
                    ConnectUiStatus::NeedCredentials,
                    None,
                    ConnectBlocker::MissingCredentials.user_message().into(),
                    auth_demo,
                );
            };
            let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
            let store = live_secret_store();
            match start_live_connect(credentials, store.as_ref(), sink) {
                Ok(live) => (
                    ConnectUiStatus::Live,
                    Some(live),
                    "TDLib connected — waiting for authorization updates".into(),
                    auth_demo,
                ),
                Err(ConnectBlocker::MissingTdjson) => (
                    ConnectUiStatus::NeedTdjson,
                    None,
                    ConnectBlocker::MissingTdjson.user_message().into(),
                    auth_demo,
                ),
                Err(err) => (
                    ConnectUiStatus::RestoreBlocked(err.user_message()),
                    None,
                    err.user_message().into(),
                    auth_demo,
                ),
            }
        }
    }
}

pub(super) fn live_status_for(auth: &AuthorizationState) -> String {
    match auth {
        AuthorizationState::WaitTdlibParameters => {
            "TDLib connected — waiting for authorization updates".into()
        }
        AuthorizationState::WaitPhoneNumber => "enter phone number".into(),
        AuthorizationState::WaitCode { .. } => "enter the verification code from Telegram".into(),
        AuthorizationState::WaitPassword { .. } => {
            "enter your two-step verification password".into()
        }
        AuthorizationState::Ready => "signed in — cloud + secret chats".into(),
        AuthorizationState::WaitOtherDeviceConfirmation { .. } => {
            "confirm on another device (QR payload is not logged)".into()
        }
        AuthorizationState::LoggingOut => "signing out".into(),
        AuthorizationState::Closing => "TDLib is closing".into(),
        AuthorizationState::Closed => "session closed".into(),
        other => view_for(other).body,
    }
}
