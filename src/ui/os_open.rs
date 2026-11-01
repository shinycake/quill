//! Opens handed to the OS, and the way back: only a failure says
//! anything, once the OS answered (Telegram Desktop shows no note when a
//! link or file opens). The raw `quill::platform` opens are disallowed
//! everywhere else (clippy.toml), so no open can skip the failure note.
#![allow(clippy::disallowed_methods)]

use super::app::QuillApp;
use super::status_note::StatusNote;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::{App, Context, SharedString, WeakEntity, Window};
use quill::lang::{keys, tr};
use quill::platform::OsOpen;

impl QuillApp {
    /// Open a link the user pressed: a web link in the browser, a
    /// `mailto:` link in the mail client. Every link Quill opens goes
    /// through here, so a refused or failed open is never silent.
    pub(in crate::ui) fn open_link(&mut self, url: &str, cx: &mut Context<Self>) {
        if url.starts_with("mailto:") {
            let open = quill::platform::open_mailto_url(url);
            self.report_failed_open(open, tr(keys::Q_OPS_COULD_NOT_OPEN_THE_MAIL), cx);
        } else {
            self.open_web_link(url, tr(keys::Q_CHAT_COULD_NOT_OPEN_LINK), cx);
        }
    }

    /// Open an `http`/`https` link in the browser, showing `failure` when
    /// it did not open.
    pub(in crate::ui) fn open_web_link(
        &mut self,
        url: &str,
        failure: impl Into<String> + 'static,
        cx: &mut Context<Self>,
    ) {
        let open = quill::platform::open_external_url(url);
        self.report_failed_open(open, failure, cx);
    }

    /// Open the mail client on a new message to `address`.
    pub(in crate::ui) fn open_mail_to(&mut self, address: &str, cx: &mut Context<Self>) {
        let open = quill::platform::open_mailto(address);
        self.report_failed_open(open, tr(keys::Q_OPS_COULD_NOT_OPEN_THE_MAIL), cx);
    }

    /// Open a local file with its app, showing `failure` when it did not.
    pub(in crate::ui) fn open_file(
        &mut self,
        path: &std::path::Path,
        failure: impl Into<String> + 'static,
        cx: &mut Context<Self>,
    ) {
        let open = quill::platform::open_local_file(path);
        self.report_failed_open(open, failure, cx);
    }

    /// Open a local file with its app, then run `answered` with whether it
    /// opened.
    pub(in crate::ui) fn open_file_then(
        &mut self,
        path: &std::path::Path,
        cx: &mut Context<Self>,
        answered: impl FnOnce(&mut Self, bool, &mut Context<Self>) + 'static,
    ) {
        let open = quill::platform::open_local_file(path);
        self.when_os_answers(open, cx, answered);
    }

    /// Open a pane of the system settings (`x-apple.systempreferences:` or
    /// `ms-settings:`), showing `failure` when it did not open.
    pub(in crate::ui) fn open_system_settings(
        &mut self,
        target: &str,
        failure: impl Into<String> + 'static,
        cx: &mut Context<Self>,
    ) {
        let open = quill::platform::open_system_settings(target);
        self.report_failed_open(open, failure, cx);
    }

    /// Show `path` in the file manager, saying so when it could not.
    pub(in crate::ui) fn reveal_in_folder(
        &mut self,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        self.reveal_in_folder_or(path, tr(keys::Q_AV_COULDNT_OPEN_THE_FOLDER), cx);
    }

    /// [`Self::reveal_in_folder`] with its own `failure` text.
    pub(in crate::ui) fn reveal_in_folder_or(
        &mut self,
        path: &std::path::Path,
        failure: impl Into<String> + 'static,
        cx: &mut Context<Self>,
    ) {
        let open = quill::platform::reveal_in_file_manager(path);
        self.report_failed_open(open, failure, cx);
    }

    /// Show `failure` if the OS could not open what it was handed.
    pub(in crate::ui) fn report_failed_open(
        &mut self,
        open: OsOpen,
        failure: impl Into<String> + 'static,
        cx: &mut Context<Self>,
    ) {
        // The window that asked, before the OS answers: by then the pointer
        // may be over another one.
        let host = self.connection.note_window;
        self.when_os_answers(open, cx, move |this, opened, cx| {
            if !opened {
                let host = host.filter(|chat| this.windows.has(*chat));
                this.connection
                    .set_note_in(host, StatusNote::error(failure));
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

/// Show `path` in the file manager from a view that is not the app, such
/// as a window notification, pushing `failure` there when it could not.
pub(in crate::ui) fn reveal_from<T: 'static>(
    path: &std::path::Path,
    failure: SharedString,
    window: &mut Window,
    cx: &mut Context<T>,
) {
    let open = quill::platform::reveal_in_file_manager(path);
    cx.spawn_in(window, async move |_, cx| {
        if open.opened().await {
            return;
        }
        let _ = cx.update(|window, cx| {
            window.push_notification(Notification::new().message(failure), cx);
        });
    })
    .detach();
}

/// [`QuillApp::open_link`] from a handler that holds only the app.
pub(in crate::ui) fn open_link_in(app: &WeakEntity<QuillApp>, url: &str, cx: &mut App) {
    let _ = app.update(cx, |app, cx| app.open_link(url, cx));
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "os_open_tests.rs"]
mod tests;
