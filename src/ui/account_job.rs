//! Work that finishes later (a file picker, an encoder, a wait for a
//! chat's history) and then acts through `self.live` acts only for the
//! account that started it. Chat, message and file ids are the account's
//! own, and the account on screen can change while the work waits
//! (`QuillApp::switch_account`): a continuation that read `self.live` then
//! would act as the account switched to.
//!
//! The work takes a [`StartedOn`] when it starts and finishes through
//! [`QuillApp::live_for`] or [`QuillApp::on_screen`], which come back
//! empty once another account is on screen.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::status_note::StatusNote;
use quill::connect::LiveConnect;
use quill::ids::AccountKey;
use quill::lang::{keys, tr};
use std::path::Path;

/// The account on screen when a piece of work started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StartedOn(AccountKey);

impl StartedOn {
    pub(super) fn new(account: AccountKey) -> Self {
        Self(account)
    }

    fn matches(&self, on_screen: Option<&AccountKey>) -> bool {
        on_screen == Some(&self.0)
    }
}

impl QuillApp {
    /// The account on screen, for work that finishes later. `None` when
    /// there is no session (nothing could act anyway).
    pub(super) fn started_on(&self) -> Option<StartedOn> {
        self.session()
            .map(|session| StartedOn(session.account.clone()))
    }

    /// Whether the account `job` started on is still the one on screen.
    pub(super) fn on_screen(&self, job: &StartedOn) -> bool {
        job.matches(self.session().map(|session| &session.account))
    }

    /// The live client, only while the account `job` started on is still
    /// on screen.
    pub(super) fn live_for(&mut self, job: &StartedOn) -> Option<&mut LiveConnect> {
        self.live
            .as_mut()
            .filter(|live| job.matches(Some(&live.driver.session.account)))
    }

    /// A recording finished after the account changed: its file is
    /// removed and a note says why, and nothing is sent. Returns whether
    /// it was dropped.
    pub(super) fn drop_recording_of_other_account(&mut self, job: &StartedOn, path: &Path) -> bool {
        if self.on_screen(job) {
            return false;
        }
        let _ = std::fs::remove_file(path);
        self.connection.set_note(StatusNote::silent(tr(
            keys::Q_NOTES_RECORDING_NOT_SENT_ACCOUNT_CHANGED,
        )));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::StartedOn;
    use quill::ids::AccountKey;

    #[test]
    fn work_acts_only_while_its_account_is_on_screen() {
        let job = StartedOn::new(AccountKey("a".into()));
        assert!(job.matches(Some(&AccountKey("a".into()))));
        assert!(!job.matches(Some(&AccountKey("b".into()))));
        assert!(!job.matches(None));
    }
}
