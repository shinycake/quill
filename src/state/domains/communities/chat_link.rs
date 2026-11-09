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
    /// A community the user lost access to (deleted) has no chats.
    pub fn chat_community_id(&self, chat_id: i64) -> Option<i64> {
        let linked = self.cached_chat_community(chat_id)?;
        match linked {
            Some(0) => None,
            Some(id) => Some(id).filter(|id| {
                self.groups
                    .communities
                    .get(id)
                    .is_none_or(|community| community.have_access)
            }),
            None => self
                .groups
                .community_full_infos
                .iter()
                .find(|(_, info)| info.chats.iter().any(|c| c.chat_id == chat_id))
                .map(|(id, _)| *id),
        }
    }

    /// The community id in the chat's cached full info: `Some(None)` when
    /// that full info is not cached, `None` for an unknown chat.
    fn cached_chat_community(&self, chat_id: i64) -> Option<Option<i64>> {
        let chat = self.chats.get(&chat_id)?;
        Some(match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => self
                .supergroup_full_info(supergroup_id)
                .map(|info| info.profile.community_id),
            ChatKind::Private { user_id } => self
                .users_state
                .user_full_infos
                .get(&user_id.0)
                .map(|info| info.extras.community_id),
            _ => None,
        })
    }

    /// Our `addCommunityChat` (`community_id` is the community) or
    /// `removeCommunityChat` (0) answered `ok`: the chat's cached full info
    /// says so too. TDLib reloads only the community's full info before
    /// its `ok` (`CommunityManager::on_change_community_dialogs`); the
    /// chat's `supergroupFullInfo.community_id` changes only on its next
    /// full fetch (`ChatManager::on_update_channel_linked_community_id`
    /// changes the supergroup alone), so without this the manage screen
    /// keeps offering Remove after a Remove. A remove only clears a link
    /// that still names `from`.
    pub(crate) fn note_chat_community(&mut self, chat_id: i64, from: Option<i64>, to: i64) {
        let Some(chat) = self.chats.get(&chat_id) else {
            return;
        };
        let linked = match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => self
                .groups
                .supergroup_full_infos
                .get_mut(&supergroup_id)
                .map(|info| &mut info.profile.community_id),
            ChatKind::Private { user_id } => self
                .users_state
                .user_full_infos
                .get_mut(&user_id.0)
                .map(|info| &mut info.extras.community_id),
            _ => None,
        };
        if let Some(linked) = linked
            && from.is_none_or(|from| *linked == from)
        {
            *linked = to;
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
