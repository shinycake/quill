//! "Send message as…": the composer's identity button and its menu
//! (tdesktop `SendAsButton` opening `Ui::PopupMenu` with one row per
//! `chatMessageSender`). The menu is a kit popover anchored to the button;
//! it used to be a panel pushed in above the composer.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::pressable::PressableDiv;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::state::Session;
use quill::telegram::envelope::MessageSender;
use std::path::PathBuf;

/// The menu's width; long names truncate.
const MENU_WIDTH: f32 = 280.;

fn sender_photo(session: &Session, sender: MessageSender) -> Option<PathBuf> {
    match sender {
        MessageSender::Chat { chat_id } => session.chat_photo_path(ChatId(chat_id)),
        MessageSender::User { user_id } => session
            .user_photo_path(user_id)
            .or_else(|| session.chat_photo_path(ChatId(user_id))),
    }
    .map(PathBuf::from)
}

impl QuillApp {
    /// Composer button showing the current "send as" identity; only for
    /// chats where TDLib reports a selectable sender. A click opens the
    /// menu above it.
    pub(super) fn send_as_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        let sender = session.selected_message_sender(chat_id)?;
        let name = session.message_sender_title(sender);
        let photo = sender_photo(session, sender);
        let view = cx.entity().downgrade();
        let menu_view = view.clone();
        Some(
            Popover::new("composer-send-as-menu")
                .anchor(Anchor::BottomLeft)
                .open(self.chat_ui.send_as_open)
                .on_open_change(move |open, _, cx| {
                    let open = *open;
                    let _ = view.update(cx, |this, cx| {
                        if this.chat_ui.send_as_open != open {
                            this.toggle_send_as(cx);
                        }
                    });
                })
                .trigger(
                    Button::new("composer-send-as")
                        .ghost()
                        .rounded(ButtonRounded::Large)
                        .p_0()
                        .accessibility_label(tr_with(
                            keys::Q_OPS_SEND_AS,
                            &[("name", &name.to_string())],
                        ))
                        .child(chat_avatar(&name, photo.as_deref(), 28.)),
                )
                .content(move |_, _, cx| {
                    menu_view
                        .update(cx, |this, cx| this.send_as_menu(cx))
                        .unwrap_or_else(|_| div().into_any_element())
                })
                .into_any_element(),
        )
    }

    pub(super) fn toggle_send_as(&mut self, cx: &mut Context<Self>) {
        self.chat_ui.send_as_open = !self.chat_ui.send_as_open;
        if self.chat_ui.send_as_open {
            let chat = self.session().and_then(|s| s.open_chat);
            if let (Some(chat), Some(live)) = (chat, self.live.as_mut()) {
                let _ = live.driver.get_chat_available_message_senders(chat);
            }
        }
        cx.notify();
    }

    fn choose_message_sender(
        &mut self,
        chat_id: ChatId,
        sender: MessageSender,
        locked: bool,
        cx: &mut Context<Self>,
    ) {
        if locked {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_OPS_SUBSCRIBE_TO_TELEGRAM_PREMIUM_TO,
            )));
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            self.screen.connection.set_note(
                match live.driver.set_chat_message_sender(chat_id, sender) {
                    Ok(()) => StatusNote::silent(tr(keys::Q_OPS_CHANGING_SENDER)),
                    Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_CHANGE_THE_SENDER)),
                },
            );
        } else {
            self.apply_demo_message_sender(chat_id, sender);
        }
        self.chat_ui.send_as_open = false;
        cx.notify();
    }

    /// The menu: a header and one row per identity (`chatMessageSender`),
    /// the current one ticked, premium-only ones dimmed with a lock.
    fn send_as_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let chat_id = session.and_then(|s| s.open_chat);
        let current = chat_id.and_then(|id| session.and_then(|s| s.selected_message_sender(id)));
        let senders = match (session, chat_id) {
            (Some(session), Some(chat_id)) => session.available_message_senders(chat_id),
            _ => &[],
        };
        let muted = cx.theme().muted_foreground;
        let mut list = div()
            .id("send-as-list")
            .flex()
            .flex_col()
            .max_h(px(320.))
            .overflow_y_scroll();
        if senders.is_empty() {
            list = list.child(
                div()
                    .flex()
                    .justify_center()
                    .py_4()
                    .child(Spinner::new().color(muted)),
            );
        }
        for (index, entry) in senders.iter().enumerate() {
            let (Some(session), Some(chat_id)) = (session, chat_id) else {
                break;
            };
            let sender = entry.sender;
            let title = session.message_sender_title(sender);
            let status = session.message_sender_status(chat_id, entry);
            let photo = sender_photo(session, sender);
            let locked = entry.needs_premium;
            let selected = current == Some(sender);
            list = list.child(
                div()
                    .id(("send-as-row", index))
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(px(48.))
                    .px_2()
                    .rounded_md()
                    .role(gpui_kit::Role::MenuItemRadio)
                    .aria_label(tr_with(
                        keys::Q_OPS_SEND_AS_2,
                        &[("title", &title.to_string()), ("status", status)],
                    ))
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choose_message_sender(chat_id, sender, locked, cx);
                    }))
                    .child(div().when(locked, |d| d.opacity(0.6)).child(chat_avatar(
                        &title,
                        photo.as_deref(),
                        36.,
                    )))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .when(locked, |d| d.opacity(0.6))
                            .child(
                                div()
                                    .text_sm()
                                    .font_medium()
                                    .child(super::bidi_line::name_line(title)),
                            )
                            .child(div().text_xs().text_color(muted).child(status)),
                    )
                    .when(locked, |row| {
                        row.child(
                            Icon::new(gpui_kit::assets::IconName::Lock)
                                .small()
                                .text_color(muted),
                        )
                    })
                    .when(selected, |row| {
                        row.child(
                            Icon::new(gpui_kit::assets::IconName::Check)
                                .small()
                                .text_color(cx.theme().primary),
                        )
                    }),
            );
        }
        div()
            .id("send-as-panel")
            .w(px(MENU_WIDTH))
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .px_2()
                    .pt_1()
                    .text_xs()
                    .font_medium()
                    .text_color(muted)
                    .child(tr(keys::Q_OPS_SEND_MESSAGE_AS)),
            )
            .child(list)
            .into_any_element()
    }
}

super::screenshot_demo::register_demos![
    // The send-as menu open over the composer: the current identity
    // ticked, a channel to switch to and a premium-only one locked.
    super::screenshot_demo::DemoSpec::chat_list("ready-send-as").setup(QuillApp::demo_send_as),
];

impl QuillApp {
    fn demo_send_as(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        use quill::telegram::envelope::AvailableMessageSender;
        let chat = ChatId(12);
        if let Some(session) = self.demo_session.as_mut() {
            session.open_chat(chat);
            let me = MessageSender::User { user_id: 11 };
            session.chats_state.chat_message_sender.insert(chat.0, me);
            session.chats_state.send_as_options.insert(
                chat.0,
                vec![
                    AvailableMessageSender {
                        sender: me,
                        needs_premium: false,
                    },
                    AvailableMessageSender {
                        sender: MessageSender::Chat { chat_id: 13 },
                        needs_premium: false,
                    },
                    AvailableMessageSender {
                        sender: MessageSender::Chat { chat_id: 12 },
                        needs_premium: true,
                    },
                ],
            );
        }
        self.chat_ui.send_as_open = true;
        cx.notify();
    }
}
