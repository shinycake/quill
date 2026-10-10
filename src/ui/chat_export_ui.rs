//! The "Export chat history" box (tdesktop `export/view/export_view_settings`
//! for one chat): format, date range and "Only my messages", then Export.

use super::app::QuillApp;
use super::folder_share::finish_dialog;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::chat_export::{ChatExportOptions, ExportFormat, ExportRange};
use quill::ids::ChatId;

/// The box's choices while it is open.
#[derive(Debug, Clone)]
pub(super) struct ChatExportDraft {
    pub chat_id: ChatId,
    pub title: String,
    pub options: ChatExportOptions,
}

impl QuillApp {
    /// Open the options box for `chat_id`; the export starts from its button.
    pub(super) fn start_chat_export(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let title = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|c| c.title.clone())
            .unwrap_or_else(|| "chat".to_string());
        self.dialogs.chat_export_dialog = Some(ChatExportDraft {
            chat_id,
            title,
            options: ChatExportOptions::default(),
        });
        cx.notify();
    }

    /// Start the export with the box's choices. The driver pages
    /// `getChatHistory` in the background; completion (or failure) surfaces
    /// as a status note from `poll_live`.
    fn run_chat_export(&mut self, draft: ChatExportDraft, cx: &mut Context<Self>) {
        let protected = self
            .session()
            .is_some_and(|s| s.chat_has_protected_content(draft.chat_id));
        let started = self.live.as_mut().is_some_and(|live| {
            live.driver
                .start_chat_export(draft.chat_id, draft.title, draft.options)
                .is_ok()
        });
        self.connection.status_note = if started {
            "Exporting chat history…".into()
        } else if protected {
            "This chat's content is protected and can't be exported.".into()
        } else {
            "Could not start the export (another export is running).".into()
        };
        cx.notify();
    }

    pub(super) fn build_chat_export_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ChatExport, |this, _, cx| {
                this.dialogs.chat_export_dialog = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let Some(draft) = this.dialogs.chat_export_dialog.clone() else {
                return finish_dialog(
                    dialog,
                    "Export chat history".into(),
                    div().into_any_element(),
                    None,
                    on_close,
                );
            };
            let options = draft.options;
            let muted = cx.theme().muted_foreground;
            let heading = |text: &'static str| div().text_sm().font_semibold().child(text);
            let format = RadioGroup::horizontal("chat-export-format")
                .selected_index(Some(usize::from(options.format == ExportFormat::Json)))
                .children([
                    Radio::new("chat-export-html").label("HTML"),
                    Radio::new("chat-export-json").label("JSON"),
                ])
                .on_click(cx.listener(|this, &ix, _, cx| {
                    if let Some(draft) = this.dialogs.chat_export_dialog.as_mut() {
                        draft.options.format = if ix == 0 {
                            ExportFormat::Html
                        } else {
                            ExportFormat::Json
                        };
                    }
                    cx.notify();
                }));
            let range = RadioGroup::vertical("chat-export-range")
                .selected_index(ExportRange::ALL.iter().position(|r| *r == options.range))
                .children(
                    ExportRange::ALL
                        .iter()
                        .enumerate()
                        .map(|(i, r)| Radio::new(("chat-export-range-item", i)).label(r.label())),
                )
                .on_click(cx.listener(|this, &ix, _, cx| {
                    if let Some(draft) = this.dialogs.chat_export_dialog.as_mut() {
                        draft.options.range = ExportRange::ALL[ix];
                    }
                    cx.notify();
                }));
            let only_mine = div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Switch::new("chat-export-only-mine")
                        .checked(options.only_mine)
                        .accessibility_label("Only my messages")
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some(draft) = this.dialogs.chat_export_dialog.as_mut() {
                                draft.options.only_mine = on;
                            }
                            cx.notify();
                        })),
                )
                .child(div().text_sm().child("Only my messages"));
            let body = div()
                .id("chat-export")
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().text_color(muted).child(format!(
                    "Save the history of {} as a file in Downloads.",
                    draft.title
                )))
                .child(heading("Format"))
                .child(format)
                .child(heading("Period"))
                .child(range)
                .child(only_mine)
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Media appears as a label. Files are not included."),
                );
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("chat-export-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.dialogs.chat_export_dialog = None;
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::ChatExport, window, cx);
                        })),
                )
                .child(
                    Button::new("chat-export-start")
                        .label("Export")
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(draft) = this.dialogs.chat_export_dialog.take() {
                                this.run_chat_export(draft, cx);
                            }
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::ChatExport, window, cx);
                        })),
                );
            finish_dialog(
                dialog,
                "Export chat history".into(),
                body.into_any_element(),
                Some(footer.into_any_element()),
                on_close,
            )
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// The "Export chat history" options box.
    ChatExport => DialogSpec::new(
        1500,
        |app| app.dialogs.chat_export_dialog.is_some(),
        QuillApp::build_chat_export_dialog,
    ),
}
