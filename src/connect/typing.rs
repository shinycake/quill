//! Connect driver: typing and recording indicators.
use super::*;
use crate::ids::TopicId;
use crate::ids::{ChatId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::requests::send_chat_action_in;

impl<S: JsonSender> ConnectDriver<S> {
    /// Send a prebuilt request JSON; roll the reserved `@extra` back when
    /// the sender refuses it.
    pub(crate) fn send_json_request(
        &mut self,
        extra: RequestId,
        json: &str,
    ) -> Result<RequestId, ConnectSendError> {
        match self.sender.send_json(json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// While the voice bar is recording, send `chatActionRecordingVoiceNote`
    /// at most every [`OUTGOING_TYPING_INTERVAL_MS`] (Unigram record action).
    /// Stopping sends `chatActionCancel`.
    ///
    /// `chat` is the chat the recording belongs to (its window's chat);
    /// `None` is the main window, whose chat is the open one.
    pub fn sync_voice_recording(
        &mut self,
        chat: Option<ChatId>,
        active: bool,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        self.sync_record_action(chat, active, now_ms, "chatActionRecordingVoiceNote")
    }

    /// MED2: same as [`Self::sync_voice_recording`] for round video-note
    /// capture (`chatActionRecordingVideoNote`, schema 1.8.67 line 6392).
    pub fn sync_video_note_recording(
        &mut self,
        chat: Option<ChatId>,
        active: bool,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        self.sync_record_action(chat, active, now_ms, "chatActionRecordingVideoNote")
    }

    fn sync_record_action(
        &mut self,
        chat: Option<ChatId>,
        active: bool,
        now_ms: u64,
        action: &str,
    ) -> Result<(), ConnectSendError> {
        let chat_id = chat
            .or(self.session.open_chat)
            .filter(|id| self.typing_chat_allowed(*id));
        let Some(chat_id) = chat_id.filter(|_| active) else {
            return self.cancel_outgoing_voice();
        };
        let _ = self.cancel_outgoing_typing();
        let topic = self.typing_topic(chat_id);
        if let Some(prev) = &self.outgoing_voice {
            if (prev.chat_id, prev.topic) != (chat_id, topic) {
                self.cancel_outgoing_voice()?;
            } else if now_ms.saturating_sub(prev.last_sent_ms) < OUTGOING_TYPING_INTERVAL_MS {
                return Ok(());
            }
        }
        self.send_voice_action(chat_id, topic, true, now_ms, action)
    }

    /// Where an action in `chat_id` goes now: the open comment or reply
    /// thread (inside a forum topic, that topic), else the forum topic or
    /// direct messages conversation the composer is in, else the chat.
    fn typing_topic(&self, chat_id: ChatId) -> TopicId {
        if let Some((thread_id, _)) = self.session.thread_send_target(chat_id) {
            return match self.session.thread_forum_topic(chat_id) {
                Some(topic) => TopicId::forum(topic),
                None => TopicId::Thread {
                    message_thread_id: thread_id,
                },
            };
        }
        self.session.send_topic(chat_id).unwrap_or_default()
    }

    fn cancel_outgoing_voice(&mut self) -> Result<(), ConnectSendError> {
        let Some(prev) = self.outgoing_voice.take() else {
            return Ok(());
        };
        if !self.typing_chat_allowed(prev.chat_id) {
            return Ok(());
        }
        self.send_voice_action(
            prev.chat_id,
            prev.topic,
            false,
            0,
            "chatActionRecordingVoiceNote",
        )
    }

    fn send_voice_action(
        &mut self,
        chat_id: ChatId,
        topic: TopicId,
        recording: bool,
        now_ms: u64,
        action: &str,
    ) -> Result<(), ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::SendChatAction, Some(chat_id));
        let json = send_chat_action_in(
            extra,
            chat_id,
            topic,
            if recording {
                action
            } else {
                "chatActionCancel"
            },
        );
        match self.sender.send_json(&json) {
            Ok(()) => {
                self.outgoing_voice = recording.then_some(OutgoingTyping {
                    chat_id,
                    topic,
                    last_sent_ms: now_ms,
                });
                Ok(())
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Composer activity → `sendChatAction`.
    ///
    /// Unigram `ChatTextBox.OnTextChanged` calls `SetTyping(ChatActionTyping)`
    /// while the field is non-empty, at most every 4s. tdesktop
    /// `HistoryWidget::fieldChanged` does the same only when the field has
    /// sendable text and the user is not editing. Empty text, edit mode, send,
    /// and leaving the chat send `chatActionCancel` (Unigram `CancelTyping`;
    /// tdesktop stops the action with progress `-1` on chat close and on send).
    pub fn sync_outgoing_typing(
        &mut self,
        text: &str,
        editing: bool,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        let composing = !editing && !text.trim().is_empty();
        let chat_id = self
            .session
            .open_chat
            .filter(|id| self.typing_chat_allowed(*id));
        let Some(chat_id) = chat_id.filter(|_| composing) else {
            return self.cancel_outgoing_typing();
        };
        let topic = self.typing_topic(chat_id);
        if let Some(prev) = &self.outgoing_typing {
            if (prev.chat_id, prev.topic) != (chat_id, topic) {
                self.cancel_outgoing_typing()?;
            } else if now_ms.saturating_sub(prev.last_sent_ms) < OUTGOING_TYPING_INTERVAL_MS {
                return Ok(());
            }
        }
        self.send_outgoing_action(chat_id, topic, true, now_ms)
    }

    fn typing_chat_allowed(&self, chat_id: ChatId) -> bool {
        self.chats_path_active()
            && self
                .session
                .chats
                .get(&chat_id.0)
                .is_some_and(|chat| chat.supported())
    }

    pub(crate) fn cancel_outgoing_typing(&mut self) -> Result<(), ConnectSendError> {
        let Some(prev) = self.outgoing_typing.take() else {
            return Ok(());
        };
        if !self.typing_chat_allowed(prev.chat_id) {
            return Ok(());
        }
        self.send_outgoing_action(prev.chat_id, prev.topic, false, 0)
    }

    fn send_outgoing_action(
        &mut self,
        chat_id: ChatId,
        topic: TopicId,
        typing: bool,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::SendChatAction, Some(chat_id));
        let action = if typing {
            "chatActionTyping"
        } else {
            "chatActionCancel"
        };
        let json = send_chat_action_in(extra, chat_id, topic, action);
        match self.sender.send_json(&json) {
            Ok(()) => {
                self.outgoing_typing = typing.then_some(OutgoingTyping {
                    chat_id,
                    topic,
                    last_sent_ms: now_ms,
                });
                Ok(())
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
}
