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
use crate::state::fetch::Fetch;
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

/// Request options fetched for a sign-in and when, reused while within
/// their own timeout (tdesktop `_passkeyLoginData` and
/// `_passkeyLoginTime`).
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

/// Where the passkey sign-in stands. Each waiting stage carries the id of
/// its request and only that answer moves it on; the error, a request that
/// is gone without one (`LoginState::drop_lost`) and the sign-in screens
/// going away (`LoginState::leave_sign_in`) each leave it.
#[derive(Debug, Clone, Default)]
pub enum LoginStage {
    #[default]
    Idle,
    /// `getAuthenticationPasskeyParameters` is on its way.
    FetchingOptions(RequestId),
    /// Options ready for a ceremony the UI has not started yet; the UI
    /// takes them.
    Ceremony(PendingLogin),
    /// The UI took the options and runs the ceremony. They stay for their
    /// own timeout, so a second click before it ends reuses them.
    Asserting(PendingLogin),
    /// `checkAuthenticationPasskey` is on its way.
    Checking(RequestId),
    /// The server's error type of the last failed step, shown under the
    /// link as tdesktop's `showError(error)` does.
    Failed(String),
}

/// The sign-in half of the passkey state, kept while signed out.
#[derive(Debug, Clone, Default)]
pub struct LoginState {
    /// `getApplicationConfig` on the sign-in screens (tdesktop refreshes
    /// the app config before login too). Asked once: a failed answer is not
    /// asked again by itself.
    pub config: Fetch<()>,
    pub stage: LoginStage,
}

impl LoginState {
    /// The options of request `id` arrived: keep them and start the
    /// ceremony. `false` when they cannot be read (tdesktop then does
    /// nothing). An answer to anything but the request the stage waits for
    /// is ignored.
    pub fn accept_parameters(&mut self, id: Option<RequestId>, json: &str, now: Instant) -> bool {
        if !matches!(&self.stage, LoginStage::FetchingOptions(waiting) if Some(*waiting) == id) {
            return true;
        }
        let Some(options) = RequestOptions::parse(json) else {
            return false;
        };
        self.stage = LoginStage::Ceremony(PendingLogin {
            options,
            fetched_at: now,
        });
        true
    }

    /// A step failed: tdesktop forgets the options and shows the error.
    pub fn fail(&mut self, error: String) {
        self.stage = LoginStage::Failed(error);
    }

    /// The request `id` failed. Only the request the stage waits for fails
    /// it; `false` when it is not that one.
    pub fn fail_request(&mut self, id: Option<RequestId>, error: String) -> bool {
        match &self.stage {
            LoginStage::FetchingOptions(waiting) | LoginStage::Checking(waiting)
                if Some(*waiting) == id =>
            {
                self.fail(error);
                true
            }
            _ => false,
        }
    }

    /// `checkAuthenticationPasskey` request `id` answered well: the
    /// authorization state moves on by itself.
    pub fn finish_check(&mut self, id: Option<RequestId>) {
        if matches!(&self.stage, LoginStage::Checking(waiting) if Some(*waiting) == id) {
            self.stage = LoginStage::Idle;
        }
    }

    /// The UI starts the ceremony with the options that landed.
    pub fn take_ceremony(&mut self) -> Option<RequestOptions> {
        match std::mem::take(&mut self.stage) {
            LoginStage::Ceremony(pending) => {
                let options = pending.options.clone();
                self.stage = LoginStage::Asserting(pending);
                Some(options)
            }
            other => {
                self.stage = other;
                None
            }
        }
    }

    /// The options of the last try, when they are still within their
    /// timeout: they go to the ceremony again without a request. `false`
    /// when a request is needed.
    pub fn reuse_options(&mut self, now: Instant) -> bool {
        match std::mem::take(&mut self.stage) {
            LoginStage::Ceremony(pending) | LoginStage::Asserting(pending)
                if pending.fresh(now) =>
            {
                self.stage = LoginStage::Ceremony(pending);
                true
            }
            other => {
                self.stage = other;
                false
            }
        }
    }

    /// A request the stage waits for is no longer pending and nothing
    /// answered it: the stage ends without a message.
    pub fn drop_lost(&mut self, pending: impl Fn(RequestId) -> bool) {
        if let LoginStage::FetchingOptions(id) | LoginStage::Checking(id) = &self.stage
            && !pending(*id)
        {
            self.stage = LoginStage::Idle;
        }
    }

    /// The sign-in screens went away (the account signed in, or the flow
    /// moved past the QR screen): options and any step under way go.
    pub fn leave_sign_in(&mut self) {
        self.stage = LoginStage::Idle;
    }

    /// Waiting for the server's options or answer.
    pub fn is_waiting(&self) -> bool {
        matches!(
            self.stage,
            LoginStage::FetchingOptions(_) | LoginStage::Checking(_)
        )
    }

    /// The error line under the link.
    pub fn error(&self) -> Option<&str> {
        match &self.stage {
            LoginStage::Failed(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "login_tests.rs"]
mod tests;
