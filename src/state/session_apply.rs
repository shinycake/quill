//! Payload dispatcher: routes every envelope payload to its handler.
use super::*;

impl Session {
    /// `request` came back (answer or error) or was lost: every stage that
    /// holds it as out lets it go.
    pub(crate) fn settle_held(&mut self, request: RequestId) {
        self.settings.quick_replies.sent.settle(request);
        self.stories.page_asks.settle(request);
        self.calls.creates.settle(request);
        self.stickers.edits.settle(request);
        self.bots.web_apps.answers_due.settle(request);
        self.communities.sent.settle(request);
    }

    pub fn apply(&mut self, owned: OwnedEnvelope) {
        self.revision = self.revision.wrapping_add(1);
        self.note_unread_inputs(&owned.envelope.payload);
        let local = owned.seq == crate::telegram::client::LOCAL_ANSWER_SEQ;
        if !local && owned.seq <= self.last_seq && self.last_seq != 0 {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some(owned.envelope.type_name.clone()),
                extra: owned.envelope.extra.map(|id| id.0),
                seq: Some(owned.seq),
                note: "out-of-order-ignored",
            });
            // The answer is dropped, but what asked for it must not wait
            // for good (a download's count, a mutation's spinner): it is
            // settled as lost, the way a swept request is.
            if let Some(pending) = owned.envelope.extra.and_then(|id| self.requests.take(id))
                && pending.account_generation == self.account_generation
            {
                self.acks.settle(
                    pending.id,
                    Answer::Failed(
                        crate::lang::tr(crate::lang::keys::Q_OPS_ERR_ANSWER_OUT_OF_ORDER)
                            .to_string(),
                    ),
                );
                self.settle_lost_request(pending);
            }
            return;
        }
        self.expire_pending_bot_messages(unix_ms_now());
        if !local {
            self.last_seq = owned.seq;
        }
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
            // The surface awaiting it must not wait for good: the answer
            // belongs to an account that is gone, so it did not take.
            self.acks.settle(
                pending.id,
                Answer::Failed("The account changed before TDLib answered".into()),
            );
            self.settle_held(pending.id);
            return;
        }
        // A box that waits on this request saves on its answer, not on the
        // request merely having left flight.
        if let Some(id) = extra
            && pending.is_some()
            && !matches!(owned.envelope.payload, EnvelopePayload::Error(_))
        {
            self.users_state.saves.answered(id);
        }
        if let Some(pending) = pending.as_ref() {
            let answer = match &owned.envelope.payload {
                EnvelopePayload::Error(err) => Answer::Failed(error_reason(err)),
                _ => Answer::Done,
            };
            self.acks.settle(pending.id, answer);
            self.settle_held(pending.id);
        }
        self.apply_payload_in_asking_view(
            owned.envelope.payload,
            pending.as_ref(),
            extra,
            owned.seq,
        );
    }

    pub(crate) fn apply_payload(
        &mut self,
        payload: EnvelopePayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        // A box waiting on this request takes its end from here.
        self.chat_list.dialog_answers.note(pending, &payload);
        self.settle_composer_link_preview(pending, &payload);
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
            EnvelopePayload::Communities(payload) => {
                self.apply_communities_payload(payload, pending, extra, seq)
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
            .and_then(|chat| chat.last_message_mut())
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
        chat.last =
            message.map(|message| LastPreview::from_message(message, &chat.title, service_preview));
    }
}

impl LastPreview {
    /// The row's view of `message`, the one place a last message becomes a
    /// preview. `service_preview` is the wording of a service message.
    pub(crate) fn from_message(
        message: &ParsedMessage,
        chat_title: &str,
        service_preview: Option<String>,
    ) -> Self {
        let content = effective_content(&message.content, message.ephemeral.as_ref());
        let text = service_preview.unwrap_or_else(|| content.preview());
        let style = preview_style(content, &text);
        let thumb = match content {
            MessageContent::Photo(photo) if !photo.is_secret && !photo.has_spoiler => photo
                .minithumbnail
                .clone()
                .filter(|mini| !mini.data.is_empty())
                .map(std::sync::Arc::new),
            _ => None,
        };
        let sender = if style.service {
            String::new()
        } else {
            preview_sender_name(
                message.is_outgoing,
                message.author_signature.as_deref(),
                chat_title,
            )
        };
        Self {
            text,
            style,
            sender,
            thumb,
            message: ChatLastMessage {
                id: message.id,
                date: message.date,
                is_outgoing: message.is_outgoing,
                sender: message.sender,
                send_state: message.send_state,
            },
        }
    }

    /// The last message's content was edited: the text and its style
    /// follow, the message's identity and thumbnail stay.
    pub(crate) fn restyle(&mut self, text: String, style: ChatPreviewStyle) {
        self.text = text;
        self.style = style;
    }
}
