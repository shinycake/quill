//! Which community a chat belongs to, and whether its manage screen
//! offers the community section (tdesktop `fillCommunitySection`).
use crate::state::*;
use crate::telegram::envelope::ChatKind;

/// What the copy calls the chat (`_bot` / `_channel` / plain variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommunityChatKind {
    Group,
    Channel,
    Bot,
}

impl Session {
    /// The kind of chat `chat_id` is, for community copy; `None` for
    /// chats that can't join a community (basic groups, people, secret
    /// chats).
    pub fn community_chat_kind(&self, chat_id: i64) -> Option<CommunityChatKind> {
        let chat = self.chats.get(&chat_id)?;
        match chat.kind {
            ChatKind::Supergroup { is_channel, .. } => Some(if is_channel {
                CommunityChatKind::Channel
            } else {
                CommunityChatKind::Group
            }),
            ChatKind::Private { user_id } => self
                .user(user_id.0)
                .filter(|user| user.is_bot)
                .map(|_| CommunityChatKind::Bot),
            _ => None,
        }
    }

    /// The community `chat_id` was added to: `supergroupFullInfo` /
    /// `userFullInfo.community_id`, else a loaded community listing it.
    pub fn chat_community_id(&self, chat_id: i64) -> Option<i64> {
        let chat = self.chats.get(&chat_id)?;
        let linked = match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => self
                .supergroup_full_info(supergroup_id)
                .map(|info| info.profile.community_id),
            ChatKind::Private { user_id } => self
                .users_state
                .user_full_infos
                .get(&user_id.0)
                .map(|info| info.extras.community_id),
            _ => None,
        };
        match linked {
            Some(0) => None,
            Some(id) => Some(id),
            None => self
                .groups
                .community_full_infos
                .iter()
                .find(|(_, info)| info.chats.iter().any(|c| c.chat_id == chat_id))
                .map(|(id, _)| *id),
        }
    }

    /// The manage screen shows the community section: the owner of a
    /// group or channel that is not a channel's direct messages, or the
    /// owner of a bot.
    pub fn chat_offers_community_section(&self, chat_id: i64) -> bool {
        match self.community_chat_kind(chat_id) {
            Some(CommunityChatKind::Bot) => self.chats.get(&chat_id).is_some_and(|chat| {
                matches!(chat.kind, ChatKind::Private { user_id }
                    if self.user(user_id.0).is_some_and(|user| user.can_be_edited))
            }),
            Some(_) => {
                self.chat_is_owner(ChatId(chat_id))
                    && self.direct_messages_role(ChatId(chat_id)).is_none()
            }
            None => false,
        }
    }
}
