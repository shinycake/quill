//! The Session reducer: central client state and constructor.
use super::*;

/// How the session is ending, one value. `logOut` is a stage of it: sent
/// under its request, answered by the auth states that follow, or refused
/// (back to running, the refusal waiting for the UI to say so once).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShutdownPhase {
    Running,
    /// `logOut` is out under this request. A refusal returns the session
    /// to `Running` (via `LogoutRefused`); a quit asked for meanwhile
    /// replaces the stage, so the refusal no longer applies.
    LogoutRequested(RequestId),
    /// TDLib refused the `logOut`: the account stays signed in and running.
    /// The UI drains it once (`Session::take_logout_refusal`).
    LogoutRefused(TdError),
    CloseRequested,
    WaitingClosed,
    Closed,
}

impl ShutdownPhase {
    /// The session is not ending (a refused `logOut` leaves it signed in).
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running | Self::LogoutRefused(_))
    }
}

#[derive(Debug, Clone)]
pub struct PendingBotMessage {
    pub draft_id: i64,
    pub can_stop: bool,
    pub keep_on_stop: bool,
    pub content: MessageContent,
    pub expires_at_ms: u64,
    pub stop: BotStopStage,
}

pub struct Session {
    pub account: AccountKey,
    pub account_generation: AccountGeneration,
    pub auth: AuthorizationState,
    /// Resumed from TDLib's database, or signed in by a step taken in this
    /// process (`set_auth`); `account_data::claim` tells them apart.
    pub sign_in_origin: crate::account_data::SignInOrigin,
    pub auth_view: AuthView,
    pub connection: ConnectionState,
    pub chats: HashMap<i64, ChatSummary>,
    pub main_order: Vec<ChatId>,
    pub archive_order: Vec<ChatId>,
    /// Whether the two orders above need a sort.
    pub(crate) chat_order_sync: super::session_chatlist::ChatOrderSync,
    /// The chat list: folders, archive, unread counts, limits, suggestions and previews.
    /// Declared in `src/state/domains/chat_list/state.rs`.
    pub chat_list: Box<ChatListState>,
    /// Chat-level look and actions: backgrounds, themes, accents, action bars, deep links, reactions, send-as.
    /// Declared in `src/state/domains/chats/state.rs`.
    pub chats_state: Box<ChatsState>,
    pub histories: HashMap<i64, HistoryState>,
    /// Which closed chats and topics keep a history tail (`history_prune.rs`).
    pub history_pruning: HistoryPruning,
    /// Bumped for every applied TDLib envelope and every optimistic unread
    /// change: views that cache derived state (history rows, badges) rebuild.
    pub revision: u64,
    /// Messages: the menu, reports, links, AI drafts, limits, link previews, scheduling, forwarding, polls, drafts, sponsored rows, translation, export.
    /// Declared in `src/state/domains/messages/state.rs`.
    pub messages: Box<MessagesState>,
    /// Files, downloads, the shared-media gallery, the media library and map thumbnails.
    /// Declared in `src/state/domains/media/state.rs`.
    pub media: Box<MediaState>,
    /// Stickers, custom emoji, GIFs and reactions.
    /// Declared in `src/state/domains/stickers/state.rs`.
    pub stickers: Box<StickersState>,
    /// Groups and channels: members, admins, rights, invite links, join requests, boosts, communities, event logs.
    /// Declared in `src/state/domains/groups/state.rs`.
    pub groups: Box<GroupsState>,
    /// What the main window shows: no chat, a chat, or a forum topic of it
    /// (`open_chat_id()` / `open_forum_topic()`). Leaving the chat leaves the
    /// topic; opening another chat drops it.
    pub open: OpenView,
    /// Chats shown in a window of their own (`parity:platform-new-window`), with each window's view; they stay open in TDLib and page their history while the open chat changes.
    pub window_chats: WindowViews,
    /// The window chats whose window is the active one: their messages are read on arrival, so they do not notify.
    pub active_window_chats: std::collections::BTreeSet<ChatId>,
    /// Phase 8.1: whether the OS considers our window focused. The UI sets
    /// this from `Window::is_window_active` on every render; it defaults to
    /// true so the reducer never notifies before the first paint measures it.
    pub app_active: bool,
    /// Notifications, storage, privacy, sessions, websites, the account and preferences.
    /// Declared in `src/state/domains/settings/state.rs`.
    pub settings: Box<SettingsState>,
    /// Account-level sync updates: silent default, downloads, dice,
    /// freeze, speech quota, live shares, age verification.
    pub sync: UpdatesSync,
    /// Sign-in errors, countries, the phone-number change and two-step verification.
    /// Declared in `src/state/domains/auth/state.rs`.
    pub auth_state: AuthState,
    /// Users: contacts, profiles, profile photos, secret chats and the info panel.
    /// Declared in `src/state/domains/users/state.rs`.
    pub users_state: UsersState,
    /// One-to-one calls, group calls, recent calls and call privacy.
    /// Declared in `src/state/domains/calls/state.rs`.
    pub calls: Box<CallsState>,
    /// Forum topics, comment threads and Saved Messages.
    /// Declared in `src/state/domains/threads/state.rs`.
    pub threads: Box<ThreadsState>,
    pub view_generation: ViewGeneration,
    pub requests: RequestRegistry,
    /// The answer to each request a surface waits on, per request id.
    pub acks: RequestAcks,
    pub shutdown: ShutdownPhase,
    pub last_seq: u64,
    /// Bots: info, commands, games, inline queries, callback answers, login URLs, mini apps, reply keyboards.
    /// Declared in `src/state/domains/bots/state.rs`.
    pub bots: Box<BotsState>,
    /// TDLib 1.8.70 community requests; `src/state/domains/communities/state.rs`.
    pub communities: Box<CommunitiesState>,
    /// Payments, receipts, Stars subscriptions, Premium and gifts.
    /// Declared in `src/state/domains/payments/state.rs`.
    pub payments: Box<PaymentsState>,
    /// Read-only TON wallet state (`crate::ton_wallet`).
    pub ton_wallet: crate::ton_wallet::TonWalletState,
    /// Send Location's map picker and venue bot (`crate::location_picker`).
    pub location: crate::location_picker::LocationSharing,
    /// Global search, search in chat and date jumps.
    /// Declared in `src/state/domains/search/state.rs`.
    pub search: SearchDomainState,
    /// Own user id: TDLib's `my_id` option (pushed after authorization),
    /// or a `getMe` answer. `None` until either arrives.
    pub my_user_id: Option<i64>,
    /// Phase 6: user directory from `updateUser`, keyed by user id. Feeds
    /// the contacts list and the user info panel.
    pub users: HashMap<i64, ParsedUser>,
    /// Stories, the story tray, albums, archive, posting, viewers and close friends.
    /// Declared in `src/state/domains/stories/state.rs`.
    pub stories: Box<StoriesState>,
    pub(crate) diagnostics: Arc<dyn DiagnosticSink>,
}
