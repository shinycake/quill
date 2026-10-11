//! Quick reply shortcuts and their messages (see `crate::quick_replies` for
//! what Telegram Desktop does).
use crate::quick_replies::{
    QuickRepliesUpdate, QuickReplyMessage, QuickReplyShortcut, count_label,
};
use crate::state::*;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickRepliesState {
    /// Shortcut ids in the order TDLib sent (`updateQuickReplyShortcuts`).
    pub order: Vec<i32>,
    pub shortcuts: HashMap<i32, QuickReplyShortcut>,
    pub messages: HashMap<i32, Vec<QuickReplyMessage>>,
    /// `loadQuickReplyShortcuts` was sent.
    pub shortcuts_requested: bool,
    /// Shortcuts whose messages were asked for.
    pub messages_requested: HashSet<i32>,
    /// The last refusal, in words, for the open dialog.
    pub error: Option<String>,
    /// Counter for `sendQuickReplyShortcutMessages.sending_id`.
    pub next_sending_id: i32,
}

impl QuickRepliesState {
    /// Shortcuts in list order. A shortcut missing from the order (an
    /// update raced ahead of it) follows, sorted by id.
    pub fn list(&self) -> Vec<&QuickReplyShortcut> {
        let mut out: Vec<&QuickReplyShortcut> = self
            .order
            .iter()
            .filter_map(|id| self.shortcuts.get(id))
            .collect();
        let mut rest: Vec<&QuickReplyShortcut> = self
            .shortcuts
            .values()
            .filter(|s| !self.order.contains(&s.id))
            .collect();
        rest.sort_by_key(|s| s.id);
        out.extend(rest);
        out
    }

    /// The shortcut named `name`, compared without case.
    pub fn by_name(&self, name: &str) -> Option<&QuickReplyShortcut> {
        self.shortcuts
            .values()
            .find(|s| s.name.eq_ignore_ascii_case(name) || s.name == name)
    }

    pub fn messages_of(&self, shortcut_id: i32) -> &[QuickReplyMessage] {
        self.messages
            .get(&shortcut_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Composer rows: `(id, name, "N messages")` for shortcuts that have
    /// messages and start with `prefix` (tdesktop `FieldAutocomplete`).
    pub fn menu_rows(&self, prefix: &str) -> Vec<(i32, String, String)> {
        let prefix = prefix.to_lowercase();
        self.list()
            .into_iter()
            .filter(|s| s.message_count >= 1 && s.name.to_lowercase().starts_with(&prefix))
            .map(|s| (s.id, s.name.clone(), count_label(s.message_count)))
            .collect()
    }
}

impl Session {
    /// `/` menu rows for a private chat with a person (not a bot, not
    /// yourself): the quick reply shortcuts that have messages.
    pub fn quick_reply_menu_items(&self, chat_id: ChatId) -> Vec<CommandMenuItem> {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return Vec::new();
        };
        let ChatKind::Private { user_id } = chat.kind else {
            return Vec::new();
        };
        if self.my_user_id == Some(user_id.0) || self.bots.bot_user_ids.contains(&user_id.0) {
            return Vec::new();
        }
        self.settings
            .quick_replies
            .menu_rows("")
            .into_iter()
            .map(|(id, name, description)| CommandMenuItem {
                command: name,
                description,
                global: false,
                is_ephemeral: false,
                quick_reply: Some(id),
            })
            .collect()
    }

    pub fn apply_quick_replies(&mut self, update: QuickRepliesUpdate) {
        let state = &mut self.settings.quick_replies;
        match update {
            QuickRepliesUpdate::Shortcut(shortcut) => {
                state.shortcuts.insert(shortcut.id, shortcut);
            }
            QuickRepliesUpdate::Deleted { shortcut_id } => {
                state.shortcuts.remove(&shortcut_id);
                state.messages.remove(&shortcut_id);
                state.messages_requested.remove(&shortcut_id);
                state.order.retain(|id| *id != shortcut_id);
            }
            QuickRepliesUpdate::Order { shortcut_ids } => state.order = shortcut_ids,
            QuickRepliesUpdate::Messages {
                shortcut_id,
                messages,
            } => {
                if let Some(shortcut) = state.shortcuts.get_mut(&shortcut_id) {
                    shortcut.message_count = messages.len() as i32;
                }
                state.messages.insert(shortcut_id, messages);
            }
        }
    }

    /// A failed quick reply request; `true` when it was one.
    pub(crate) fn apply_quick_replies_error(
        &mut self,
        err: &TdError,
        purpose: Option<RequestPurpose>,
    ) -> bool {
        let state = &mut self.settings.quick_replies;
        match purpose {
            Some(RequestPurpose::Settings(SettingsPurpose::LoadQuickReplyShortcuts)) => {
                state.shortcuts_requested = false;
            }
            Some(RequestPurpose::Settings(SettingsPurpose::LoadQuickReplyMessages {
                shortcut_id,
            })) => {
                state.messages_requested.remove(&shortcut_id);
            }
            Some(RequestPurpose::Settings(SettingsPurpose::MutateQuickReplies)) => {}
            _ => return false,
        }
        state.error = Some(format!("Quick replies: {}", error_reason(err)));
        true
    }
}
