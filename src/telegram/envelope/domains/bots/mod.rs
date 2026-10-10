//! TDLib updates and answers for bots: inline queries, callback buttons, commands, games and login URLs.
mod parse;

use crate::ids::{ChatId, MessageId, UserId};
use crate::telegram::envelope::*;
pub(crate) use parse::parse_bots_payload;

/// Payloads for bots: inline queries, callback buttons, commands, games and login URLs; wrapped as
/// [`EnvelopePayload::Bots`].
#[derive(Debug, Clone, PartialEq)]
pub enum BotsPayload {
    /// Bots slice: `inlineQueryResults` (TDLib 1.8.67,
    /// `schema/td_api.tl:7716`) — the `getInlineQueryResults` answer
    /// (schema line 13019). One page of result summaries; the reducer
    /// appends pages into `Session::inline_query`.
    InlineQueryResults(InlineQueryResultsPage),
    /// `updateChatReplyMarkup` (schema 1.8.67, line 10558): the message
    /// whose reply markup the chat shows changed. `message_id` is `None`
    /// when the markup was removed; `reply_markup` is that message's markup.
    UpdateChatReplyMarkup {
        chat_id: ChatId,
        message_id: Option<MessageId>,
        reply_markup: Option<ReplyMarkup>,
    },
    /// `botCommands` — `getCommands` response (TDLib 1.8.67,
    /// `schema/td_api.tl:829`): the bot's commands for the requested scope
    /// as a bare `vector<botCommand>`. The schema annotates `getCommands`
    /// "for bots only" (line 14953); on a user session the response is an
    /// `error` instead, which `Session::apply` absorbs silently. The
    /// response's `bot_user_id` is cached as the global-scope command set
    /// shown below the bot's `botInfo` commands in the 3.3 `/` menu.
    BotCommands {
        bot_user_id: UserId,
        commands: Vec<BotCommand>,
    },
    /// `callbackQueryAnswer` — response to `getCallbackQueryAnswer` after an
    /// inline keyboard callback-button press (Phase 3.2).
    CallbackQueryAnswer(CallbackQueryAnswer),
    /// Slice bots-games: `gameHighScores` — response to `getGameHighScores`
    /// (TDLib 1.8.67, `schema/td_api.tl:13174`).
    GameHighScores(Vec<GameHighScore>),
    /// B1: `loginUrlInfo*` — response to `getLoginUrlInfo` after a
    /// login-URL button press.
    LoginUrlInfo(LoginUrlInfo),
}
