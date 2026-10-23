//! Files handed to Quill from outside (the Windows "Send to" menu,
//! `quill <file>`, a file opened with Quill): the "Forward to…" chat chooser
//! and the hand-off to the composer. tdesktop shows
//! `ShowChooseRecipientBox` from `MainWidget::handleStartFiles`, refuses a
//! chat that takes no files with a toast, and otherwise opens the chat and
//! its send-files dialog (`MainWidget::sendPaths`); Quill's counterpart of
//! that dialog is the composer's attachment strip. See
//! `docs/decisions/codex-platform-updates-bundle.md`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::local_time::now_unix;
use quill::send_rights::SendRights;
use quill::telegram::envelope::AuthorizationState;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// How many chats the chooser lists at once.
const CHOICES: usize = 60;
/// `lng_forward_send_files_cant`.
fn no_files_here() -> &'static str {
    tr(keys::FORWARD_SEND_FILES_CANT)
}

impl QuillApp {
    /// Runs from the poll loop: once signed in and unlocked, files that
    /// arrived from outside open the chooser (tdesktop `checkStartUrls`
    /// waits for the passcode and the main window the same way).
    pub(super) fn pump_start_files(&mut self, cx: &mut Context<Self>) {
        let ready = self
            .live
            .as_ref()
            .is_some_and(|live| matches!(live.driver.session.auth, AuthorizationState::Ready));
        if !ready || self.account.passcode.locked {
            return;
        }
        let files = quill::start_files::take_files();
        if !files.is_empty() {
            self.begin_start_files(files, cx);
        }
    }

    /// Open the chooser for `files`; files already being chosen for stay.
    pub(super) fn begin_start_files(&mut self, files: Vec<PathBuf>, cx: &mut Context<Self>) {
        for file in files {
            if !self.start_files.choosing.contains(&file) {
                self.start_files.choosing.push(file);
            }
        }
        cx.notify();
    }

    fn close_start_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_files.choosing.clear();
        self.start_files
            .search
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    /// The typed query changed: ask the server too (`searchChatsOnServer`).
    pub(super) fn start_files_search_changed(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.start_files.choosing.is_empty() {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.search_share_chats(text);
        }
        cx.notify();
    }

    /// The chats the chooser lists for `query`.
    fn start_files_choices(&self, query: &str) -> Vec<(ChatId, String)> {
        self.session()
            .map(|session| {
                session
                    .share_destinations(query)
                    .into_iter()
                    .take(CHOICES)
                    .map(|chat| (chat.id, chat.title.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Enter in the search box picks the first match.
    pub(super) fn choose_first_start_files_chat(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = self.start_files.search.read(cx).value().to_string();
        if let Some((chat, _)) = self.start_files_choices(&query).into_iter().next() {
            self.choose_start_files_chat(chat, window, cx);
        }
    }

    /// Why `chat` takes none of the files, if it does not.
    pub(in crate::ui) fn start_files_refusal(&self, chat: ChatId) -> Option<&'static str> {
        let session = self.session()?;
        let summary = session.chats.get(&chat.0)?;
        (!SendRights::for_chat(session, summary, now_unix()).allows_any_file())
            .then_some(no_files_here())
    }

    /// A chat was picked. A chat that takes no files keeps the chooser open
    /// with a toast (tdesktop returns `false` from `sendPaths`); otherwise
    /// the chat opens with the files attached to its composer, to be sent
    /// when the user presses Send.
    pub(super) fn choose_start_files_chat(
        &mut self,
        chat: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.start_files.choosing.is_empty() {
            return;
        }
        if let Some(note) = self.start_files_refusal(chat) {
            self.connection.set_note(StatusNote::error(note));
            cx.notify();
            return;
        }
        let files = std::mem::take(&mut self.start_files.choosing);
        self.start_files
            .search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.select_listed_chat(chat, window, cx);
        if self.open_chat_id() == Some(chat) {
            self.attach_dropped_files(&files, cx);
        } else {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MISC_OPEN_THE_CHAT_TO_ATTACH)));
        }
        cx.notify();
    }

    pub(super) fn build_start_files_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::StartFiles, |this, window, cx| {
                this.close_start_files(window, cx);
            });
        app.update(cx, |this, cx| {
            let query = this.start_files.search.read(cx).value().to_string();
            let choices = this.start_files_choices(&query);
            let mut list = div().flex().flex_col().gap_1();
            if choices.is_empty() {
                list = list.child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child(tr(keys::Q_MISC_NO_CHATS_FOUND)),
                );
            }
            for (id, title) in choices {
                list = list.child(
                    Button::new(SharedString::from(format!("start-files-{}", id.0)))
                        .label(title)
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.choose_start_files_chat(id, window, cx);
                            this.close_kit_dialog_if_done(DialogKind::StartFiles, window, cx);
                        })),
                );
            }
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    Textarea::new(&this.start_files.search)
                        .aria_label(tr(keys::Q_MISC_SEARCH_CHATS))
                        .h(px(36.)),
                )
                .child(list)
                .into_any_element();
            let footer = div().flex().justify_end().gap_2().child(
                Button::new("start-files-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_start_files(window, cx);
                        this.close_kit_dialog_if_done(DialogKind::StartFiles, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::Q_MISC_FORWARD_TO)))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// Files sent to Quill from outside: the "Forward to…" chat chooser.
    StartFiles => DialogSpec::new(
        2905,
        |app| !app.start_files.choosing.is_empty(),
        QuillApp::build_start_files_dialog,
    ),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "start_files_ui_tests.rs"]
mod tests;
