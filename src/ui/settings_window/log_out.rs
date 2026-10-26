//! "Log out?" from the end of the Account page (tdesktop
//! `lng_sure_logout`), naming the account and what is lost on this device.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use crate::ui::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};

impl QuillApp {
    fn build_log_out_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(app, shell, DialogKind::LogOut, |this, _, cx| {
            this.settings.log_out_confirm = false;
            cx.notify();
        });
        app.update(cx, |this, cx| {
            let name = this
                .session()
                .and_then(|s| s.user(s.my_user_id?))
                .map(|me| me.display_name())
                .unwrap_or_default();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("log-out-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.settings.log_out_confirm = false;
                            this.close_kit_dialog_if_done(DialogKind::LogOut, window, cx);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("log-out-confirm")
                        .label(tr(keys::SETTINGS_LOGOUT))
                        .danger()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.settings.log_out_confirm = false;
                            this.submit_logout(cx);
                            this.close_kit_dialog_if_done(DialogKind::LogOut, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .width(px(400.))
                .title(crate::ui::shell::dialog_title(tr(keys::SURE_LOGOUT)))
                .content(move |content, _, cx| {
                    content.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .text_sm()
                            .when(!name.is_empty(), |this| {
                                this.child(div().font_semibold().child(name.clone()))
                            })
                            .child(
                                div()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(tr(keys::Q_SETTINGS_LOG_OUT_WARNING)),
                            ),
                    )
                })
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// Log out confirmation from the settings window.
    LogOut => DialogSpec::new(
        7310,
        |app| app.settings.open && app.settings.log_out_confirm,
        QuillApp::build_log_out_dialog,
    )
    .closes(|app, _, cx| { app.settings.log_out_confirm = false; cx.notify(); }),
}
