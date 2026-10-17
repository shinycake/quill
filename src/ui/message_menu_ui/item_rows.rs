//! Rows of the message menu that depend on what was right-clicked inside the
//! bubble or on who wrote it: "Reply to Task", the poll-option rows ("Reply
//! to Option", "Copy Option", "Delete Item"), "Copy Phone Number" on a shared
//! contact and the "Sent by" line (tdesktop `history_view_context_menu.cpp`:
//! `AddReplyToMessageAction`, `FillPollOptionPage`, the contact branch of
//! `FillContextMenu`, `MakeMessageAuthorAction`).
use super::*;
use quill::composer::reply_item::{item_text, option_menu};
use quill::composer::{ComposerReplyTo, ReplyItem};
use quill::state::PollOptionMenu;
use quill::telegram::envelope::MessageSender;

impl QuillApp {
    /// Remember a right-press on a checklist task or poll option; the
    /// message menu picks it up if it opens at this very point.
    pub(in crate::ui) fn note_right_clicked_item(
        &mut self,
        position: Point<Pixels>,
        item: ReplyItem,
    ) {
        self.message_ui.right_clicked_item = Some((position, item));
    }

    /// The task or option under the right-click that opened a menu at
    /// `position`; the whole message when there is none.
    pub(in crate::ui) fn take_right_clicked_item(&mut self, position: Point<Pixels>) -> ReplyItem {
        self.message_ui
            .right_clicked_item
            .take()
            .filter(|(at, _)| *at == position)
            .map(|(_, item)| item)
            .unwrap_or_default()
    }

    /// The menu is over a task of this checklist: Reply is "Reply to Task"
    /// and replaces the usual Reply row.
    pub(in crate::ui) fn menu_replies_to_task(&self, content: &MessageContent) -> bool {
        matches!(self.message_ui.menu_item, ReplyItem::Task(_))
            && item_text(content, &self.message_ui.menu_item).is_some()
    }

    /// A reply whose task or option is gone from the message (it was
    /// deleted) falls back to a plain reply, so the bar, the send and the
    /// draft all agree.
    pub(in crate::ui) fn drop_stale_reply_item(&mut self) {
        let Some(reply) = self.composer_ui.reply() else {
            return;
        };
        if reply.item.is_message() {
            return;
        }
        let stale = self
            .session()
            .and_then(|s| s.reply_target(reply.chat_id, reply.message_id))
            .is_some_and(|message| {
                item_text(
                    quill::telegram::envelope::effective_content(
                        &message.content,
                        message.ephemeral.as_ref(),
                    ),
                    &reply.item,
                )
                .is_none()
            });
        if stale {
            let mut plain = reply.clone();
            plain.item = ReplyItem::Message;
            self.composer_ui.set_reply(Some(plain));
        }
    }

    /// Per-frame upkeep: a closed menu forgets the task or option it was
    /// opened over (and what TDLib allowed for it), and a reply to an item
    /// that no longer exists becomes a plain reply.
    pub(in crate::ui) fn sync_item_menu(&mut self) {
        if self.message_ui.menu.is_none() && !self.message_ui.menu_item.is_message() {
            self.message_ui.menu_item = ReplyItem::Message;
            let session = match self.live.as_mut() {
                Some(live) => Some(&mut live.driver.session),
                None => self.demo_session.as_mut(),
            };
            if let Some(session) = session {
                session.messages.poll_option_menu = None;
            }
        }
        self.drop_stale_reply_item();
    }

