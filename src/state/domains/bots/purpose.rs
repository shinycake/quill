//! Request purposes for bots: inline queries, callback buttons, commands, games and login URLs.
use crate::state::request_purpose::flat_purposes;
use crate::state::*;

/// In-flight requests for bots: inline queries, callback buttons, commands, games and login URLs; wrapped as
/// [`RequestPurpose::Bots`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotsPurpose {
    StopPendingMessage {
        topic_id: i32,
        draft_id: i64,
    },
    /// Bots slice: `getInlineQueryResults` (schema 1.8.67, line 13019).
    /// Response is `inlineQueryResults`; the single active fetch lives in
    /// `Session::inline_query` (the composer has one active query).
    GetInlineQueryResults {
        chat_id: ChatId,
        bot_user_id: i64,
        first_page: bool,
    },
    /// Bots slice: `searchPublicChat` for `@botname` → bot user id
    /// resolution (schema 1.8.67, line 11603). Response is `chat`; the
    /// outcome lands in `Session::inline_bot_resolve`. `generation`
    /// matches `InlineBotResolve::Resolving::generation` so a stale
    /// answer (a newer username is already being resolved) is ignored —
    /// a `u64` keeps the `Copy` purpose enum intact.
    ResolveInlineBot {
        generation: u64,
    },
    /// Bots slice: `sendInlineQueryResultMessage` (schema 1.8.67, line
    /// 12226). Response is the sent `message`; failures surface through
    /// the normal message-send failure path.
    SendInlineQueryResult,
    /// `getCommands` for a bot's global (default) command scope (Phase
    /// 3.3). Response is `botCommands`; the user id is resolved from the
    /// request's chat. The schema annotates the method "for bots only",
    /// so a user session gets an `error` answer — absorbed silently, no
    /// retry loop.
    GetCommands,
    /// `getCallbackQueryAnswer` for an inline keyboard callback-button press
    /// (Phase 3.2). Response is `callbackQueryAnswer`; the answer is shown
    /// via the transient status line (URL answers open in the OS browser).
    GetCallbackQueryAnswer,
    /// B1: `getCallbackQueryAnswer` with
    /// `callbackQueryPayloadDataWithPassword` (password-protected button).
    /// A 400 answer surfaces as "wrong 2-step password" instead of the
    /// generic callback note.
    GetCallbackQueryAnswerWithPassword,
    /// B1: `getCallbackQueryAnswer` with `callbackQueryPayloadGame` (game
    /// button); the answer URL (if any) opens the game in the OS browser.
    GetCallbackQueryAnswerGame,
    /// Slice bots-games: `getGameHighScores` after a Scores press.
    /// Response is `gameHighScores`; the panel shows a loading row until
    /// it lands, and an error closes the panel with a status note.
    GetGameHighScores,
    /// B1: `getLoginUrlInfo` for a login-URL button press. Response is
    /// `loginUrlInfo*`; on error the button degrades to a plain URL button
    /// (schema 1.8.67 doc on `getLoginUrl`).
    GetLoginUrlInfo,
    /// B1: `getLoginUrl` after the user consented to a
    /// `loginUrlInfoRequestConfirmation`. Response is `httpUrl`; on error
    /// the button degrades to a plain URL button (schema 1.8.67 doc on
    /// `getLoginUrl`).
    GetLoginUrl,
    /// B1: `deleteChatReplyMarkup` after a one-time custom keyboard is used
    /// (schema 1.8.67, line 13183).
    DeleteChatReplyMarkup,
    /// Slice B2: `sendBotStartMessage` (schema 1.8.67, line 12216) from
    /// the START button / "Restart bot". Response is the sent `message`.
    SendBotStartMessage,
    /// Slice B2: `getBotSimilarBots` (schema 1.8.67, line 11640) for the
    /// similar-bots section of the bot profile. Response is `users`;
    /// the bot ids land in `Session::similar_bots` (keyed by the pending
    /// request's `user_id`).
    GetBotSimilarBots,
    /// `getMessage` for `chat.reply_markup_message_id` when that message is
    /// not in the loaded history; the `message` answer feeds the chat's
    /// reply keyboard.
    GetChatReplyMarkupMessage,
    /// `shareUsersWithBot` / `shareChatWithBot` / `sharePhoneNumber`
    /// (schema 1.8.67, lines 13001 / 13010 / 14584). Response is `ok`.
    ShareWithBot,
    /// `getRecentInlineBots` (schema 1.8.67, line 14776). Response is
    /// `users`.
    GetRecentInlineBots,
    /// Mini apps (docs/decisions/codex-miniapp-webview.md). The open calls
    /// answer `webAppInfo` / `webAppUrl` / `mainWebApp`; the context of
    /// the launch waits in `Session::web_apps.pending`.
    OpenWebApp {
        bot_user_id: i64,
    },
    GetWebAppUrl {
        bot_user_id: i64,
    },
    GetMainWebApp {
        bot_user_id: i64,
    },
    GetWebAppLinkUrl {
        bot_user_id: i64,
    },
    /// `searchWebApp` for a `t.me/bot/app` link; answers `foundWebApp`.
    SearchWebApp {
        bot_user_id: i64,
    },
    CloseWebApp,
    SendWebAppData,
    /// `canBotSendMessages`: ok, or 404 when consent is needed.
    CanBotSendMessages {
        bot_user_id: i64,
    },
    AllowBotToSendMessages {
        bot_user_id: i64,
    },
    GetAttachmentMenuBot {
        bot_user_id: i64,
    },
    ToggleBotInAttachmentMenu {
        bot_user_id: i64,
        added: bool,
    },
    /// `getGrossingWebAppBots` for the Apps tab; answers `foundUsers`.
    GetGrossingWebAppBots,
    /// `sendWebAppCustomRequest`; the app's `req_id` waits in
    /// `Session::web_apps.custom_requests`.
    SendWebAppCustomRequest,
}

flat_purposes!(Bots(BotsPurpose) {
    SendInlineQueryResult,
    GetCommands,
    GetCallbackQueryAnswer,
    GetCallbackQueryAnswerWithPassword,
    GetCallbackQueryAnswerGame,
    GetGameHighScores,
    GetLoginUrlInfo,
    GetLoginUrl,
    DeleteChatReplyMarkup,
    SendBotStartMessage,
    GetBotSimilarBots,
    GetChatReplyMarkupMessage,
    ShareWithBot,
    GetRecentInlineBots,
    CloseWebApp,
    SendWebAppData,
    GetGrossingWebAppBots,
    SendWebAppCustomRequest,
});
