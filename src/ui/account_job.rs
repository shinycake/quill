//! Work that finishes later (a file picker, an encoder, a wait for a
//! chat's history) and then acts through `self.live` acts only for the
//! account that started it. Chat, message and file ids are the account's
//! own, and the account on screen can change while the work waits
//! (`QuillApp::switch_account`): a continuation that read `self.live` then
//! would act as the account switched to.
//!
//! The work takes a [`StartedOn`] when it starts and finishes through
//! [`QuillApp::live_for`] or [`QuillApp::on_screen`], which come back
//! empty once another account is on screen. It names the screen's epoch,
//! not the account: A → B → A gives A a new epoch, so work from A's first
//! turn does not run in its second (its box is gone with the first
//! switch). Work that needs nothing but the drop uses `spawn_screen`
//! instead (`chat_window::scoped`); this is for a value kept between
//! steps (the recording's send target).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::on_screen::ScreenEpoch;
use super::status_note::StatusNote;
use quill::lang::{keys, tr};

/// The screen the account on screen had when a piece of work started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StartedOn(ScreenEpoch);

impl StartedOn {
    #[cfg(test)]
    pub(super) fn new(screen: ScreenEpoch) -> Self {
        Self(screen)
    }
}

impl QuillApp {
    /// The account on screen, for work that finishes later. `None` when
    /// there is no session (nothing could act anyway).
    pub(super) fn started_on(&self) -> Option<StartedOn> {
        self.session().map(|_| StartedOn(self.screen_epoch()))
    }

    /// Whether the account `job` started on is still the one on screen.
    pub(super) fn on_screen(&self, job: &StartedOn) -> bool {
        job.0 == self.screen_epoch()
    }

    /// A recording's `spawn_screen` continuation came back [`Gone`]: the
    /// screen it was recorded on is over. Runs as the app
    /// (`ScopedApp::app`): the file goes, and the account now on screen
    /// hears why nothing was sent.
    ///
    /// [`Gone`]: super::chat_window::scoped::Gone
    pub(super) fn recording_dropped_with_screen(
        &mut self,
        path: Option<std::path::PathBuf>,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if let Some(path) = path {
            let _ = std::fs::remove_file(path);
        }
        self.connection.set_note(StatusNote::silent(tr(
            keys::Q_NOTES_RECORDING_NOT_SENT_ACCOUNT_CHANGED,
        )));
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::StartedOn;
    use crate::ui::on_screen::ScreenEpoch;

    #[test]
    fn work_names_the_screen_it_started_on() {
        let first = ScreenEpoch::default();
        let job = StartedOn::new(first);
        assert_eq!(job, StartedOn::new(first));
        assert_ne!(job, StartedOn::new(first.next()));
        // Back to the same account is still another screen.
        assert_ne!(job, StartedOn::new(first.next().next()));
    }
}
