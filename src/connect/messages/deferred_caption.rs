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
                m.is_outgoing && m.pending() && !m.failed() && has_editable_caption(&m.content)
            });
        if !pending_media {
            return Err(ConnectSendError::InvalidRequest);
        }
        let caption = text.trim();
        self.check_caption_length(caption)?;
        let shown = self
            .session
            .histories
            .get(&edit.chat_id.0)
            .and_then(|history| history.messages.get(&edit.message_id.0))
            .and_then(|message| ComposerEdit::editable_text(&message.content))
            .map(|(_, text)| text)
            .unwrap_or_default();
        let deferred = &mut self.session.messages.deferred_captions;
        // A second choice keeps the caption from before the first.
        let previous = deferred
            .iter()
            .find(|d| d.chat_id == edit.chat_id && d.message_id == edit.message_id)
            .map_or(shown, |d| d.previous.clone());
        deferred.retain(|d| d.chat_id != edit.chat_id || d.message_id != edit.message_id);
        deferred.push(DeferredCaption {
            chat_id: edit.chat_id,
            message_id: edit.message_id,
            text: caption.to_string(),
            caption_above: edit.caption_above,
            previous,
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
        // A caption whose message left the history without a send outcome
        // (the chat was deleted, the account state reset) has nothing to
        // wait for.
        // (The reducer already replaced the row of the message this outcome
        // is about, so that entry stays for the code below.)
        let ended = outcome.map(|outcome| match outcome {
            SendOutcome::Succeeded { chat_id, old, .. } | SendOutcome::Failed { chat_id, old } => {
                (chat_id, old)
            }
        });
        let histories = &self.session.histories;
        self.session.messages.deferred_captions.retain(|d| {
            ended == Some((d.chat_id, d.message_id))
                || histories
                    .get(&d.chat_id.0)
                    .is_some_and(|history| history.messages.contains_key(&d.message_id.0))
        });
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
        let sent_text = ComposerEdit::editable_text(&message.content).map(|(_, text)| text);
        // The row shows the new caption while the edit is on its way.
        set_caption(&mut message.content, &deferred.text);
        if sent_text.as_deref() != Some(deferred.text.as_str())
            && self.edit_snapshot(&edit, &deferred.text).is_err()
        {
            // The server never got the caption: the row shows the one it
            // has, and the user is told.
            if let Some(message) = self
                .session
                .histories
                .get_mut(&chat_id.0)
                .and_then(|history| history.messages.get_mut(&new.0))
            {
                set_caption(&mut message.content, &deferred.previous);
            }
            self.session
                .messages
                .message_action_note
                .push("could not change the caption".into());
        }
    }
}
