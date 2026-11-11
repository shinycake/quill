//! Failed requests for history, sending, editing, reactions, polls, translation and message menus.
use crate::state::*;

impl Session {
    /// Reacts to a failed messages request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_messages_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        seq: u64,
    ) {
        // The Edit box's Auto-translate switch is a Messages request: its
        // refusal keeps the box open with the reason.
        self.record_group_info_edit_error(err, pending);
        // A preload that failed leaves no entry: the next pull asks again.
        if let Some(p) = pending
            && p.purpose == RequestPurpose::PreloadHistory
            && let Some(chat_id) = p.chat_id
            && self.chat_list.history_preloads.get(&chat_id.0)
                == Some(&crate::state::HistoryPreload::Loading(p.id))
        {
            self.chat_list.history_preloads.remove(&chat_id.0);
        }
        // A failed pinned-messages fetch ends in `Failed`: the pull to the
        // next channel does not ask again, the chat opening does.
        if let Some(p) = pending
            && p.purpose == RequestPurpose::GetPinnedMessages
            && let Some(chat_id) = p.chat_id
            && let Some(fetch) = self.messages.pinned_messages.get_mut(&chat_id.0)
        {
            fetch.failed(
                Some(p.id),
                format!("could not load the pinned messages (error {})", err.code),
                false,
            );
        }
        if let Some(RequestPurpose::GetRepliedMessage {
            chat_id,
            message_id,
        }) = pending.map(|p| p.purpose)
        {
            self.reject_replied_message(chat_id, message_id, err.code, std::time::Instant::now());
        }
        if let Some(RequestPurpose::Messages(MessagesPurpose::GetComposerReplyTarget {
            chat_id,
            message_id,
        })) = pending.map(|p| p.purpose)
        {
            self.reject_composer_reply_target(chat_id, message_id, err);
        }
        // The option menu's facts: a refusal leaves `Loading` so the page
        // stops waiting for rows that will not come.
        if let Some(RequestPurpose::Messages(MessagesPurpose::GetPollOptionProperties {
            chat_id,
            message_id,
            option_key,
        })) = pending.map(|p| p.purpose)
            && let Some(menu) = self.messages.poll_option_menu.as_mut()
            && menu.chat_id == chat_id
            && menu.message_id == message_id
            && poll_option_key(&menu.option_id) == option_key
        {
            menu.properties = crate::state::fetch::Fetch::Failed {
                error: format!("{:?}", err.class),
                stale: None,
            };
        }
        // A caption over the limit opens its box (tdesktop `CaptionLimitBox`).
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::SendMessage | RequestPurpose::SendMessageAlbum)
        ) {
            self.note_premium_limit(err, pending);
        }
        if let Some(RequestPurpose::Messages(MessagesPurpose::GetFullRichMessage {
            chat_id,
            message_id,
        })) = pending.map(|p| p.purpose)
        {
            let failed = self
                .messages
                .full_rich_loads
                .get_mut(&crate::ids::ChatMessage::new(chat_id, message_id))
                .is_some_and(|load| {
                    load.failed(
                        pending.map(|p| p.id),
                        format!("could not load the full message (error {})", err.code),
                        true,
                    )
                });
            // An older ask's refusal, while a newer one waits, says nothing
            // about the message now being loaded.
            if failed {
                self.chats_state.chat_action_error.push(
                    crate::lang::tr(crate::lang::keys::Q_MISC_COULD_NOT_LOAD_FULL_MESSAGE)
                        .to_string(),
                );
            }
        }
        if let Some(RequestPurpose::Messages(MessagesPurpose::GetMessageAvailableReactions {
            message_id,
        })) = pending.map(|p| p.purpose)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.fail_message_reaction_options(chat_id, MessageId(message_id));
        }
        if let Some(RequestPurpose::Messages(MessagesPurpose::GetSharingProtection { chat_id })) =
            pending.map(|p| p.purpose)
        {
            // Who protects the chat is unknown: turning protection off could
            // send the other person a request nobody confirmed, so the
            // "Send Request?" box asks first.
            self.groups.private_sharing.enable = Some(EnableSharing::AskToRequest(chat_id));
        }
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::Messages(MessagesPurpose::AiCompose(purpose))) => {
                self.fail_ai_compose_request(purpose, err, pending);
            }
            // The message menu's Report flow and audience lists.
            Some(RequestPurpose::ReportMessages) => {
                if let Some(pending) = pending {
                    self.fail_message_report(
                        pending.id,
                        format!("Reporting failed: {}", error_reason(err)),
                    );
                }
            }
            Some(
                purpose @ (RequestPurpose::Messages(MessagesPurpose::GetMessageViewers { .. })
                | RequestPurpose::Messages(MessagesPurpose::GetMessageReadDate { .. })
                | RequestPurpose::Messages(MessagesPurpose::GetMessageAuthor { .. })
                | RequestPurpose::Messages(MessagesPurpose::GetMessageAddedReactions {
                    ..
                })),
            ) => self.fail_audience(purpose),
            Some(RequestPurpose::GetChatAvailableMessageSenders) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.fail_message_senders(chat_id, pending.map(|p| p.id), error_reason(err));
                }
            }
            Some(RequestPurpose::Messages(MessagesPurpose::GetMessageMenuActions {
                chat_id,
                message_id,
            })) => self.messages.menu_actions.fail(chat_id, message_id),
            Some(RequestPurpose::SetChatMessageSender) => {
                self.messages.message_action_note.push(format!(
                    "could not change the sender: {}",
                    error_reason(err)
                ));
            }
            Some(RequestPurpose::DeleteChatMessagesBySender) => {
                self.messages.message_action_note.push(format!(
                    "could not delete the messages: {}",
                    error_reason(err)
                ));
            }
            Some(RequestPurpose::ReportSupergroupSpam) => {
                self.messages
                    .message_action_note
                    .push(format!("could not report the spam: {}", error_reason(err)));
            }
            Some(RequestPurpose::Messages(MessagesPurpose::DeleteMessageReactionsFromSender {
                ..
            })) => {
                self.messages.message_action_note.push(format!(
                    "could not delete the reaction: {}",
                    error_reason(err)
                ));
            }
            // A failed `searchChatMembers` for the `@` menu drops its search
            // (only the one waiting for that request), so the same query is
            // asked again on the next keystroke instead of never.
            Some(RequestPurpose::SearchMentionMembers) => {
                if let Some(id) = pending.map(|p| p.id) {
                    self.fail_mention_search(id);
                }
            }
            // kit Phase 9: a failed first `getChatHistory` must not
            // leave the message list without a history entry — the
            // skeleton shimmer would run forever. Create the entry
            // so the UI settles into the empty state.
            Some(RequestPurpose::GetHistory | RequestPurpose::GetHistoryAround) => {
                if let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                    && !self.history_request_stale(pending)
                {
                    // R5: the empty window shows "Couldn't load messages ·
                    // Retry" instead of a skeleton that never settles.
                    self.history_entry(chat_id).fail_load();
                }
            }
            // B15: a failed `getPollVoteStatistics` lands in the fetch
            // state so the dialog shows an honest error.
            Some(RequestPurpose::Messages(MessagesPurpose::GetPollVoteStatistics {
                chat_id,
                message_id,
            })) => {
                // Only the request the dialog waits for fails it.
                if let Some(pending) = pending
                    && let Some(slot) = self.messages.poll_stats.get_mut(&(chat_id.0, message_id.0))
                    && *slot == PollStatsFetch::Loading(pending.id)
                {
                    *slot = PollStatsFetch::Failed(call_request_error_line(
                        err,
                        "Could not load poll stats",
                    ));
                }
            }
            // B15: poll option / checklist mutations surface their
            // failure in the status note (tdesktop shows a toast:
            // `lng_polls_add_option_error`).
            Some(RequestPurpose::AddPollOption) => {
                self.messages.message_action_note.push(if err.code == 400 {
                    "Could not add the option. Please try again.".to_string()
                } else {
                    call_request_error_line(err, "Could not add the option")
                });
            }
            Some(RequestPurpose::DeletePollOption) => {
                self.messages
                    .message_action_note
                    .push(call_request_error_line(err, "Could not delete the item"));
            }
            Some(RequestPurpose::MarkChecklistTasks) => {
                self.messages
                    .message_action_note
                    .push(call_request_error_line(
                        err,
                        "Could not update the checklist",
                    ));
            }
            Some(RequestPurpose::AddChecklistTasks) => {
                self.messages
                    .message_action_note
                    .push(call_request_error_line(err, "Could not add the tasks"));
            }
            // B4: a failed `getPollVoters` first page lands in the
            // fetch state so the dialog shows an honest error; a
            // failed "load more" keeps the loaded page retryable.
            Some(RequestPurpose::Messages(MessagesPurpose::GetPollVoters {
                chat_id,
                message_id,
                option_id,
                offset,
            })) => {
                // Only the request the entry waits for fails it: a later
                // page's error leaves the loaded page retryable, and an
                // entry an update dropped has nothing to fail.
                let asked = pending.map(|p| p.id);
                let key = (chat_id.0, message_id.0, option_id);
                match self.messages.poll_voters.get_mut(&key) {
                    Some(PollVotersFetch::Loading(id)) if offset == 0 && Some(*id) == asked => {
                        self.messages.poll_voters.insert(
                            key,
                            PollVotersFetch::Failed(call_request_error_line(
                                err,
                                "Could not load voters",
                            )),
                        );
                    }
                    Some(PollVotersFetch::Loaded { more, .. }) if offset > 0 => {
                        if let VotersMore::Asking(id) = *more
                            && Some(id) == asked
                        {
                            *more = VotersMore::Ready;
                        }
                    }
                    _ => {}
                }
            }
            // A failed translation shows "Translate failed." where the text
            // would have gone, in the box and in the translated bubble alike.
            Some(RequestPurpose::Messages(MessagesPurpose::TranslateJob { job })) => {
                self.finish_translation(job, Translation::Failed(error_reason(err)));
            }
            // M1 fix-up: a failed `resendMessages` surfaces in the
            // status note instead of vanishing into `_ => {}` —
            // the menu item says "retrying send…" and the user
            // deserves an answer either way.
            Some(RequestPurpose::Messages(MessagesPurpose::EditMessageSchedulingState {
                scheduling,
                ..
            })) => {
                let action = if scheduling == ComposerScheduling::None {
                    "Could not send the message now"
                } else {
                    "Could not reschedule the message"
                };
                self.messages
                    .resend_error
                    .push(call_request_error_line(err, action));
            }
            Some(RequestPurpose::ResendMessages) => {
                self.messages
                    .resend_error
                    .push(call_request_error_line(err, "Could not retry the send"));
            }
            // M1 fix-up: a failed "Share link" surfaces in the
            // status note instead of silently doing nothing.
            Some(
                RequestPurpose::GetMessageLink
                | RequestPurpose::Messages(MessagesPurpose::GetMessageLinkProperties { .. }),
            ) => {
                self.messages
                    .message_link_error
                    .push(call_request_error_line(err, "Could not get message link"));
            }
            // MED2 fix-up: a refused `recognizeSpeech` surfaces in
            // the status note instead of vanishing into `_ => {}` —
            // the row says "transcription requested" and the user
            // deserves an answer either way.
            Some(RequestPurpose::RecognizeSpeech) => {
                self.messages
                    .recognize_speech_error
                    .push(call_request_error_line(
                        err,
                        "Could not transcribe this message",
                    ));
            }
            // Slice msg-richtext-ai-tools: a failed AI request surfaces
            // in the status note instead of vanishing into `_ => {}` —
            // the button said "AI working…" and the user deserves an
            // answer either way. `AICOMPOSE_FLOOD_PREMIUM` (classified in
            // `parse_error`) gets the documented plain-language line.
            Some(
                RequestPurpose::FixTextWithAi
                | RequestPurpose::ComposeTextWithAi
                | RequestPurpose::ComposeRichMessageWithAi
                | RequestPurpose::CreateRichMessageWithAi
                | RequestPurpose::FixRichMessageWithAi,
            ) => {
                if let Some(pending) = pending
                    && let Some(chat) = pending.chat_id
                {
                    self.messages.ai_answers.push(AiAnswer {
                        request: pending.id,
                        window: pending.asked_in.chat(),
                        chat,
                        result: AiResult::Failed,
                    });
                }
                self.messages.ai_error.push(match err.class {
                    ErrorClass::AiComposeFloodPremium => {
                        "AI limit reached — Telegram Premium is required for more requests"
                            .to_string()
                    }
                    _ => format!("AI tools failed: {}", error_reason(err)),
                });
            }
            _ => {}
        }
        if let Some(viewed) = pending.filter(|p| p.purpose == RequestPurpose::ViewMessages)
            && let Some(chat_id) = viewed.chat_id
        {
            self.abort_viewing(chat_id, viewed.id);
        }
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::GetHistoryNewer
        {
            self.fail_history_newer(pending);
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetHistoryAround)
            && let Some(message_id) = pending.and_then(|p| p.around_message_id)
        {
            self.finish_history_around(pending, message_id, true, seq);
        }
        // A failed share box search ends that half with nothing found, as
        // "Add People" does, so the box says "No matching chats" instead of
        // "Searching..." for good. Only the current search's requests count.
        if let Some(pending) = pending
            && matches!(
                pending.purpose,
                RequestPurpose::SearchShareChats | RequestPurpose::SearchShareChatsOnServer
            )
        {
            self.messages.share_search.accept(pending.id, &[]);
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ForwardMessages)
            && let Some(pending) = pending
        {
            self.finish_forward(pending, &[], true);
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ReportChatSponsoredMessage) {
            // A TDLib error dismisses the option picker; no outcome is shown.
            // Only for the request the flow waits on: a report already
            // dismissed, or an earlier step, has nothing left to dismiss.
            let waits = pending.is_some_and(|p| {
                self.messages
                    .sponsored_report
                    .target()
                    .is_some_and(|target| target.request == Some(p.id))
            });
            if waits {
                self.messages.sponsored_report = SponsoredReportFlow::Idle;
            }
        }
    }
}
