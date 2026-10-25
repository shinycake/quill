//! Disable Sharing for a private chat (tdesktop `Filler::addToggleNoForwards`
//! and `ShowDisableSharingBox` in `window_peer_menu.cpp`): the first three
//! times a box explains No Forwarding and No Saving, after that the entry
//! acts at once. Without Premium the box's button, and the entry, open the
//! Premium preview instead.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::folder_share::finish_dialog;
use crate::ui::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};

/// tdesktop `kDisableSharingBoxMaxShowsCount`.
const BOX_MAX_SHOWS: u32 = 3;

impl QuillApp {
    /// The menu's Disable Sharing / Enable Sharing entry.
    pub(in crate::ui) fn toggle_private_sharing(
        &mut self,
        chat_id: ChatId,
        disabled_now: bool,
        cx: &mut Context<Self>,
    ) {
        if disabled_now {
            self.enable_private_sharing(chat_id, cx);
            return;
        }
        if self.chat_prefs.disable_sharing_box_shown < BOX_MAX_SHOWS {
            self.set_chat_prefs(cx, |prefs| prefs.disable_sharing_box_shown += 1);
            self.dialogs.disable_sharing = Some(chat_id);
            cx.notify();
            return;
        }
        self.disable_sharing_or_upsell(chat_id, cx);
    }

    fn disable_sharing_or_upsell(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if self.session().is_some_and(|s| s.my_is_premium()) {
            self.disable_private_sharing(chat_id, cx);
        } else {
            self.dismiss_profile_modal();
            self.open_premium_at(
                quill::premium_hub::PremiumFeature::ProtectPrivateChatContent,
                cx,
            );
        }
    }

    fn disable_private_sharing(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_PEOPLE_DEMO_MODE_THIS_NEEDS_A)));
            cx.notify();
            return;
        };
        if let Err(err) = live.driver.set_chat_protected_content(chat_id, true) {
            self.connection.set_note(StatusNote::error(user_error_for(
                keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
                &err,
            )));
        }
        cx.notify();
    }

    pub(in crate::ui) fn build_disable_sharing_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::DisableSharing, |this, _, cx| {
                this.dialogs.disable_sharing = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let premium = this.session().is_some_and(|s| s.my_is_premium());
            let muted = cx.theme().muted_foreground;
            let features = [
                (
                    IconName::Forward,
                    tr(keys::DISABLE_SHARING_NO_FORWARDING),
                    tr(keys::DISABLE_SHARING_NO_FORWARDING_ABOUT),
                ),
                (
                    IconName::Download,
                    tr(keys::DISABLE_SHARING_NO_SAVING),
                    tr(keys::DISABLE_SHARING_NO_SAVING_ABOUT),
                ),
            ];
            let mut body = div().id("disable-sharing").flex().flex_col().gap_4().child(
                div().flex().justify_center().child(
                    Icon::new(IconName::Hand)
                        .size(px(48.))
                        .text_color(cx.theme().primary),
                ),
            );
            for (icon, title, about) in features {
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
                                .child(div().text_sm().font_semibold().child(title))
                                .child(div().text_sm().text_color(muted).child(about)),
                        ),
                );
            }
            let button = Button::new("disable-sharing-confirm").primary().w_full();
            let button = if premium {
                button.label(tr(keys::DISABLE_SHARING_BUTTON))
            } else {
                button
                    .icon(IconName::Lock)
                    .label(tr(keys::DISABLE_SHARING_UNLOCK))
            };
            let footer =
                div()
                    .flex()
                    .w_full()
                    .child(button.on_click(cx.listener(|this, _, window, cx| {
                        let Some(chat_id) = this.dialogs.disable_sharing else {
                            return;
                        };
                        this.dialogs.disable_sharing = None;
                        this.disable_sharing_or_upsell(chat_id, cx);
                        this.close_kit_dialog_if_done(DialogKind::DisableSharing, window, cx);
                    })));
            finish_dialog(
                dialog,
                tr(keys::DISABLE_SHARING_TITLE).to_string(),
                body.into_any_element(),
                Some(footer.into_any_element()),
                on_close,
            )
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// The profile menu's Disable Sharing box.
    DisableSharing => DialogSpec::new(
        6150,
        |app| app.dialogs.disable_sharing.is_some(),
        QuillApp::build_disable_sharing_dialog,
    ),
}
