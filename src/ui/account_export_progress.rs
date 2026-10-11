//! Progress of a running account export, shown in the "Export Telegram
//! data" dialog (tdesktop `export/view/export_view_progress`): a bar, the
//! current step and chat, Cancel; when it ends, a way to open the folder.

use super::app::QuillApp;
use super::shell::DialogKind;
use gpui_kit::component::button::*;
use gpui_kit::component::progress::Progress as ProgressBar;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::export::account::{AccountProgress, AccountStage};

/// The status note for an export that ended while its box was hidden.
fn ended_note(progress: &AccountProgress) -> String {
    match &progress.stage {
        AccountStage::Finished(summary) => {
            format!("Your data was exported to {}", summary.folder.display())
        }
        _ => progress.label(),
    }
}

impl QuillApp {
    /// Poll-loop step: keep the box fresh while the export advances, and
    /// when it ended with the box hidden, say how it went and clear it.
    pub(super) fn poll_account_export_progress(&mut self) -> bool {
        let Some(live) = self.live.as_mut() else {
            return false;
        };
        let Some(export) = live.driver.session.settings.account_export.as_ref() else {
            self.dialogs.account_export_seen = None;
            return false;
        };
        let progress = export.progress();
        if progress.is_settled() && self.dialogs.account_export_progress_hidden {
            live.driver.session.settings.account_export = None;
            self.dialogs.account_export_seen = None;
            self.dialogs.account_export_progress_hidden = false;
            self.connection.status_note = ended_note(&progress);
            return true;
        }
        let changed = self.dialogs.account_export_seen.as_ref() != Some(&progress);
        self.dialogs.account_export_seen = Some(progress);
        changed
    }

    /// Body and footer of the dialog while an export exists.
    pub(super) fn account_export_progress(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (AnyElement, AnyElement) {
        let muted = cx.theme().muted_foreground;
        let Some(export) = self
            .session()
            .and_then(|s| s.settings.account_export.as_ref())
        else {
            return (div().into_any_element(), div().into_any_element());
        };
        let progress = export.progress();
        let folder = export.folder.clone();
        let settled = progress.is_settled();
        let label = progress.label();
        let mut bar = ProgressBar::new("account-export-progress");
        bar = match progress.fraction() {
            Some(fraction) => bar.value(fraction * 100.),
            None => bar.loading(!settled),
        };
        let mut body = div()
            .id("account-export-running")
            .flex()
            .flex_col()
            .gap_3()
            .child(bar)
            .child(
                div()
                    .id("account-export-status")
                    .role(Role::Status)
                    .aria_label(label.clone())
                    .text_sm()
                    .child(label),
            );
        if let Some(detail) = progress.detail() {
            body = body.child(div().text_xs().text_color(muted).child(detail));
        }
        if let AccountStage::Finished(summary) = &progress.stage {
            let mut notes = Vec::new();
            if summary.protected_chats > 0 {
                notes.push(format!(
                    "{} chats with protected content were left out.",
                    summary.protected_chats
                ));
            }
            if summary.failed_chats > 0 {
                notes.push(format!(
                    "{} chats could not be read and have no messages in the export.",
                    summary.failed_chats
                ));
            }
            if summary.contacts_partial {
                notes.push(
                    "Saved phone contacts weren't available, so the contacts list has your \
                     Telegram contacts instead."
                        .into(),
                );
            }
            for note in notes {
                body = body.child(div().text_xs().text_color(muted).child(note));
            }
        } else if !settled {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("You can close this window. Keep Quill open until the export is done."),
            );
        }
        let mut footer = div().flex().justify_end().gap_2();
        if settled {
            if matches!(progress.stage, AccountStage::Finished(_)) {
                footer = footer.child(
                    Button::new("account-export-show")
                        .label("Show my data")
                        .outline()
                        .on_click(move |_, _, cx| cx.reveal_path(&folder)),
                );
            }
            footer = footer.child(
                Button::new("account-export-done")
                    .label("Done")
                    .primary()
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(live) = this.live.as_mut() {
                            live.driver.session.settings.account_export = None;
                        }
                        this.dialogs.account_export_seen = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::AccountExport, window, cx);
                    })),
            );
        } else {
            footer = footer
                .child(
                    Button::new("account-export-hide")
                        .label("Hide")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.dialogs.account_export_progress_hidden = true;
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::AccountExport, window, cx);
                        })),
                )
                .child(
                    Button::new("account-export-stop")
                        .label("Cancel export")
                        .outline()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(export) = this
                                .session()
                                .and_then(|s| s.settings.account_export.as_ref())
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
