//! The Archive's "How does it work?" box (tdesktop `ArchiveHintBox`,
//! `boxes/about_box.cpp`): a title, what a new message does to archived
//! chats, three short sections and a "Got it" button.

use super::app::QuillApp;
use super::folder_share::finish_dialog;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::chatlist_archive::{
    ARCHIVE_HINT_SECTIONS, ARCHIVE_HINT_TITLE, archive_hint_about, unarchive_on_new_message,
};

impl QuillApp {
    pub(super) fn open_archive_hint(&mut self, cx: &mut Context<Self>) {
        self.chat_list.archive_hint_open = true;
        // The wording depends on the account's archive settings.
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .session
                .chat_list
                .archive_chat_list_settings
                .is_none()
        {
            let _ = live.driver.fetch_archive_chat_list_settings();
        }
        cx.notify();
    }

    pub(super) fn build_archive_hint_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ArchiveHint, |this, _, cx| {
                this.chat_list.archive_hint_open = false;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let muted = cx.theme().muted_foreground;
            let keep = this
                .session()
                .and_then(|s| s.chat_list.archive_chat_list_settings)
                .map(|settings| settings.keep_unmuted_chats_archived);
            let about = archive_hint_about(unarchive_on_new_message(keep));
            let icons = [IconName::Archive, IconName::EyeOff, IconName::CircleUser];
            let mut body = div().id("archive-hint").flex().flex_col().gap_4().child(
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap_1()
                    .child(div().id("archive-hint-about").text_sm().child(about))
                    .child(
                        Button::new("archive-hint-change")
                            .label("Tap to change")
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.chat_list.archive_hint_open = false;
                                this.open_archive_settings(cx);
                                this.close_kit_dialog_if_done(DialogKind::ArchiveHint, window, cx);
                            })),
                    ),
            );
            for ((title, info), icon) in ARCHIVE_HINT_SECTIONS.iter().zip(icons) {
                body = body.child(
                    div()
                        .flex()
                        .gap_3()
                        .child(
                            div()
                                .flex_none()
                                .pt_0p5()
                                .child(Icon::new(icon).size(px(18.)).text_color(muted)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .gap_0p5()
                                .child(div().text_sm().font_semibold().child(*title))
                                .child(div().text_sm().text_color(muted).child(*info)),
                        ),
                );
            }
            let footer = div().flex().justify_end().child(
                Button::new("archive-hint-got-it")
                    .label("Got it")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.chat_list.archive_hint_open = false;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::ArchiveHint, window, cx);
                    })),
            );
            finish_dialog(
                dialog,
                ARCHIVE_HINT_TITLE.to_string(),
                body.into_any_element(),
                Some(footer.into_any_element()),
                on_close,
            )
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// The Archive menu's "How does it work?" box.
    ArchiveHint => DialogSpec::new(
        1400,
        |app| app.chat_list.archive_hint_open,
        QuillApp::build_archive_hint_dialog,
    ),
}
