//! Progress of a running chat export, shown in the export box
//! (tdesktop `export/view/export_view_progress`): a bar, what is being
//! done, and Cancel; when it ends, a way to open the folder.

use super::app::QuillApp;
use super::shell::DialogKind;
use gpui_kit::component::button::*;
use gpui_kit::component::progress::Progress as ProgressBar;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::export::{Progress, Stage};

impl QuillApp {
    /// Poll-loop step: keep the box fresh while the export advances, and when
    /// it ended with the box hidden, say how it went and clear it.
    pub(super) fn poll_chat_export_progress(&mut self) -> bool {
        // The poll loop calls this one hook for both exports.
        let account = self.poll_account_export_progress();
        self.poll_one_chat_export() || account
    }

    fn poll_one_chat_export(&mut self) -> bool {
        let Some(live) = self.live.as_mut() else {
            return false;
        };
        let Some(export) = live.driver.session.settings.chat_export.as_ref() else {
            self.dialogs.chat_export_seen = None;
            return false;
        };
        let progress = export.progress();
        if progress.is_settled() && self.dialogs.chat_export_progress_hidden {
            let note = match &progress.stage {
                Stage::Finished(summary) => format!(
                    "Exported {} messages to {}",
                    summary.messages,
                    summary.folder.display()
                ),
                other => Progress {
                    stage: other.clone(),
                    ..progress.clone()
                }
                .label(),
            };
            live.driver.session.settings.chat_export = None;
            self.dialogs.chat_export_seen = None;
            self.dialogs.chat_export_progress_hidden = false;
            self.connection.status_note = note;
            return true;
        }
        let changed = self.dialogs.chat_export_seen.as_ref() != Some(&progress);
        self.dialogs.chat_export_seen = Some(progress);
        changed
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
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(format!("History of {title}")),
            )
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
            body = body.child(div().text_xs().text_color(muted).child(format!(
                "{} files saved, {} left out of the export.",
                summary.files, summary.skipped
            )));
        } else if !settled {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("You can close this window. The export keeps running."),
            );
        }
        let mut footer = div().flex().justify_end().gap_2();
        if settled {
            if matches!(progress.stage, Stage::Finished(_)) {
                footer = footer.child(
                    Button::new("chat-export-show")
                        .label("Show folder")
                        .outline()
                        .on_click(move |_, _, cx| cx.reveal_path(&folder)),
                );
            }
            footer = footer.child(Button::new("chat-export-done").label("Done").on_click(
                cx.listener(|this, _, window, cx| {
                    if let Some(live) = this.live.as_mut() {
                        live.driver.session.settings.chat_export = None;
                    }
                    this.dialogs.chat_export_seen = None;
                    cx.notify();
                    this.close_kit_dialog_if_done(DialogKind::ChatExport, window, cx);
                }),
            ));
        } else {
            footer = footer
                .child(
                    Button::new("chat-export-hide")
                        .label("Hide")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.dialogs.chat_export_progress_hidden = true;
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::ChatExport, window, cx);
                        })),
                )
                .child(
                    Button::new("chat-export-stop")
                        .label("Cancel export")
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
