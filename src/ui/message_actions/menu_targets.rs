//! A context menu points at a message or a chat by id and draws nothing
//! once that target is gone from the session. The open value must go with
//! it, or the menu is invisible yet open: the arrow keys are swallowed, the
//! sidebar stays peeked and only Escape clears it.

use super::*;

impl QuillApp {
    /// Frame upkeep, every window: a menu whose message or chat the session
    /// no longer holds is closed. The message menu is each window's own
    /// (the slot in place); the chat-row and top-person menus are the main
    /// window's.
    pub(in crate::ui) fn close_menus_of_gone_targets(&mut self) {
        let Some(session) = self.session() else {
            return;
        };
        let message_gone = self.message_ui.menu.is_some_and(|menu| {
            !session
                .histories
                .get(&menu.chat_id.0)
                .is_some_and(|history| history.messages.contains_key(&menu.message_id.0))
        });
        let chat_menu_gone = !self.windows.in_window()
            && self
                .chat_list
                .menu
                .is_some_and(|menu| !session.chats.contains_key(&menu.chat_id.0));
        let list_elsewhere = self.history.pinned_list_open.as_ref().is_some_and(|key| {
            Some(key.chat) != session.open_chat || key.account != session.account
        });
        if list_elsewhere {
            self.history.pinned_list_open = None;
        }
        if message_gone {
            self.close_message_menu();
        }
        if chat_menu_gone {
            self.chat_list.menu = None;
        }
    }
}
