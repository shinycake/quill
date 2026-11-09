//! "Mute for..." over a community (tdesktop `MuteBox` from the row
//! menu's Mute submenu): the days, hours and minutes fields the chat
//! mute panel uses, applied with `setCommunityNotificationSettings`.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;

impl QuillApp {
    pub(super) fn open_community_mute_for(
        &mut self,
        community_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reset_mute_time_inputs(window, cx);
        self.show_community_box(CommunityBox::MuteFor { community_id }, cx);
    }

    /// The duration the fields show, in seconds; 0 unmutes.
    pub(super) fn community_mute_for_value(&self, cx: &App) -> i32 {
        self.mute_time_value(cx)
            .map_or(0, |custom| custom.mute_for())
    }

    fn submit_community_mute_for(&mut self, cx: &mut Context<Self>) {
        let Some(CommunityBox::MuteFor { community_id }) = self.admin.community.flow.open else {
            return;
        };
        let mute_for = self.community_mute_for_value(cx);
        self.flush_and_close_community_box(cx);
        self.set_community_mute_for(community_id, mute_for, cx);
    }

    fn build_community_mute_for_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CommunityMuteFor, |this, _, cx| {
                this.flush_and_close_community_box(cx)
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true).w(px(420.));
            let Some(CommunityBox::MuteFor { community_id }) = this.admin.community.flow.open
            else {
                return dialog.on_close(on_close);
            };
            let mute_for = this.community_mute_for_value(cx);
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(this.community_name(community_id)),
                )
                .children(this.mute_time_fields(cx));
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("community-mute-for-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(
                            cx.listener(|this, _, _, cx| this.flush_and_close_community_box(cx)),
                        ),
                )
                .child(
                    Button::new("community-mute-for-ok")
                        .label(if mute_for == 0 {
                            tr(keys::MUTE_MENU_UNMUTE)
                        } else {
                            tr(keys::MUTE_MENU_MUTE)
                        })
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| this.submit_community_mute_for(cx))),
                );
            dialog
                .title(crate::ui::shell::dialog_title(tr(keys::MUTE_BOX_TITLE)))
                .content(super::boxes::once(body.into_any_element()))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// "Mute for..." over a community.
    CommunityMuteFor => DialogSpec::new(
        5417,
        |app| matches!(app.admin.community.flow.open, Some(CommunityBox::MuteFor { .. })),
        QuillApp::build_community_mute_for_dialog,
    )
    .close(|app, _, cx| app.flush_and_close_community_box(cx)),
}
