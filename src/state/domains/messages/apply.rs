//! Applies TDLib updates and answers for history, sending, editing, reactions, polls, translation and message menus.
use crate::state::*;
use crate::telegram::envelope::MessagesPayload;

impl Session {
    /// Applies one messages payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_messages_payload(
        &mut self,
        payload: MessagesPayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        match payload {
            MessagesPayload::UpdateChatDraftMessage {
                chat_id,
                draft,
                positions,
            } => {
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                if !self.draft_dirty.contains(&chat_id.0) {
                    chat.draft = draft;
                }
                self.replace_main_list_from_positions(chat_id, &positions);
                self.rebuild_main_order();
            }
            MessagesPayload::UpdateChatHasScheduledMessages {
                chat_id,
                has_scheduled_messages,
            } => self.set_chat_has_scheduled(chat_id, has_scheduled_messages),
            MessagesPayload::UpdateChatMessageSender {
                chat_id,
                message_sender,
            } => self.set_chat_message_sender(chat_id, message_sender),
            MessagesPayload::ChatMessageSenders { senders } => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.chats_state.send_as_options.insert(chat_id.0, senders);
                }
            }
            MessagesPayload::UpdateChatIsTranslatable {
                chat_id,
                is_translatable,
            } => self.set_chat_translatable(chat_id, is_translatable),
            // B4: `getPollVoters` answer — a first page (offset 0)
            // replaces the cached list for this (chat, message, option);
            // a later page appends, deduped by sender, keeping server
            // order (the list is per-option; `option_id` is part of the
            // key so switching options refetches).
            MessagesPayload::PollVoters {
                total_count,
                voters,
            } => self.apply_poll_voters(total_count, voters, pending, extra, seq),
            MessagesPayload::UpdateActiveLiveLocationMessages { shares } => {
                self.sync.set_live_shares(shares)
            }
            MessagesPayload::UpdateMessageLiveLocationViewed {
                chat_id,
                message_id,
            } => self.sync.mark_live_viewed(chat_id, message_id),
            // B15: `getPollVoteStatistics` answer — cached per message.
            MessagesPayload::PollVoteStatistics { graph } => {
                if let Some(RequestPurpose::Messages(MessagesPurpose::GetPollVoteStatistics {
                    chat_id,
                    message_id,
                })) = pending.map(|p| p.purpose)
                {
                    self.poll_stats
                        .insert((chat_id.0, message_id.0), PollStatsFetch::Loaded(graph));
                }
            }
            MessagesPayload::UpdateMessageUnreadReactions {
                chat_id,
                unread_reaction_count,
                newest,
                ..
            } => {
                let previous = self
                    .chats
                    .get(&chat_id.0)
                    .map_or(0, |chat| chat.unread_reaction_count);
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .unread_reaction_count = unread_reaction_count;
                // A grown counter with a visible newest reaction is a new
                // reaction on one of our messages.
                if unread_reaction_count > previous
                    && let Some(reaction) = newest
                {
                    self.queue_reaction_notification(chat_id, &reaction);
                }
            }
            // The message menu's "N Seen" / "Seen at" / "N Reacted" rows.
            MessagesPayload::MessageViewers(viewers) => {
                if let Some(RequestPurpose::Messages(MessagesPurpose::GetMessageViewers {
                    chat_id,
                    message_id,
                })) = pending.map(|p| p.purpose)
                {
                    self.accept_message_viewers(chat_id, message_id, viewers);
                }
            }
            MessagesPayload::MessageReadDate(date) => {
                if let Some(RequestPurpose::Messages(MessagesPurpose::GetMessageReadDate {
                    chat_id,
                    message_id,
                })) = pending.map(|p| p.purpose)
                {
                    self.accept_message_read_date(chat_id, message_id, date);
                }
            }
            MessagesPayload::AddedReactions(page) => {
                if let Some(RequestPurpose::Messages(MessagesPurpose::GetMessageAddedReactions {
                    chat_id,
                    message_id,
                    filter,
                    append,
                })) = pending.map(|p| p.purpose)
                {
                    self.accept_added_reactions(chat_id, message_id, filter, append, page);
                }
            }
            MessagesPayload::UpdatePendingMessage {
                chat_id,
                forum_topic_id,
                draft_id,
                can_stop,
                keep_on_stop,
                content,
                files,
            } => {
                if chat_id.0 != 0
                    && draft_id != 0
                    && forum_topic_id >= 0
                    && matches!(
                        content,
                        MessageContent::Text(_) | MessageContent::RichMessage(_)
                    )
                {
                    let stopped = self
                        .pending_bot_messages
                        .get(&(chat_id.0, forum_topic_id))
                        .is_some_and(|old| old.draft_id == draft_id && old.stopped);
                    self.remember_files(&files);
                    self.pending_bot_messages.insert(
                        (chat_id.0, forum_topic_id),
                        PendingBotMessage {
                            draft_id,
                            can_stop: can_stop && !stopped,
                            keep_on_stop,
                            content,
                            stop_failed: false,
                            stopped,
                            expires_at_ms: unix_ms_now()
                                .saturating_add(self.pending_bot_period_secs.saturating_mul(1000)),
                        },
                    );
                }
            }
            MessagesPayload::UpdateStopMessageDraft {
                chat_id,
                forum_topic_id,
                draft_id,
            } => {
                self.finish_pending_bot_stop(chat_id, forum_topic_id, draft_id);
            }
            MessagesPayload::UpdateNewMessage(message) => {
                self.note_forum_topic_message(&message);
                self.apply_update_new_message(message);
            }
            MessagesPayload::UpdateMessageSendSucceeded {
                message,
                old_message_id,
            } => {
                let chat_id = message.chat_id;
                let topic_id = message.topic_id;
                self.note_last_message_send_state(
                    chat_id,
                    old_message_id,
                    message.id,
                    crate::telegram::envelope::MessageSendState::Sent,
                );
                self.remember_files(&message.files);
                let row = history_message(message, false);
                self.index_poll(&row);
                let history = self.histories.entry(chat_id.0).or_default();
                history.replace_id(old_message_id, row.clone());
                // Parity slice 4: the pending row in the topic's history
                // resolves the same way (the succeeded message carries its
                // topic).
                if let Some(topic_id) = topic_id
                    && let Some(topic_history) =
                        self.threads.topic_histories.get_mut(&(chat_id.0, topic_id))
                {
                    topic_history.replace_id(old_message_id, row.clone());
                }
                if let Some(thread) = self.threads.thread.as_mut()
                    && thread.chat_id == chat_id
                    && thread.history.messages.contains_key(&old_message_id.0)
                {
                    thread.history.replace_id(old_message_id, row);
                }
                self.draft_clears.push(chat_id);
            }
            MessagesPayload::UpdateMessageSendFailed {
                message,
                old_message_id,
                error,
            } => {
                if let Some(notice) = error.send_permission_notice() {
                    self.send_permission_error = Some(notice.into());
                } else if let Some(notice) = error.flood_notice() {
                    // Q1: the failed row keeps its retry affordance
                    // (`can_retry` comes from TDLib, which marks rate
                    // limits retryable); the text stays in the history.
                    self.flood_notice = Some(notice);
                }
                let chat_id = message.chat_id;
                let topic_id = message.topic_id;
                self.note_last_message_send_state(
                    chat_id,
                    old_message_id,
                    message.id,
                    crate::telegram::envelope::MessageSendState::Failed,
                );
                self.remember_files(&message.files);
                // M1: mark the row failed. The retry affordance is gated
                // separately on `can_retry` (`resendMessages` via
                // `driver.resend_failed_message`) — not every failed send
                // may be retried.
                let mut row = history_message(message, true);
                row.failed = true;
                self.index_poll(&row);
                let history = self.histories.entry(chat_id.0).or_default();
                history.replace_id(old_message_id, row.clone());
                // Parity slice 4: the failed pending row shows in the topic
                // view too.
                if let Some(topic_id) = topic_id
                    && let Some(topic_history) =
                        self.threads.topic_histories.get_mut(&(chat_id.0, topic_id))
                {
                    topic_history.replace_id(old_message_id, row.clone());
                }
                if let Some(thread) = self.threads.thread.as_mut()
                    && thread.chat_id == chat_id
                    && thread.history.messages.contains_key(&old_message_id.0)
                {
                    thread.history.replace_id(old_message_id, row);
                }
            }
            MessagesPayload::UpdateMessageSendAcknowledged { .. } => {
                // Not success. Keep the pending row until Succeeded/Failed.
            }
            MessagesPayload::UpdateMessageFactCheck {
                chat_id,
                message_id,
                text,
            } => {
                self.edit_loaded_message(chat_id, message_id, |message| {
                    message.extras.fact_check = text.clone();
                });
            }
            MessagesPayload::UpdateMessageInteractionInfo {
                chat_id,
                message_id,
                interaction_info,
            } => {
                self.edit_loaded_message(chat_id, message_id, |message| {
                    message.interaction_info = interaction_info.clone();
                });
                self.sync_thread_reply_info(
                    chat_id,
                    message_id,
                    interaction_info
                        .as_ref()
                        .and_then(|info| info.reply_info.as_ref()),
                );
            }
            MessagesPayload::UpdateMessageIsPinned {
                chat_id,
                message_id,
                is_pinned,
            } => {
                self.edit_loaded_message(chat_id, message_id, |message| {
                    message.is_pinned = is_pinned;
                });
                // An unpin leaves the pinned list at once; a pin is added
                // by the refetch the driver sends for the open chat.
                if !is_pinned && let Some(list) = self.pinned_messages.get_mut(&chat_id.0) {
                    list.retain(|message| message.id != message_id);
                }
            }
            MessagesPayload::UpdateMessageContentOpened {
                chat_id,
                message_id,
            } => {
                self.edit_loaded_message(chat_id, message_id, |message| {
                    message.content.mark_content_opened();
                });
            }
            MessagesPayload::UpdateMessageEdited {
                chat_id,
                message_id,
                edit_date,
                reply_markup,
            } => {
                // Phase 3.2: bots edit inline keyboards via `updateMessageEdited`
                // (schema 1.8.67 line 10431) — the new `reply_markup` (possibly
                // None) replaces the message's keyboard.
                // The same update stamps the edit date shown as "edited".
                self.edit_loaded_message(chat_id, message_id, |message| {
                    message.reply_markup = reply_markup.clone();
                    message.extras.edit_date = edit_date;
                });
            }
            MessagesPayload::UpdatePoll { poll } => {
                // Phase 4.2: `updatePoll` (schema 1.8.67 line 11179) carries
                // only the new `poll` — no chat or message id — so its rows
                // are found through the poll-id index and the poll is
                // replaced in place.
                self.apply_update_poll(poll);
            }
            MessagesPayload::UpdateMessageContent {
                chat_id,
                message_id,
                content,
                files,
            } => {
                self.apply_update_message_content(chat_id, message_id, content, files);
            }
            MessagesPayload::UpdateMessageEphemeralContent {
                chat_id,
                message_id,
                ephemeral,
            } => self.apply_update_message_ephemeral_content(
                chat_id, message_id, ephemeral, pending, extra, seq,
            ),
            MessagesPayload::UpdateDeleteMessages {
                chat_id,
                message_ids,
                is_permanent,
                from_cache,
            } => {
                // `from_cache`: TDLib only dropped its in-memory copy; the
                // messages "can possibly be retrieved again" (schema 1.8.67,
                // line 10699). Telegram X ignores these
                // (`Tdlib.updateMessagesDeleted`); dropping the rows here
                // would punch holes that oldest-first paging never refills.
                let message_ids = if from_cache && !is_permanent {
                    Vec::new()
                } else {
                    message_ids
                };
                if let Some(list) = self.pinned_messages.get_mut(&chat_id.0) {
                    list.retain(|message| !message_ids.contains(&message.id));
                }
                let history = self.histories.entry(chat_id.0).or_default();
                for id in message_ids.iter().copied() {
                    // Permanent or "became inaccessible": either way the
                    // row must not come back from a stale page.
                    history.remove(id, true);
                    if is_permanent
                        && matches!(
                            self.chat_search.jump,
                            ChatSearchJump::Ready { message_id }
                                | ChatSearchJump::Loading { message_id }
                                if message_id == id
                        )
                    {
                        self.chat_search.jump = ChatSearchJump::Missing { message_id: id };
                    }
                }
                // Parity slice 4: the topic view reads only
                // `topic_histories`, so deletions must reach its rows too.
                for ((topic_chat_id, _), topic) in self.threads.topic_histories.iter_mut() {
                    if *topic_chat_id == chat_id.0 {
                        for id in &message_ids {
                            topic.messages.remove(&id.0);
                        }
                    }
                }
                self.thread_remove(chat_id, &message_ids);
                self.saved_remove_messages(chat_id, &message_ids);
            }
            MessagesPayload::Messages(messages) => {
                self.apply_messages(messages, pending, extra, seq)
            }
            MessagesPayload::Message(message) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatReplyMarkupMessage) {
                    self.set_chat_reply_keyboard(
                        message.chat_id,
                        Some(message.id),
                        message.reply_markup.clone(),
                    );
                    return;
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatMessageByDate) {
                    self.accept_date_message(message.id);
                    return;
                }
                if let Some(RequestPurpose::GetRepliedMessage {
                    chat_id,
                    message_id,
                }) = pending.map(|p| p.purpose)
                {
                    self.accept_replied_message(chat_id, message_id, message);
                    return;
                }
                // M1 fix-up: editing a scheduled send returns the edited
                // `message` with `scheduling_state` set — refresh the
                // scheduled-list entry instead of inserting a phantom row
                // into chat history (which also left the scheduled list
                // showing the stale pre-edit text).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::EditMessage)
                    && message.scheduling_state.is_some()
                {
                    self.remember_files(&message.files);
                    if let Some(slot) = self
                        .scheduled_messages
                        .iter_mut()
                        .find(|m| m.id == message.id)
                    {
                        *slot = message;
                    } else {
                        self.scheduled_messages.push(message);
                    }
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::SendMessage) {
                    self.upsert_message(message, true);
                } else {
                    self.upsert_message(message, false);
                }
            }
            MessagesPayload::SponsoredMessages {
                messages,
                files,
                messages_between,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatSponsoredMessages)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    if self.open_chat != Some(chat_id) {
                        self.diagnostics.record(Diagnostic {
                            category: "reducer",
                            type_name: Some("sponsoredMessages".into()),
                            extra: Some(pending.id.0),
                            seq: Some(seq),
                            note: "stale-chat-sponsored",
                        });
                        return;
                    }
                    self.accept_sponsored_messages(chat_id, messages, messages_between, &files);
                }
            }
            MessagesPayload::ReportSponsoredResult(result) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ReportChatSponsoredMessage)
                    && let Some(pending) = pending
                {
                    self.accept_sponsored_report(pending, result);
                }
            }
            MessagesPayload::MessageAutoDeleteTime { seconds } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetDefaultAutoDelete) {
                    self.settings.default_auto_delete_secs = Some(seconds);
                    self.settings.default_auto_delete_busy = false;
                    self.settings.default_auto_delete_error = None;
                }
            }
            // M1: `getMessageLink` returns `messageLink`. The driver
            // stashes the link in `Session::message_link_result` before
            // `apply` takes the pending request; nothing to reduce here.
            MessagesPayload::MessageLink { .. } => {}
            // M2: handled by the driver before `apply` (blocks land in
            // history there); nothing to reduce here.
            MessagesPayload::RichMessage { .. } => {}
            // MED4: `webPageInstantView` — captured by the driver before
            // `apply` into `Session::instant_view` (success) or
            // `Session::instant_view_fallback_url` (error); nothing to
            // reduce here.
            MessagesPayload::WebPageInstantView { .. } => {}
            // MED4b: `linkPreview` (`getLinkPreview` answer) — captured
            // by the driver before `apply` into
            // `Session::composer_preview`; nothing to reduce here.
            MessagesPayload::LinkPreview { .. } => {}
            // M1 fix-up: `getMessageProperties` returns
            // `messageProperties`. The driver gates the chained
            // `getMessageLink` on `can_get_link` before `apply` takes
            // the pending request; nothing to reduce here.
            MessagesPayload::MessageProperties(actions) => {
                if let Some(RequestPurpose::Messages(MessagesPurpose::GetMessageMenuActions {
                    chat_id,
                    message_id,
                })) = pending.map(|p| p.purpose)
                {
                    self.message_menu_actions = Some((chat_id, message_id, actions));
                }
            }
        }
    }
}
