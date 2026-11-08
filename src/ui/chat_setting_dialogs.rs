//! A chat's notifications, auto-delete timer and folders as dialogs over the
//! window. tdesktop opens a box for each (`MuteSettingsBox`, `TTLBox`, the
//! folders submenu); they used to be panels pushed in under the chat header,
//! which only showed with a chat open and moved the messages down.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::lang::{keys, tr};

/// tdesktop `boxWidth`.
const BOX_WIDTH: f32 = 380.;
/// tdesktop `boxWideWidth`, for the notification settings.
const WIDE_WIDTH: f32 = 420.;

/// A dialog that shows `body` from the app on every frame.
fn chat_setting_dialog(
    app: &Entity<QuillApp>,
    shell: &Entity<QuillShell>,
    dialog: Dialog,
    (kind, width, title): (DialogKind, f32, String),
    close: fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>),
    body: fn(&QuillApp, &mut Context<QuillApp>) -> AnyElement,
) -> Dialog {
    let on_close = QuillShell::on_close_kind(app, shell, kind, close);
    let body_app = app.clone();
    dialog
        .overlay(true)
        .w(px(width))
        .title(crate::ui::shell::dialog_title(title))
        .content(move |content, _, cx| {
            content.child(body_app.update(cx, |this, cx| body(this, cx)))
        })
        .on_close(on_close)
}

impl QuillApp {
    fn build_mute_menu_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let title = app.read(cx).mute_menu_title();
        chat_setting_dialog(
            app,
            shell,
            dialog,
            (DialogKind::MuteMenu, WIDE_WIDTH, title),
            |this, _, cx| this.close_mute_menu(cx),
            |this, cx| this.mute_menu_panel(cx).into_any_element(),
        )
    }

    fn build_ttl_picker_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let title = app.read(cx).ttl_picker_title().to_string();
        chat_setting_dialog(
            app,
            shell,
            dialog,
            (DialogKind::TtlPicker, BOX_WIDTH, title),
            |this, _, cx| {
                this.notify.ttl_picker = None;
                cx.notify();
            },
            |this, cx| this.ttl_picker_panel(cx).into_any_element(),
        )
    }

    fn build_folder_menu_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        _cx: &mut App,
    ) -> Dialog {
        chat_setting_dialog(
            app,
            shell,
            dialog,
            (
                DialogKind::FolderMenu,
                BOX_WIDTH,
                tr(keys::Q_CHATLIST_CHAT_FOLDERS).to_string(),
            ),
            |this, _, cx| this.close_folder_menu(cx),
            |this, cx| this.folder_menu_panel(cx).into_any_element(),
        )
    }
}

crate::ui::shell::register_dialogs! {
    /// A chat's notification settings (tdesktop `MuteSettingsBox`).
    MuteMenu => DialogSpec::new(
        2932,
        |app| app.notify.mute_menu_open,
        QuillApp::build_mute_menu_dialog,
    )
    .close(|app, _, cx| app.close_mute_menu(cx)),
    /// The auto-delete timer (tdesktop `TTLBox`).
    TtlPicker => DialogSpec::new(
        2933,
        |app| app.notify.ttl_picker.is_some(),
        QuillApp::build_ttl_picker_dialog,
    )
    .close(|app, _, cx| {
            app.notify.ttl_picker = None;
            cx.notify();
        }),
    /// The chat's folders.
    FolderMenu => DialogSpec::new(
        2934,
        |app| app.folders.menu_open,
        QuillApp::build_folder_menu_dialog,
    )
    .close(|app, _, cx| app.close_folder_menu(cx)),
}
