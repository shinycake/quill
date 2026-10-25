#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::folder_share::finish_dialog;
use crate::ui::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::telegram::envelope::MessageSender;

impl QuillApp {
    pub(super) fn enable_private_sharing(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_PEOPLE_DEMO_MODE_THIS_NEEDS_A)));
            cx.notify();
            return;
        };
        if let Err(err) = live.driver.enable_private_sharing(chat_id) {
            self.connection.set_note(StatusNote::error(user_error_for(
                keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
                &err,
            )));
        }
        cx.notify();
    }

    fn sharing_request_box_chat(&self) -> Option<ChatId> {
        self.session()
            .and_then(|session| session.groups.private_sharing.request_box_chat())
    }

    fn close_sharing_request_box(&mut self, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => live.driver.close_sharing_request_box(),
            None => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.groups.private_sharing.enable = None;
                }
            }
        }
        cx.notify();
    }

    fn send_sharing_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(chat_id) = self.sharing_request_box_chat() else {
            return;
        };
        let Some(live) = self.live.as_mut() else {
            self.close_sharing_request_box(cx);
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_PEOPLE_DEMO_MODE_THIS_NEEDS_A)));
            return;
        };
        if let Err(err) = live.driver.send_sharing_request() {
            self.connection.set_note(StatusNote::error(user_error_for(
                keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
                &err,
            )));
        }
        self.dismiss_profile_modal();
        self.select_listed_chat(chat_id, window, cx);
        cx.notify();
    }

    pub(in crate::ui) fn build_sharing_request_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::SharingRequest, |this, _, cx| {
                this.close_sharing_request_box(cx);
            });
        app.update(cx, |this, cx| {
            let name = this
                .sharing_request_box_chat()
                .and_then(|chat_id| {
                    let session = this.session()?;
                    let user_id = session.private_chat_user_id(chat_id)?;
                    Some(crate::ui::suggested_posts_ui::sender_short_name(
                        session,
                        Some(MessageSender::User { user_id }),
                    ))
                })
                .unwrap_or_default()
                .replace("**", "*\u{2060}*");
            let text = tr_with(keys::ENABLE_SHARING_REQUEST_TEXT, &[("name", &name)]);
            let (plain, bold) = crate::ui::screen_chooser::split_bold(&text);
            let style = HighlightStyle {
                font_weight: Some(FontWeight::SEMIBOLD),
                ..Default::default()
            };
            let body = div().id("sharing-request-box").text_sm().child(
                StyledText::new(plain)
                    .with_highlights(bold.into_iter().map(|range| (range, style))),
            );
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .w_full()
                .child(
                    Button::new("sharing-request-cancel")
                        .debug_selector(|| "sharing-request-cancel".to_string())
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_sharing_request_box(cx);
                            this.close_kit_dialog_if_done(DialogKind::SharingRequest, window, cx);
                        })),
                )
                .child(
                    Button::new("sharing-request-send")
                        .debug_selector(|| "sharing-request-send".to_string())
                        .label(tr(keys::ENABLE_SHARING_REQUEST_BUTTON))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.send_sharing_request(window, cx);
                            this.close_kit_dialog_if_done(DialogKind::SharingRequest, window, cx);
                        })),
                );
            finish_dialog(
                dialog,
                tr(keys::ENABLE_SHARING_REQUEST_TITLE).to_string(),
                body.into_any_element(),
                Some(footer.into_any_element()),
                on_close,
            )
        })
    }
}

crate::ui::shell::register_dialogs! {
    SharingRequest => DialogSpec::new(
        6151,
        |app| app.sharing_request_box_chat().is_some(),
        QuillApp::build_sharing_request_dialog,
    ),
}
