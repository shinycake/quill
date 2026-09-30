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
        // Slice G2: channel-comments viewer — cache the thread
        // history for the requesting channel post.
        if let Some(pending) = pending
            && let RequestPurpose::GetMessageThreadHistory { message_id } = pending.purpose
            && let Some(chat_id) = pending.chat_id
        {
            self.comment_thread = Some(CommentThreadFetch {
                chat_id,
                message_id: MessageId(message_id),
                messages: messages.to_vec(),
                failed: None,
            });
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
        // buffer. A short page means the server has no more history.
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::ExportChatHistory
            && let Some(chat_id) = pending.chat_id
        {
            if let Some(export) = self.chat_export.as_mut()
                && export.chat_id == chat_id
            {
                let short_page = messages.len() < crate::chat_export::EXPORT_PAGE_LIMIT as usize;
                // `parity:platform-data-export` — queue the page's media
                // files for the bundle (deduped by file id; local files
                // are copied without a download).
                if let Some(dx) = self.data_export.as_mut() {
                    for message in &messages {
                        dx.enqueue_media_from(message, chat_id.0);
                    }
                }
                // `getChatHistory` is inclusive of `from_message_id`, so the
                // first message of every non-first page is the boundary
                // message already in the buffer — skip it (matched by id,
                // not position, so a boundary deleted between pages doesn't
                // cost a message) so each message exports exactly once.
                let boundary_id = export.messages.last().map(|m| m.id);
                for message in messages {
                    let exported = crate::chat_export::project_message(&message);
                    if Some(exported.id) == boundary_id {
                        continue;
                    }
                    export.messages.push(exported);
                }
                export.in_flight = false;
                if short_page {
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
            && pending.purpose == RequestPurpose::GetHistory
        {
            if pending.view_generation != Some(self.view_generation) {
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
                if messages.is_empty() {
                    self.histories.entry(chat_id.0).or_default().loaded_complete = true;
                }
                for message in messages {
                    self.upsert_message(message, false);
                }
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
                let entry = self
                    .topic_histories
                    .entry((chat_id.0, forum_topic_id))
                    .or_default();
                let empty = messages.is_empty();
                for message in messages {
                    entry
                        .messages
                        .insert(message.id.0, history_message(message, false));
                }
                if next_from_message_id.0 == 0 || empty {
                    entry.loaded_complete = true;
                }
                entry.next_from_message_id = next_from_message_id;
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
            self.shared_media
                .accept(chat_id, tab, generation, items, total_count);
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
        let updated = self
            .histories
            .get_mut(&chat_id.0)
            .is_some_and(|history| history.update_ephemeral(message_id, ephemeral));
        if updated {
            let is_last = self
                .histories
                .get(&chat_id.0)
                .and_then(|history| history.messages.keys().next_back().copied())
                == Some(message_id.0);
            if is_last && let Some(chat) = self.chats.get_mut(&chat_id.0) {
                let styled = self
                    .histories
                    .get(&chat_id.0)
                    .and_then(|history| history.messages.get(&message_id.0))
                    .map(|message| {
                        let content =
                            effective_content(&message.content, message.ephemeral.as_ref());
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
        let updated = self
            .histories
            .get_mut(&chat_id.0)
            .is_some_and(|history| history.update_content(message_id, content));
        if updated {
            let is_last = self
                .histories
                .get(&chat_id.0)
                .and_then(|history| history.messages.keys().next_back().copied())
                == Some(message_id.0);
            if is_last && let Some(chat) = self.chats.get_mut(&chat_id.0) {
                chat.last_preview = preview;
                chat.last_preview_style = style;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_update_new_message(&mut self, message: ParsedMessage) {
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
        self.upsert_message(message, false);
        if let Some(target) = force_reply {
            self.pending_force_reply = Some(target);
        }
        if let Some(notification) = notification {
            self.queue_notification_with_sound(notification, sound);
        }
    }
}
