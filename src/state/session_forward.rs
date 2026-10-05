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
    /// marks) in the main history and any loaded topic history. Returns the
    /// number of main-history rows updated.
    pub fn apply_update_poll(&mut self, poll: Poll) -> usize {
        let Some(rows) = self.poll_messages.remove(&poll.id) else {
            return 0;
        };
        let mut updated = 0;
        let mut touched = Vec::new();
        let mut live = HashSet::new();
        for (chat_id, message_id) in rows {
            let replace = |message: &mut HistoryMessage| match &mut message.content {
                MessageContent::Poll(poll_content) if poll_content.poll.id == poll.id => {
                    poll_content.poll = poll.clone();
                    true
                }
                _ => false,
            };
            let mut found = false;
            if let Some(message) = self
                .histories
                .get_mut(&chat_id)
                .and_then(|history| history.messages.get_mut(&message_id))
                && replace(message)
            {
                touched.push((chat_id, message_id));
                updated += 1;
                found = true;
            }
            // The topic view reads only `topic_histories`.
            for ((topic_chat_id, _), topic) in self.topic_histories.iter_mut() {
                if *topic_chat_id == chat_id
                    && let Some(message) = topic.messages.get_mut(&message_id)
                    && replace(message)
                {
                    found = true;
                }
            }
            // Rows that left (deleted, edited away) drop out of the index.
            if found {
                live.insert((chat_id, message_id));
            }
        }
        if !live.is_empty() {
            self.poll_messages.insert(poll.id, live);
        }
        // N2: an open voter dialog goes stale when the poll updates
        // (counts/options change) — drop cached pages for touched
        // messages so the next open refetches.
        if !touched.is_empty() {
            self.poll_voters
                .retain(|(chat_id, message_id, _), _| !touched.contains(&(*chat_id, *message_id)));
        }
        updated
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
        self.local_search_chats(query)
            .into_iter()
            .filter(|chat| chat.supported())
            .collect()
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
        let flight = self
            .in_flight_forward
            .take()
            .filter(|flight| flight.extra == pending.id);
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
        self.last_forward = Some(ForwardResult {
            dest_chat_id,
            dest_title,
            from_chat_id: flight.as_ref().map(|f| f.from_chat_id).unwrap_or(ChatId(0)),
            requested: flight
                .as_ref()
                .map(|f| f.requested)
                .unwrap_or(forwarded.len()),
            forwarded_ids: forwarded.iter().map(|m| m.id).collect(),
        });
    }
}