    /// What TDLib allowed for the right-clicked option, once it answered.
    fn option_properties(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Option<quill::telegram::envelope::PollOptionProperties> {
        let ReplyItem::PollOption(option_id) = &self.message_ui.menu_item else {
            return None;
        };
        self.session()
            .and_then(|s| s.messages.poll_option_menu.as_ref())
            .filter(|menu: &&PollOptionMenu| {
                menu.chat_id == chat_id
                    && menu.message_id == message_id
                    && menu.option_id == *option_id
            })
            .and_then(|menu| menu.properties)
    }

    pub(in crate::ui) fn menu_item_rows(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        content: &MessageContent,
        can_reply: bool,
        cx: &mut Context<Self>,
    ) -> Vec<MenuRow> {
        let mut rows = Vec::new();
        let item = self.message_ui.menu_item.clone();
        let text = item_text(content, &item);
        match &item {
            ReplyItem::Task(_) if can_reply && text.is_some() => {
                rows.push(self.reply_to_item_row(
                    order::REPLY,
                    "menu-reply-task",
                    keys::CONTEXT_REPLY_TO_TASK,
                    chat_id,
                    message_id,
                    item.clone(),
                    cx,
                ));
            }
            _ => {}
        }
        // tdesktop: a shared contact offers its number.
        if item.is_message()
            && let MessageContent::Contact(contact) = content
            && !contact.phone_number.is_empty()
        {
            let phone = contact.phone_number.clone();
            rows.push(menu_row(
                order::COPY_TEXT,
                IconName::Copy,
                "menu-copy-phone",
                tr(keys::PROFILE_COPY_PHONE),
                false,
                cx,
                move |this, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(phone.clone()));
                    this.message_ui.menu = None;
                    cx.notify();
                },
            ));
        }
        rows
    }

    /// The option page (tdesktop `FillPollOptionPage`): Retract vote, Reply
    /// to Option, Copy Option, Copy Option Link, Delete Item and who added
    /// the option. It replaces the message menu while it shows; Back returns.
    pub(in crate::ui) fn menu_option_page_rows(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        message: &quill::state::HistoryMessage,
        can_reply: bool,
        cx: &mut Context<Self>,
    ) -> Vec<MenuRow> {
        let mut rows = Vec::new();
        let item = self.message_ui.menu_item.clone();
        let ReplyItem::PollOption(option_id) = &item else {
            return rows;
        };
        let MessageContent::Poll(poll) = &message.content else {
            return rows;
        };
        let Some(option) = poll.poll.options.iter().find(|o| o.id == *option_id) else {
            return rows;
        };
        let properties = self.option_properties(chat_id, message_id);
        let menu = option_menu(true, can_reply, properties);
        if quill::poll::can_retract_vote(&poll.poll) && message.id.0 > 0 && !message.pending {
            rows.push(menu_row(
                1,
                IconName::Undo2,
                "menu-retract-vote",
                tr(keys::Q_CHAT_RETRACT_VOTE),
                false,
                cx,
                move |this, _, cx| {
                    this.retract_poll_vote(chat_id, message_id, cx);
                    this.message_ui.menu = None;
                    cx.notify();
                },
            ));
        }
        if menu.reply {
            rows.push(self.reply_to_item_row(
                2,
                "menu-reply-option",
                keys::CONTEXT_REPLY_TO_POLL_OPTION,
                chat_id,
                message_id,
                item.clone(),
                cx,
            ));
        }
        let text = option.text.clone();
        rows.push(menu_row(
            3,
            IconName::Copy,
            "menu-copy-option",
            tr(keys::CONTEXT_COPY_POLL_OPTION),
            false,
            cx,
            move |this, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                this.message_ui.menu = None;
                cx.notify();
            },
        ));
        if properties.is_some_and(|p| p.can_get_link) {
            let link_item = item.clone();
            rows.push(menu_row(
                4,
                IconName::Link,
                "menu-copy-option-link",
                tr(keys::CONTEXT_COPY_POLL_OPTION_LINK),
                false,
                cx,
                move |this, _, cx| {
                    this.copy_item_link(chat_id, message_id, &link_item, cx);
                    this.message_ui.menu = None;
                    cx.notify();
                },
            ));
        }
        if menu.delete {
            let option_id = option_id.clone();
            rows.push(menu_row(
                5,
                IconName::Trash,
                "menu-delete-option",
                tr(keys::CONTEXT_DELETE_POLL_OPTION),
                true,
                cx,
                move |this, _, cx| {
                    this.delete_poll_option_item(chat_id, message_id, &option_id, cx);
                    this.message_ui.menu = None;
                    cx.notify();
                },
            ));
        }
        if let Some(author) = option.author
            && let Some(session) = self.session()
        {
            let name = match author {
                MessageSender::User { user_id } => session.user(user_id).map(|u| u.display_name()),
                MessageSender::Chat { chat_id } => {
                    session.chats.get(&chat_id).map(|c| c.title.clone())
                }
            };
            if let Some(name) = name {
                rows.push(menu_separator(6));
                rows.push(menu_row(
                    7,
                    IconName::User,
                    "menu-option-added-by",
                    tr_with(keys::POLLS_OPTION_ADDED_BY, &[("user", &name)]),
                    false,
                    cx,
                    move |this, window, cx| {
                        this.message_ui.menu = None;
                        if let MessageSender::User { user_id } = author {
                            this.open_user_panel(user_id, window, cx);
                        }
                    },
                ));
                if option.addition_date > 0 {
                    let now = quill::local_time::civil_local(quill::local_time::now_unix());
                    let date = quill::local_time::civil_local(i64::from(option.addition_date));
                    rows.push(info_row(
                        8,
                        "menu-option-added-date",
                        None,
                        quill::message_menu::read_date_label(&date, &now),
                    ));
                }
            }
        }
        rows
    }

    /// "Copy Option Link": `getMessageLink` for the item; the poll loop puts
    /// the answer on the clipboard like any message link.
    pub(in crate::ui) fn copy_item_link(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        item: &ReplyItem,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.get_item_link(chat_id, message_id, item);
        }
        cx.notify();
    }

    #[allow(clippy::too_many_arguments)]
    fn reply_to_item_row(
        &self,
        row_order: u8,
        id: &'static str,
        label: quill::lang::Key,
        chat_id: ChatId,
        message_id: MessageId,
        item: ReplyItem,
        cx: &mut Context<Self>,
    ) -> MenuRow {
        menu_row(
            row_order,
            IconName::Reply,
            id,
            tr(label),
            false,
            cx,
            move |this, window, cx| {
                this.begin_reply_to(
                    ComposerReplyTo::new(chat_id, message_id).to_item(item.clone()),
                    window,
                    cx,
                );
                this.message_ui.menu = None;
                cx.notify();
            },
        )
    }

    /// "Sent by {user}" under the date lines, for a post a chat sent on
    /// behalf of someone (`getMessageAuthor`; tdesktop `MakeMessageAuthorAction`).
    pub(in crate::ui) fn menu_author_row(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) -> Option<MenuRow> {
        let session = self.session()?;
        let audience = session
            .messages
            .message_audience
            .as_ref()
            .filter(|a| a.chat_id == chat_id && a.message_id == message_id)?;
        match &audience.author {
            Audience::Loading => Some(info_row(
                order::SENT + 2,
                "menu-author-loading",
                Some(IconName::User),
                tr(keys::CONTACTS_LOADING),
            )),
            Audience::Ready(user_id) => {
                let user_id = *user_id;
                let name = session.user(user_id)?.display_name();
                Some(menu_row(
                    order::SENT + 2,
                    IconName::User,
                    "menu-author",
                    tr_with(keys::CONTEXT_SENT_BY, &[("user", &name)]),
                    false,
                    cx,
                    move |this, window, cx| {
                        this.message_ui.menu = None;
                        this.open_user_panel(user_id, window, cx);
                    },
                ))
            }
            _ => None,
        }
    }

    /// "Delete Item": `deletePollOption` once TDLib allowed it for this
    /// option; the poll updates through `updateMessageContent`.
    pub(in crate::ui) fn delete_poll_option_item(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_id: &str,
        cx: &mut Context<Self>,
    ) {
        // A refused or failed request reaches the status line through
        // `message_action_note`.
        if let Some(live) = self.live.as_mut() {
            let _ = live
                .driver
                .delete_poll_option(chat_id, message_id, option_id);
        }
        cx.notify();
    }
}

