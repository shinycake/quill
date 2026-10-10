//! Payload dispatcher: routes every envelope payload to its handler.
use super::*;

impl Session {
    pub fn apply(&mut self, owned: OwnedEnvelope) {
        self.revision = self.revision.wrapping_add(1);
        if owned.seq <= self.last_seq && self.last_seq != 0 {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some(owned.envelope.type_name.clone()),
                extra: owned.envelope.extra.map(|id| id.0),
                seq: Some(owned.seq),
                note: "out-of-order-ignored",
            });
            return;
        }
        self.expire_pending_bot_messages(unix_ms_now());
        self.last_seq = owned.seq;
        let extra = owned.envelope.extra;
        let pending = extra.and_then(|id| self.requests.take(id));
        if let Some(pending) = pending.as_ref()
            && pending.account_generation != self.account_generation
        {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some(owned.envelope.type_name.clone()),
                extra: Some(pending.id.0),
                seq: Some(owned.seq),
                note: "stale-account-generation",
            });
            return;
        }
        self.apply_payload(owned.envelope.payload, pending.as_ref(), extra, owned.seq);
    }

    pub(crate) fn apply_payload(
        &mut self,
        payload: EnvelopePayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        match payload {
            // Each domain applies its own payloads
            // (`src/state/domains/<domain>/apply.rs`).
            EnvelopePayload::Auth(payload) => self.apply_auth_payload(payload, pending, extra, seq),
            EnvelopePayload::Bots(payload) => self.apply_bots_payload(payload, pending, extra, seq),
            EnvelopePayload::Calls(payload) => {
                self.apply_calls_payload(payload, pending, extra, seq)
            }
            EnvelopePayload::ChatList(payload) => {
                self.apply_chat_list_payload(payload, pending, extra, seq)
            }
            EnvelopePayload::Chats(payload) => {
                self.apply_chats_payload(payload, pending, extra, seq)
            }
            EnvelopePayload::Common(payload) => {
                self.apply_common_payload(payload, pending, extra, seq)
            }
            EnvelopePayload::Groups(payload) => {
                self.apply_groups_payload(payload, pending, extra, seq)
            }
            EnvelopePayload::Media(payload) => {
                self.apply_media_payload(payload, pending, extra, seq)
            }
            EnvelopePayload::Messages(payload) => {
                self.apply_messages_payload(payload, pending, extra, seq)
            }
            EnvelopePayload::Payments(payload) => {
                self.apply_payments_payload(payload, pending, extra, seq)
            }
            EnvelopePayload::Search(payload) => {
                self.apply_search_payload(payload, pending, extra, seq)
            }
            EnvelopePayload::Settings(payload) => {
                self.apply_settings_payload(payload, pending, extra, seq)
            }
            EnvelopePayload::Stickers(payload) => {
                self.apply_stickers_payload(payload, pending, extra, seq)
            }
            EnvelopePayload::Stories(payload) => {
                self.apply_stories_payload(payload, pending, extra, seq)
            }
            EnvelopePayload::Threads(payload) => {
                self.apply_threads_payload(payload, pending, extra, seq)
            }
            EnvelopePayload::Users(payload) => {
                self.apply_users_payload(payload, pending, extra, seq)
            }
            EnvelopePayload::Ok => self.apply_ok(pending, extra, seq),
            EnvelopePayload::Error(err) => self.apply_error(err, pending, extra, seq),
            EnvelopePayload::Unknown(kind) => {
                self.diagnostics.record(Diagnostic {
                    category: "reducer",
                    type_name: Some(kind.type_name),
                    extra: pending.map(|p| p.id.0),
                    seq: Some(seq),
                    note: "unknown-variant",
                });
            }
        }
    }
}

impl Session {
    /// Set (or clear) a chat's last-message preview fields: preview text,
    /// its style inputs, the 3-line sender name, and the row's
    /// id/date/direction for the timestamp and receipt.
    /// A send resolved: if it was the chat's last message, the row's
    /// clock becomes a check (or a failed mark) without waiting for the
    /// next `updateChatLastMessage`.
    pub(crate) fn note_last_message_send_state(
        &mut self,
        chat_id: ChatId,
        old_id: MessageId,
        new_id: MessageId,
        state: crate::telegram::envelope::MessageSendState,
    ) {
        if let Some(last) = self
            .chats
            .get_mut(&chat_id.0)
            .and_then(|chat| chat.last_message.as_mut())
            .filter(|last| last.id == old_id || last.id == new_id)
        {
            last.id = new_id;
            last.send_state = state;
        }
    }

    pub(crate) fn set_chat_last_message(
        &mut self,
        chat_id: ChatId,
        message: Option<&ParsedMessage>,
    ) {
        // Service messages preview as their wording ("Dana pinned \"hi\""),
        // computed before the chat is borrowed mutably.
        let service_preview = message.and_then(|message| {
            let content = effective_content(&message.content, message.ephemeral.as_ref());
            self.service_text_for(chat_id, content, message.sender, message.is_outgoing)
                .map(|text| text.plain())
        });
        let chat = self
            .chats
            .entry(chat_id.0)
            .or_insert_with(|| placeholder_chat(chat_id));
        if let Some(message) = message {
            let content = effective_content(&message.content, message.ephemeral.as_ref());
            chat.last_preview = service_preview.unwrap_or_else(|| content.preview());
            chat.last_preview_style = preview_style(content, &chat.last_preview);
            chat.last_preview_thumb = match content {
                MessageContent::Photo(photo) if !photo.is_secret && !photo.has_spoiler => photo
                    .minithumbnail
                    .clone()
                    .filter(|mini| !mini.data.is_empty())
                    .map(std::sync::Arc::new),
                _ => None,
            };
            chat.last_preview_sender = if chat.last_preview_style.service {
                String::new()
            } else {
                preview_sender_name(
                    message.is_outgoing,
                    message.author_signature.as_deref(),
                    &chat.title,
                )
            };
            chat.last_message = Some(ChatLastMessage {
                id: message.id,
                date: message.date,
                is_outgoing: message.is_outgoing,
                sender: message.sender,
                send_state: message.send_state,
            });
        } else {
            chat.last_preview = String::new();
            chat.last_preview_style = ChatPreviewStyle::default();
            chat.last_preview_thumb = None;
            chat.last_preview_sender = String::new();
            chat.last_message = None;
        }
    }
}
