//! A bot's alert: the box a callback answer with `show_alert` opens
//! (tdesktop `HistoryWidget` answers it with `Ui::MakeInformBox`).

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::lang::{keys, tr};

impl QuillApp {
    /// Show `text` in the bot alert box, replacing one still open.
    pub(super) fn open_bot_alert(&mut self, text: String, cx: &mut Context<Self>) {
        self.links.bot_alert = Some(text);
        self.open_box(DialogKind::BotAlert);
        cx.notify();
    }

    fn close_bot_alert(&mut self, cx: &mut Context<Self>) {
        self.links.bot_alert = None;
        cx.notify();
    }

    fn build_bot_alert_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::BotAlert, |this, _, cx| {
                this.close_bot_alert(cx);
            });
        app.update(cx, |this, cx| {
            let text = this.links.bot_alert.clone().unwrap_or_default();
            let footer = div().flex().justify_end().child(
                Button::new("bot-alert-ok")
                    .label(tr(keys::BOX_OK))
                    .primary()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_bot_alert(cx);
                        this.close_kit_dialog_if_done(DialogKind::BotAlert, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .w(px(360.))
                .content(move |content, _, _| {
                    content.child(
                        div()
                            .text_sm()
                            .text_color(text_primary())
                            .whitespace_normal()
                            .child(text.clone()),
                    )
                })
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    BotAlert => DialogSpec::new(
        2510,
        |app| app.links.bot_alert.is_some(),
        QuillApp::build_bot_alert_dialog,
    )
    .close(|app, _, cx| app.close_bot_alert(cx)),
}
