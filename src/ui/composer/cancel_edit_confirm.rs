//! "Cancel editing?" (tdesktop `ComposeControls::maybeCancelEditMessage`,
//! `lng_cancel_edit_post_sure`): Escape in an edit whose text changed asks
//! first; the header's X and an unchanged edit cancel at once.

use super::*;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::dialog::Dialog;
use quill::composer::edit_text_changed;
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    /// Whether the field differs from the message being edited.
    fn edit_text_is_changed(&self, cx: &App) -> bool {
        let Some(edit) = self.composer_ui.pending_edit.as_ref() else {
            return false;
        };
        let Some(original) = self
            .session()
            .and_then(|session| session.edit_target_text(edit))
        else {
            return false;
        };
        edit_text_changed(&original, &self.composer_markup(cx))
    }

    /// Escape in the composer while editing: cancel, or confirm first when
    /// the text changed.
    pub(in crate::ui) fn request_cancel_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.edit_text_is_changed(cx) {
            self.composer_ui.cancel_edit_confirm = true;
            cx.notify();
        } else {
            self.clear_edit(window, cx);
        }
    }

    fn close_cancel_edit_confirm(&mut self, cx: &mut Context<Self>) {
        self.composer_ui.cancel_edit_confirm = false;
        cx.notify();
    }

    pub(in crate::ui) fn build_cancel_edit_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CancelEditConfirm, |this, _, cx| {
                this.close_cancel_edit_confirm(cx);
            });
        app.update(cx, |_, cx| {
            let body = div()
                .text_sm()
                .child(tr(keys::CANCEL_EDIT_POST_SURE))
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("cancel-edit-no")
                        .label(tr(keys::CANCEL_EDIT_POST_NO))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_cancel_edit_confirm(cx);
                            this.close_kit_dialog_if_done(
                                DialogKind::CancelEditConfirm,
                                window,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new("cancel-edit-yes")
                        .label(tr(keys::CANCEL_EDIT_POST_YES))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_cancel_edit_confirm(cx);
                            this.clear_edit(window, cx);
                            this.close_kit_dialog_if_done(
                                DialogKind::CancelEditConfirm,
                                window,
                                cx,
                            );
                        })),
                );
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// tdesktop's "Cancel editing?" box.
    CancelEditConfirm => DialogSpec::new(
        7625,
        |app| app.composer_ui.cancel_edit_confirm,
        QuillApp::build_cancel_edit_dialog,
    ),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "bars_tests.rs"]
mod bars_tests;
