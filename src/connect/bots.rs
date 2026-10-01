//! Connect driver: bots, callback queries, login URLs.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::{
    InlineBotResolve, InlineQueryFetch, InlineQuerySlot, LoginUrlRequest, RequestPurpose,
};
use crate::telegram::envelope::ChatKind;
use crate::telegram::requests::{
    delete_chat_reply_markup as delete_chat_reply_markup_request, get_bot_similar_bots,
    get_callback_query_answer, get_callback_query_answer_game,
    get_callback_query_answer_with_password, get_commands, get_game_high_scores,
    get_inline_query_results, get_login_url, get_login_url_info, get_user_full_info,
    search_public_chat, send_bot_start_message as send_bot_start_message_request,
    send_game as send_game_request, send_inline_query_result_message,
    set_message_sender_block_list,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase 3.1: lazy `getUserFullInfo` for the open bot chat. Fires once
    /// per uncached bot (deduped by cache + in-flight purpose); the
    /// `userFullInfo` response and `updateUserFullInfo` both populate
    /// `Session::bot_info`. No-op for non-bot chats.
    pub(crate) fn maybe_fetch_bot_info(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let Some(chat_id) = self.session.open_chat else {
            return Ok(());
        };
        let Some(user_id) = self.session.bot_user_id_for_chat(chat_id) else {
            return Ok(());
        };
        if self.session.bot_info.contains_key(&user_id) {
            return Ok(());
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetUserFullInfo, chat_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetUserFullInfo, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_user_full_info(extra, user_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Phase 3.3: `getCommands` for the open bot chat's global commands.
    /// A null scope selects the default scope (`botCommandScopeDefault`,
    /// schema 1.8.67 line 10360) with an empty language code. Fires once
    /// per bot (deduped by cache and in-flight purpose) alongside the
    /// `getUserFullInfo` fetch. The schema annotates `getCommands`
    /// "for bots only" (TDLib 1.8.67 line 14953), so a user session gets
    /// an `error` answer — recorded as an empty command set by
    /// `Session::apply`, with no retry on later chat selections; the `/`
    /// menu then shows the `botInfo` commands only.
    pub(crate) fn maybe_fetch_bot_commands(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let Some(chat_id) = self.session.open_chat else {
            return Ok(());
        };
        let Some(user_id) = self.session.bot_user_id_for_chat(chat_id) else {
            return Ok(());
        };
        if self.session.bot_commands.contains_key(&user_id) {
            return Ok(());
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetCommands, chat_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetCommands, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_commands(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Slice B2: `sendBotStartMessage` (schema 1.8.67, line 12216) — what
    /// the START button and "Restart bot" send. `parameter` is the
    /// `internalLinkTypeBotStart.start_parameter` (line 9399); empty for
    /// a plain restart. When the chat is fully blocked, unblocks first
    /// (Telegram X `MessagesController.ACTION_BOT_START` likewise
    /// unblocks before the send). Unlike TGX, the start is queued without
    /// waiting for the unblock result; a refused unblock surfaces via
    /// `chat_action_error` and the start fails closed.
    pub fn send_bot_start_message(
        &mut self,
        chat_id: ChatId,
        bot_user_id: i64,
        parameter: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::SendBotStartMessage;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        // Slice B2: on a fully-blocked bot chat the start message would
        // fail; unblock first so the send lands (no-op on unblocked or
        // non-private chats).
        if self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.blocked)
        {
            self.set_chat_user_blocked(chat_id, false)?;
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&send_bot_start_message_request(
            extra,
            bot_user_id,
            chat_id.0,
            parameter,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice B2: `getBotSimilarBots` (schema 1.8.67, line 11640) for the
    /// similar-bots section of the bot profile. Fires once per bot
    /// (deduped by `Session::similar_bots` and in-flight purpose);
    /// response `users` lands via the reducer by `user_id`.
    pub fn fetch_similar_bots(
        &mut self,
        bot_user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.similar_bots.contains_key(&bot_user_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::GetBotSimilarBots;
        if self
            .session
            .requests
            .has_purpose_for_user(purpose, bot_user_id)
        {
            return Ok(None);
        }
        let extra = self.session.request_for_user(purpose, bot_user_id);
        if let Err(err) = self
            .sender
            .send_json(&get_bot_similar_bots(extra, bot_user_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice B2: "Restart bot" — clear the chat's history (`deleteChatHistory`,
    /// schema line 11845, kept in the chat list, `revoke: false`), then
    /// send `sendBotStartMessage` with an empty parameter. The clear is
    /// confirm-gated in the UI (both calls fail closed: a send failure
    /// leaves the request bookkeeping clean).
    pub fn restart_bot(
        &mut self,
        chat_id: ChatId,
        bot_user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if self.clear_chat_history(chat_id, false)?.is_none() {
            return Ok(None);
        }
        self.send_bot_start_message(chat_id, bot_user_id, "")
    }

    /// Slice CL3: `setMessageSenderBlockList` (TDLib 1.8.67, schema line
    /// 14492) for a private/secret chat's peer (schema:3674); `block =
    /// false` passes null `block_list` to unblock (TGX
    /// `Tdlib.unblockSender`). Returns `Ok(None)` when the chat isn't a
    /// private/secret chat or is the user's own chat. The new state
    /// arrives via `updateChatBlockList`.
    pub fn set_chat_user_blocked(
        &mut self,
        chat_id: ChatId,
        block: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let user_id = match self.session.chats.get(&chat_id.0).map(|chat| &chat.kind) {
            Some(ChatKind::Private { user_id }) | Some(ChatKind::Secret { user_id, .. }) => {
                user_id.0
            }
            _ => return Ok(None),
        };
        if self.session.my_user_id.is_some_and(|me| me == user_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetMessageSenderBlockList { block };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&set_message_sender_block_list(extra, user_id, block))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase 3.2: send a callback query for an inline keyboard button press
    /// (`getCallbackQueryAnswer`; TDLib answers with `callbackQueryAnswer`).
    /// Only real (non-pending) messages in a supported chat can be answered.
    pub fn send_callback_query(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        data: &[u8],
    ) -> Result<RequestId, ConnectSendError> {
        let extra =
            self.callback_query_extra(chat_id, message_id, RequestPurpose::GetCallbackQueryAnswer)?;
        let json = get_callback_query_answer(extra, chat_id, message_id, data);
        self.send_json_request(extra, &json)
    }

    /// B1: password-protected callback button —
    /// `callbackQueryPayloadDataWithPassword` (schema 1.8.67, line 7740).
    /// The caller drops the password right after the call.
    pub fn send_callback_query_with_password(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        password: &str,
        data: &[u8],
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.callback_query_extra(
            chat_id,
            message_id,
            RequestPurpose::GetCallbackQueryAnswerWithPassword,
        )?;
        let json =
            get_callback_query_answer_with_password(extra, chat_id, message_id, password, data);
        self.send_json_request(extra, &json)
    }

    /// B1: game button — `callbackQueryPayloadGame` with the `game.short_name`
    /// from the message's `messageGame` content (schema 1.8.67, line 7743).
    pub fn send_game_callback_query(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        game_short_name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.callback_query_extra(
            chat_id,
            message_id,
            RequestPurpose::GetCallbackQueryAnswerGame,
        )?;
        let json = get_callback_query_answer_game(extra, chat_id, message_id, game_short_name);
        self.send_json_request(extra, &json)
    }

    /// Slice bots-games: `getGameHighScores` (TDLib 1.8.67,
    /// `schema/td_api.tl:13174`). The message id is stamped on the pending
    /// request (`request_for_message`) so the answer lands in the right
    /// scores panel.
    pub fn send_game_high_scores(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        user_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request_for_message(
            RequestPurpose::GetGameHighScores,
            chat_id,
            message_id,
        );
        let json = get_game_high_scores(extra, chat_id, message_id, user_id);
        self.send_json_request(extra, &json)
    }

    /// Slice bots-games: `sendMessage` + `inputMessageGame` (TDLib 1.8.67,
    /// line 6156) — send the bot's game to the chat. Not supported in
    /// channels or secret chats; the driver pre-checks the same way the
    /// other `sendMessage` paths do. Rides `RequestPurpose::SendMessage`
    /// so the optimistic row flows through the normal send path.
    pub fn send_game_message(
        &mut self,
        chat_id: ChatId,
        bot_user_id: i64,
        game_short_name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_post = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported() && chat.can_post());
        if !can_post {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        let json = send_game_request(extra, chat_id, topic_id, bot_user_id, game_short_name);
        self.send_json_request(extra, &json)
    }

    /// B1: resolve a login-URL button (`getLoginUrlInfo`, schema 1.8.67,
    /// line 12985). `fallback_url` is the button's raw URL, kept in the
    /// session so a TDLib error degrades to a plain URL-button press.
    pub fn send_login_url_info(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        button_id: i64,
        fallback_url: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let extra =
            self.callback_query_extra(chat_id, message_id, RequestPurpose::GetLoginUrlInfo)?;
        self.session.login_url_request = Some(LoginUrlRequest {
            chat_id,
            message_id,
            button_id,
            raw_url: fallback_url.to_string(),
        });
        let json = get_login_url_info(extra, chat_id, message_id, button_id);
        self.send_json_request(extra, &json)
    }

    /// B1: the authorized URL after the user consented to a
    /// `loginUrlInfoRequestConfirmation` (`getLoginUrl`, schema 1.8.67,
    /// line 12993; TGX `TGInlineKeyboard` does exactly this). `fallback_url`
    /// is the button's raw URL, used when TDLib errors.
    pub fn send_login_url(
        &mut self,
        request: &LoginUrlRequest,
        allow_write_access: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.callback_query_extra(
            request.chat_id,
            request.message_id,
            RequestPurpose::GetLoginUrl,
        )?;
        self.session.login_url_request = Some(request.clone());
        let json = get_login_url(
            extra,
            request.chat_id,
            request.message_id,
            request.button_id,
            allow_write_access,
        );
        self.send_json_request(extra, &json)
    }

    /// B1: `deleteChatReplyMarkup` after a one-time custom keyboard was used
    /// (schema 1.8.67, line 13183). Best-effort: the local keyboard hides
    /// regardless of the answer.
    pub fn delete_chat_reply_markup(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteChatReplyMarkup, Some(chat_id));
        let json = delete_chat_reply_markup_request(extra, chat_id, message_id);
        self.send_json_request(extra, &json)
    }

    /// Shared gate for callback-query sends: real (non-pending) messages in
    /// a supported chat. Returns the reserved `@extra`.
    pub(crate) fn callback_query_extra(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        purpose: RequestPurpose,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if message.pending || message.id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        Ok(self.session.request(purpose, Some(chat_id)))
    }

    /// Bots slice: `getInlineQueryResults` (TDLib 1.8.67, line 13019).
    /// `offset` is "" for the first chunk, the loaded page's
    /// `next_offset` for the next. No-op while a request is in flight for
    /// the (chat, bot) pair. A first page marks the single slot `Loading`
    /// (a re-query replaces it); pagination leaves the loaded page.
    pub fn inline_query(
        &mut self,
        bot_user_id: i64,
        chat_id: ChatId,
        query: &str,
        offset: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_inline_query_in_flight(chat_id, bot_user_id)
        {
            return Ok(None);
        }
        let first_page = offset.is_empty();
        if first_page {
            self.session.inline_query = Some(InlineQuerySlot {
                chat_id,
                bot_user_id,
                query: query.to_string(),
                fetch: InlineQueryFetch::Loading,
            });
        }
        let extra = self.session.request(
            RequestPurpose::GetInlineQueryResults {
                chat_id,
                bot_user_id,
                first_page,
            },
            Some(chat_id),
        );
        let json = get_inline_query_results(extra, bot_user_id, chat_id, query, offset);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                if first_page {
                    self.session.inline_query = None;
                }
                Err(err)
            }
        }
    }

    /// Bots slice: resolve `@botname` → bot user id via `searchPublicChat`
    /// (schema 1.8.67, line 11603). Sets the resolve slot to `Resolving`
    /// with a fresh generation; the answer (or error) resolves it in
    /// `Session::apply` (stale generations are ignored). A local
    /// user-cache hit short-circuits: the caller should check
    /// `session.users` first and skip this when the username is known.
    pub fn resolve_inline_bot(
        &mut self,
        username: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.inline_bot_resolve_seq = self.session.inline_bot_resolve_seq.wrapping_add(1);
        let generation = self.session.inline_bot_resolve_seq;
        self.session.inline_bot_resolve = Some(InlineBotResolve::Resolving {
            username: username.to_string(),
            generation,
        });
        let extra = self
            .session
            .request(RequestPurpose::ResolveInlineBot { generation }, None);
        let json = search_public_chat(extra, username);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.inline_bot_resolve = Some(InlineBotResolve::Failed {
                    username: username.to_string(),
                    reason: "could not look up the bot".to_string(),
                });
                Err(err)
            }
        }
    }

    /// Bots slice: send the picked inline result via
    /// `sendInlineQueryResultMessage` (schema 1.8.67, line 12226).
    /// `hide_via_bot` is false (TGX default); the schema always clears
    /// the chat draft on send, so the caller clears the composer first.
    /// Response is the sent `message`.
    pub fn send_inline_query_result(
        &mut self,
        chat_id: ChatId,
        query_id: i64,
        result_id: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SendInlineQueryResult, Some(chat_id));
        let json = send_inline_query_result_message(
            extra, chat_id, None, None, query_id, result_id, false,
        );
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    pub fn stop_pending_bot_message(
        &mut self,
        chat_id: ChatId,
        topic_id: i32,
        draft_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || topic_id < 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session
            .expire_pending_bot_messages(crate::state::unix_ms_now());
        let Some(draft) = self
            .session
            .pending_bot_messages
            .get(&(chat_id.0, topic_id))
        else {
            return Ok(None);
        };
        if draft.draft_id != draft_id || !draft.can_stop {
            return Ok(None);
        }
        let purpose = RequestPurpose::StopPendingMessage { topic_id, draft_id };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&crate::telegram::requests::stop_pending_message(
                extra,
                chat_id,
                (topic_id > 0).then_some(topic_id),
                draft_id,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }
}
