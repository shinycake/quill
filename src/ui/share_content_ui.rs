//! The attach menu's Contact item: a contact chooser box over the window
//! that sends an `inputMessageContact` (tdesktop `Ui::attach` menu: "Contact"
//! opens the contact chooser; "Location" opens `ui/location_picker`). The
//! dice has no menu item: like
//! tdesktop, a message that is just a dice emoji rolls a die
//! (`quill::telegram::requests::dice_emoji`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::dialog_focus::focus_after_mount;
use super::pressable::PressableDiv;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::telegram::requests::ContactShare;

/// tdesktop `boxWidth`.
const BOX_WIDTH: f32 = 380.;

/// Most contacts listed in the chooser at once; the search narrows them.
const CONTACT_LIST_MAX: usize = 8;

impl QuillApp {
    /// The chat a share can go to: the open chat when the user may post.
    pub(super) fn share_target(&self) -> Option<ChatId> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        session.chats.get(&chat_id.0)?.can_post().then_some(chat_id)
    }

    pub(super) fn open_share_contact_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.share_target().is_none() {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_OPS_YOU_CANT_SEND_CONTACTS_HERE,
            )));
            cx.notify();
            return;
        }
        self.share.content_dialog = Some(ShareContentDialog::contact(window, cx));
        cx.notify();
    }

    pub(super) fn close_share_content_dialog(&mut self, cx: &mut Context<Self>) {
        self.share.content_dialog = None;
        cx.notify();
    }

    /// Send the picked address-book entry as a contact card.
    pub(super) fn submit_share_contact(&mut self, user_id: i64, cx: &mut Context<Self>) {
        let Some(chat_id) = self.share_target() else {
            return;
        };
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        let Some(contact) = self
            .session()
            .and_then(|s| s.user(user_id))
            .map(|user| ContactShare {
                phone_number: user.phone_number.clone(),
                first_name: user.first_name.clone(),
                last_name: user.last_name.clone(),
                user_id,
            })
        else {
            return;
        };
        let reply_to = self.send_reply_for(chat_id, cx);
        let options = self.composer_send_options();
        let Some(live) = self.live.as_mut() else {
            self.share.content_dialog = None;
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_OPS_SHARING_NEEDS_A_LIVE_CONNECTION,
            )));
            cx.notify();
            return;
        };
        match live
            .driver
            .share_contact_to_chat(chat_id, &contact, reply_to, &options)
        {
            Ok(_) => {
                self.share.content_dialog = None;
                self.composer_ui.set_reply(None);
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_OPS_SENDING_CONTACT)));
            }
            Err(_) => self.connection.set_note(StatusNote::error(tr(
                keys::Q_OPS_COULD_NOT_SEND_THE_CONTACT,
            ))),
        }
        cx.notify();
    }

    /// Roll a die from the attach menu's Dice list.
    pub(super) fn roll_dice(&mut self, emoji: &str, cx: &mut Context<Self>) {
        let Some(chat_id) = self.share_target() else {
            return;
        };
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        let reply_to = self.send_reply_for(chat_id, cx);
        let options = self.composer_send_options();
        let Some(live) = self.live.as_mut() else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_OPS_SENDING_DICE_NEEDS_A_LIVE)));
            cx.notify();
            return;
        };
        match live
            .driver
            .share_dice_to_chat(chat_id, emoji, reply_to, &options)
        {
            Ok(_) => {
                self.composer_ui.set_reply(None);
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_OPS_ROLLING)));
            }
            Err(_) => self
                .connection
                .set_note(StatusNote::error(tr(keys::Q_OPS_COULD_NOT_SEND_THE_DICE))),
        }
        cx.notify();
    }

    /// Address-book entries with a phone number matching the search text.
    fn share_contact_rows(&self, query: &str) -> Vec<(i64, String, String)> {
        let query = query.trim().to_lowercase();
        let Some(session) = self.session() else {
            return Vec::new();
        };
        session
            .contact_rows()
            .into_iter()
            .filter_map(|row| {
                let phone = session.user(row.user_id)?.phone_number.clone();
                (!phone.is_empty()).then_some((row.user_id, row.name, phone))
            })
            .filter(|(_, name, phone)| {
                query.is_empty()
                    || name.to_lowercase().contains(&query)
                    || phone.contains(query.trim_start_matches('+'))
            })
            .take(CONTACT_LIST_MAX)
            .collect()
    }

    /// The contact chooser's body: the search field and up to
    /// `CONTACT_LIST_MAX` contacts, one click sends.
    fn share_content_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.share.content_dialog.as_ref()?;
        let query = &dialog.query;
        let typed = query.read(cx).value().to_string();
        let rows = self.share_contact_rows(&typed);
        let mut list = div().id("share-contact-list").flex().flex_col();
        for (user_id, name, phone) in rows.iter().cloned() {
            let photo = self
                .session()
                .and_then(|s| s.user_photo_path(user_id))
                .and_then(|path| {
                    quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
                });
            list = list.child(
                div()
                    .id(("share-contact", user_id as u64))
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(px(52.))
                    .px_2()
                    .rounded_md()
                    .role(gpui_kit::Role::Button)
                    .aria_label(tr_with(keys::Q_OPS_SEND, &[("name", &name.to_string())]))
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.submit_share_contact(user_id, cx);
                    }))
                    .child(super::message_text::kit_avatar_element(
                        &name,
                        photo.as_deref(),
                        px(40.),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .text_sm()
                                    .font_medium()
                                    .child(super::bidi_line::name_line(name)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(text_muted())
                                    .child(super::group_panels::format_phone(&phone)),
                            ),
                    ),
            );
        }
        let list = if rows.is_empty() {
            super::empty_state::EmptyState::new("share-contact-empty", IconName::Contact)
                .hint(tr(keys::Q_OPS_NO_CONTACTS_WITH_A_PHONE))
                .into_any_element()
        } else {
            list.into_any_element()
        };
        Some(
            div()
                .id("share-content-panel")
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    Textarea::new(query)
                        .aria_label(tr(keys::Q_OPS_SEARCH_CONTACTS))
                        .h(px(36.)),
                )
                .child(list)
                .into_any_element(),
        )
    }

    fn build_share_contact_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ShareContact, |this, _, cx| {
                this.close_share_content_dialog(cx);
            });
        let footer = app.update(cx, |_, cx| {
            div().flex().justify_end().child(
                Button::new("share-content-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_share_content_dialog(cx);
                    })),
            )
        });
        let body_app = app.clone();
        dialog
            .overlay(true)
            .w(px(BOX_WIDTH))
            .title(crate::ui::shell::dialog_title(tr(keys::Q_OPS_SEND_CONTACT)))
            .content(crate::ui::shell::scrollable_dialog_content(
                move |content, window, cx| {
                    let body = body_app.update(cx, |this, cx| {
                        let focus = this.share.content_dialog.as_mut().and_then(|dialog| {
                            std::mem::take(&mut dialog.focus_on_mount).then(|| dialog.query.clone())
                        });
                        focus_after_mount(focus, window, cx);
                        this.share_content_panel(cx)
                    });
                    content.children(body)
                },
            ))
            .footer(footer)
            .on_close(on_close)
    }
}

crate::ui::shell::register_dialogs! {
    ShareContact => DialogSpec::new(
        2947,
        |app| app.share.content_dialog.is_some(),
        QuillApp::build_share_contact_dialog,
    )
    .close(|app, _, cx| app.close_share_content_dialog(cx)),
}
