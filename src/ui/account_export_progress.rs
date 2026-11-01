//! Progress of a running account export, shown in the "Export Telegram
//! data" dialog (tdesktop `export/view/export_view_progress`): a bar, the
//! current step and chat, Cancel; when it ends, a way to open the folder.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::DialogKind;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::progress::Progress as ProgressBar;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::export::account::{AccountProgress, AccountStage};
use quill::lang::{keys, tr, tr_with};
use quill::state::RedrawNeed;

/// The status note for an export that ended while its box was hidden.
fn ended_note(progress: &AccountProgress) -> String {
    match &progress.stage {
        AccountStage::Finished(summary) => tr_with(
            keys::Q_OPS_YOUR_DATA_WAS_EXPORTED_TO,
            &[("display", &(summary.folder.display()).to_string())],
        ),
        _ => progress.label(),
    }
}

impl QuillApp {
    /// Poll-loop step: keep the box fresh while the export advances, and
    /// when it ended with the box hidden, say how it went and clear it.
    pub(super) fn poll_account_export_progress(&mut self) -> RedrawNeed {
        let Some(export) = self
            .session()
            .and_then(|s| s.settings.account_export.as_ref())
        else {
            self.dialogs.account_export_seen = None;
            return RedrawNeed::Nothing;
        };
        let progress = export.progress();
        let hidden = self.dialogs.account_export_progress_hidden;
        if progress.is_settled() && hidden {
            if let Some(session) = self.session_mut() {
                session.settings.account_export = None;
            }
            self.dialogs.account_export_seen = None;
            self.dialogs.account_export_progress_hidden = false;
            self.connection
                .set_note(StatusNote::info(ended_note(&progress)));
            return RedrawNeed::Now;
        }
        let changed = self.dialogs.account_export_seen.as_ref() != Some(&progress);
        let need = super::export_progress::progress_redraw(changed, progress.is_settled(), hidden);
        self.dialogs.account_export_seen = Some(progress);
        need
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
                notes.push(tr_with(
                    keys::Q_OPS_CHATS_WITH_PROTECTED_CONTENT_WERE,
                    &[("protected_chats", &summary.protected_chats.to_string())],
                ));
            }
            if summary.failed_chats > 0 {
                notes.push(tr_with(
                    keys::Q_OPS_CHATS_COULD_NOT_BE_READ,
                    &[("failed_chats", &summary.failed_chats.to_string())],
                ));
            }
            if summary.contacts_partial {
                notes.push(tr(keys::Q_OPS_SAVED_PHONE_CONTACTS_WERENT_AVAILABLE).into());
            }
            for note in notes {
                body = body.child(div().text_xs().text_color(muted).child(note));
            }
        } else if !settled {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_OPS_YOU_CAN_CLOSE_THIS_WINDOW)),
            );
        }
        let mut footer = div().flex().justify_end().gap_2();
        if settled {
            if matches!(progress.stage, AccountStage::Finished(_)) {
                footer = footer.child(
                    Button::new("account-export-show")
                        .label(tr(keys::EXPORT_DONE))
                        .outline()
                        .on_click(move |_, _, cx| cx.reveal_path(&folder)),
                );
            }
            footer = footer.child(
                Button::new("account-export-done")
                    .label(tr(keys::BOX_DONE))
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
                        .label(tr(keys::Q_OPS_HIDE))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.dialogs.account_export_progress_hidden = true;
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::AccountExport, window, cx);
                        })),
                )
                .child(
                    Button::new("account-export-stop")
                        .label(tr(keys::Q_OPS_CANCEL_EXPORT))
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
