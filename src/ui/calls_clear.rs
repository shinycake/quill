//! "Clear all" on the Calls list (tdesktop `ClearCallsBox`): a confirm box
//! with a "Delete for everyone" checkbox. Copy and rules are in
//! `quill::chatlist_calls`.

use super::app::QuillApp;
use super::folder_share::finish_dialog;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::chatlist_calls::{
    CLEAR_ABOUT, CLEAR_BUTTON, CLEAR_REVOKE_LABEL, CLEAR_TITLE, can_clear,
};

impl QuillApp {
    pub(super) fn open_clear_calls(&mut self, cx: &mut Context<Self>) {
        let (count, clearing) = self
            .session()
            .map(|s| (s.recent_calls.len(), s.recent_calls_clearing))
            .unwrap_or_default();
        if !can_clear(count, clearing) {
            return;
        }
        self.chat_list.global.clear_calls_open = true;
        self.chat_list.global.clear_calls_revoke = false;
        cx.notify();
    }

    /// Confirmed: send the request. Demo sessions have no driver and
    /// only empty their own fixture, so a capture never reaches an account.
    fn confirm_clear_calls(&mut self, cx: &mut Context<Self>) {
        let revoke = self.chat_list.global.clear_calls_revoke;
        self.chat_list.global.clear_calls_open = false;
        match self.live.as_mut() {
            Some(live) => {
                if let Err(err) = live.driver.clear_call_history(revoke) {
                    self.connection.status_note =
                        format!("could not clear the call history: {err:?}");
                }
            }
            None => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.recent_calls.clear();
                }
            }
        }
        cx.notify();
    }

    pub(super) fn build_clear_calls_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ClearCalls, |this, _, cx| {
                this.chat_list.global.clear_calls_open = false;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let revoke = this.chat_list.global.clear_calls_revoke;
            let body = div()
                .id("clear-calls")
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().child(CLEAR_ABOUT))
                .child(
                    Checkbox::new("clear-calls-revoke")
                        .checked(revoke)
                        .label(CLEAR_REVOKE_LABEL)
                        .on_click(cx.listener(|this, &on, _, cx| {
                            this.chat_list.global.clear_calls_revoke = on;
                            cx.notify();
                        })),
                );
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("clear-calls-cancel")
                        .label("Cancel")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.chat_list.global.clear_calls_open = false;
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::ClearCalls, window, cx);
                        })),
                )
                .child(
                    Button::new("clear-calls-confirm")
                        .label(CLEAR_BUTTON)
                        .danger()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_clear_calls(cx);
                            this.close_kit_dialog_if_done(DialogKind::ClearCalls, window, cx);
                        })),
                );
            finish_dialog(
                dialog,
                CLEAR_TITLE.to_string(),
                body.into_any_element(),
                Some(footer.into_any_element()),
                on_close,
            )
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// "Clear all" on the Calls list.
    ClearCalls => DialogSpec::new(
        1600,
        |app| app.chat_list.global.clear_calls_open,
        QuillApp::build_clear_calls_dialog,
    ),
}
