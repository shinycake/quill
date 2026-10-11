//! Applies TDLib updates and answers for global and in-chat search, top chats and date jumps.
use crate::state::*;
use crate::telegram::envelope::SearchPayload;

impl Session {
    /// Applies one search payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_search_payload(
        &mut self,
        payload: SearchPayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        match payload {
            SearchPayload::FoundMessages {
                messages,
                next_offset,
                ..
            } => {
                if self.search.search.matches_generation(pending)
                    && matches!(
                        pending.map(|p| p.purpose),
                        Some(
                            RequestPurpose::SearchMessages
                                | RequestPurpose::SearchPublicMessagesByTag
                        )
                    )
                {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    let hits = messages.iter().map(SearchMessageHit::from_parsed).collect();
                    self.search.search.accept_messages(hits, false);
                }
                // Phase C2i: `searchCallMessages` pages for the
                // Recent-calls tab. `searchCallMessages` returns call and
                // group-call messages newest-first; the rows render both.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::SearchCallMessages) {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    self.calls.recent_calls.extend(messages);
                    self.calls.recent_calls_offset = next_offset;
                    self.calls.recent_calls_loading = false;
                    self.calls.recent_calls_error = false;
                }
            }
            // `searchPublicPosts` answer: posts of public channels; an
            // exhausted free quota is flagged, never paid for.
            SearchPayload::FoundPublicPosts {
                messages,
                are_limits_exceeded,
                ..
            } => {
                if self.search.search.matches_generation(pending)
                    && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchPublicPosts)
                {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    let hits = messages.iter().map(SearchMessageHit::from_parsed).collect();
                    self.search.search.public_limits_exceeded = are_limits_exceeded;
                    self.search.search.accept_messages(hits, false);
                }
            }
            SearchPayload::MessageCalendar { days, .. } => {
                self.apply_message_calendar(days, pending);
            }
            SearchPayload::FoundChatMessages {
                messages,
                total_count,
                next_from_message_id,
            } => self.apply_found_chat_messages(
                messages,
                total_count,
                next_from_message_id,
                pending,
                extra,
                seq,
            ),
        }
    }
}
