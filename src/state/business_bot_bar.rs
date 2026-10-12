//! Session access to the business bot manage bar of each chat.
use super::*;
use crate::business_bot_bar::BusinessBotManageBar;

impl Session {
    /// Store (or drop) a chat's business bot manage bar.
    pub fn set_business_bot_bar(&mut self, chat_id: i64, bar: Option<BusinessBotManageBar>) {
        match bar {
            Some(bar) => {
                self.chats_state.business_bot_bars.insert(chat_id, bar);
            }
            None => {
                self.chats_state.business_bot_bars.remove(&chat_id);
            }
        }
    }

    /// The bar of a chat managed by a connected business bot.
    pub fn business_bot_bar(&self, chat_id: ChatId) -> Option<&BusinessBotManageBar> {
        self.chats_state.business_bot_bars.get(&chat_id.0)
    }
}
