//! Opens handed to the OS, and the way back: only a failure says
//! anything, once the OS answered (Telegram Desktop shows no note when a
//! link or file opens).

use super::app::QuillApp;
use super::status_note::StatusNote;
use gpui_kit::{App, Context, WeakEntity};
use quill::lang::{keys, tr};
use quill::platform::OsOpen;

impl QuillApp {
    /// Open a link the user pressed: a web link in the browser, a
    /// `mailto:` link in the mail client. Every link Quill opens goes
    /// through here, so a refused or failed open is never silent.
    pub(in crate::ui) fn open_link(&mut self, url: &str, cx: &mut Context<Self>) {
        let (open, failure) = if url.starts_with("mailto:") {
            (
                quill::platform::open_mailto_url(url),
                keys::Q_OPS_COULD_NOT_OPEN_THE_MAIL,
            )
        } else {
            (
                quill::platform::open_external_url(url),
                keys::Q_CHAT_COULD_NOT_OPEN_LINK,
            )
        };
        self.report_failed_open(open, tr(failure), cx);
    }

    /// Show `failure` if the OS could not open what it was handed.
    pub(in crate::ui) fn report_failed_open(
        &mut self,
        open: OsOpen,
        failure: impl Into<String> + 'static,
        cx: &mut Context<Self>,
    ) {
        self.when_os_answers(open, cx, move |this, opened, cx| {
            if !opened {
                this.connection.set_note(StatusNote::error(failure));
                cx.notify();
            }
        });
    }

    pub(super) fn when_os_answers(
        &mut self,
        open: OsOpen,
        cx: &mut Context<Self>,
        answered: impl FnOnce(&mut Self, bool, &mut Context<Self>) + 'static,
    ) {
        match open {
            OsOpen::Answered(opened) => answered(self, opened, cx),
            OsOpen::Pending(answer) => Self::spawn_main(cx, async move |app, cx| {
                let opened = answer.await.unwrap_or(false);
                let _ = app.update(cx, |this, cx| answered(this, opened, cx));
            })
            .detach(),
        }
    }
}

/// [`QuillApp::open_link`] from a handler that holds only the app.
pub(in crate::ui) fn open_link_in(app: &WeakEntity<QuillApp>, url: &str, cx: &mut App) {
    let _ = app.update(cx, |app, cx| app.open_link(url, cx));
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "os_open_tests.rs"]
mod tests;
