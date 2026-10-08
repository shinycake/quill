//! Payload handlers: messages and stories.
use super::*;

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_messages(
        &mut self,
        messages: Vec<ParsedMessage>,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        seq: u64,
    ) {
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::SendMessageAlbum
        {
            for message in messages {
                self.upsert_message(message, true);
            }
            return;
        }
        // M1: scheduled sends go to the scheduled list, not history.
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::GetChatScheduledMessages
        {
            self.scheduled_messages = messages.to_vec();
            return;
        }
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::ForwardMessages
        {
            self.finish_forward(pending, &messages, false);
            return;
        }
        // Comment / reply thread page.
        if let Some(pending) = pending
            && matches!(
                pending.purpose,
                RequestPurpose::GetMessageThreadHistory { .. }
            )
        {
            self.apply_thread_history(messages, Some(pending));
            return;
        }
        // Slice CL: chat-list peek preview — cache the latest
        // messages for the previewed (unopened) chat.
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::GetChatPreview
            && let Some(chat_id) = pending.chat_id
        {
            self.chat_preview_fetch = Some(PreviewHistoryFetch {
                chat_id,
                messages: messages.to_vec(),
                failed: None,
            });
            return;
        }
        // `parity:platform-chat-export` — append the page to the export
        // buffer. A page that adds nothing means the server has no more
        // history. A merely short page does not: TDLib picks the page size
        // and "can be smaller than the specified limit" (schema 1.8.67,
        // line 11827) — its first answer is often the last message alone.
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::ExportChatHistory
            && let Some(chat_id) = pending.chat_id
        {
            if let Some(export) = self.chat_export.as_mut()
                && export.chat_id == chat_id
            {
                // `getChatHistory` is inclusive of `from_message_id`, so the
                // first message of every non-first page is the boundary
                // message already in the buffer — keep only messages older
                // than it (matched by id, not position, so a boundary
                // deleted between pages doesn't cost a message) so each
                // message exports exactly once.
                let boundary_id = export.messages.last().map(|m| m.id);
                let before = export.messages.len();
                for message in messages {
                    let exported = crate::chat_export::project_message(&message);
                    if boundary_id.is_some_and(|boundary| exported.id >= boundary) {
                        continue;
                    }
                    export.messages.push(exported);
                }
                export.in_flight = false;
                if export.messages.len() == before {
                    export.done_paging = true;
                }
            }
            return;
        }
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::GetHistoryAround
        {
            self.apply_history_around(pending, &messages, seq);
            return;
        }
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::GetHistoryNewer
        {
            self.apply_history_newer(pending, messages, seq);
            return;
        }
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::GetHistory
        {
            if self.take_stale_history_request(pending)
                || pending.view_generation != Some(self.view_generation)
            {
                self.diagnostics.record(Diagnostic {
                    category: "reducer",
                    type_name: Some("messages".into()),
                    extra: Some(pending.id.0),
                    seq: Some(seq),
                    note: "stale-view-generation",
                });
                return;
            }
            if let Some(chat_id) = pending.chat_id {
                if self.open_chat != Some(chat_id) {
                    self.diagnostics.record(Diagnostic {
                        category: "reducer",
                        type_name: Some("messages".into()),
                        extra: Some(pending.id.0),
                        seq: Some(seq),
                        note: "stale-chat-history",
                    });
                    return;
                }
                // TDLib may answer with fewer messages than asked ("the
                // number of returned messages is chosen by TDLib", schema
                // 1.8.67, line 11822) — that is not the end; the UI keeps
                // paging. The end is a page with nothing older than the
                // request's `from_message_id` (stamped by `fetch_history`):
                // empty (Telegram X `ListManager.processData`), or only the
                // boundary message itself, which the identical next request
                // would return again forever.
                let from = pending.around_message_id.map_or(0, |id| id.0);
                if !messages.iter().any(|m| from == 0 || m.id.0 < from) {
                    self.histories.entry(chat_id.0).or_default().loaded_complete = true;
                }
                for message in messages {
                    self.upsert_message(message, false);
                }
                self.refresh_history_has_newer(chat_id);
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_stories(
        &mut self,
        total_count: i32,
        stories: Vec<(ParsedStory, Vec<ParsedFile>)>,
        pinned_story_ids: Vec<i32>,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        // Phase 9.7: `getStoryAlbumStories` /
        // `getChatArchivedStories` /
        // `getChatPostedToChatPageStories` — stories are cached in
        // `Session::stories` (like `getStory`); the id lists
        // accumulate per purpose.
        for (story, files) in &stories {
            self.remember_files(files);
            self.stories
                .insert((story.poster_chat_id, story.id), story.clone());
        }
        let ids: Vec<i32> = stories.iter().map(|(story, _)| story.id).collect();
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::GetStoryAlbumStories) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && let Some(album_id) = pending.and_then(|p| p.story_album_id)
                {
                    let list = self
                        .story_album_stories
                        .entry((chat_id.0, album_id))
                        .or_default();
                    // Phase 9.7: preserve server order (the
                    // album's own order) — append new ids,
                    // dedupe, never re-sort.
                    for id in ids {
                        if !list.contains(&id) {
                            list.push(id);
                        }
                    }
                    self.clear_story_page_op(RequestPurpose::GetStoryAlbumStories);
                }
            }
            Some(RequestPurpose::GetChatArchivedStories) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    let entry = self.archived_stories.entry(chat_id.0).or_default();
                    // Phase 9.7: TDLib returns archive stories newest
                    // first (decreasing id); preserve server order
                    // across pages — append new ids, dedupe.
                    for id in ids.iter().copied() {
                        if !entry.story_ids.contains(&id) {
                            entry.story_ids.push(id);
                        }
                    }
                    entry.total_count = total_count;
                    entry.next_from_story_id = entry.story_ids.iter().copied().min();
                    self.clear_story_page_op(RequestPurpose::GetChatArchivedStories);
                }
            }
            Some(RequestPurpose::GetChatPostedToChatPageStories) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    let entry = self.chat_page_stories.entry(chat_id.0).or_default();
                    // Phase 9.7: preserve server order (newest
                    // first) across pages — append new ids, dedupe.
                    for id in ids.iter().copied() {
                        if !entry.story_ids.contains(&id) {
                            entry.story_ids.push(id);
                        }
                    }
                    entry.total_count = total_count;
                    if !pinned_story_ids.is_empty() {
                        entry.pinned_story_ids = pinned_story_ids;
                    }
                    self.clear_story_page_op(RequestPurpose::GetChatPostedToChatPageStories);
                }
            }
            _ => {}
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_found_chat_messages(
        &mut self,
        messages: Vec<ParsedMessage>,
        total_count: i32,
        next_from_message_id: MessageId,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetTopicHistory) {
            // Phase 5.1: per-topic history page. Correlated by chat +
            // topic; stored separately from the chat's general history.
            if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                && let Some(forum_topic_id) = pending.and_then(|p| p.forum_topic_id)
            {
                for message in &messages {
                    self.remember_files(&message.files);
                }
                let empty = messages.is_empty();
                let rows: Vec<HistoryMessage> = messages
                    .into_iter()
                    .map(|message| history_message(message, false))
                    .collect();
                for row in &rows {
                    self.index_poll(row);
                }
                let entry = self
                    .topic_histories
                    .entry((chat_id.0, forum_topic_id))
                    .or_default();
                for row in rows {
                    entry.messages.insert(row.id.0, row);
                }
                if next_from_message_id.0 == 0 || empty {
                    entry.loaded_complete = true;
                }
                entry.next_from_message_id = next_from_message_id;
                entry.total_count = total_count.max(entry.messages.len() as i32);
            }
            return;
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::JumpToUnread { .. })
        ) {
            if pending.and_then(|p| p.chat_id) == self.open_chat {
                self.unread_jump = oldest_message_id(messages.iter().map(|m| m.id));
            }
            return;
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPinnedMessages) {
            if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                for message in &messages {
                    self.remember_files(&message.files);
                }
                let mut rows: Vec<HistoryMessage> = messages
                    .into_iter()
                    .map(|message| history_message(message, false))
                    .collect();
                rows.sort_by_key(|row| std::cmp::Reverse(row.id.0));
                self.pinned_messages.insert(chat_id.0, rows);
            }
            return;
        }
        if self.chat_search.matches_generation(pending)
            && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchChatMessages)
        {
            for message in &messages {
                self.remember_files(&message.files);
            }
            let hits = messages.iter().map(SearchMessageHit::from_parsed).collect();
            self.chat_search
                .accept_hits(hits, total_count, next_from_message_id, false);
        }
        // Slice media-shared-gallery: one gallery-tab page.
        // Correlated by chat + tab + generation stamped on the
        // request purpose; late answers drop in `accept`.
        if let Some(RequestPurpose::GetSharedMedia { tab, generation }) = pending.map(|p| p.purpose)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            for message in &messages {
                self.remember_files(&message.files);
            }
            let items = messages
                .iter()
                .map(|message| SharedMediaItem::from_parsed(tab, message))
                .collect();
            self.shared_media.accept(
                chat_id,
                tab,
                generation,
                items,
                total_count,
                next_from_message_id,
            );
        }
        if let Some(RequestPurpose::GetSharedMediaMore { tab, generation }) =
            pending.map(|p| p.purpose)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            for message in &messages {
                self.remember_files(&message.files);
            }
            let items = messages
                .iter()
                .map(|message| SharedMediaItem::from_parsed(tab, message))
                .collect();
            self.shared_media.accept_more(
                chat_id,
                tab,
                generation,
                items,
                total_count,
                next_from_message_id,
            );
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_update_message_ephemeral_content(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        ephemeral: Option<EphemeralMessageContent>,
        _pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        // `parity:msg-ephemeral-updates` (schema 1.8.67 line 10424,
        // secret-chat lane): replace the stored ephemeral content in
        // place; the row re-renders via `effective_content` (ephemeral
        // wins) and the chat-list preview refreshes when it's the
        // last message.
        let updated = self.edit_loaded_message(chat_id, message_id, |message| {
            message.ephemeral = ephemeral.clone();
        });
        if updated
            && self.is_chat_last_message(chat_id, message_id)
            && let Some(chat) = self.chats.get_mut(&chat_id.0)
        {
            let styled = self
                .histories
                .get(&chat_id.0)
                .and_then(|history| history.messages.get(&message_id.0))
                .map(|message| {
                    let content = effective_content(&message.content, message.ephemeral.as_ref());
                    let preview = content.preview();
                    let style = preview_style(content, &preview);
                    (preview, style)
                });
            if let Some((preview, style)) = styled {
                chat.last_preview = preview;
                chat.last_preview_style = style;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_update_message_content(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        content: MessageContent,
        files: Vec<ParsedFile>,
    ) {
        self.remember_files(&files);
        let preview = content.preview();
        // Slice chatlist-list-style: style inputs for the new
        // content, before `content` moves into the history below.
        let style = preview_style(&content, &preview);
        // M1 fix-up: the edited message may be a scheduled send —
        // refresh the scheduled-list entry too, not just history.
        if let Some(slot) = self
            .scheduled_messages
            .iter_mut()
            .find(|m| m.chat_id == chat_id && m.id == message_id)
        {
            slot.content = content.clone();
        }
        let loaded = self.edit_loaded_message(chat_id, message_id, |message| {
            message.content = content.clone();
        });
        if loaded && let MessageContent::Poll(poll) = &content {
            self.poll_messages
                .entry(poll.poll.id)
                .or_default()
                .insert((chat_id.0, message_id.0));
        }
        // The row preview follows the chat's last message, loaded or not
        // (Telegram X `TGChat.updateMessageContent`). The newest loaded
        // row is not it after a jump, and while TDLib reports the last
        // message as unknown there is nothing to refresh.
        if self.is_chat_last_message(chat_id, message_id)
            && let Some(chat) = self.chats.get_mut(&chat_id.0)
        {
            chat.last_preview = preview;
            chat.last_preview_style = style;
        }
    }

    /// Whether `message_id` is the chat's `last_message` as TDLib last
    /// reported it (`updateChatLastMessage`).
    fn is_chat_last_message(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.chats
            .get(&chat_id.0)
            .and_then(|chat| chat.last_message.as_ref())
            .is_some_and(|last| last.id == message_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_update_new_message(&mut self, message: ParsedMessage) {
        if !message.is_outgoing {
            self.pending_bot_messages
                .remove(&(message.chat_id.0, message.topic_id.unwrap_or(0)));
        }
        // Phase 8.1: decide before upserting; the queue is drained by
        // the UI for OS dispatch. The sound decision is made at the
        // same moment (parity slice: notification sounds).
        let notification = self.notification_for_new_message(&message);
        let sound = notification.as_ref().and_then(|_| {
            self.chats
                .get(&message.chat_id.0)
                .and_then(|chat| self.notification_sound_for(chat))
        });
        // B1: an incoming message demanding a reply (force-reply
        // markup) arms the composer's reply-to; the UI drains
        // `pending_force_reply` on the next render. Computed before
        // `upsert_message` moves `message`.
        let force_reply = (!message.is_outgoing
            && message
                .reply_markup
                .as_ref()
                .is_some_and(reply_markup_demands_reply))
        .then_some(ForceReplyTarget {
            chat_id: message.chat_id,
            message_id: message.id,
        });
        // The open comment / reply thread shows its own replies.
        if self.thread_accepts(&message) {
            self.remember_files(&message.files);
            let row = history_message(message.clone(), false);
            self.thread_upsert(row);
        }
        if self.route_new_message_into_window(&message) {
            self.upsert_message(message, false);
        } else {
            // Outside the loaded window: keep its files and any loaded
            // forum-topic copy, but leave the main history alone.
            self.remember_files(&message.files);
            if let Some(topic_id) = message.topic_id
                && self
                    .topic_histories
                    .contains_key(&(message.chat_id.0, topic_id))
            {
                let chat_id = message.chat_id;
                let row = history_message(message, false);
                if let Some(topic_history) = self.topic_histories.get_mut(&(chat_id.0, topic_id)) {
                    topic_history.upsert(row);
                }
            }
        }
        if let Some(target) = force_reply {
            self.pending_force_reply = Some(target);
        }
        if let Some(notification) = notification {
            self.queue_notification_with_sound(notification, sound);
        }
    }
}

impl Session {
    pub fn expire_pending_bot_messages(&mut self, now_ms: u64) -> bool {
        let before = self.pending_bot_messages.len();
        self.pending_bot_messages
            .retain(|_, pending| pending.expires_at_ms > now_ms);
        before != self.pending_bot_messages.len()
    }

    pub(crate) fn finish_pending_bot_stop(
        &mut self,
        chat_id: ChatId,
        topic_id: i32,
        draft_id: i64,
    ) {
        let key = (chat_id.0, topic_id);
        if let Some(pending) = self.pending_bot_messages.get_mut(&key)
            && pending.draft_id == draft_id
        {
            if pending.keep_on_stop {
                pending.can_stop = false;
                pending.stopped = true;
                pending.stop_failed = false;
            } else {
                self.pending_bot_messages.remove(&key);
            }
        }
    }
}
