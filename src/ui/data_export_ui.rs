//! `parity:platform-data-export` — the "Export Telegram data" dialog.
//!
//! Telegram Desktop puts this under Settings → Advanced. Quill has no
//! settings screen, so the dialog opens from a "📦 Export Telegram data"
//! entry in the chat-list hamburger menu (next to the other
//! settings-adjacent entries). It hosts the scope toggles (chat history /
//! contacts / media), the destination folder, the live progress view and
//! the completion view. The batch itself runs in the connect driver
//! (`connect::data_export`), pumped from the app's poll loop — the dialog
//! only reads driver state and re-renders.
//!
//! Design notes:
//! - The dialog stays open while the export runs so the user can watch
//!   progress and cancel. Closing the dialog does NOT cancel the export;
//!   completion/failure surfaces once as a status note (see
//!   `notifications.rs`).
//! - While an account export owns the shared per-chat export machinery,
//!   the chat header's per-chat Export button is hidden (see
//!   `conversation.rs`).

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::{App, Context, Entity, ParentElement, Window};
use quill::chat_export::default_export_dir;
use quill::data_export::DataExportOptions;
use std::cell::RefCell;
use std::rc::Rc;

/// UI half of the account data export: dialog open flag, scope toggles
/// and the destination field. Progress/result come from
/// `session.data_export` in the driver.
pub struct DataExportUiState {
    pub open: bool,
    pub include_chats: bool,
    pub include_contacts: bool,
    pub include_media: bool,
    pub dest: Entity<TextareaState>,
}

impl DataExportUiState {
    pub fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            open: false,
            include_chats: true,
            include_contacts: true,
            include_media: true,
            dest: cx.new(|cx| TextareaState::new(window, cx)),
        }
    }

    fn options(&self) -> DataExportOptions {
        DataExportOptions {
            chats: self.include_chats,
            contacts: self.include_contacts,
            media: self.include_media,
        }
    }
}

impl QuillApp {
    /// Open the "Export Telegram data" dialog (hamburger menu entry).
    pub(crate) fn open_data_export(&mut self, cx: &mut Context<Self>) {
        self.data_export_ui.open = true;
        cx.notify();
    }

