//! Progress of a running chat export, shown in the export box
//! (tdesktop `export/view/export_view_progress`): a bar, what is being
//! done, and Cancel; when it ends, a way to open the folder.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::DialogKind;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::progress::Progress as ProgressBar;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::export::{Progress, Stage};
use quill::lang::{keys, tr, tr_with};
use quill::state::RedrawNeed;

pub(super) fn progress_redraw(changed: bool, settled: bool, hidden: bool) -> RedrawNeed {
    match (changed && !hidden, settled) {
        (false, _) => RedrawNeed::Nothing,
        (true, false) => RedrawNeed::Later,
        (true, true) => RedrawNeed::Now,
    }
}

fn ended_note(progress: &Progress) -> String {
    match &progress.stage {
        Stage::Finished(summary) => tr_with(
            keys::Q_OPS_EXPORTED_MESSAGES_TO,
            &[
                ("messages", &summary.messages.to_string()),
                ("display", &(summary.folder.display()).to_string()),
            ],
        ),
        _ => progress.label(),
    }
}

impl QuillApp {
    /// Poll-loop step: keep the box fresh while the export advances, and when
    /// it ended with the box hidden, say how it went and clear it.
    pub(super) fn poll_chat_export_progress(&mut self) -> RedrawNeed {
        // The poll loop calls this one hook for every export: the account,
        // the chat, and "Save as HTML" (`parity:menu-save-selection-html`).
        let account = self.poll_account_export_progress();
        let rich_html = if self.poll_rich_html_exports() {
            RedrawNeed::Now
        } else {
            RedrawNeed::Nothing
        };
        let chat = self.poll_one_chat_export();
        account.max(rich_html).max(chat)
    }

    fn poll_one_chat_export(&mut self) -> RedrawNeed {
        let Some(export) = self.session().and_then(|s| s.settings.chat_export.as_ref()) else {
            self.dialogs.chat_export_seen = None;
            return RedrawNeed::Nothing;
        };
        let progress = export.progress();
        let hidden = self.dialogs.chat_export_progress_hidden;
        if progress.is_settled() && hidden {
            if let Some(session) = self.session_mut() {
                session.settings.chat_export = None;
            }
            self.dialogs.chat_export_seen = None;
            self.dialogs.chat_export_progress_hidden = false;
            self.connection
                .set_note(StatusNote::info(ended_note(&progress)));
            return RedrawNeed::Now;
        }
        let changed = self.dialogs.chat_export_seen.as_ref() != Some(&progress);
        let need = progress_redraw(changed, progress.is_settled(), hidden);
        self.dialogs.chat_export_seen = Some(progress);
        need
    }

    /// Body and footer of the box while an export exists.
    pub(super) fn chat_export_progress(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (AnyElement, AnyElement) {
        let muted = cx.theme().muted_foreground;
        let Some(export) = self.session().and_then(|s| s.settings.chat_export.as_ref()) else {
            return (div().into_any_element(), div().into_any_element());
        };
        let progress = export.progress();
        let folder = export.folder.clone();
        let title = export.title.clone();
        let settled = progress.is_settled();
        let label = progress.label();
        let mut bar = ProgressBar::new("chat-export-progress");
        bar = match progress.fraction() {
            Some(fraction) => bar.value(fraction * 100.),
            None => bar.loading(!settled),
        };
        let mut body = div()
            .id("chat-export-running")
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_sm().text_color(muted).child(tr_with(
                keys::Q_OPS_HISTORY_OF,
                &[("title", &title.to_string())],
            )))
            .child(bar)
            .child(
                div()
                    .id("chat-export-status")
                    .role(Role::Status)
                    .aria_label(label.clone())
                    .text_sm()
                    .child(label),
            );
        if let Stage::Finished(summary) = &progress.stage {
            body = body.child(div().text_xs().text_color(muted).child(tr_with(
                keys::Q_OPS_FILES_SAVED_LEFT_OUT_OF,
                &[
                    ("files", &summary.files.to_string()),
                    ("skipped", &summary.skipped.to_string()),
                ],
            )));
        } else if !settled {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_OPS_YOU_CAN_CLOSE_THIS_WINDOW_2)),
            );
        }
        let mut footer = div().flex().justify_end().gap_2();
        if settled {
            if matches!(progress.stage, Stage::Finished(_)) {
                footer = footer.child(
                    Button::new("chat-export-show")
                        .label(tr(keys::Q_OPS_SHOW_FOLDER))
                        .outline()
                        .on_click(move |_, _, cx| cx.reveal_path(&folder)),
                );
            }
            footer = footer.child(
                Button::new("chat-export-done")
                    .label(tr(keys::BOX_DONE))
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(live) = this.live.as_mut() {
                            live.driver.session.settings.chat_export = None;
                        }
                        this.dialogs.chat_export_seen = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::ChatExport, window, cx);
                    })),
            );
        } else {
            footer = footer
                .child(
                    Button::new("chat-export-hide")
                        .label(tr(keys::Q_OPS_HIDE))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.dialogs.chat_export_progress_hidden = true;
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::ChatExport, window, cx);
                        })),
                )
                .child(
                    Button::new("chat-export-stop")
                        .label(tr(keys::Q_OPS_CANCEL_EXPORT))
                        .outline()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(export) =
                                this.session().and_then(|s| s.settings.chat_export.as_ref())
                            {
                                export.cancel();
                            }
                            cx.notify();
                        })),
                );
        }
        (body.into_any_element(), footer.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::{RedrawNeed, progress_redraw};

    #[test]
    fn a_running_export_redraws_in_the_coalesced_batch_and_only_when_shown() {
        assert_eq!(progress_redraw(true, false, false), RedrawNeed::Later);
        assert_eq!(progress_redraw(false, false, false), RedrawNeed::Nothing);
        assert_eq!(progress_redraw(true, false, true), RedrawNeed::Nothing);
    }

    #[test]
    fn the_end_of_an_export_shows_at_once_in_its_box() {
        assert_eq!(progress_redraw(true, true, false), RedrawNeed::Now);
        assert_eq!(progress_redraw(false, true, false), RedrawNeed::Nothing);
    }
}
