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

/// The sign-in half of the passkey state, kept while signed out.
#[derive(Debug, Clone, Default)]
pub struct LoginState {
    /// `getApplicationConfig` was asked for on the sign-in screens (tdesktop
    /// refreshes the app config before login too).
    pub config_requested: bool,
    /// The options of the last `getAuthenticationPasskeyParameters`.
    pub pending: Option<PendingLogin>,
    /// Options ready for a ceremony the UI has not started yet; the UI
    /// takes them.
    pub ceremony: Option<RequestOptions>,
    /// `checkAuthenticationPasskey` is on its way.
    pub checking: bool,
    /// The server's error type of the last failed step, shown under the
    /// link as tdesktop's `showError(error)` does.
    pub error: Option<String>,
}

impl LoginState {
    /// The options arrived: keep them and start the ceremony. `false` when
    /// they cannot be read (tdesktop then does nothing).
    pub fn accept_parameters(&mut self, json: &str, now: Instant) -> bool {
        let Some(options) = RequestOptions::parse(json) else {
            return false;
        };
        self.pending = Some(PendingLogin {
            options: options.clone(),
            fetched_at: now,
        });
        self.ceremony = Some(options);
        true
    }

    /// A step failed: tdesktop forgets the options and shows the error.
    pub fn fail(&mut self, error: String) {
        self.pending = None;
        self.ceremony = None;
        self.checking = false;
        self.error = Some(error);
    }
}

#[cfg(test)]
#[path = "login_tests.rs"]
mod tests;