    fn close_data_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.data_export_ui.open = false;
        // Explicit close: keep the progress visible as a status note
        // (surfaced once from the poll loop), not as a lingering dialog.
        self.close_kit_dialog_if_done(DialogKind::DataExport, window, cx);
        cx.notify();
    }

    fn set_option(&mut self, id: &'static str, value: bool, cx: &mut Context<Self>) {
        let ui = &mut self.data_export_ui;
        match id {
            "chats" => ui.include_chats = value,
            "contacts" => ui.include_contacts = value,
            "media" => ui.include_media = value,
            _ => {}
        }
        cx.notify();
    }

    /// Read the destination field; empty means "use the Downloads default".
    fn export_dest(&self, cx: &App) -> std::path::PathBuf {
        let typed = self.data_export_ui.dest.read(cx).value().trim().to_string();
        if typed.is_empty() {
            default_export_dir()
        } else {
            std::path::PathBuf::from(typed)
        }
    }

    /// Footer Start: hand the scope + destination to the driver. The
    /// driver refuses when not signed in or an export already runs; both
    /// surface as a status note.
    fn start_data_export_from_dialog(&mut self, cx: &mut Context<Self>) {
        let dest = self.export_dest(cx);
        let options = self.data_export_ui.options();
        let started = self
            .live
            .as_mut()
            .is_some_and(|live| live.driver.start_data_export(options, dest).is_ok());
        self.status_note = if started {
            "Exporting Telegram data…".into()
        } else {
            "Could not start the export (not signed in, or an export is already running).".into()
        };
        cx.notify();
    }

    fn cancel_data_export_from_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.cancel_data_export();
        }
        self.status_note = "Data export cancelled.".into();
        cx.notify();
    }

    fn toggle_row(
        &self,
        id: &'static str,
        label: &str,
        hint: &str,
        value: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().child(label.to_string()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(hint.to_string()),
                    ),
            )
            .child(
                Switch::new(format!("data-export-{id}"))
                    .checked(value)
                    .accessibility_label(label)
                    .on_change(cx.listener(move |this, &v, _, cx| {
                        this.set_option(id, v, cx);
                    })),
            )
            .into_any_element()
    }

    /// Setup body: scope toggles + destination folder field.
    fn data_export_setup_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = &self.data_export_ui;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Download everything in your account — every chat's history, your contacts and media files — into one folder."),
            )
            .child(self.toggle_row(
                "chats",
                "Chat history",
                "Every chat's messages as JSON, one file per chat.",
                ui.include_chats,
                cx,
            ))
            .child(self.toggle_row(
                "contacts",
                "Contacts",
                "Your contact list with names and phone numbers.",
                ui.include_contacts,
                cx,
            ))
            .child(self.toggle_row(
                "media",
                "Media",
                "Photos, videos, files and voice messages.",
                ui.include_media,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().child("Destination folder"))
                    .child(Textarea::new(&self.data_export_ui.dest).h(px(40.)))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Empty = {}", default_export_dir().display())),
                    ),
            )
            .into_any_element()
    }

    /// Progress body: phase text + progress bar + Cancel.
    fn data_export_progress_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let chat_active = session
            .as_ref()
            .and_then(|s| s.chat_export.as_ref())
            .is_some();
        let (phase, frac) = session
            .and_then(|s| s.data_export.as_ref())
            .map(|dx| (dx.phase_label(chat_active), dx.progress_fraction()))
            .unwrap_or(("Exporting…", 0.0));
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_sm().child(phase.to_string()))
            .child(Progress::new("data-export-progress").value(frac.clamp(0.0, 1.0)))
            .child(
                div().flex().justify_end().child(
                    Button::new("data-export-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cancel_data_export_from_dialog(cx);
                        })),
                ),
            )
            .into_any_element()
    }

    /// Result body: output path + stats, or the failure reason.
    fn data_export_result_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let body = div().flex().flex_col().gap_2();
        let Some(dx) = self.session().and_then(|s| s.data_export.as_ref()) else {
            return body.into_any_element();
        };
        if let Some(failed) = dx.failed.as_deref() {
            body.child(div().text_sm().child("Export failed."))
                .child(
                    div()
                        .text_xs()
                        .text_color(danger())
                        .child(failed.to_string()),
                )
                .into_any_element()
        } else {
            let mut stats = format!(
                "{} chats · {} messages · {} media files",
                dx.exported_chats, dx.total_messages, dx.media_done
            );
            if dx.media_failed > 0 {
                stats.push_str(&format!(" ({} failed)", dx.media_failed));
            }
            if dx.skipped_secret_chats > 0 {
                stats.push_str(&format!(
                    " · {} secret chats skipped",
                    dx.skipped_secret_chats
                ));
            }
            body.child(div().text_sm().child("Export complete."))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(dx.dir.display().to_string()),
                )
                .child(div().text_xs().child(stats))
                .into_any_element()
        }
    }

    /// The kit `window.open_dialog` builder for [`DialogKind::DataExport`].
    /// Signature matches [`super::shell::DialogBuilder`].
    pub(super) fn build_data_export_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::DataExport, |this, _, _| {
                this.data_export_ui.open = false;
            });
        app.update(cx, |this, cx| {
            let running = this
                .session()
                .and_then(|s| s.data_export.as_ref())
                .is_some_and(|dx| !dx.settled());
            let finished = this
                .session()
                .and_then(|s| s.data_export.as_ref())
                .is_some_and(|dx| dx.settled());
            let live = this.live.is_some();

            let mut body = div().flex().flex_col().gap_3();
            if running {
                body = body.child(this.data_export_progress_body(cx));
            } else if finished {
                body = body.child(this.data_export_result_body(cx));
            } else {
                body = body.child(this.data_export_setup_body(cx));
            }
            let body_cell = Rc::new(RefCell::new(Some(body.into_any_element())));

            let can_start = live
                && !running
                && (this.data_export_ui.include_chats
                    || this.data_export_ui.include_contacts
                    || this.data_export_ui.include_media);
            let footer = div().flex().justify_end().gap_2();
            let footer =
                if running {
                    footer
                } else if finished {
                    footer.child(Button::new("data-export-close").label("Close").on_click(
                        cx.listener(|this, _, window, cx| {
                            this.close_data_export(window, cx);
                        }),
                    ))
                } else {
                    footer
                        .child(
                            Button::new("data-export-close")
                                .label("Close")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_data_export(window, cx);
                                })),
                        )
                        .child(
                            Button::new("data-export-start")
                                .label("Start export")
                                .primary()
                                .disabled(!can_start)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.start_data_export_from_dialog(cx);
                                })),
                        )
                };

            dialog
                .overlay(true)
                .title("Export Telegram data")
                .content(move |content, _, _| {
                    let body = body_cell
                        .borrow_mut()
                        .take()
                        .unwrap_or_else(|| div().into_any_element());
                    content.child(body)
                })
                .footer(footer)
                .on_close(on_close)
        })
    }
}
