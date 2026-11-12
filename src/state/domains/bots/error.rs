//! Failed requests for bots: inline queries, callback buttons, commands, games and login URLs.
use crate::state::*;

impl Session {
    /// Reacts to a failed bots request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_bots_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        if self.apply_owned_bots_error(err, pending) || self.apply_widget_error(err, pending) {
            return;
        }
        match pending.map(|p| p.purpose) {
            // The lookup of a group bot failed: the next time the group's
            // commands arrive it is asked again.
            Some(RequestPurpose::Bots(BotsPurpose::GroupBotUser)) => {
                if let Some(pending) = pending
                    && let Some(bot) = pending.user_id
                    && let Some(fetch) = self.bots.bot_users.get_mut(&bot)
                {
                    fetch.failed(Some(pending.id), "could not load the bot", true);
                }
            }
            // Slice B2: refused `sendBotStartMessage` / `getBotSimilarBots` —
            // a refusal is never shown as success.
            Some(RequestPurpose::SendBotStartMessage) => {
                self.chats_state
                    .chat_action_error
                    .push(format!("could not start the bot (error {})", err.code));
            }
            // The list of recent inline bots goes back to being askable
            // (the next "@" asks again), instead of staying in flight.
            Some(RequestPurpose::GetRecentInlineBots) => {
                self.bots.reply_keyboards.recent_inline_bots.failed(
                    pending.map(|p| p.id),
                    format!("could not load recent inline bots (error {})", err.code),
                    false,
                );
            }
            Some(RequestPurpose::GetBotSimilarBots) => {
                // The section shows the reason beside its button, so it can
                // ask again; only the request it waits for fails it.
                if let Some(pending) = pending
                    && let Some(bot) = pending.user_id
                    && let Some(fetch) = self.bots.similar_bots.get_mut(&bot)
                {
                    fetch.failed(
                        Some(pending.id),
                        crate::state::call_request_error_line(err, "Could not load similar bots"),
                        false,
                    );
                }
                self.chats_state
                    .chat_action_error
                    .push(format!("could not load similar bots (error {})", err.code));
            }
            // The chat shows the keyboard among its loaded messages and
            // does not ask again at every re-select.
            Some(RequestPurpose::GetChatReplyMarkupMessage) => {
                if let Some(pending) = pending
                    && let Some(chat) = pending.chat_id
                {
                    self.fail_reply_markup_fetch(chat, pending.id);
                }
            }
            Some(RequestPurpose::SetMessageSenderBotVerification) => {
                self.chats_state
                    .chat_action_error
                    .push(format!("could not verify the account (error {})", err.code));
            }
            Some(RequestPurpose::RemoveMessageSenderBotVerification) => {
                self.chats_state.chat_action_error.push(format!(
                    "could not remove the verification (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::ShareWithBot) => {
                self.chats_state.chat_action_error.push(format!(
                    "the bot could not receive what you shared (error {})",
                    err.code
                ));
            }
            // Bots slice: a failed first page lands in the slot so
            // the picker shows an honest error; a failed "load
            // more" keeps the loaded page retryable.
            Some(RequestPurpose::Bots(BotsPurpose::GetInlineQueryResults { .. })) => {
                let asked = pending.map(|p| p.id);
                if let Some(slot) = self.bots.inline_query.as_mut() {
                    match &mut slot.fetch {
                        InlineQueryFetch::Loading { request } if Some(*request) == asked => {
                            slot.fetch = InlineQueryFetch::Failed(call_request_error_line(
                                err,
                                "Could not load inline results",
                            ));
                        }
                        InlineQueryFetch::Loaded {
                            next_offset, more, ..
                        } => {
                            if matches!(more, InlineMore::Asking { request } if Some(*request) == asked)
                            {
                                *more = InlineMore::Failed {
                                    offset: next_offset.clone(),
                                };
                            }
                        }
                        _ => {}
                    }
                }
            }
            // Bots slice: a failed `@botname` lookup lands on the entry
            // that waits for this request, so the composer shows an honest
            // hint; an entry replaced since (a newer lookup) is untouched.
            Some(RequestPurpose::Bots(BotsPurpose::ResolveInlineBot)) => {
                if let Some(asked) = pending
                    && let Some(username) = self.bots.inline_bot_asked(asked.id).map(str::to_string)
                {
                    self.bots.set_inline_bot(InlineBotResolve::Failed {
                        reason: format!("could not find @{username} (error {})", err.code),
                        username,
                    });
                }
            }
            _ => {}
        }
        // Phase 3.3: `getCommands` failed — on a user session the
        // method is annotated "for bots only" (schema 1.8.67 line
        // 14953), so the error is permanent. Record an empty set
        // so the fetch is never retried; the `/` menu falls back
        // to the bot's `botInfo` commands.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCommands)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
            && let Some(user_id) = self.bot_user_id_for_chat(chat_id)
        {
            self.bots.bot_commands.entry(user_id).or_default();
        }
        if let Some(request) = pending
            && let RequestPurpose::Bots(BotsPurpose::StopPendingMessage { topic_id, draft_id }) =
                request.purpose
            && let Some(chat_id) = request.chat_id
            && let Some(draft) = self
                .messages
                .pending_bot_messages
                .get_mut(&(chat_id.0, topic_id))
            && draft.draft_id == draft_id
            && draft.stop == BotStopStage::Stopping(request.id)
        {
            draft.stop = BotStopStage::Failed;
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetGameHighScores) {
            // Slice bots-games: a refused `getGameHighScores` leaves the
            // panel `Failed` (it never spins forever; the panel draws Retry)
            // and surfaces as a status note via the callback-answer channel.
            if let Some(pending) = pending
                && let (Some(chat_id), Some(message_id)) =
                    (pending.chat_id, pending.around_message_id)
                && let Some(scores) = self.bots.game_scores.get_mut(&(chat_id.0, message_id.0))
            {
                scores.failed(
                    Some(pending.id),
                    crate::lang::tr(crate::lang::keys::Q_MISC_COULDNT_LOAD_THE_SCORES),
                    false,
                );
            }
            self.note_callback_answer(
                pending.and_then(|p| p.chat_id),
                CallbackQueryAnswer {
                    text: "couldn't load the scores".to_string(),
                    show_alert: false,
                    url: String::new(),
                },
            );
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(
                RequestPurpose::GetCallbackQueryAnswer | RequestPurpose::GetCallbackQueryAnswerGame
            )
        ) {
            // TDLib returns error 502 when the bot misses the query
            // timeout: surface it as an answer note (no TDLib text is
            // echoed) so the press gets visible feedback.
            self.note_callback_answer(
                pending.and_then(|p| p.chat_id),
                CallbackQueryAnswer {
                    text: "bot did not answer".to_string(),
                    show_alert: false,
                    url: String::new(),
                },
            );
        }
        // B1: a refused password-protected callback surfaces
        // honestly — 400 is the wrong-password case, anything else
        // is the generic bot-timeout note.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCallbackQueryAnswerWithPassword) {
            let text = if err.code == 400 {
                "wrong 2-step verification password"
            } else {
                "bot did not answer"
            };
            self.note_callback_answer(
                pending.and_then(|p| p.chat_id),
                CallbackQueryAnswer {
                    text: text.to_string(),
                    show_alert: false,
                    url: String::new(),
                },
            );
        }
        // B1: a refused `getLoginUrlInfo` / `getLoginUrl` degrades
        // the login button to an ordinary URL button (schema 1.8.67
        // doc on `getLoginUrl`).
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::GetLoginUrlInfo) | Some(RequestPurpose::GetLoginUrl)
        ) {
            self.bots.login_url.fail(pending.map(|p| p.id));
        }
        self.apply_web_app_error(pending, err);
    }
}
