//! Bots: info, commands, games, inline queries, callback answers, login URLs, mini apps, reply keyboards: the `bots` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;
use crate::telegram::envelope::GroupBotCommands;

/// A bot's answer to an inline button press, with the chat of the button's
/// message: a URL in it opens as a link clicked in the window showing that
/// chat. One value, so the two are always written and taken together
/// (`Session::note_callback_answer`).
#[derive(Debug, Clone, PartialEq)]
pub struct CallbackAnswer {
    pub answer: CallbackQueryAnswer,
    pub chat: Option<ChatId>,
}

/// Which group a `bot_commands` list belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupCommandsKey {
    BasicGroup(i64),
    Supergroup(i64),
}

pub struct BotsState {
    /// `userTypeBot` ids from `updateUser`. Private chats with these users skip drafts.
    pub(crate) bot_user_ids: HashSet<i64>,
    /// Cached `botInfo` from `getUserFullInfo` / `updateUserFullInfo`, keyed
    /// by bot user id. `None` records "fetched, not a bot" so a null
    /// `bot_info` does not trigger a refetch loop.
    pub bot_info: HashMap<i64, Option<BotInfo>>,
    /// Slice B2: pending bot `start_parameter` per chat
    /// (`internalLinkTypeBotStart`, schema 1.8.67 line 9399) — from a
    /// `t.me/<bot>?start=<param>` deep link. The UI shows the START
    /// button while set; pressing it sends `sendBotStartMessage` (line
    /// 12216) with the parameter and clears the entry.
    pub bot_start_params: HashMap<i64, String>,
    /// Slice bots-games: games seen via `messageGame` in a bot's chat,
    /// keyed by bot user id. Only short names TDLib actually delivered
    /// are cached — the bot info panel's Send buttons never offer an
    /// invented short name.
    pub bot_games: HashMap<i64, Vec<GameInfo>>,
    /// Cached `getCommands` results for the default scope (a null `scope`
    /// selects `botCommandScopeDefault`, Phase 3.3), keyed by bot user id. Presence records "fetched"
    /// so the driver never retries — including when the response was an
    /// `error` (user sessions; `getCommands` is annotated "for bots only").
    pub bot_commands: HashMap<i64, Vec<BotCommand>>,
    /// The commands of the bots in a group, from the group's full info
    /// (`bot_commands`); feeds the composer's `/` menu in groups.
    pub group_bot_commands: HashMap<GroupCommandsKey, Vec<GroupBotCommands>>,
    /// Bots named in a group's `bot_commands` whose user is not cached;
    /// the driver asks for them (`getUser`) and clears the list.
    pub bot_users_wanted: Vec<i64>,
    /// Bot users already asked for, so each is requested once.
    pub asked_bot_users: HashSet<i64>,
    /// Slice B2: `getBotSimilarBots` results (schema 1.8.67, line 11640)
    /// for the similar-bots section of the bot profile, keyed by bot
    /// user id. The `users` ids resolve to names via `Session::users`.
    pub similar_bots: HashMap<i64, SimilarBotsFetch>,
    /// Slice bots-games: high-score panels, keyed by
    /// `(chat_id, message_id)`. Present = panel open; `None` = request in
    /// flight (the panel shows a loading row); `Some` = loaded rows.
    pub game_scores: HashMap<(i64, i64), Option<Vec<GameHighScore>>>,
    /// Last `callbackQueryAnswer` to an inline keyboard callback-button press
    /// (Phase 3.2), with the chat of the press. The UI takes it on the next
    /// poll and shows the answer in the status line (URL answers open in the
    /// OS browser).
    pub last_callback_answer: Option<CallbackAnswer>,
    /// Inline button presses and the requests they sent
    /// (`Session::callback_button_waiting`).
    pub callback_presses: Vec<CallbackPress>,
    /// Mini apps (docs/decisions/codex-miniapp-webview.md): open answers,
    /// consent answers and the attachment menu bots.
    pub web_apps: WebApps,
    /// B1: last `loginUrlInfo*` / `httpUrl` answer for a login-URL button
    /// press. The UI takes it on the next poll: `Open` opens the URL in the
    /// OS browser, `RequestConfirmation` asks for consent, `Failed` opens
    /// the button's raw URL in the browser.
    pub last_login_url_info: Option<LoginUrlInfo>,
    /// B1: the login button's request context, kept while `getLoginUrlInfo`
    /// (then `getLoginUrl`) is in flight so an error can degrade to a plain
    /// URL button press.
    pub login_url_request: Option<LoginUrlRequest>,
    /// B1: force-reply targets set when an incoming message carrying
    /// force-reply markup (`replyMarkupForceReply`, or `force_reply` on an
    /// inline / show-keyboard markup) arrives, one per chat (a newer
    /// message of the chat replaces the older). Each window arms its own
    /// chat's on a frame: composer gets the reply-to and focus; the other
    /// chats' wait until a window shows them.
    pub pending_force_replies: std::collections::HashMap<crate::ids::ChatId, ForceReplyTarget>,
    /// Bot reply keyboards as TDLib reports them, and recent inline bots.
    pub reply_keyboards: ReplyKeyboardState,
    /// Bots slice: the single active `getInlineQueryResults` fetch (the
    /// composer has one active inline query, so a slot — not a map).
    pub inline_query: Option<InlineQuerySlot>,
    /// Bots slice: `@botname` → bot user id resolution for inline mode.
    /// Set by the driver before `searchPublicChat`; the answer (or error)
    /// resolves it. The composer has one active trigger, so a single
    /// slot — not a map.
    pub inline_bot_resolve: Option<InlineBotResolve>,
    /// Bots slice: generation counter for `ResolveInlineBot` request
    /// correlation (bumped per resolve; see the purpose docs).
    pub inline_bot_resolve_seq: u64,
    /// Editing an owned bot and creating a managed one.
    pub owned: OwnedBotsState,
    /// Message widgets: TDLib's answers to their queries and public data
    /// polls, in order, until the UI takes them for the runtimes.
    pub widget_answers: Vec<crate::message_widget::WidgetAnswer>,
}

impl BotsState {
    pub(crate) fn new() -> Self {
        Self {
            bot_user_ids: HashSet::new(),
            bot_info: HashMap::new(),
            bot_start_params: HashMap::new(),
            bot_games: HashMap::new(),
            bot_commands: HashMap::new(),
            group_bot_commands: HashMap::new(),
            bot_users_wanted: Vec::new(),
            asked_bot_users: HashSet::new(),
            similar_bots: HashMap::new(),
            game_scores: HashMap::new(),
            last_callback_answer: None,
            callback_presses: Vec::new(),
            web_apps: WebApps::default(),
            last_login_url_info: None,
            login_url_request: None,
            pending_force_replies: std::collections::HashMap::new(),
            reply_keyboards: ReplyKeyboardState::default(),
            inline_query: None,
            inline_bot_resolve: None,
            inline_bot_resolve_seq: 0,
            owned: OwnedBotsState::default(),
            widget_answers: Vec::new(),
        }
    }
}
