//! Delete confirmation and forward drafts.

use super::*;

/// Pending delete confirm (tdesktop `DeleteMessagesBox` / Unigram
/// `DeleteMessagesPopup`). `revoke` maps to `deleteMessages.revoke`
/// (schema 1.8.67 line 12282): true deletes for everyone, false only for
/// the current user. The revoke toggle is only offered for own outgoing
/// messages (`can_revoke`); incoming deletes are always for-me.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteConfirm {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub revoke: bool,
    pub can_revoke: bool,
}

impl DeleteConfirm {
    pub fn own(
        chat_id: ChatId,
        message_id: MessageId,
        is_outgoing: bool,
        pending: bool,
    ) -> Option<Self> {
        if is_outgoing && !pending {
            Some(Self {
                chat_id,
                message_id,
                revoke: true,
                can_revoke: true,
            })
        } else {
            None
        }
    }

    /// M1: any already-sent message (incoming included) can be deleted
    /// for the current user (`revoke: false`); the for-everyone toggle
    /// stays hidden.
    pub fn for_message(
        chat_id: ChatId,
        message_id: MessageId,
        is_outgoing: bool,
        pending: bool,
    ) -> Option<Self> {
        if pending || message_id.0 <= 0 {
            return None;
        }
        Some(Self {
            chat_id,
            message_id,
            revoke: is_outgoing,
            can_revoke: is_outgoing,
        })
    }
}

/// Messages queued for `forwardMessages` (tdesktop `Data::ForwardDraft` /
/// `ShowForwardMessagesBox`). Ids stay strictly increasing (schema).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardDraft {
    pub from_chat_id: ChatId,
    /// The conversation inside `from_chat_id` the messages were picked in
    /// (a forum topic, a thread, a Saved sublist). With the chat it is the
    /// selection's key: a forum's topics share the chat id, so a selection
    /// made in one topic must not show, delete or forward from the next.
    pub from_topic: TopicId,
    pub message_ids: Vec<MessageId>,
    /// M1: `forwardMessages.send_copy` — drop the "Forwarded from"
    /// attribution (TGX "Hide sender name"; schema 1.8.67 line 12237).
    pub send_copy: bool,
    /// M1: `forwardMessages.remove_caption` — strip captions on the copies
    /// (ignored by TDLib unless `send_copy` is true).
    pub remove_caption: bool,
    /// The forum topic of the destination chosen when the drop picked it
    /// (`forwardMessages.topic_id`); `None` sends to the chat itself.
    pub topic_id: Option<i32>,
}

impl ForwardDraft {
    /// Already-sent history only. Pending / local ids cannot be forwarded.
    pub fn from_message(
        chat_id: ChatId,
        topic: TopicId,
        message_id: MessageId,
        pending: bool,
    ) -> Option<Self> {
        if pending || message_id.0 <= 0 {
            return None;
        }
        Some(Self {
            from_chat_id: chat_id,
            from_topic: topic,
            message_ids: vec![message_id],
            send_copy: false,
            remove_caption: false,
            topic_id: None,
        })
    }

    pub fn contains(&self, message_id: MessageId) -> bool {
        self.message_ids.contains(&message_id)
    }

    /// Whether the messages were picked in `topic` of `chat_id`.
    pub fn is_from(&self, chat_id: ChatId, topic: TopicId) -> bool {
        self.from_chat_id == chat_id && self.from_topic == topic
    }

    pub fn is_empty(&self) -> bool {
        self.message_ids.is_empty()
    }

    /// Toggle an id of the same conversation. Cross-chat (or cross-topic)
    /// selection is not official history multi-select — start a new draft
    /// instead.
    pub fn toggle(
        &mut self,
        chat_id: ChatId,
        topic: TopicId,
        message_id: MessageId,
        pending: bool,
    ) {
        if pending || message_id.0 <= 0 {
            return;
        }
        if !self.is_from(chat_id, topic) {
            self.from_chat_id = chat_id;
            self.from_topic = topic;
            self.message_ids = vec![message_id];
            return;
        }
        if let Some(index) = self.message_ids.iter().position(|id| *id == message_id) {
            self.message_ids.remove(index);
        } else {
            self.message_ids.push(message_id);
            self.message_ids.sort_by_key(|id| id.0);
        }
    }

    pub fn count(&self) -> usize {
        self.message_ids.len()
    }

    /// The selection as it stands now: only ids `alive` still knows. The
    /// stored ids are what was picked; this is what counts and acts.
    #[must_use]
    pub fn retaining(mut self, alive: impl Fn(MessageId) -> bool) -> Self {
        self.message_ids.retain(|id| alive(*id));
        self
    }
}

/// tdesktop ShareBox / `ShowForwardMessagesBox` Escape: leave the picker
/// (and optional selection) without sending.
pub fn cancel_forward_draft(draft: Option<ForwardDraft>) -> (Option<ForwardDraft>, bool) {
    let _ = draft;
    (None, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_confirm_is_own_outgoing_only() {
        assert!(DeleteConfirm::own(ChatId(11), MessageId(102), true, false).is_some());
        assert!(DeleteConfirm::own(ChatId(11), MessageId(101), false, false).is_none());
        assert!(DeleteConfirm::own(ChatId(11), MessageId(-5), true, true).is_none());
    }

    #[test]
    fn forward_draft_sorts_and_rejects_pending() {
        let none = TopicId::None;
        assert!(ForwardDraft::from_message(ChatId(11), none, MessageId(-1), true).is_none());
        assert!(ForwardDraft::from_message(ChatId(11), none, MessageId(0), false).is_none());
        let mut draft =
            ForwardDraft::from_message(ChatId(11), none, MessageId(102), false).unwrap();
        draft.toggle(ChatId(11), none, MessageId(101), false);
        assert_eq!(draft.message_ids, vec![MessageId(101), MessageId(102)]);
        draft.toggle(ChatId(11), none, MessageId(102), false);
        assert_eq!(draft.message_ids, vec![MessageId(101)]);
        draft.toggle(ChatId(12), none, MessageId(40), false);
        assert_eq!(draft.from_chat_id, ChatId(12));
        assert_eq!(draft.message_ids, vec![MessageId(40)]);
        // Another topic of the same chat starts a new selection.
        let topic = TopicId::forum(3);
        draft.toggle(ChatId(12), topic, MessageId(41), false);
        assert!(draft.is_from(ChatId(12), topic));
        assert_eq!(draft.message_ids, vec![MessageId(41)]);
        let (cleared, picker) = cancel_forward_draft(Some(draft));
        assert_eq!(cleared, None);
        assert!(!picker);
    }
}
