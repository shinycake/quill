//! Message menu rows that depend on where a message came from: the original
//! date of a forward, "Send Another Gift" on a gift service message, and
//! "Block user" on a reply in the Replies chat, with the box it opens
//! (tdesktop `history_view_context_menu.cpp`:
//! `AddWhenEditedForwardedAuthorActionHelper`, the gift branch of
//! `FillContextMenu`, `AddBlockSenderAction`; `window_peer_menu.cpp`:
//! `BlockSenderFromRepliesBox`).
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::checkbox::Checkbox;
use quill::message_menu_origin::{
    RepliesBlock, RepliesSender, original_date, replies_sender, send_another_gift_offered,
};
use quill::telegram::envelope::{ChatKind, effective_content};

impl QuillApp {
    /// The "Original" date line of a forward, in the audience section: above
    /// the read time in a private chat, below the "N Seen" row elsewhere.
    pub(in crate::ui) fn add_origin_rows(
        &self,
        rows: &mut Vec<MenuRow>,
        chat_id: ChatId,
        message: &quill::state::HistoryMessage,
    ) {
        let Some(date) = original_date(message) else {
            return;
        };
        let now = quill::local_time::civil_local(quill::local_time::now_unix());
        let when = quill::local_time::civil_local(i64::from(date));
        let row = info_row(
            order::AUDIENCE,
            "menu-original-date",
            Some(IconName::Forward),
            quill::message_menu_origin::original_date_text(&when, &now),
        );
        let private = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Private { .. }));
        if private {
            rows.insert(0, row);
        } else {
            rows.push(row);
        }
    }

    /// "Send Another Gift" on a gift service message, for a recipient a gift
    /// can go to.
    pub(in crate::ui) fn menu_gift_row(
        &self,
        chat_id: ChatId,
        message: &quill::state::HistoryMessage,
        cx: &mut Context<Self>,
    ) -> Option<MenuRow> {
        let content = effective_content(&message.content, message.ephemeral.as_ref());
        if !send_another_gift_offered(content, message.is_outgoing) {
            return None;
        }
        let session = self.session()?;
        // tdesktop skips the notifications account and deleted accounts.
        let unreachable = chat_id.0 == quill::ton_wallet::SERVICE_NOTIFICATIONS_CHAT_ID
            || matches!(
                session.chats.get(&chat_id.0).map(|chat| &chat.kind),
                Some(ChatKind::Private { user_id })
                    if session.user(user_id.0).is_some_and(|user| user.is_deleted)
            );
        if unreachable {
            return None;
        }
        session.gift_recipient(chat_id)?;
        Some(menu_row(
            order::MEDIA,
            IconName::Gift,
            "menu-send-another-gift",
            tr(keys::CONTEXT_GIFT_SEND),
            false,
            cx,
            move |this, window, cx| {
                this.message_ui.menu = None;
                this.open_send_gift_for(chat_id, window, cx);
            },
        ))
    }

    /// "Block user" on a message in the Replies chat.
    pub(in crate::ui) fn menu_block_sender_row(
        &self,
        chat_id: ChatId,
        message: &quill::state::HistoryMessage,
        cx: &mut Context<Self>,
    ) -> Option<MenuRow> {
        let session = self.session()?;
        if !session.is_replies_chat(chat_id) || message.id.0 <= 0 || message.pending {
            return None;
        }
        let block = match replies_sender(message)? {
            RepliesSender::User(id) => {
                let user = session.user(id)?;
                let short = Some(user.first_name.trim())
                    .filter(|name| !name.is_empty())
                    .map_or_else(|| user.display_name(), str::to_string);
                RepliesBlock::new(message.id, short, user.display_name(), true)
            }
            RepliesSender::Chat { chat_id, signature } => {
                let title = session
                    .chats
                    .get(&chat_id)
                    .map(|chat| chat.title.clone())
                    .filter(|title| !title.is_empty())
                    .unwrap_or(signature);
                RepliesBlock::new(message.id, title.clone(), title, false)
            }
            RepliesSender::Hidden(name) => RepliesBlock::new(message.id, name.clone(), name, false),
        };
        Some(menu_row(
            order::SELECT + 1,
            IconName::Ban,
            "menu-block-sender",
            tr(keys::PROFILE_BLOCK_USER),
            false,
            cx,
            move |this, _, cx| {
                this.message_ui.menu = None;
                if let Some(session) = this.origin_session_mut() {
                    session.users_state.replies_block = Some(block.clone());
                }
                cx.notify();
            },
        ))
    }

    fn origin_session_mut(&mut self) -> Option<&mut Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_mut(),
        }
    }

    fn replies_block_mut(&mut self) -> Option<&mut RepliesBlock> {
        self.origin_session_mut()?
            .users_state
            .replies_block
            .as_mut()
    }

    fn close_replies_block(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.origin_session_mut() {
            session.users_state.replies_block = None;
        }
        cx.notify();
    }

    fn submit_replies_block(&mut self, cx: &mut Context<Self>) {
        let name = self
            .session()
            .and_then(|s| s.users_state.replies_block.as_ref())
            .map(|block| block.name.clone());
        let sent = match self.live.as_mut() {
            Some(live) => live.driver.submit_replies_block().is_ok(),
            None => {
                self.close_replies_block(cx);
                true
            }
        };
        if let Some(name) = name {
            self.connection.set_note(if sent {
                StatusNote::silent(tr_with(keys::NEW_CONTACT_BLOCK_DONE, &[("user", &name)]))
            } else {
                StatusNote::error(tr(keys::Q_CHAT_COULDNT_BLOCK_THE_SENDER_TRY_AGAIN))
            });
        }
        cx.notify();
    }

    pub(in crate::ui) fn build_replies_block_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::BlockFromReplies, |this, _, cx| {
                this.close_replies_block(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true);
            let Some(block) = this
                .session()
                .and_then(|s| s.users_state.replies_block.clone())
            else {
                return dialog.on_close(on_close);
            };
            let mut body = div().flex().flex_col().gap_3();
            if block.is_user {
                body = body.child(div().text_sm().child(tr_with(
                    keys::BLOCKED_LIST_CONFIRM_TEXT,
                    &[("name", &block.name)],
                )));
            }
            body = body
                .child(
                    Checkbox::new("replies-block-report")
                        .label(tr(keys::REPORT_SPAM))
                        .checked(block.report_spam)
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some(block) = this.replies_block_mut() {
                                block.report_spam = on;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Checkbox::new("replies-block-delete")
                        .label(tr(keys::BLOCKED_LIST_CONFIRM_REPLY))
                        .checked(block.delete_message)
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some(block) = this.replies_block_mut() {
                                block.delete_message = on;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Checkbox::new("replies-block-delete-all")
                        .label(tr_with(
                            keys::BLOCKED_LIST_CONFIRM_REPLY_ALL,
                            &[("user", &block.full_name)],
                        ))
                        .checked(block.delete_all)
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some(block) = this.replies_block_mut() {
                                block.delete_all = on;
                            }
                            cx.notify();
                        })),
                );
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("replies-block-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_replies_block(cx);
                            this.close_kit_dialog_if_done(DialogKind::BlockFromReplies, window, cx);
                        })),
                )
                .child(
                    Button::new("replies-block-submit")
                        .label(tr(keys::BLOCKED_LIST_CONFIRM_OK))
                        .danger()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_replies_block(cx);
                            this.close_kit_dialog_if_done(DialogKind::BlockFromReplies, window, cx);
                        })),
                );
            dialog
                .title(crate::ui::shell::dialog_title(tr_with(
                    keys::BLOCKED_LIST_CONFIRM_TITLE,
                    &[("name", &block.name)],
                )))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
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
    BlockFromReplies => DialogSpec::new(
        3951,
        |app| app.session().is_some_and(|s| s.users_state.replies_block.is_some()),
        QuillApp::build_replies_block_dialog,
    ),
}

/// The chats of the `ready-origin-menus` demo.
#[cfg(feature = "demo-capture")]
mod demo_ids {
    use quill::ids::ChatId;
    pub(in crate::ui) const FORWARD_CHAT: ChatId = ChatId(41);
    pub(in crate::ui) const GIFT_CHAT: ChatId = ChatId(42);
    pub(in crate::ui) const REPLIES_CHAT: ChatId = ChatId(1_271_266_957);
}
#[cfg(feature = "demo-capture")]
pub(in crate::ui) use demo_ids::*;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "origin_rows_tests.rs"]
mod origin_rows_tests;
