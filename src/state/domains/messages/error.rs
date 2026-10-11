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
        if let Some(RequestPurpose::GetRepliedMessage {
            chat_id,
            message_id,
        }) = pending.map(|p| p.purpose)
        {
            self.reject_replied_message(chat_id, message_id);
        }
        match pending.map(|p| p.purpose) {
            // The message menu's Report flow and audience lists.
            Some(RequestPurpose::ReportMessages) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.fail_message_report(
                        chat_id,
                        format!("Reporting failed: {}", error_reason(err)),
                    );
                }
            }
            Some(
                purpose @ (RequestPurpose::Messages(MessagesPurpose::GetMessageViewers { .. })
                | RequestPurpose::Messages(MessagesPurpose::GetMessageReadDate { .. })
                | RequestPurpose::Messages(MessagesPurpose::GetMessageAddedReactions {
                    ..
                })),
            ) => self.fail_audience(purpose),
            Some(RequestPurpose::SetChatMessageSender) => {
                self.messages.message_action_note = Some(format!(
                    "could not change the sender: {}",
                    error_reason(err)
                ));
            }
            Some(RequestPurpose::DeleteChatMessagesBySender) => {
                self.messages.message_action_note = Some(format!(
                    "could not delete the messages: {}",
                    error_reason(err)
                ));
            }
            Some(RequestPurpose::ReportSupergroupSpam) => {
                self.messages.message_action_note =
                    Some(format!("could not report the spam: {}", error_reason(err)));
            }
            Some(RequestPurpose::Messages(MessagesPurpose::DeleteMessageReactionsFromSender {
                ..
            })) => {
                self.messages.message_action_note = Some(format!(
                    "could not delete the reaction: {}",
                    error_reason(err)
                ));
            }
            // kit Phase 9: a failed first `getChatHistory` must not
            // leave the message list without a history entry — the
            // skeleton shimmer would run forever. Create the entry
            // so the UI settles into the empty state.
            Some(RequestPurpose::GetHistory | RequestPurpose::GetHistoryAround) => {
                if let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                    && !self.take_stale_history_request(pending)
                {
                    // R5: the empty window shows "Couldn't load messages ·
                    // Retry" instead of a skeleton that never settles.
                    self.histories.entry(chat_id.0).or_default().load_failed = true;
                }
            }
            // `parity:platform-chat-export` — a failed export page must not
            // strand the export with `in_flight` set: mark it failed (and
            // clear `in_flight`) so the UI surfaces the error and the state
            // can be cleared and retried.
            Some(RequestPurpose::ExportChatHistory) => {
                if let Some(export) = self.messages.chat_export.as_mut()
                    && pending.and_then(|p| p.chat_id) == Some(export.chat_id)
                {
                    export.failed = Some(error_reason(err).to_string());
                    export.in_flight = false;
                }
            }
            // B15: a failed `getPollVoteStatistics` lands in the fetch
            // state so the dialog shows an honest error.
            Some(RequestPurpose::Messages(MessagesPurpose::GetPollVoteStatistics {
                chat_id,
                message_id,
            })) => {
                self.messages.poll_stats.insert(
                    (chat_id.0, message_id.0),
                    PollStatsFetch::Failed(call_request_error_line(
                        err,
                        "Could not load poll stats",
                    )),
                );
            }
            // B15: poll option / checklist mutations surface their
            // failure in the status note (tdesktop shows a toast:
            // `lng_polls_add_option_error`).
            Some(RequestPurpose::AddPollOption) => {
                self.messages.message_action_note = Some(if err.code == 400 {
                    "Could not add the option. Please try again.".to_string()
                } else {
                    call_request_error_line(err, "Could not add the option")
                });
            }
            Some(RequestPurpose::MarkChecklistTasks) => {
                self.messages.message_action_note = Some(call_request_error_line(
                    err,
                    "Could not update the checklist",
                ));
            }
            Some(RequestPurpose::AddChecklistTasks) => {
                self.messages.message_action_note =
                    Some(call_request_error_line(err, "Could not add the tasks"));
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
                if offset == 0
                    || !matches!(
                        self.messages
                            .poll_voters
                            .get(&(chat_id.0, message_id.0, option_id)),
                        Some(PollVotersFetch::Loaded { .. })
                    )
                {
                    self.messages.poll_voters.insert(
                        (chat_id.0, message_id.0, option_id),
                        PollVotersFetch::Failed(call_request_error_line(
                            err,
                            "Could not load voters",
                        )),
                    );
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
                self.messages.resend_error = Some(call_request_error_line(err, action));
            }
            Some(RequestPurpose::ResendMessages) => {
                self.messages.resend_error =
                    Some(call_request_error_line(err, "Could not retry the send"));
            }
            // M1 fix-up: a failed "Share link" surfaces in the
            // status note instead of silently doing nothing.
            Some(
                RequestPurpose::GetMessageLink
                | RequestPurpose::Messages(MessagesPurpose::GetMessageLinkProperties { .. }),
            ) => {
                self.messages.message_link_error =
                    Some(call_request_error_line(err, "Could not get message link"));
            }
            // MED2 fix-up: a refused `recognizeSpeech` surfaces in
            // the status note instead of vanishing into `_ => {}` —
            // the row says "transcription requested" and the user
            // deserves an answer either way.
            Some(RequestPurpose::RecognizeSpeech) => {
                self.messages.recognize_speech_error = Some(call_request_error_line(
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
                self.messages.ai_error = Some(match err.class {
                    ErrorClass::AiComposeFloodPremium => {
                        "AI limit reached — Telegram Premium is required for more requests"
                            .to_string()
                    }
                    _ => format!("AI tools failed: {}", error_reason(err)),
                });
            }
            _ => {}
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ViewMessages)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.abort_viewing(chat_id);
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
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ForwardMessages)
            && let Some(pending) = pending
        {
            self.finish_forward(pending, &[], true);
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ReportChatSponsoredMessage) {
            // A TDLib error dismisses the option picker; no outcome is shown.
            self.messages.sponsored_report = None;
            self.messages.sponsored_report_target = None;
        }
    }
}
