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
                    self.search
                        .search
                        .set_first_page_offset(next_offset.clone());
                    self.search.search.accept_messages(hits, false);
                } else if self.search.search.matches_generation(pending)
                    && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchMessagesMore)
                {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    let hits = messages.iter().map(SearchMessageHit::from_parsed).collect();
                    self.search.search.append_page(hits, next_offset.clone());
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
                next_offset,
                are_limits_exceeded,
            } => {
                if self.search.search.matches_generation(pending)
                    && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchPublicPosts)
                {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    let hits = messages.iter().map(SearchMessageHit::from_parsed).collect();
                    self.search.search.public_limits_exceeded = are_limits_exceeded;
                    self.search.search.set_first_page_offset(next_offset);
                    self.search.search.accept_messages(hits, false);
                } else if self.search.search.matches_generation(pending)
                    && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchPublicPostsMore)
                {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    let hits = messages.iter().map(SearchMessageHit::from_parsed).collect();
                    self.search.search.public_limits_exceeded = are_limits_exceeded;
                    self.search.search.append_page(hits, next_offset);
                }
            }
            SearchPayload::PublicPostLimits {
                daily_free,
                remaining_free,
                next_free_in,
                star_count,
                current_query_free,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPublicPostSearchLimits) {
                    self.search.search.tabs.limits = Some(PublicPostLimits {
                        daily_free,
                        remaining_free,
                        next_free_in,
                        star_count,
                        current_query_free,
                    });
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
