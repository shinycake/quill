//! The attach menu's Contact and Location items: an inline panel above the
//! composer that sends an `inputMessageContact` / `inputMessageLocation`
//! (tdesktop `Ui::attach` menu: "Contact" opens the contact chooser,
//! "Location" the location picker). The dice has no menu item: like
//! tdesktop, a message that is just a dice emoji rolls a die
//! (`quill::telegram::requests::dice_emoji`).

use super::app::QuillApp;
use super::chat_theme::{accent, bg_canvas, danger};
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::telegram::SendReply;
use quill::telegram::requests::{ContactShare, parse_coordinates};

/// Most contacts listed in the chooser at once; the search narrows them.
const CONTACT_LIST_MAX: usize = 8;

impl QuillApp {
    /// The chat a share can go to: the open chat when the user may post.
    fn share_target(&self) -> Option<ChatId> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        session.chats.get(&chat_id.0)?.can_post().then_some(chat_id)
    }

    pub(super) fn open_share_contact_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.share_target().is_none() {
            self.status_note = "you can't send contacts here".into();
            cx.notify();
            return;
        }
        let dialog = ShareContentDialog::contact(window, cx);
        if let ShareContentKind::Contact { query } = &dialog.kind {
            query.update(cx, |input, cx| input.focus(window, cx));
        }
        self.share_content_dialog = Some(dialog);
        cx.notify();
    }

    pub(super) fn open_share_location_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.share_target().is_none() {
            self.status_note = "you can't send locations here".into();
            cx.notify();
            return;
        }
        let dialog = ShareContentDialog::location(window, cx);
        if let ShareContentKind::Location { latitude, .. } = &dialog.kind {
            latitude.update(cx, |input, cx| input.focus(window, cx));
        }
        self.share_content_dialog = Some(dialog);
        cx.notify();
    }

    pub(super) fn close_share_content_dialog(&mut self, cx: &mut Context<Self>) {
        self.share_content_dialog = None;
        cx.notify();
    }

    /// The composer's reply (with its quote), carried by the share like by
    /// any other send.
    fn share_reply(&self, chat_id: ChatId) -> Option<SendReply> {
        self.pending_reply
            .as_ref()
            .and_then(|reply| reply.send_target(chat_id))
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
        let reply_to = self.share_reply(chat_id);
        let options = self.composer_send_options();
        let Some(live) = self.live.as_mut() else {
            self.share_content_dialog = None;
            self.status_note = "sharing needs a live connection (demo)".into();
            cx.notify();
            return;
        };
        match live
            .driver
            .share_contact_to_chat(chat_id, &contact, reply_to, &options)
        {
            Ok(_) => {
                self.share_content_dialog = None;
                self.pending_reply = None;
                self.status_note = "sending contact…".into();
            }
            Err(_) => self.status_note = "could not send the contact".into(),
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
        let reply_to = self.share_reply(chat_id);
        let options = self.composer_send_options();
        let Some(live) = self.live.as_mut() else {
            self.status_note = "sending dice needs a live connection (demo)".into();
            cx.notify();
            return;
        };
        match live
            .driver
            .share_dice_to_chat(chat_id, emoji, reply_to, &options)
        {
            Ok(_) => {
                self.pending_reply = None;
                self.status_note = "rolling…".into();
            }
            Err(_) => self.status_note = "could not send the dice".into(),
        }
        cx.notify();
    }

    /// Validate the typed coordinates and send a static location.
    pub(super) fn submit_share_location(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self.share_target() else {
            return;
        };
        let typed = match self.share_content_dialog.as_ref().map(|d| &d.kind) {
            Some(ShareContentKind::Location {
                latitude,
                longitude,
            }) => (
                latitude.read(cx).value().to_string(),
                longitude.read(cx).value().to_string(),
            ),
            _ => return,
        };
        let (latitude, longitude) = match parse_coordinates(&typed.0, &typed.1) {
            Ok(pair) => pair,
            Err(reason) => {
                if let Some(dialog) = self.share_content_dialog.as_mut() {
                    dialog.error = Some(reason);
                }
                cx.notify();
                return;
            }
        };
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        let reply_to = self.share_reply(chat_id);
        let options = self.composer_send_options();
        let Some(live) = self.live.as_mut() else {
            self.share_content_dialog = None;
            self.status_note = "sharing needs a live connection (demo)".into();
            cx.notify();
            return;
        };
        match live
            .driver
            .share_location_to_chat(chat_id, latitude, longitude, reply_to, &options)
        {
            Ok(_) => {
                self.share_content_dialog = None;
                self.pending_reply = None;
                self.status_note = "sending location…".into();
            }
            Err(_) => self.status_note = "could not send the location".into(),
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

    pub(super) fn share_content_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.share_content_dialog.as_ref()?;
        let mut body = div()
            .id("share-content-panel")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas());
        match &dialog.kind {
            ShareContentKind::Contact { query } => {
                let typed = query.read(cx).value().to_string();
                let rows = self.share_contact_rows(&typed);
                let mut list = div().flex().flex_col().gap_0p5();
                for (user_id, name, phone) in rows.iter().cloned() {
                    let photo = self
                        .session()
                        .and_then(|s| s.user_photo_path(user_id))
                        .and_then(|path| {
                            quill::local_path::sandboxed_display_path(
                                path,
                                &self.media_display_roots(),
                            )
                        });
                    list = list.child(
                        div()
                            .id(("share-contact", user_id as u64))
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .role(gpui_kit::Role::Button)
                            .aria_label(format!("Send {name}"))
                            .tab_index(0)
                            .cursor_pointer()
                            .pressable(cx.theme())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.submit_share_contact(user_id, cx);
                            }))
                            .child(super::message_text::kit_avatar_element(
                                &name,
                                photo.as_deref(),
                                px(28.),
                            ))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .min_w_0()
                                    .child(div().text_sm().font_medium().truncate().child(name))
                                    .child(div().text_xs().text_color(text_muted()).child(phone)),
                            ),
                    );
                }
                if rows.is_empty() {
                    list = list.child(
                        div()
                            .text_xs()
                            .text_color(text_muted())
                            .child("No contacts with a phone number match."),
                    );
                }
                body = body
                    .child(div().text_sm().font_semibold().child("Send Contact"))
                    .child(
                        Textarea::new(query)
                            .aria_label("Search contacts")
                            .h(px(36.)),
                    )
                    .child(list);
            }
            ShareContentKind::Location {
                latitude,
                longitude,
            } => {
                body = body
                    .child(div().text_sm().font_semibold().child("Send Location"))
                    .child(Textarea::new(latitude).aria_label("Latitude").h(px(36.)))
                    .child(Textarea::new(longitude).aria_label("Longitude").h(px(36.)))
                    .child(
                        div()
                            .text_xs()
                            .text_color(text_muted())
                            .child("The location is sent once; it does not follow you."),
                    );
            }
        }
        if let Some(error) = dialog.error {
            body = body.child(
                div()
                    .id("share-content-error")
                    .text_xs()
                    .text_color(danger())
                    .child(error),
            );
        }
        let is_location = matches!(dialog.kind, ShareContentKind::Location { .. });
        let mut actions = div().flex().gap_2();
        if is_location {
            actions = actions.child(
                Button::new("share-content-send")
                    .label("Send")
                    .on_click(cx.listener(|this, _, _, cx| this.submit_share_location(cx))),
            );
        }
        Some(
            body.child(
                actions.child(
                    Button::new("share-content-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_share_content_dialog(cx);
                        })),
                ),
            )
            .into_any_element(),
        )
    }
}
