//! Connect driver: editing sent and scheduled messages, resending failed ones.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Save an own-message edit via `editMessageText` or `editMessageCaption`.
    pub fn edit_snapshot(
        &mut self,
        edit: &ComposerEdit,
        text: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&edit.chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        // M1: scheduled sends live in `session.messages.scheduled_messages`
        // (`ParsedMessage`, never pending), not in history
        // (`HistoryMessage`) — same `editMessageText` request, different
        // validation source.
        let owned = if edit.scheduled {
            self.session
                .messages
                .scheduled_messages
                .iter()
                .find(|m| m.chat_id == edit.chat_id && m.id == edit.message_id)
                .map(|m| (m.chat_id, m.id, m.is_outgoing, false, &m.content))
        } else {
            self.session
                .histories
                .get(&edit.chat_id.0)
                .and_then(|history| history.messages.get(&edit.message_id.0))
                .map(|m| (m.chat_id, m.id, m.is_outgoing, m.pending, &m.content))
        };
        let Some((chat_id, message_id, is_outgoing, pending, content)) = owned else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if ComposerEdit::from_own_content(chat_id, message_id, is_outgoing, pending, content)
            .is_none()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let caption = text.trim();
        if matches!(edit.kind, ComposerEditKind::Text) && caption.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // MED4: caption-length gate for caption edits (same runtime
        // option as media sends).
        if matches!(edit.kind, ComposerEditKind::Caption) {
            self.check_caption_length(caption)?;
        }
        // R8: text edits are bounded by `message_text_length_max`
        // (counted after markup parsing, in UTF-16 units).
        if matches!(edit.kind, ComposerEditKind::Text) {
            let limit = self.session.messages.message_text_length_max;
            if crate::text_split::units_over_limit(caption, limit) > 0 {
                return Err(ConnectSendError::TextTooLong { limit });
            }
        }
        // M1 fix-up: secret chats strip `textEntityTypeBlockQuote` from
        // the edited caption too (unsupported in secret chats).
        let strip_blockquote = self
            .session
            .chats
            .get(&edit.chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        // B5: a replacement file turns the caption edit into
        // `editMessageMedia` (tdesktop `EditCaptionBox` with a prepared
        // list). Validate path and probe video before allocating `@extra`.
        let replacement = match edit.media_edit.replacement.as_ref() {
            Some(_) if !edit.allows_replace() => return Err(ConnectSendError::InvalidRequest),
            other => other,
        };
        let mut replacement_content = None;
        if let Some(rep) = replacement {
            let path = rep
                .send_path_str()
                .ok_or(ConnectSendError::InvalidRequest)?;
            let video = if rep.kind == EditMediaKind::Video {
                let probe = crate::video::probe_local_video(&rep.path)
                    .map_err(|_| ConnectSendError::InvalidRequest)?;
                Some(VideoSend {
                    duration: probe.duration,
                    width: probe.width,
                    height: probe.height,
                    supports_streaming: probe.supports_streaming,
                    self_destruct: None,
                })
            } else {
                None
            };
            replacement_content = Some(
                edit_media_content(
                    rep,
                    &path,
                    caption,
                    edit.caption_above && !strip_blockquote,
                    video.as_ref(),
                    strip_blockquote,
                )
                .ok_or(ConnectSendError::InvalidRequest)?,
            );
        }
        let extra = self
            .session
            .request(RequestPurpose::EditMessage, Some(edit.chat_id));
        let replacement_json = replacement_content
            .map(|content| edit_message_media(extra, edit.chat_id, edit.message_id, content));
        let json = match edit.kind {
            ComposerEditKind::Caption if replacement_json.is_some() => {
                replacement_json.unwrap_or_default()
            }
            ComposerEditKind::Text => edit_message_text(
                extra,
                edit.chat_id,
                edit.message_id,
                caption,
                strip_blockquote,
                // Secret chats never get previews (same rule as sends).
                &LinkPreviewChoice {
                    disabled: edit.link_preview.disabled || strip_blockquote,
                    ..edit.link_preview
                },
            ),
            ComposerEditKind::Caption => edit_message_caption(
                extra,
                edit.chat_id,
                edit.message_id,
                caption,
                // MED4 review nit: secret chats force caption-below on send
                // too (TGX `allowShowCaptionAboveMedia`).
                edit.caption_above && !strip_blockquote,
                strip_blockquote,
            ),
        };
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    pub fn delete_scheduled_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // M1: delete a scheduled send. Scheduled messages live in
        // `session.messages.scheduled_messages`, not in history, so the
        // history-validated `delete_confirmed` can't take them. `revoke`
        // is always false (no for-everyone distinction before sending).
        let known = self
            .session
            .messages
            .scheduled_messages
            .iter()
            .any(|m| m.chat_id == chat_id && m.id == message_id);
        if !known {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteMessages, Some(chat_id));
        let json = delete_messages(extra, chat_id, &[message_id], false);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `editMessageSchedulingState`: reschedule a scheduled message, or send
    /// it now with `ComposerScheduling::None`. Only messages in the loaded
    /// scheduled list qualify.
    pub fn edit_scheduled_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        scheduling: ComposerScheduling,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let known = self
            .session
            .messages
            .scheduled_messages
            .iter()
            .any(|m| m.chat_id == chat_id && m.id == message_id);
        if !known {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Messages(MessagesPurpose::EditMessageSchedulingState {
                message_id,
                scheduling,
            }),
            Some(chat_id),
        );
        let json = edit_message_scheduling_state(extra, chat_id, message_id, scheduling);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// M1: retry a failed send (`resendMessages`, TDLib 1.8.67,
    /// `schema/td_api.tl:12251`; `message.sending_state.can_retry`, schema
    /// line 3038). The driver only retries rows the reducer marked
    /// `failed` **and** retryable — not every failed send may be
    /// retried, and the context menu offers "Retry send" on the same
    /// gate.
    pub fn resend_failed_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_retry = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| message.failed && message.can_retry);
        if !can_retry {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ResendMessages, Some(chat_id));
        let json = resend_messages(extra, chat_id, &[message_id]);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// M1: load a chat's scheduled (pending) sends into
    /// `session.messages.scheduled_messages` (TDLib 1.8.67,
    /// `schema/td_api.tl:12000`).
    pub fn get_chat_scheduled_messages(
        &mut self,
        chat_id: ChatId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatScheduledMessages, Some(chat_id));
        let json = get_chat_scheduled_messages(extra, chat_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
}
