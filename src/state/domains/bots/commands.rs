//! The composer's `/` menu rows: one bot's commands in a bot chat, every bot's commands in a group.
use crate::composer::{
    CommandBot, CommandMenuItem, group_command_menu_items, merge_command_menu_items,
};
use crate::state::*;
use crate::telegram::envelope::GroupBotCommands;

impl Session {
    /// Phase 3.3: the `/` menu rows of a chat. A bot chat lists the bot's
    /// `botInfo` commands, then cached `getCommands` (global scope) results,
    /// deduped by command name. A group lists the commands of every bot in
    /// it (tdesktop `FieldAutocomplete::updateFiltered`). A chat with a
    /// person lists their quick reply shortcuts. Empty while nothing is
    /// known yet.
    pub fn command_menu_items(&self, chat_id: ChatId) -> Vec<CommandMenuItem> {
        let Some(user_id) = self.bot_user_id_for_chat(chat_id) else {
            return match self.group_commands_key(chat_id) {
                Some(key) => self.group_command_rows(chat_id, key),
                None => self.quick_reply_menu_items(chat_id),
            };
        };
        let specific: &[BotCommand] = self
            .bot_info_for_chat(chat_id)
            .map(|info| info.commands.as_slice())
            .unwrap_or(&[]);
        let global: &[BotCommand] = self
            .bots
            .bot_commands
            .get(&user_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let bot = self.command_bot(user_id, false);
        let mut items = merge_command_menu_items(specific, global);
        for item in &mut items {
            item.bot = bot.clone();
        }
        items
    }

    /// Keep a group's `bot_commands` and note the bots whose user is not
    /// cached yet; the driver asks for them once each.
    pub(crate) fn store_group_bot_commands(
        &mut self,
        key: GroupCommandsKey,
        bot_commands: Vec<GroupBotCommands>,
    ) {
        for entry in &bot_commands {
            let id = entry.bot_user_id;
            if !self.users.contains_key(&id)
                && !self.bots.asked_bot_users.contains(&id)
                && !self.bots.bot_users_wanted.contains(&id)
            {
                self.bots.bot_users_wanted.push(id);
            }
        }
        self.bots.group_bot_commands.insert(key, bot_commands);
    }

    /// Which `bot_commands` list a chat reads: basic groups and megagroups
    /// have one, broadcast channels do not (tdesktop `isMegagroup`).
    pub fn group_commands_key(&self, chat_id: ChatId) -> Option<GroupCommandsKey> {
        match self.chats.get(&chat_id.0)?.kind {
            ChatKind::BasicGroup { basic_group_id } => {
                Some(GroupCommandsKey::BasicGroup(basic_group_id))
            }
            ChatKind::Supergroup {
                supergroup_id,
                is_channel: false,
            } => Some(GroupCommandsKey::Supergroup(supergroup_id)),
            _ => None,
        }
    }

    /// The bot's menu identity, from the user cache. `None` while the user
    /// is not known.
    fn command_bot(&self, user_id: i64, in_group: bool) -> Option<CommandBot> {
        let user = self.user(user_id)?;
        Some(CommandBot {
            user_id,
            username: user.username.clone(),
            name: user.display_name(),
            in_group,
        })
    }

    fn group_command_rows(&self, chat_id: ChatId, key: GroupCommandsKey) -> Vec<CommandMenuItem> {
        let Some(lists) = self.bots.group_bot_commands.get(&key) else {
            return Vec::new();
        };
        // Basic groups list the bots that wrote last first; the others
        // keep TDLib's order.
        let recent = match key {
            GroupCommandsKey::BasicGroup(_) => self.recent_senders(chat_id),
            GroupCommandsKey::Supergroup(_) => Vec::new(),
        };
        let rank = |bot_id: i64| recent.iter().position(|id| *id == bot_id);
        let mut bots: Vec<(CommandBot, Vec<BotCommand>)> = lists
            .iter()
            .filter_map(|entry| {
                Some((
                    self.command_bot(entry.bot_user_id, true)?,
                    entry.commands.clone(),
                ))
            })
            .collect();
        bots.sort_by_key(|(bot, _)| rank(bot.user_id).unwrap_or(usize::MAX));
        group_command_menu_items(&bots)
    }

    /// User ids that wrote in the loaded part of a chat, newest first,
    /// without repeats.
    fn recent_senders(&self, chat_id: ChatId) -> Vec<i64> {
        let Some(history) = self.histories.get(&chat_id.0) else {
            return Vec::new();
        };
        let mut seen = Vec::new();
        for message in history.messages.values().rev() {
            if let Some(MessageSender::User { user_id }) = message.sender
                && !seen.contains(&user_id)
            {
                seen.push(user_id);
            }
        }
        seen
    }
}
