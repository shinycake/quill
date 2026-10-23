//! Methods moved out of `chat_list.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr};

impl QuillApp {
    /// Slice CL1: Pin / Unpin from the chat-row context menu
    /// (`toggleChatIsPinned`, schema 1.8.67, line 13678). The pin-limit
    /// pre-check mirrors TGX `ChatsController`: count pinned chats of
    /// the same secrecy class in the target list against
    /// `pinned_chat_count_max` / `pinned_archived_chat_count_max`
    /// (`updateOption`); at the limit the request is never sent and the
    /// TGX message shows instead.
    pub(in crate::ui) fn toggle_chat_pin(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let session = match self.session() {
            Some(session) => session,
            None => return,
        };
        let chat = match session.chats.get(&chat_id.0) {
            Some(chat) => chat,
            None => return,
        };
        let archived = chat.in_archive;
        let pinned = if archived {
            chat.archive_is_pinned
        } else {
            chat.is_pinned
        };
        let secret = matches!(chat.kind, ChatKind::Secret { .. });
        if !pinned {
            let limit = if archived {
                session.chat_list.pinned_archived_chat_count_max
            } else {
                session.chat_list.pinned_chat_count_max
            };
            let pinned_count = session
                .chats
                .values()
                .filter(|c| {
                    let is_pinned = if archived {
                        c.in_archive && c.archive_is_pinned
                    } else {
                        c.in_main_list && c.is_pinned
                    };
                    is_pinned && matches!(c.kind, ChatKind::Secret { .. }) == secret
                })
                .count() as i32;
            if pinned_count >= limit.max(0) {
                // tdesktop `PinsLimitBox`: the limit box, with Premium's
                // higher limit as the way out.
                self.show_premium_limit(
                    if archived {
                        quill::premium_limits::PremiumLimitKind::PinsArchive
                    } else {
                        quill::premium_limits::PremiumLimitKind::Pins
                    },
                    0,
                    cx,
                );
                return;
            }
        }
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.toggle_chat_pin(chat_id, !pinned);
            self.connection.set_note(match result {
                Ok(Some(_)) if pinned => StatusNote::silent(tr(keys::Q_CHATLIST_UNPINNING)),
                Ok(Some(_)) => StatusNote::silent(tr(keys::Q_CHATLIST_PINNING)),
                Ok(None) => StatusNote::silent(tr(keys::Q_CHATLIST_PIN_REQUEST_ALREADY_IN_FLIGHT)),
                Err(_) => StatusNote::error(tr(keys::Q_CHATLIST_COULD_NOT_CHANGE_PIN)),
            });
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_pin(chat_id, !pinned, archived);
            self.connection.set_note(if pinned {
                StatusNote::silent(tr(keys::Q_LEFTOVERS_UNPINNED))
            } else {
                StatusNote::silent(tr(keys::Q_LEFTOVERS_PINNED))
            });
            cx.notify();
        }
    }

    /// Slice CL1: Mark as read / Mark as unread from the chat-row
    /// context menu. Mark-as-read follows Telegram X
    /// (`Tdlib.markChatAsRead` with `MessageSourceChatList`):
    /// `viewMessages` over the newest known message reads real unread
    /// history, plus `toggleChatIsMarkedAsUnread(false)` clears the
    /// manual marker. Mark-as-unread is the plain toggle (TGX only
    /// offers it when `unreadCount == 0`, which the menu label gates).
    pub(in crate::ui) fn toggle_chat_marked_as_unread(
        &mut self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) {
        let marked = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.is_marked_as_unread || chat.unread_count > 0);
        // Slice CL1 review nit: server-side unread with no cached history
        // can't be marked read (the driver has nothing to view and sends
        // nothing); say so honestly instead of "request already in flight".
        let nothing_to_view = self.session().is_some_and(|session| {
            session
                .chats
                .get(&chat_id.0)
                .is_some_and(|c| c.unread_count > 0 && !c.is_marked_as_unread)
                && !session
                    .histories
                    .get(&chat_id.0)
                    .is_some_and(|h| !h.messages.is_empty())
        });
        if marked && nothing_to_view && self.live.is_some() {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHATLIST_OPEN_THE_CHAT_TO_MARK,
            )));
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let result = if marked {
                live.driver.mark_chat_as_read(chat_id)
            } else {
                live.driver.toggle_chat_marked_as_unread(chat_id, true)
            };
            self.connection.set_note(match result {
                Ok(Some(_)) if marked => StatusNote::silent(tr(keys::Q_CHATLIST_MARKING_AS_READ)),
                Ok(Some(_)) => StatusNote::silent(tr(keys::Q_CHATLIST_MARKING_AS_UNREAD)),
                Ok(None) => StatusNote::silent(tr(keys::Q_CHATLIST_REQUEST_ALREADY_IN_FLIGHT)),
                Err(_) => StatusNote::error(tr(keys::Q_CHATLIST_COULD_NOT_CHANGE_READ_STATE)),
            });
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_marked_as_unread(chat_id, !marked);
            self.connection.set_note(if marked {
                StatusNote::silent(tr(keys::Q_CHATLIST_MARKED_AS_READ))
            } else {
                StatusNote::silent(tr(keys::Q_CHATLIST_MARKED_AS_UNREAD))
            });
            cx.notify();
        }
    }
}
