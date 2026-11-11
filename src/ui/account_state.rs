//! Accounts, account lifecycle, local passcode, freeze and age checks.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::*;
use quill::telegram::envelope::AgeVerificationParams;

/// The process's account state: the account registry's dialog and the
/// local passcode, which locks every account at once. What acts for the
/// account on screen is [`AccountBoxes`], on the screen.
pub(crate) struct AccountUi {
    /// Slice parity:auth-multi-account (UI): the Accounts dialog state
    /// (list / switch / add / remove). Working state lives in
    /// `accounts.rs`; this is the one field the dialog machinery reads.
    pub(super) accounts: AccountsUiState,
    /// Local passcode: settings dialog, lock screen, auto-lock.
    pub(super) passcode: super::passcode::PasscodeUi,
}

impl AccountUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            accounts: AccountsUiState::new(window, cx),
            passcode: super::passcode::PasscodeUi::new(window, cx),
        }
    }
}

/// The boxes about the account on screen: Delete Account and Log Out
/// (with the password typed for them), the frozen-account details and the
/// age check. A field of `OnScreen`, so a switch drops them: an armed
/// Delete Account must not come back over the next account and send
/// through its client.
pub(crate) struct AccountBoxes {
    /// Slice A9: account lifecycle dialog (delete account + self-destruct
    /// TTL). Working state lives in the named module; this is the one
    /// field the dialog machinery reads.
    pub(super) lifecycle: AccountLifecycleState,
    /// The notice the user opened from the banner or the 18+ switch, if
    /// any (one box, so one value).
    pub(super) notice: OpenNotice,
    /// Whether the user already started the age verification the server
    /// asks for (so turning on 18+ content goes to the server).
    pub(super) age_check: AgeCheck,
}

/// The account notice the user has open. It ends with its box, and with
/// the condition that asked for it (`QuillApp::drain_account_notices`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OpenNotice {
    #[default]
    None,
    /// The frozen-account details.
    Frozen,
    /// The age verification prompt.
    AgeVerify,
}

impl OpenNotice {
    /// The box closed, or what it showed is gone.
    pub(super) fn close(&mut self) {
        *self = OpenNotice::None;
    }
}

/// How far the user took the age verification.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum AgeCheck {
    #[default]
    NotStarted,
    /// The verification bot was opened for this request of the server. A
    /// different request, or none, ends it (see [`AgeCheck::follow`]).
    Started { asked_for: AgeVerificationParams },
}

impl AgeCheck {
    /// The verification bot was opened for what the server asks now.
    pub(super) fn started_for(&self, asked: Option<&AgeVerificationParams>) -> bool {
        matches!(self, Self::Started { asked_for } if Some(asked_for) == asked)
    }

    /// Drop a start that answers a request the server no longer makes.
    pub(super) fn follow(&mut self, asked: Option<&AgeVerificationParams>) {
        if !self.started_for(asked) {
            *self = Self::NotStarted;
        }
    }
}

impl AccountBoxes {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            lifecycle: AccountLifecycleState::new(window, cx),
            notice: OpenNotice::None,
            age_check: AgeCheck::NotStarted,
        }
    }
}

/// Flags and two text fields, dropped with the screen: nothing to say.
impl super::on_screen::Farewell for AccountBoxes {}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests {
    use super::AgeCheck;
    use quill::telegram::envelope::AgeVerificationParams;

    fn params(country: &str) -> AgeVerificationParams {
        AgeVerificationParams {
            min_age: 18,
            verification_bot_username: "age_bot".into(),
            country: country.into(),
        }
    }

    #[test]
    fn a_started_verification_ends_with_the_request_it_answered() {
        let first = params("GB");
        let mut check = AgeCheck::Started {
            asked_for: first.clone(),
        };
        assert!(check.started_for(Some(&first)));
        check.follow(Some(&first));
        assert!(check.started_for(Some(&first)));
        // The server asks again, for another country: the start no longer counts.
        let second = params("AU");
        assert!(!check.started_for(Some(&second)));
        check.follow(Some(&second));
        assert_eq!(check, AgeCheck::NotStarted);
        // The request clearing also ends it.
        let mut check = AgeCheck::Started { asked_for: first };
        check.follow(None);
        assert_eq!(check, AgeCheck::NotStarted);
    }
}
