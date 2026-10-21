//! "Save as HTML" in the selection bar (`parity:menu-save-selection-html`):
//! with exactly one rich message selected, save it as a web page with its
//! media (tdesktop `Iv::AddSaveRichMessageHtmlAction`, offered in the
//! menu over a selection; Quill's selection actions live in the bar). The
//! export itself runs off the main thread (`quill::rich_html`); the poll
//! loop reports how it ended with a toast that reveals the saved page.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::rich_html::RichHtmlOutcome;
use std::path::{Path, PathBuf};

impl QuillApp {
    /// The message "Save as HTML" would save, while the selection allows
    /// it ([`quill::rich_html::selection_target`]).
    pub(super) fn selection_html_target(&self, chat_id: ChatId) -> Option<MessageId> {
        if !self.selecting_in(chat_id) {
            return None;
        }
        let draft = self.forward_draft()?;
        quill::rich_html::selection_target(self.session()?, chat_id, &draft.message_ids)
    }

    /// The bar's "Save as HTML" button.
    pub(super) fn selection_html_button(&self, cx: &mut Context<Self>) -> Button {
        Button::new("selection-save-html")
            .label(tr(keys::CONTEXT_SAVE_HTML))
            .ghost()
            .on_click(cx.listener(|this, _, window, cx| {
                this.save_selection_html(window, cx);
            }))
    }

    /// Save the selected rich message under the download folder, or under
    /// a folder the user picks when "Ask where to save each file" is on
    /// (tdesktop `Instance::exportRichMessageHtml`). The selection ends,
    /// as tdesktop cancels it after the action.
    fn save_selection_html(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(chat_id) = self.forward_draft().map(|draft| draft.from_chat_id) else {
            return;
        };
        if self.refuse_protected_copy(chat_id, cx) {
            return;
        }
        let Some(message_id) = self.selection_html_target(chat_id) else {
            return;
        };
        self.clear_forward(window, cx);
        if !quill::file_prefs::current().ask_download_path {
            let base = quill::media_viewer::downloads_dir().unwrap_or_else(std::env::temp_dir);
            self.start_rich_html(chat_id, message_id, base, cx);
            return;
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr(keys::DOWNLOAD_PATH_CHOOSE).into()),
        });
        // window-scope: the export names its own chat and message and
        // reports through the app-wide toast.
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dirs))) = picker.await else {
                return;
            };
            let Some(base) = dirs.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.start_rich_html(chat_id, message_id, base, cx);
            });
        })
        .detach();
    }

    fn start_rich_html(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        base: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let started = self.live.as_mut().map(|live| {
            live.driver
                .start_rich_html_export(chat_id, message_id, base)
        });
        if matches!(started, Some(Err(_))) {
            self.connection
                .set_note(StatusNote::error(tr(keys::EXPORT_HTML_FAILED)));
        }
        cx.notify();
    }

    /// Forward the exports' TDLib requests and toast the ones that ended.
    /// Returns whether one is still running, so the poll loop keeps going.
    pub(super) fn poll_rich_html_exports(&mut self) -> bool {
        let Some(live) = self.live.as_mut() else {
            return false;
        };
        live.driver.pump_rich_html_exports();
        let finished = live.driver.take_finished_rich_html_exports();
        let running = live.driver.rich_html_exports_running();
        for outcome in finished {
            match outcome {
                RichHtmlOutcome::Saved(html) => {
                    let note = StatusNote::info(saved_text(&html));
                    self.connection.set_note(note.with_reveal(html));
                }
                RichHtmlOutcome::Failed => self
                    .connection
                    .set_note(StatusNote::error(tr(keys::EXPORT_HTML_FAILED))),
                RichHtmlOutcome::Cancelled => {}
            }
        }
        running
    }
}

/// "Message was saved to your Downloads folder" (`lng_export_html_saved_to`).
/// The folder reads "Downloads" for the download folder and otherwise the
/// folder's own name, as the media viewer's toast does, so a folder picked
/// in the dialog is never mislabeled.
fn saved_text(html: &Path) -> String {
    let base = html.parent().and_then(Path::parent);
    let downloads = quill::media_viewer::downloads_dir();
    let folder = match base {
        Some(base) if Some(base) != downloads.as_deref() => base
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| tr(keys::MEDIAVIEW_DOWNLOADS).to_owned()),
        _ => tr(keys::MEDIAVIEW_DOWNLOADS).to_owned(),
    };
    tr_with(keys::EXPORT_HTML_SAVED_TO, &[("downloads", &folder)])
}
