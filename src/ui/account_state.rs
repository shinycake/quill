//! Accounts, account lifecycle, local passcode, freeze and age checks.

use super::*;
use gpui_kit::*;

pub(crate) struct AccountUi {
    /// Slice A9: account lifecycle dialog (delete account + self-destruct
    /// TTL). Working state lives in the named module; this is the one
    /// field the dialog machinery reads.
    pub(super) lifecycle: AccountLifecycleState,
    /// Slice parity:auth-multi-account (UI): the Accounts dialog state
    /// (list / switch / add / remove). Working state lives in
    /// `accounts.rs`; this is the one field the dialog machinery reads.
    pub(super) accounts: AccountsUiState,
    /// Local passcode: settings dialog, lock screen, auto-lock.
    pub(super) passcode: super::passcode::PasscodeUi,
    /// The frozen-account details dialog is open.
    pub(super) freeze_info_open: bool,
    /// The age verification prompt is open, and the user already started
    /// the verification (so turning on 18+ content goes to the server).
    pub(super) age_verify_open: bool,
    pub(super) age_verify_started: bool,
}

impl AccountUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            lifecycle: AccountLifecycleState::new(window, cx),
            accounts: AccountsUiState::new(window, cx),
            passcode: super::passcode::PasscodeUi::new(window, cx),
            freeze_info_open: false,
            age_verify_open: false,
            age_verify_started: false,
        }
    }
}
