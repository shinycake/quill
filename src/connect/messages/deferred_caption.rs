//! Connect driver: a caption chosen for a media message that is still being
//! sent (`crate::composer::deferred_caption`).
use super::*;
use crate::composer::{DeferredCaption, has_editable_caption, set_caption};
use crate::telegram::envelope::{EnvelopePayload, MessagesPayload};

/// How a message that was being sent ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SendOutcome {
    /// `updateMessageSendSucceeded`: the temporary id became a real one.
    Succeeded {
        chat_id: ChatId,
        old: MessageId,
        new: MessageId,
    },
    /// `updateMessageSendFailed`.
    Failed { chat_id: ChatId, old: MessageId },
}

impl<S: JsonSender> ConnectDriver<S> {
    /// The send outcome an inbound update reports, read before the reducer
    /// replaces the pending row.
    pub(crate) fn send_outcome(payload: &EnvelopePayload) -> Option<SendOutcome> {
        match payload {
            EnvelopePayload::Messages(MessagesPayload::UpdateMessageSendSucceeded {
                message,
                old_message_id,
            }) => Some(SendOutcome::Succeeded {
                chat_id: message.chat_id,
                old: *old_message_id,
                new: message.id,
            }),
            EnvelopePayload::Messages(MessagesPayload::UpdateMessageSendFailed {
                message,
                old_message_id,
                ..
            }) => Some(SendOutcome::Failed {
                chat_id: message.chat_id,
                old: *old_message_id,
            }),
            _ => None,
        }
    }

    /// Keep `text` as the caption of the pending media message `edit`
    /// points at, and show it on the row at once. The edit goes out when
    /// the message has its real id ([`Self::run_deferred_caption`]).
    pub fn edit_pending_caption(
        &mut self,
        edit: &ComposerEdit,
        text: &str,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let pending_media = self
            .session
            .histories
            .get(&edit.chat_id.0)
            .and_then(|history| history.messages.get(&edit.message_id.0))
            .is_some_and(|m| {
                m.is_outgoing && m.pending && !m.failed && has_editable_caption(&m.content)
            });
        if !pending_media {
            return Err(ConnectSendError::InvalidRequest);
        }
        let caption = text.trim();
        self.check_caption_length(caption)?;
        let deferred = &mut self.session.messages.deferred_captions;
        deferred.retain(|d| d.chat_id != edit.chat_id || d.message_id != edit.message_id);
        deferred.push(DeferredCaption {
            chat_id: edit.chat_id,
            message_id: edit.message_id,
            text: caption.to_string(),
            caption_above: edit.caption_above,
        });
        if let Some(message) = self
            .session
            .histories
            .get_mut(&edit.chat_id.0)
            .and_then(|history| history.messages.get_mut(&edit.message_id.0))
        {
            set_caption(&mut message.content, caption);
        }
        Ok(())
    }

    /// After the reducer handled a send outcome: send the caption chosen
    /// while the message was still pending, or drop it when the send failed.
    pub(crate) fn run_deferred_caption(&mut self, outcome: Option<SendOutcome>) {
        let Some(outcome) = outcome else {
            return;
        };
        let (chat_id, old) = match outcome {
            SendOutcome::Succeeded { chat_id, old, .. } | SendOutcome::Failed { chat_id, old } => {
                (chat_id, old)
            }
        };
        let deferred = &mut self.session.messages.deferred_captions;
        let Some(at) = deferred
            .iter()
            .position(|d| d.chat_id == chat_id && d.message_id == old)
        else {
            return;
        };
        let deferred = deferred.remove(at);
        let SendOutcome::Succeeded { new, .. } = outcome else {
            return;
        };
        let Some(message) = self
            .session
            .histories
            .get_mut(&chat_id.0)
            .and_then(|history| history.messages.get_mut(&new.0))
        else {
            return;
        };
        let Some(mut edit) =
            ComposerEdit::from_own_content(chat_id, new, true, false, &message.content)
        else {
            return;
        };
        edit.caption_above = deferred.caption_above;
        // The row shows the new caption while the edit is on its way.
        set_caption(&mut message.content, &deferred.text);
        if edit.original_text != deferred.text {
            let _ = self.edit_snapshot(&edit, &deferred.text);
        }
    }
}
