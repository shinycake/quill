//! "Chat history for new members" (tdesktop
//! `EditPeerHistoryVisibilityBox`): Visible or Hidden with what each
//! means; a basic group keeps its last 100 messages visible. Save hands
//! the choice to the Edit box, which sends it with its own Save (a basic
//! group is upgraded first); Cancel with a change asks first.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::group_manage::history_hidden_about;
use quill::lang::{keys, tr};
use quill::telegram::envelope::ChatKind;

impl QuillApp {
    fn history_box_choice(&self) -> Option<bool> {
        self.group_edit_box()?.history_box
    }

    fn set_history_box_choice(&mut self, visible: bool, cx: &mut Context<Self>) {
        if let Some(edit) = self.group_edit_box_mut()
            && edit.history_box.is_some()
        {
            edit.history_box = Some(visible);
        }
        cx.notify();
    }

    fn close_history_box(&mut self, cx: &mut Context<Self>) {
        if let Some(edit) = self.group_edit_box_mut() {
            edit.history_box = None;
        }
        cx.notify();
    }

    fn save_history_box(&mut self, cx: &mut Context<Self>) {
        if let Some(edit) = self.group_edit_box_mut() {
            edit.history_visible = edit.history_box.take();
        }
        cx.notify();
    }

    /// The value the row showed when the box opened.
    fn history_box_saved(&self) -> Option<bool> {
        let edit = self.group_edit_box()?;
        let control = self
            .session()?
            .group_admin_controls(edit.chat_id)
            .history?;
        Some(edit.history_visible.unwrap_or(control.visible))
    }

    /// tdesktop `tryClose`: an unsaved change asks before it is dropped.
    fn cancel_history_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history_box_choice() == self.history_box_saved() {
            self.close_history_box(cx);
            self.close_kit_dialog_if_done(DialogKind::GroupHistoryBox, window, cx);
            return;
        }
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let app = app.clone();
            alert
                .description(tr(keys::BOT_CLOSE_WARNING))
                .ok_text(tr(keys::BOT_CLOSE_WARNING_SURE))
                .cancel_text(tr(keys::CREATE_GROUP_BACK))
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    let _ = app.update(cx, |this, cx| {
                        this.close_history_box(cx);
                        this.close_kit_dialog_if_done(DialogKind::GroupHistoryBox, window, cx);
                    });
                    true
                })
        });
    }

    pub(super) fn build_group_history_box(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GroupHistoryBox, |this, _, cx| {
                this.close_history_box(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .overlay_closable(false)
                .keyboard(false)
                .close_button(false)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::MANAGE_HISTORY_VISIBILITY_TITLE,
                )))
                .on_close(on_close);
            let Some(visible) = this.history_box_choice() else {
                return dialog;
            };
            let basic_group = this
                .group_edit_box()
                .and_then(|edit| this.session()?.chats.get(&edit.chat_id.0))
                .is_some_and(|chat| matches!(chat.kind, ChatKind::BasicGroup { .. }));
            let muted = cx.theme().muted_foreground;
            let about = |text: &'static str| div().text_xs().text_color(muted).child(text);
            let entity = cx.entity();
            let body = RadioGroup::vertical("group-history-visibility")
                .selected_index(Some(if visible { 0 } else { 1 }))
                .on_click(move |index, _, cx| {
                    entity.update(cx, |this, cx| this.set_history_box_choice(*index == 0, cx));
                })
                .child(
                    Radio::new("group-history-visible")
                        .label(tr(keys::MANAGE_HISTORY_VISIBILITY_SHOWN))
                        .child(about(tr(keys::MANAGE_HISTORY_VISIBILITY_SHOWN_ABOUT))),
                )
                .child(
                    Radio::new("group-history-hidden")
                        .label(tr(keys::MANAGE_HISTORY_VISIBILITY_HIDDEN))
                        .child(about(tr(history_hidden_about(basic_group)))),
                );
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("group-history-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_history_box(window, cx);
                        })),
                )
                .child(
                    Button::new("group-history-save")
                        .label(tr(keys::SETTINGS_SAVE))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.save_history_box(cx);
                            this.close_kit_dialog_if_done(DialogKind::GroupHistoryBox, window, cx);
                        })),
                );
            let body = std::rc::Rc::new(std::cell::RefCell::new(Some(body.into_any_element())));
            dialog
                .content(move |content, _, _| {
                    let body = body
                        .borrow_mut()
                        .take()
                        .unwrap_or_else(|| div().into_any_element());
                    content.child(body)
                })
                .footer(footer)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// Over the Edit box that opened it.
    GroupHistoryBox => DialogSpec::new(
        7690,
        |app| app.group_edit_box().is_some_and(|edit| edit.history_box.is_some()),
        QuillApp::build_group_history_box,
    ),
}
