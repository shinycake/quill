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
        match pending.map(|p| p.purpose) {
            // Slice B2: refused `sendBotStartMessage` / `getBotSimilarBots` —
            // a refusal is never shown as success.
            Some(RequestPurpose::SendBotStartMessage) => {
                self.chat_action_error =
                    Some(format!("could not start the bot (error {})", err.code));
            }
            Some(RequestPurpose::GetBotSimilarBots) => {
                self.chat_action_error =
                    Some(format!("could not load similar bots (error {})", err.code));
            }
            Some(RequestPurpose::ShareWithBot) => {
                self.chat_action_error = Some(format!(
                    "the bot could not receive what you shared (error {})",
                    err.code
                ));
            }
            // Bots slice: a failed first page lands in the slot so
            // the picker shows an honest error; a failed "load
            // more" keeps the loaded page retryable.
            Some(RequestPurpose::Bots(BotsPurpose::GetInlineQueryResults {
                chat_id,
                bot_user_id,
                first_page,
            })) => {
                let failed_first_page = first_page
                    && matches!(
                        &self.inline_query,
                        Some(slot)
                            if slot.chat_id == chat_id
                                && slot.bot_user_id == bot_user_id
                    );
                if failed_first_page && let Some(slot) = self.inline_query.as_mut() {
                    slot.fetch = InlineQueryFetch::Failed(call_request_error_line(
                        err,
                        "Could not load inline results",
                    ));
                }
            }
            // Bots slice: a failed `@botname` lookup lands in the
            // resolve slot so the composer shows an honest hint;
            // stale failures (a newer username is already being
            // resolved) are ignored via the generation guard.
            Some(RequestPurpose::Bots(BotsPurpose::ResolveInlineBot { generation })) => {
                let stale = !matches!(
                    &self.inline_bot_resolve,
                    Some(InlineBotResolve::Resolving {
                        generation: slot_generation,
                        ..
                    }) if *slot_generation == generation
                );
                if !stale {
                    let username = match &self.inline_bot_resolve {
                        Some(InlineBotResolve::Resolving { username, .. }) => username.clone(),
                        _ => String::new(),
                    };
                    self.inline_bot_resolve = Some(InlineBotResolve::Failed {
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
            self.bot_commands.entry(user_id).or_default();
        }
        if let Some(request) = pending
            && let RequestPurpose::Bots(BotsPurpose::StopPendingMessage { topic_id, draft_id }) =
                request.purpose
            && let Some(chat_id) = request.chat_id
            && let Some(draft) = self.pending_bot_messages.get_mut(&(chat_id.0, topic_id))
            && draft.draft_id == draft_id
        {
            draft.stop_failed = true;
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetGameHighScores) {
            // Slice bots-games: a refused `getGameHighScores` closes the
            // scores panel (it never spins forever) and surfaces as a
            // status note via the callback-answer channel.
            if let Some(pending) = pending
                && let (Some(chat_id), Some(message_id)) =
                    (pending.chat_id, pending.around_message_id)
            {
                self.game_scores.remove(&(chat_id.0, message_id.0));
            }
            self.last_callback_answer = Some(CallbackQueryAnswer {
                text: "couldn't load the scores".to_string(),
                show_alert: false,
                url: String::new(),
            });
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCallbackQueryAnswer) {
            // TDLib returns error 502 when the bot misses the query
            // timeout: surface it as an answer note (no TDLib text is
            // echoed) so the press gets visible feedback.
            self.last_callback_answer = Some(CallbackQueryAnswer {
                text: "bot did not answer".to_string(),
                show_alert: false,
                url: String::new(),
            });
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
            self.last_callback_answer = Some(CallbackQueryAnswer {
                text: text.to_string(),
                show_alert: false,
                url: String::new(),
            });
        }
        // B1: a refused `getLoginUrlInfo` / `getLoginUrl` degrades
        // the login button to an ordinary URL button (schema 1.8.67
        // doc on `getLoginUrl`).
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::GetLoginUrlInfo) | Some(RequestPurpose::GetLoginUrl)
        ) {
            let fallback_url = self
                .login_url_request
                .take()
                .map(|request| request.raw_url)
                .unwrap_or_default();
            self.last_login_url_info = Some(LoginUrlInfo::Failed { fallback_url });
        }
    }
}
