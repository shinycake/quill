//! The profile photo viewer's delete box (tdesktop `OverlayWidget::
//! deleteMedia`): "Do you want to delete this photo?" for your own photo
//! or a group or channel's current one, "Do you want to delete this
//! message?" for an earlier group or channel photo, whose service message
//! is what goes.

use super::*;
use quill::state::PhotoDelete;

impl QuillApp {
    /// Delete the box's photo once it is confirmed. The viewer closed when
    /// the box opened; the photo list follows the server's updates.
    fn confirm_photo_delete(&mut self, cx: &mut Context<Self>) {
        let Some((delete, revoke)) = self.dialogs.photo_delete.take() else {
            return;
        };
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_PROFILE_PHOTO_CHANGES_NEED_A,
            )));
            cx.notify();
            return;
        };
        if live.driver.delete_photo(delete, revoke).is_err() {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
            )));
        }
        cx.notify();
    }

    fn build_photo_delete_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PhotoDelete, |this, _, cx| {
                this.dialogs.photo_delete = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let question = match this.dialogs.photo_delete {
                Some((PhotoDelete::Message { .. }, _)) => tr(keys::SELECTED_DELETE_SURE_THIS),
                _ => tr(keys::DELETE_PHOTO_SURE),
            };
            dialog
                .overlay(true)
                .w(px(320.))
                .content(move |content, _, _| {
                    content.child(div().text_sm().child(question))
                })
                .footer(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("photo-delete-cancel")
                                .label(tr(keys::CANCEL))
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.dialogs.photo_delete = None;
                                    this.close_kit_dialog_if_done(
                                        DialogKind::PhotoDelete,
                                        window,
                                        cx,
                                    );
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("photo-delete-confirm")
                                .label(tr(keys::BOX_DELETE))
                                .danger()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.confirm_photo_delete(cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::PhotoDelete,
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                )
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// The profile photo viewer's delete box.
    PhotoDelete => DialogSpec::new(
        6110,
        |app| app.dialogs.photo_delete.is_some(),
        QuillApp::build_photo_delete_dialog,
    ),
}
