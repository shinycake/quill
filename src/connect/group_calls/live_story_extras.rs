//! Connect driver: the live story's custom emoji and Ban User.
//!
//! Comments carry custom emoji; their stickers are looked up once per id
//! through the custom emoji cache message text uses. Ban User in a
//! comment's delete box removes the sender from the poster's channel, or
//! blocks them when the live story is a user's (tdesktop
//! `Messages::deleteConfirmed`: `chatParticipants().kick` /
//! `blockedPeers().block`).
use super::*;
use crate::ids::ChatId;
use crate::state::{GroupsPurpose, MemberStatusChange};
use crate::telegram::envelope::ChatKind;
use crate::telegram::requests::{
    MessageSenderRef, chat_member_status_banned_json, set_chat_member_status,
};
use crate::text::TextEntityKind;

/// Ids looked up in one request, at most (as message text does).
const MAX_EMOJI_LOOKUP: usize = 200;

impl<S: JsonSender> ConnectDriver<S> {
    /// `getCustomEmojiStickers` for custom emoji in the watched live
    /// story's comments that are neither known nor already asked for.
    /// Called from `ingest`.
    pub(crate) fn maybe_resolve_live_story_custom_emoji(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active()
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetCustomEmojiStickers)
        {
            return Ok(());
        }
        let Some(group_call_id) = self.session.stories.live.as_ref().map(|l| l.group_call_id)
        else {
            return Ok(());
        };
        let Some(call) = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .filter(|call| call.id == group_call_id)
        else {
            return Ok(());
        };
        let emoji = &self.session.stickers.emoji;
        let mut ids: Vec<i64> = call
            .messages
            .iter()
            .flat_map(|message| message.entities.iter())
            .filter_map(|entity| match entity.kind {
                TextEntityKind::CustomEmoji { custom_emoji_id } => Some(custom_emoji_id),
                _ => None,
            })
            .filter(|id| {
                *id > 0
                    && !emoji.status_resolution_attempted.contains(id)
                    && !emoji
                        .custom_emoji_stickers
                        .iter()
                        .any(|item| item.custom_emoji_id == Some(*id))
            })
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids.truncate(MAX_EMOJI_LOOKUP);
        if ids.is_empty() {
            return Ok(());
        }
        self.session
            .stickers
            .emoji
            .status_resolution_attempted
            .extend(ids.iter().copied());
        let extra = self
            .session
            .request(RequestPurpose::GetCustomEmojiStickers, None);
        let json = crate::telegram::requests_emoji::get_custom_emoji_stickers(extra, &ids);
        self.send_json_request(extra, &json).map(|_| ())
    }

    /// Ban User from a comment's delete box: a user is banned from the
    /// poster's channel, or blocked when the live story is a user's. A
    /// chat sender can only be banned from a channel. Only on your own
    /// live story (`can_be_managed`).
    pub fn ban_live_story_commenter(
        &mut self,
        sender: MessageSender,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(chat_id) = self.session.stories.live.as_ref().map(|l| ChatId(l.chat_id)) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let manage = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.is_live_story && call.can_be_managed);
        if !manage {
            return Err(ConnectSendError::InvalidRequest);
        }
        let channel = matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup { .. })
        );
        match (sender, channel) {
            (MessageSender::User { user_id }, true) => self.ban_chat_member(chat_id, user_id, 0),
            (MessageSender::User { user_id }, false) => self.set_user_blocked(user_id, true),
            (MessageSender::Chat { chat_id: sender }, true) => self.ban_chat_sender(chat_id, sender),
            (MessageSender::Chat { .. }, false) => Err(ConnectSendError::InvalidRequest),
        }
    }

    /// `setChatMemberStatus` banned for a chat that comments as itself.
    fn ban_chat_sender(
        &mut self,
        chat_id: ChatId,
        sender: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::Groups(GroupsPurpose::SetChatMemberStatus {
            user_id: sender,
            kind: MemberStatusChange::Ban,
        });
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let member = MessageSenderRef::Chat(sender).to_value();
        let status = chat_member_status_banned_json(0);
        let json = set_chat_member_status(extra, chat_id.0, &member, &status);
        self.send_json_request(extra, &json).map(Some)
    }
}
