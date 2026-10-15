//! The Collect button of the Gram withdrawal section (tdesktop
//! `BuildCurrencyWithdrawalSection` → `Api::HandleWithdrawalButton` with a
//! `currencyReceiver`): the 2-step password box, then the Fragment URL of
//! `getGramWithdrawalUrl`, opened in the browser.
//!
//! Pure core: the steps and the request. Nothing here moves Grams; the
//! request only returns Fragment's page, and the password goes into that
//! one request and is never kept. The driver is `connect::grams`.
#![deny(clippy::indexing_slicing)]

use crate::gift_manage::ExportError;
use crate::ids::RequestId;
use serde_json::json;

/// Where an open withdrawal stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GramWithdraw {
    /// tdesktop `PasscodeBox` with "Two-step verification" and "Please
    /// enter your password to collect."; `sending` while the request is
    /// out. The password itself is never kept here.
    Password {
        sending: bool,
        error: Option<ExportError>,
    },
    /// tdesktop `PrePasswordErrorBox`: no password set (`later: false`) or
    /// one set too recently (`later: true`).
    SecurityCheck { later: bool },
}

impl GramWithdraw {
    /// Collect was clicked. tdesktop reads the password state
    /// (`cloudPassword().state() | take(1)`): `Some(false)` (known, no
    /// password) is the security check; otherwise the password box, which a
    /// late "no password" state still turns into the security check
    /// ([`Self::password_state_known`]).
    pub fn open(has_password: Option<bool>) -> Self {
        match has_password {
            Some(false) => Self::SecurityCheck { later: false },
            _ => Self::idle(None),
        }
    }

    /// The password box waiting for a password, with the last error.
    pub fn idle(error: Option<ExportError>) -> Self {
        Self::Password {
            sending: false,
            error,
        }
    }

    /// The request with the password is out (the buttons wait).
    pub fn sending(&self) -> bool {
        matches!(self, Self::Password { sending: true, .. })
    }

    /// The password box is open and idle: Submit may send.
    pub fn can_submit(&self) -> bool {
        matches!(self, Self::Password { sending: false, .. })
    }

    /// A fresh password state arrived: with no password set, the idle
    /// password box becomes the security check.
    pub fn password_state_known(&mut self, has_password: bool) {
        if !has_password && self.can_submit() {
            *self = Self::SecurityCheck { later: false };
        }
    }
}

/// `getGramWithdrawalUrl` (td_api.tl: `password:string = HttpUrl`). Only
/// sent from the password box with a typed password.
pub fn get_gram_withdrawal_url(extra: RequestId, password: &str) -> String {
    json!({
        "@type": "getGramWithdrawalUrl",
        "@extra": extra.as_extra(),
        "password": password,
    })
    .to_string()
}
