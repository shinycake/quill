//! Chat-row "Mark as read": reads the whole chat, as tdesktop's
//! `MarkAsReadMenu::MarkAsReadThread` does.
use super::*;
use crate::state::UnreadJumpKind;

impl<S: JsonSender> ConnectDriver<S> {
    /// Mark the whole chat read (tdesktop `MarkAsReadThread`):
    /// - `viewMessages` with `force_read` on the chat's newest message
    ///   (`Histories::readInbox` reads till `lastServerMessage`, never
    ///   the newest row of whatever history page happens to be loaded);
    /// - every loaded forum topic with unread messages reads till its
    ///   own last message (`forum->enumerateTopics`);
    /// - unread mentions and reactions clear (`readAllChatMentions`,
    ///   `readAllChatReactions`);
    /// - the manual marked-unread flag is cleared.
    ///
    /// `Ok(None)` = nothing unread or nothing could be sent.
    pub fn mark_chat_as_read(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat) = self
            .session
            .chats
            .get(&chat_id.0)
            .filter(|chat| chat.supported())
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let unread = chat.unread_count > 0;
        let marked = chat.is_marked_as_unread;
        let mentions = chat.unread_mention_count > 0;
        let reactions = chat.unread_reaction_count > 0;
        let topics = self.session.unread_forum_topics(chat_id);
        if !unread && !marked && !mentions && !reactions && topics.is_empty() {
            return Ok(None);
        }
        let mut sent: Option<RequestId> = None;
        if unread && let Some(id) = self.session.mark_read_target(chat_id) {
            sent = self.read_chat_inbox_till(chat_id, id)?.or(sent);
        }
        for forum_topic_id in topics {
            sent = self
                .mark_forum_topic_read(chat_id, forum_topic_id)?
                .or(sent);
        }
        if mentions {
            self.read_all_chat_unread_markers(chat_id, UnreadJumpKind::Mention)?;
        }
        if reactions {
            self.read_all_chat_unread_markers(chat_id, UnreadJumpKind::Reaction)?;
        }
        if marked {
            sent = self.toggle_chat_marked_as_unread(chat_id, false)?.or(sent);
        }
        Ok(sent)
    }

    /// `viewMessages` on `id` with `messageSourceChatList` and
    /// `force_read`: TDLib reads the chat's inbox up to that message and
    /// confirms with `updateChatReadInbox`. Kept apart from the open
    /// chat's on-screen `ViewMessages` so a view in flight never swallows
    /// the read.
    fn read_chat_inbox_till(
        &mut self,
        chat_id: ChatId,
        id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let purpose = RequestPurpose::ReadChatInbox;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&view_messages(
            extra,
            chat_id,
            &[id],
            "messageSourceChatList",
            true,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }
}
