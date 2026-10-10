//! Applies TDLib updates and answers for bots: inline queries, callback buttons, commands, games and login URLs.
use crate::state::*;
use crate::telegram::envelope::BotsPayload;

impl Session {
    /// Applies one bots payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_bots_payload(
        &mut self,
        payload: BotsPayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        match payload {
            BotsPayload::UpdateChatReplyMarkup {
                chat_id,
                message_id,
                reply_markup,
            } => self.set_chat_reply_keyboard(chat_id, message_id, reply_markup),
            // Bots slice: `inlineQueryResults` — the `getInlineQueryResults`
            // answer (schema 1.8.67, line 7716). A first page replaces the
            // slot; a later page appends, deduped by result id, keeping
            // the new page's id/offset.
            // ponytail: rapid re-queries can let an older response land on
            // a newer slot — the response never echoes the query text, so
            // the slot keys on (chat, bot) only; the UI slice debounces
            // queries anyway.
            BotsPayload::InlineQueryResults(page) => {
                self.apply_inline_query_results(page, pending, extra, seq)
            }
            BotsPayload::BotCommands {
                bot_user_id,
                commands,
            } => {
                // Phase 3.3: `getCommands` response — cache the global-scope
                // commands for the bot. Only answers to our own fetch are
                // cached (matched by `@extra`); a user session gets an
                // `error` instead of `botCommands` (schema: "for bots
                // only"), recorded as an empty set by the `Error` arm.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCommands) {
                    self.bot_commands.insert(bot_user_id.0, commands);
                }
            }
            BotsPayload::CallbackQueryAnswer(answer) => {
                // `getCallbackQueryAnswer` response: only answers to our own
                // inline-button presses are surfaced (matched by `@extra`).
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(
                        RequestPurpose::GetCallbackQueryAnswer
                            | RequestPurpose::GetCallbackQueryAnswerWithPassword
                            | RequestPurpose::GetCallbackQueryAnswerGame
                    )
                ) {
                    self.last_callback_answer = Some(answer);
                }
            }
            BotsPayload::GameHighScores(scores) => {
                // Slice bots-games: `getGameHighScores` answer to our own
                // Scores press (matched by `@extra`). The message id rides
                // `around_message_id` (`request_for_message`); the panel
                // flips from its loading row to the rows.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetGameHighScores)
                    && let Some(pending) = pending
                    && let (Some(chat_id), Some(message_id)) =
                        (pending.chat_id, pending.around_message_id)
                {
                    // Guard: a panel the user closed while the answer was in
                    // flight must stay closed — only fill the loading entry.
                    if self.game_scores.contains_key(&(chat_id.0, message_id.0)) {
                        self.game_scores
                            .insert((chat_id.0, message_id.0), Some(scores));
                    }
                }
            }
            payload @ (BotsPayload::WebAppInfo { .. }
            | BotsPayload::WebAppUrl { .. }
            | BotsPayload::MainWebApp { .. }
            | BotsPayload::FoundWebApp(_)
            | BotsPayload::AttachmentMenuBot(_)
            | BotsPayload::UpdateAttachmentMenuBots(_)
            | BotsPayload::UpdateWebAppMessageSent { .. }
            | BotsPayload::CustomRequestResult { .. }) => {
                self.apply_web_app_payload(payload, pending);
            }
            BotsPayload::LoginUrlInfo(info) => {
                // B1: `getLoginUrlInfo` response to our own login-button
                // press (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetLoginUrlInfo) {
                    self.last_login_url_info = Some(info);
                }
            }
        }
    }
}
