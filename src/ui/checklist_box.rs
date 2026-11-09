//! The checklist box ("New Checklist" / "Add Tasks", tdesktop
//! `EditTodoListBox`) and the poll "Add an Option" box over the window.
//! Both used to be panels pushed in above the composer, framed by an
//! accent border; now they are kit dialogs with their buttons pinned under
//! the content, like the poll box (`polls/poll_box.rs`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::dialog_focus::focus_after_mount;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::lang::{keys, tr};

/// tdesktop `boxWidth`.
const BOX_WIDTH: f32 = 380.;

impl QuillApp {
    fn build_checklist_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let adding = app
            .read(cx)
            .chat_ui
            .checklist_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.add_to.is_some());
        let title = if adding {
            tr(keys::Q_MEDIA_ADD_TASKS)
        } else {
            tr(keys::Q_MEDIA_NEW_CHECKLIST)
        };
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ChecklistBox, |this, _, cx| {
                this.close_checklist_dialog(cx);
            });
        let footer = app.update(cx, |this, cx| this.checklist_box_footer(cx));
        let body_app = app.clone();
        dialog
            .overlay(true)
            .w(px(BOX_WIDTH))
            .title(crate::ui::shell::dialog_title(title))
            .content(crate::ui::shell::scrollable_dialog_content(
                move |content, window, cx| {
                    let body = body_app.update(cx, |this, cx| {
                        let focus = this
                            .chat_ui
                            .checklist_dialog
                            .as_mut()
                            .and_then(|dialog| dialog.focus_on_mount.take());
                        focus_after_mount(focus, window, cx);
                        this.checklist_dialog_panel(cx)
                    });
                    content.children(body)
                },
            ))
            .footer(footer)
            .on_close(on_close)
    }

    fn build_poll_add_option_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PollAddOption, |this, _, cx| {
                this.close_poll_add_option(cx);
            });
        let footer = app.update(cx, |this, cx| this.poll_add_option_footer(cx));
        let body_app = app.clone();
        dialog
            .overlay(true)
            .w(px(BOX_WIDTH))
            .title(crate::ui::shell::dialog_title(tr(
                keys::Q_MEDIA_ADD_AN_OPTION,
            )))
            .content(move |content, window, cx| {
                let body = body_app.update(cx, |this, cx| {
                    let focus = this.message_ui.poll_add_option.as_mut().and_then(|panel| {
                        std::mem::take(&mut panel.focus_on_mount).then(|| panel.input.clone())
                    });
                    focus_after_mount(focus, window, cx);
                    this.poll_add_option_panel(cx)
                });
                content.children(body)
            })
            .footer(footer)
            .on_close(on_close)
    }
}

crate::ui::shell::register_dialogs! {
    ChecklistBox => DialogSpec::new(
        2938,
        |app| app.chat_ui.checklist_dialog.is_some(),
        QuillApp::build_checklist_dialog,
    )
    .per_window()
    .close(|app, _, cx| app.close_checklist_dialog(cx)),
    PollAddOption => DialogSpec::new(
        2939,
        |app| app.message_ui.poll_add_option.is_some(),
        QuillApp::build_poll_add_option_dialog,
    )
    .per_window()
    .close(|app, _, cx| app.close_poll_add_option(cx)),
}
