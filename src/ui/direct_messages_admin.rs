//! A channel's direct messages as its admins see them: the header (the
//! channel with "N chats", or the person whose chat is open with "N
//! messages"), the open chat's menu (ban, the message fee), and the
//! Delete chat / ban confirmations (tdesktop `TopBarWidget` for a
//! monoforum, `Filler::fillMonoforumPeerActions`, `DeleteSublistBox`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::direct_messages_state::{DirectConfirm, DirectConfirmKind};
use super::direct_messages_tabs::{direct_ban_target, sender_userpic};
use super::shell::{DialogKind, QuillShell};
use gpui_kit::assets::IconName;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn};
use quill::paid_messages::stars_text;
use quill::state::InfoPanelTarget;
use quill::telegram::envelope::MessageSender;
use std::cell::RefCell;
use std::rc::Rc;

pub(super) struct AdminHeader {
    /// The open chat's person; `None` keeps the channel's name.
    pub title: Option<String>,
    pub status: String,
    /// The info panel the title opens: the person, when a chat is open.
    pub target: Option<InfoPanelTarget>,
}

impl QuillApp {
    /// tdesktop `TopBarWidget::paintTopBar` for a monoforum admin: "N
    /// chats" / "No chats" in All, the person and "N messages" in a chat.
    pub(super) fn direct_admin_header(&self) -> Option<AdminHeader> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        if !session.is_direct_messages_admin(chat_id) {
            return None;
        }
        let Some(topic) = session.open_direct_topic_info(chat_id) else {
            let count = session.direct_topic_count(chat_id);
            return Some(AdminHeader {
                title: None,
                status: if count > 0 {
                    trn(keys::FILTERS_CHATS_COUNT, count)
                } else {
                    tr(keys::FILTERS_NO_CHATS).to_string()
                },
                target: None,
            });
        };
        let status = match session.direct_topic_message_count(chat_id) {
            Some(count) => trn(keys::FORUM_MESSAGES, count),
            None => tr(keys::CONTACTS_LOADING).to_string(),
        };
        let target = match topic.sender {
            MessageSender::User { user_id } => Some(InfoPanelTarget::User(user_id)),
            MessageSender::Chat { chat_id } => session.chat_info_target(ChatId(chat_id)),
        };
        Some(AdminHeader {
            title: Some(session.direct_topic_name(topic)),
            status,
            target,
        })
    }

    /// The header's menu: an open person's chat has only its own actions,
    /// and only while messages cost Stars (tdesktop `hasMenu` for
    /// `Section::SavedSublist`); everywhere else the chat menu.
    pub(super) fn header_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.session().and_then(|s| {
            let chat_id = s.open_chat?;
            Some((chat_id, s.open_direct_topic(chat_id)?))
        });
        match open {
            Some((chat_id, topic_id)) => self.direct_topic_menu(chat_id, topic_id, cx),
            None => Some(self.chat_navigation_menu(cx)),
        }
    }

    fn direct_topic_menu(
        &self,
        chat_id: ChatId,
        topic_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let price = session.direct_messages_price(chat_id);
        if price <= 0 {
            return None;
        }
        let topic = session.direct_topic(chat_id, topic_id)?;
        let user_id = topic.user_id()?;
        let free = topic.can_send_unpaid_messages;
        let short = session.direct_topic_short_name(topic);
        let ban = direct_ban_target(session, chat_id, topic);
        // `None` until the member's status has answered: the item says it
        // is checking instead of offering Ban for someone already banned.
        let ban_status =
            ban.and_then(|(channel, user)| session.direct_member_banned(channel, user));
        let banned = ban_status.unwrap_or(false);
        let info = if free {
            tr_with(keys::CONTEXT_FEE_FREE, &[("name", &short)])
        } else {
            tr_with(
                keys::CONTEXT_FEE_NOW,
                &[("name", &short), ("amount", &stars_text(price))],
            )
        };
        let owner = cx.entity().downgrade();
        Some(
            Button::new("direct-topic-menu")
                .icon(IconName::EllipsisVertical)
                .ghost()
                .accessibility_label(tr(keys::Q_CHAT_CHAT_ACTIONS))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some((channel, user)) = ban
                        && let Some(live) = this.live.as_mut()
                    {
                        let _ = live.driver.fetch_direct_member(channel, user);
                        cx.notify();
                    }
                }))
                .dropdown_menu(move |mut menu, _, _| {
                    if let Some((channel, user)) = ban {
                        let owner = owner.clone();
                        menu = menu.item(
                            PopupMenuItem::new(match ban_status {
                                None => tr(keys::Q_OPS_CHECKING),
                                Some(true) => tr(keys::CONTEXT_MONOFORUM_UNBAN),
                                Some(false) => tr(keys::CONTEXT_MONOFORUM_BAN),
                            })
                            .disabled(ban_status.is_none())
                            .icon(if banned {
                                IconName::UserCheck
                            } else {
                                IconName::UserX
                            })
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.direct_ban_ui(chat_id, topic_id, channel, user, !banned);
                                    cx.notify();
                                });
                            }),
                        );
                    }
                    let owner = owner.clone();
                    menu.item(
                        PopupMenuItem::new(if free {
                            tr(keys::CONTEXT_CHARGE_FEE)
                        } else {
                            tr(keys::CONTEXT_REMOVE_FEE)
                        })
                        .icon(IconName::CircleDollarSign)
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                this.toggle_direct_fee_ui(chat_id, topic_id, user_id, !free, cx);
                            });
                        }),
                    )
                    .separator()
                    .item(PopupMenuItem::label(info.clone()))
                })
                .into_any_element(),
        )
    }

    /// Ban asks first (tdesktop `lng_profile_sure_kick_channel`); unban
    /// runs at once.
    fn direct_ban_ui(
        &mut self,
        chat_id: ChatId,
        topic_id: i64,
        channel: ChatId,
        user_id: i64,
        ban: bool,
    ) {
        if !ban {
            self.run_direct_live(|driver| {
                driver
                    .set_direct_member_banned(channel, user_id, false)
                    .map(|_| ())
            });
            return;
        }
        let name = self
            .session()
            .and_then(|s| {
                s.direct_topic(chat_id, topic_id)
                    .map(|t| s.direct_topic_name(t))
            })
            .unwrap_or_default();
        self.direct_messages.confirm = Some(DirectConfirm {
            chat_id,
            topic_id,
            name,
            kind: DirectConfirmKind::Ban { channel, user_id },
        });
        self.open_box(DialogKind::DirectConfirm);
    }

    /// "Remove Fee" opens the box with the refund choice; "Charge Fee"
    /// asks the person to pay again at once (tdesktop
    /// `PeerMenuConfirmToggleFee`).
    fn toggle_direct_fee_ui(
        &mut self,
        chat_id: ChatId,
        topic_id: i64,
        user_id: i64,
        remove: bool,
        cx: &mut Context<Self>,
    ) {
        if remove {
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.fetch_direct_topic_revenue(chat_id, topic_id);
            }
            self.open_remove_fee_for(user_id, Some((chat_id, topic_id)), cx);
            return;
        }
        self.run_direct_live(|driver| {
            driver
                .set_direct_topic_free(chat_id, topic_id, false, false)
                .map(|_| ())
        });
        cx.notify();
    }

    fn close_direct_confirm(&mut self, cx: &mut Context<Self>) {
        self.direct_messages.confirm = None;
        cx.notify();
    }

    fn accept_direct_confirm(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.direct_messages.confirm.take() else {
            return;
        };
        match confirm.kind {
            DirectConfirmKind::Delete => self.run_direct_live(|driver| {
                driver
                    .delete_direct_topic(confirm.chat_id, confirm.topic_id)
                    .map(|_| ())
            }),
            DirectConfirmKind::Ban { channel, user_id } => self.run_direct_live(|driver| {
                driver
                    .set_direct_member_banned(channel, user_id, true)
                    .map(|_| ())
            }),
        }
        cx.notify();
    }

    pub(super) fn build_direct_confirm_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::DirectConfirm, |this, _, cx| {
                this.close_direct_confirm(cx);
            });
        app.update(cx, |this, cx| {
            let Some(confirm) = this.direct_messages.confirm.clone() else {
                return dialog.overlay(true).on_close(on_close);
            };
            let (title, text, button) = match confirm.kind {
                DirectConfirmKind::Delete => (
                    Some(tr(keys::PROFILE_DELETE_CONVERSATION)),
                    tr_with(keys::SURE_DELETE_HISTORY, &[("contact", &confirm.name)]),
                    tr(keys::BOX_DELETE),
                ),
                DirectConfirmKind::Ban { .. } => (
                    None,
                    tr_with(keys::PROFILE_SURE_KICK_CHANNEL, &[("user", &confirm.name)]),
                    tr(keys::BOX_REMOVE),
                ),
            };
            let userpic = this.session().and_then(|s| {
                let topic = s.direct_topic(confirm.chat_id, confirm.topic_id)?;
                Some(sender_userpic(s, topic.sender, 44.))
            });
            let mut body = div().id("direct-confirm-body").flex().flex_col().gap_4();
            if let Some(title) = title {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .children(userpic)
                        .child(div().text_lg().font_semibold().child(title)),
                );
            }
            let body = body
                .child(div().text_sm().whitespace_normal().child(text))
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("direct-confirm-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_direct_confirm(cx);
                            this.close_kit_dialog_if_done(DialogKind::DirectConfirm, window, cx);
                        })),
                )
                .child(
                    Button::new("direct-confirm-accept")
                        .label(button)
                        .danger()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.accept_direct_confirm(cx);
                            this.close_kit_dialog_if_done(DialogKind::DirectConfirm, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    DirectConfirm => DialogSpec::new(
        3856,
        |app| app.direct_messages.confirm.is_some(),
        QuillApp::build_direct_confirm_dialog,
    )
    .close(|app, _, cx| app.close_direct_confirm(cx)),
}
