//! Reply, forward and poll helpers.
use super::*;

impl Session {
    /// Compact quote label: chosen `textQuote`, else the loaded original,
    /// else `messageReplyToMessage.content` preview.
    pub fn reply_quote_preview(&self, message: &HistoryMessage) -> Option<String> {
        let reply = message.reply_to.as_ref()?;
        Some(self.resolve_reply_preview(reply, message.chat_id))
    }

    /// Phase 4.2: apply `updatePoll` (schema 1.8.67 line 11179). The update
    /// carries no chat or message id, so the rows holding that poll are
    /// found through `poll_messages` (filled wherever a row is loaded) and
    /// the poll is replaced in place (vote counts, percentages, chosen
    /// marks) in every copy of the row: the main history, a loaded topic
    /// history, and every window's comment thread and Saved Messages
    /// views (`edit_loaded_message`). Returns the number of rows updated.
    pub fn apply_update_poll(&mut self, poll: Poll) -> usize {
        let Some(rows) = self.messages.poll_messages.remove(&poll.id) else {
            return 0;
        };
        let mut touched = Vec::new();
        for (chat_id, message_id) in rows {
            let mut replaced = false;
            self.edit_loaded_message(ChatId(chat_id), MessageId(message_id), |message| {
                if let MessageContent::Poll(poll_content) = &mut message.content
                    && poll_content.poll.id == poll.id
                {
                    poll_content.poll = poll.clone();
                    replaced = true;
                }
            });
            // Rows that left (deleted, edited away) drop out of the index.
            if replaced {
                touched.push((chat_id, message_id));
            }
        }
        if !touched.is_empty() {
            self.messages
                .poll_messages
                .insert(poll.id, touched.iter().copied().collect());
        }
        // N2: an open voter dialog goes stale when the poll updates
        // (counts/options change) — drop cached pages for touched
        // messages so the next open refetches.
        if !touched.is_empty() {
            self.messages
                .poll_voters
                .retain(|(chat_id, message_id, _), _| !touched.contains(&(*chat_id, *message_id)));
        }
        touched.len()
    }

    /// Set the chosen marks of a poll row in every copy of it (an
    /// optimistic vote, or its rollback). With `only_if`, a copy changes
    /// only while it still shows those marks: an update that came since
    /// tells the truth.
    pub(crate) fn set_poll_marks(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        marks: &[bool],
        only_if: Option<&[bool]>,
    ) {
        self.edit_loaded_message(chat_id, message_id, |message| {
            let MessageContent::Poll(content) = &mut message.content else {
                return;
            };
            let options = &mut content.poll.options;
            let current: Vec<bool> = options.iter().map(|option| option.is_chosen).collect();
            if only_if.is_some_and(|expected| expected != current.as_slice()) {
                return;
            }
            for (option, mark) in options.iter_mut().zip(marks) {
                option.is_chosen = *mark;
            }
        });
    }

    pub fn resolve_reply_preview(&self, reply: &MessageReplyTo, fallback_chat: ChatId) -> String {
        if let Some(quote) = reply
            .quote_text
            .as_ref()
            .map(|text| text.trim())
            .filter(|text| !text.is_empty())
        {
            return quote.to_string();
        }
        let chat_id = if reply.chat_id.0 != 0 {
            reply.chat_id
        } else {
            fallback_chat
        };
        if let Some(original) = self
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&reply.message_id.0))
        {
            return effective_preview(original);
        }
        reply
            .content_preview
            .clone()
            .unwrap_or_else(|| "Message".into())
    }

    /// Loaded Main-list destinations for the forward picker. Local title filter
    /// (tdesktop ShareBox search field). Unsupported kinds stay out.
    pub fn forward_destinations(&self, query: &str) -> Vec<&ChatSummary> {
        self.share_destinations(query)
    }

    /// Official "Forwarded from" label from `messageForwardInfo.origin`.
    pub fn forward_from_label(&self, info: &MessageForwardInfo) -> String {
        match &info.origin {
            MessageOrigin::HiddenUser { sender_name } if !sender_name.trim().is_empty() => {
                format!("Forwarded from {sender_name}")
            }
            MessageOrigin::User { user_id } => self
                .chats
                .values()
                .find(
                    |chat| matches!(chat.kind, ChatKind::Private { user_id: id } if id == *user_id),
                )
                .map(|chat| format!("Forwarded from {}", chat.title))
                .unwrap_or_else(|| "Forwarded message".into()),
            MessageOrigin::Chat {
                chat_id,
                author_signature,
            }
            | MessageOrigin::Channel {
                chat_id,
                author_signature,
                ..
            } => {
                if let Some(chat) = self.chats.get(&chat_id.0) {
                    if author_signature.is_empty() {
                        format!("Forwarded from {}", chat.title)
                    } else {
                        format!("Forwarded from {} ({author_signature})", chat.title)
                    }
                } else if !author_signature.is_empty() {
                    format!("Forwarded from {author_signature}")
                } else {
                    "Forwarded message".into()
                }
            }
            _ => "Forwarded message".into(),
        }
    }

    pub(crate) fn finish_forward(
        &mut self,
        pending: &PendingRequest,
        messages: &[ParsedMessage],
        failed: bool,
    ) {
        let forwarded: Vec<ParsedMessage> = if failed {
            Vec::new()
        } else {
            messages.to_vec()
        };
        for message in &forwarded {
            self.remember_files(&message.files);
            self.upsert_message(message.clone(), message.id.0 < 0);
        }
        let flight = match self.messages.in_flight_forward.take() {
            Some(flight) if flight.extra == pending.id => Some(flight),
            other => {
                self.messages.in_flight_forward = other;
                self.messages
                    .queued_forward_flights
                    .iter()
                    .position(|flight| flight.extra == pending.id)
                    .map(|index| self.messages.queued_forward_flights.remove(index))
            }
        };
        let dest_chat_id = flight
            .as_ref()
            .map(|f| f.dest_chat_id)
            .or(pending.chat_id)
            .unwrap_or(ChatId(0));
        let dest_title = self
            .chats
            .get(&dest_chat_id.0)
            .map(|chat| chat.title.clone())
            .unwrap_or_else(|| format!("chat {}", dest_chat_id.0));
        self.messages.last_forward = Some(ForwardResult {
            dest_chat_id,
            dest_title,
            from_chat_id: flight.as_ref().map(|f| f.from_chat_id).unwrap_or(ChatId(0)),
            from_topic: flight.as_ref().map(|f| f.from_topic).unwrap_or_default(),
            requested: flight
                .as_ref()
                .map(|f| f.requested)
                .unwrap_or(forwarded.len()),
            forwarded_ids: forwarded.iter().map(|m| m.id).collect(),
        });
    }
}
