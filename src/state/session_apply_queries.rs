//! Payload handlers: inline queries, polls, call ids.
use super::*;

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_inline_query_results(
        &mut self,
        page: InlineQueryResultsPage,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        if let Some(RequestPurpose::GetInlineQueryResults {
            chat_id,
            bot_user_id,
            first_page,
        }) = pending.map(|p| p.purpose)
        {
            let slot_ok = match (first_page, self.inline_query.as_ref()) {
                (true, Some(slot)) => {
                    slot.chat_id == chat_id
                        && slot.bot_user_id == bot_user_id
                        && matches!(slot.fetch, InlineQueryFetch::Loading)
                }
                (false, Some(slot)) => {
                    slot.chat_id == chat_id
                        && slot.bot_user_id == bot_user_id
                        && matches!(slot.fetch, InlineQueryFetch::Loaded { .. })
                }
                _ => false,
            };
            if slot_ok {
                let fetch = match (first_page, &self.inline_query) {
                    (
                        false,
                        Some(InlineQuerySlot {
                            fetch: InlineQueryFetch::Loaded { results: old, .. },
                            ..
                        }),
                    ) => {
                        let mut results = old.clone();
                        for result in page.results {
                            if !results.iter().any(|r| r.id == result.id) {
                                results.push(result);
                            }
                        }
                        InlineQueryFetch::Loaded {
                            inline_query_id: page.inline_query_id,
                            button: page.button,
                            results,
                            next_offset: page.next_offset,
                        }
                    }
                    _ => InlineQueryFetch::Loaded {
                        inline_query_id: page.inline_query_id,
                        button: page.button,
                        results: page.results,
                        next_offset: page.next_offset,
                    },
                };
                if let Some(slot) = self.inline_query.as_mut() {
                    slot.fetch = fetch;
                }
            }
        } else if let Some(RequestPurpose::GetGifSearchResults { first_page, .. }) =
            pending.map(|p| p.purpose)
        {
            // Slice S9: GIF-panel search — the
            // `inlineQueryResultAnimation` entries land in `GifPanel`,
            // never the composer's `inline_query` slot. First page
            // replaces; later pages append (deduped); `next_offset`
            // pages the bot's result list.
            self.remember_files(&page.files);
            self.accept_gif_search_results(page.animations, page.next_offset, first_page);
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_poll_voters(
        &mut self,
        total_count: i32,
        voters: Vec<MessageSender>,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        if let Some(RequestPurpose::GetPollVoters {
            chat_id,
            message_id,
            option_id,
            offset,
        }) = pending.map(|p| p.purpose)
        {
            let key = (chat_id.0, message_id.0, option_id);
            let page_len = voters.len();
            let merged = if offset == 0 {
                voters
            } else {
                match self.poll_voters.get(&key) {
                    Some(PollVotersFetch::Loaded { voters: old, .. }) => {
                        let mut merged = old.clone();
                        for voter in voters {
                            if !merged.contains(&voter) {
                                merged.push(voter);
                            }
                        }
                        merged
                    }
                    _ => voters,
                }
            };
            // B4: a short page is the honest exhaustion signal —
            // `total_count` is approximate per the schema, so on a
            // short page (limit is 50, schema line 12941) clamp it
            // to what we actually hold; the UI hides "Load more"
            // when `voters.len() >= total_count`.
            let total_count = if page_len < 50 {
                merged.len() as i32
            } else {
                total_count
            };
            self.poll_voters.insert(
                key,
                PollVotersFetch::Loaded {
                    voters: merged,
                    total_count,
                },
            );
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_call_id(
        &mut self,
        id: i32,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        if let Some(pending) = pending
            && let RequestPurpose::CreateCall { is_video } = pending.purpose
            && let Some(user_id) = pending.user_id
            && self.active_call.is_none()
        {
            self.active_call = Some(ActiveCall {
                id,
                user_id,
                is_outgoing: true,
                // Phase C1b: the `callId` answer carries no
                // `is_video` (schema 1.8.67, :7034), so it is
                // derived from the `createCall` request args
                // stashed in the request purpose.
                is_video,
                muted: false,
                camera_on: is_video,
                screen_sharing: false,
                remote_video: RemoteVideoState::Inactive,
                remote_screen: RemoteVideoState::Inactive,
                state: CallState::Pending {
                    is_created: true,
                    is_received: false,
                },
                started_at: Instant::now(),
                ready_at: None,
                ready: None,
                transport: None,
                transport_error: None,
                signaling_queue: Vec::new(),
                signaling_dropped: 0,
            });
            self.call_summary = None;
            self.call_error = None;
        }
    }
}
