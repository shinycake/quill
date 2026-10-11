//! Connect driver: deleting and forwarding messages.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// After UI confirm (tdesktop `DeleteMessagesBox`), send `deleteMessages`.
    /// `revoke: true` deletes for everyone (own outgoing default), false only
    /// for the current user (schema 1.8.67 line 12282).
    pub fn delete_confirmed(
        &mut self,
        confirm: &DeleteConfirm,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&confirm.chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&confirm.chat_id.0)
            .and_then(|history| history.messages.get(&confirm.message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        // M1: any sent message may be deleted; the for-everyone toggle is
        // only honored for own outgoing (schema 1.8.67 lines 6228–6229).
        if DeleteConfirm::for_message(
            message.chat_id,
            message.id,
            message.is_outgoing,
            message.pending,
        )
        .is_none()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        // For everyone only where the UI offered it: TDLib's
        // `can_be_deleted_for_all_users` (or, before that arrives, your own
        // messages). `can_revoke` carries that decision.
        let revoke = confirm.revoke && confirm.can_revoke;
        let extra = self
            .session
            .request(RequestPurpose::DeleteMessages, Some(confirm.chat_id));
        let json = delete_messages(extra, confirm.chat_id, &[confirm.message_id], revoke);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Delete a selection of messages (Telegram Desktop's "Delete N" in
    /// selection mode) with one `deleteMessages`. Every message must be
    /// loaded and deletable; `revoke` (delete for everyone) is only honored
    /// when they're all your own.
    pub fn delete_selected(
        &mut self,
        chat_id: ChatId,
        message_ids: &[MessageId],
        revoke: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || message_ids.is_empty() || message_ids.len() > 100 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        let history = self.session.histories.get(&chat_id.0);
        let mut all_outgoing = true;
        for id in message_ids {
            let Some(message) = history.and_then(|history| history.messages.get(&id.0)) else {
                return Err(ConnectSendError::InvalidRequest);
            };
            if DeleteConfirm::for_message(
                message.chat_id,
                message.id,
                message.is_outgoing,
                message.pending,
            )
            .is_none()
            {
                return Err(ConnectSendError::InvalidRequest);
            }
            all_outgoing &= message.is_outgoing;
        }
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteMessages, Some(chat_id));
        let json = delete_messages(extra, chat_id, message_ids, revoke && all_outgoing);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Send `forwardMessages` after the dest picker chooses a supported chat.
    /// `send_copy: false` preserves official "Forwarded from" attribution.
    pub fn forward_messages(
        &mut self,
        dest: ChatId,
        draft: &ForwardDraft,
    ) -> Result<RequestId, ConnectSendError> {
        self.forward_messages_with_options(dest, draft, &SendOptions::default())
    }

    /// `forwardMessages` with the share box's silent / scheduled options.
    pub fn forward_messages_with_options(
        &mut self,
        dest: ChatId,
        draft: &ForwardDraft,
        options: &SendOptions,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if draft.is_empty() || draft.message_ids.len() > 100 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let dest_ok = self
            .session
            .chats
            .get(&dest.0)
            .is_some_and(|chat| chat.supported());
        let from_ok = self
            .session
            .chats
            .get(&draft.from_chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !dest_ok || !from_ok {
            return Err(ConnectSendError::InvalidRequest);
        }
        for id in &draft.message_ids {
            let Some(message) = self
                .session
                .histories
                .get(&draft.from_chat_id.0)
                .and_then(|history| history.messages.get(&id.0))
            else {
                return Err(ConnectSendError::InvalidRequest);
            };
            if message.pending || id.0 <= 0 {
                return Err(ConnectSendError::InvalidRequest);
            }
        }
        let extra = self
            .session
            .request(RequestPurpose::ForwardMessages, Some(dest));
        let flight = ForwardFlight {
            extra,
            dest_chat_id: dest,
            from_chat_id: draft.from_chat_id,
            requested: draft.message_ids.len(),
        };
        // Several destinations (share box) are in flight at once: the first
        // takes the main slot, the rest queue behind it.
        let queued = self.session.messages.in_flight_forward.is_some();
        if queued {
            self.session.messages.queued_forward_flights.push(flight);
        } else {
            self.session.messages.in_flight_forward = Some(flight);
        }
        let json = forward_messages_with_options(
            extra,
            dest,
            draft.from_chat_id,
            &draft.message_ids,
            draft.send_copy,
            draft.remove_caption,
            options,
        );
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                if queued {
                    self.session
                        .messages
                        .queued_forward_flights
                        .retain(|f| f.extra != extra);
                } else {
                    self.session.messages.in_flight_forward = None;
                }
                Err(err)
            }
        }
    }
}
