//! Signing in with a passkey from the QR sign-in screen, after Telegram
//! Desktop's `intro/intro_qr.cpp` (`QrWidget::setupPasskeyLink`) and
//! `data/components/passkeys.cpp` (`InitPasskeyLogin`,
//! `FinishPasskeyLogin`).
//!
//! Pure core: the requests (`getAuthenticationPasskeyParameters`,
//! `checkAuthenticationPasskey`), the user-handle check tdesktop makes
//! before it sends the assertion, and the state the driver and the QR
//! screen share. The ceremony itself is `crate::webauthn`.
#![deny(clippy::indexing_slicing)]

use super::request;
use crate::ids::RequestId;
use crate::webauthn::{Assertion, RequestOptions, options::encode_base64url};
use serde_json::json;
use std::time::{Duration, Instant};

/// The error tdesktop reports for a user handle that names no data centre
/// (`FinishPasskeyLogin`).
pub const HANDLE_INVALID: &str = "PASSKEY_HANDLE_INVALID";

/// The line under the link when the server's options cannot be read
/// (tdesktop shows nothing; the user would be left with a dead click).
pub const UNREADABLE_OPTIONS: &str = "Could not sign in with the passkey: unreadable parameters";

/// tdesktop's `MTP::kDcShift`: data centre ids are below it.
const DC_SHIFT: i32 = 10_000;

/// `getAuthenticationPasskeyParameters = Text` (`td_api.tl:11728`).
pub fn get_authentication_passkey_parameters(extra: RequestId) -> String {
    request(
        extra,
        json!({"@type": "getAuthenticationPasskeyParameters"}),
    )
}

/// `checkAuthenticationPasskey credential_id:string client_data:string
/// authenticator_data:bytes signature:bytes user_handle:bytes = Ok`
/// (`td_api.tl:11738`). The credential id is base64url without padding,
/// as tdesktop sends it; `bytes` travel as standard base64 in TDLib's
/// JSON.
pub fn check_authentication_passkey(extra: RequestId, assertion: &Assertion) -> String {
    use base64::Engine;
    let b64 = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    request(
        extra,
        json!({
            "@type": "checkAuthenticationPasskey",
            "credential_id": encode_base64url(&assertion.credential_id),
            "client_data": assertion.client_data_json,
            "authenticator_data": b64(&assertion.authenticator_data),
            "signature": b64(&assertion.signature),
            "user_handle": b64(&assertion.user_handle),
        }),
    )
}

/// tdesktop `FinishPasskeyLogin`'s check: the user handle is UTF-8
/// `"<dc>:<rest>"` with exactly one colon and a data centre id in
/// `1..10000`. Anything else fails with `PASSKEY_HANDLE_INVALID` before a
/// request is sent.
pub fn user_handle_valid(handle: &[u8]) -> bool {
    let text = String::from_utf8_lossy(handle);
    let mut parts = text.split(':');
    let (Some(dc), Some(_), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    // Qt's `toInt` allows surrounding whitespace and a sign.
    dc.trim()
        .parse::<i32>()
        .is_ok_and(|dc| dc > 0 && dc < DC_SHIFT)
}

/// Request options fetched for a sign-in, reused while within their own
/// timeout (tdesktop `_passkeyLoginData` and `_passkeyLoginTime`).
#[derive(Debug, Clone)]
pub struct PendingLogin {
    pub options: RequestOptions,
    pub fetched_at: Instant,
}

impl PendingLogin {
    pub fn fresh(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.fetched_at)
            < Duration::from_millis(u64::from(self.options.timeout_ms))
    }
}

/// Signing in with a passkey, one stage at a time (tdesktop
/// `_passkeyLoginData`, `_passkeyLoginTime`). The error, a dropped request
/// and the options timing out each leave a stage.
#[derive(Debug, Clone, Default)]
pub enum LoginStage {
    #[default]
    Idle,
    /// Options ready for a ceremony the UI has not started yet; the UI
    /// takes them.
    Starting(PendingLogin),
    /// The ceremony runs or ended; the options are kept for their timeout
    /// and reused on the next try.
    Cached(PendingLogin),
    /// `checkAuthenticationPasskey` is on its way.
    Checking,
    /// The server's error type of the last failed step, shown under the
    /// link as tdesktop's `showError(error)` does.
    Failed(String),
}

/// The sign-in half of the passkey state, kept while signed out.
#[derive(Debug, Clone, Default)]
pub struct LoginState {
    /// `getApplicationConfig` on the sign-in screens (tdesktop refreshes
    /// the app config before login too). Asked once: a failure is not
    /// asked again from every ingest.
    pub config: crate::state::fetch::Fetch<()>,
    pub stage: LoginStage,
}

impl LoginState {
    /// The options arrived: keep them and start the ceremony. `false` when
    /// they cannot be read (tdesktop then does nothing).
    pub fn accept_parameters(&mut self, json: &str, now: Instant) -> bool {
        let Some(options) = RequestOptions::parse(json) else {
            return false;
        };
        self.stage = LoginStage::Starting(PendingLogin {
            options,
            fetched_at: now,
        });
        true
    }

    /// A step failed: tdesktop forgets the options and shows the error.
    pub fn fail(&mut self, error: String) {
        self.stage = LoginStage::Failed(error);
    }

    /// The options of the last try, while they are within their timeout.
    pub fn fresh_options(&self, now: Instant) -> Option<&PendingLogin> {
        match &self.stage {
            LoginStage::Starting(p) | LoginStage::Cached(p) if p.fresh(now) => Some(p),
            _ => None,
        }
    }

    /// The UI starts the ceremony: it takes the ready options and the
    /// stage keeps them for reuse.
    pub fn take_ceremony(&mut self) -> Option<RequestOptions> {
        match std::mem::take(&mut self.stage) {
            LoginStage::Starting(pending) => {
                let options = pending.options.clone();
                self.stage = LoginStage::Cached(pending);
                Some(options)
            }
            other => {
                self.stage = other;
                None
            }
        }
    }

    /// Options are ready and no ceremony took them yet.
    pub fn ceremony_ready(&self) -> bool {
        matches!(self.stage, LoginStage::Starting(_))
    }

    pub fn checking(&self) -> bool {
        matches!(self.stage, LoginStage::Checking)
    }

    pub fn error(&self) -> Option<&str> {
        match &self.stage {
            LoginStage::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// The ceremony ended without a result (the box was closed): drop
    /// options nobody started.
    pub fn drop_unstarted(&mut self) {
        if let LoginStage::Starting(pending) = std::mem::take(&mut self.stage) {
            self.stage = LoginStage::Cached(pending);
        }
    }

    /// `checkAuthenticationPasskey` answered: nothing is kept.
    pub fn finish_check(&mut self) {
        self.stage = LoginStage::Idle;
    }
}

#[cfg(test)]
#[path = "login_tests.rs"]
mod tests;