mod demo_facts {
    use super::*;
    use quill::state::{Audience, MessageAudience, PollOptionMenu, Session};
    use quill::telegram::envelope::{MessageActions, PollOptionProperties};
    pub(in crate::ui) const GROUP: ChatId = ChatId(-2003);
    pub(in crate::ui) const CHANNEL: ChatId = ChatId(-1002000000002);
    pub(in crate::ui) const POLL: MessageId = MessageId(20);
    pub(in crate::ui) const CHECKLIST: MessageId = MessageId(21);
    pub(in crate::ui) const CONTACT: MessageId = MessageId(22);
    pub(in crate::ui) const POST: MessageId = MessageId(30);

    /// What TDLib would have answered for the message the menu opens on.
    pub(in crate::ui) fn inject_menu_facts(session: &mut Session, mode: &str) {
        let (chat, message) = match mode {
            "task" => (GROUP, CHECKLIST),
            "contact" => (GROUP, CONTACT),
            "author" => (CHANNEL, POST),
            _ => (GROUP, POLL),
        };
        session.messages.message_menu_actions = Some((
            chat,
            message,
            MessageActions {
                can_be_replied: true,
                can_be_copied: true,
                can_be_forwarded: true,
                can_get_author: mode == "author",
                ..MessageActions::default()
            },
        ));
        if mode == "poll" {
            session.messages.poll_option_menu = Some(PollOptionMenu {
                chat_id: GROUP,
                message_id: POLL,
                option_id: "b".into(),
                properties: Some(PollOptionProperties {
                    can_be_deleted: true,
                    can_get_link: true,
                    can_be_replied: true,
                    ..PollOptionProperties::default()
                }),
            });
        }
        if mode == "author" {
            let mut audience = MessageAudience::new(CHANNEL, POST);
            audience.author = Audience::Ready(32);
            session.messages.message_audience = Some(audience);
        }
    }
}
pub(in crate::ui) use demo_facts::*;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "item_rows_tests.rs"]
mod item_rows_tests;
