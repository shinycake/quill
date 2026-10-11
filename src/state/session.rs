//! The Session reducer: central client state and constructor.
use super::*;

/// MED4b: composer `getLinkPreview` prefetch state (TGX `LinkPreview`).
#[derive(Debug, Clone, Default)]
pub struct ComposerLinkPreview {
    /// URL the prefetch was requested for.
    pub url: String,
    /// `None` while the request is in flight; `Some(None)` when TDLib
    /// 404s (no preview for this URL) — a refusal is never rendered as
    /// a card.
    pub preview: Option<Option<LinkPreview>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownPhase {
    Running,
    CloseRequested,
    WaitingClosed,
    Closed,
}

#[derive(Debug, Clone)]
pub struct PendingBotMessage {
    pub draft_id: i64,
    pub can_stop: bool,
    pub keep_on_stop: bool,
    pub content: MessageContent,
    pub expires_at_ms: u64,
    pub stop_failed: bool,
    pub stopped: bool,
}

pub struct Session {
    pub account: AccountKey,
    pub account_generation: AccountGeneration,
    pub auth: AuthorizationState,
    pub auth_view: AuthView,
    pub connection: ConnectionState,
    pub chats: HashMap<i64, ChatSummary>,
    pub main_order: Vec<ChatId>,
    pub archive_order: Vec<ChatId>,
    /// The chat list: folders, archive, unread counts, limits, suggestions and previews.
    /// Declared in `src/state/domains/chat_list/state.rs`.
    pub chat_list: ChatListState,
    /// Chat-level look and actions: backgrounds, themes, accents, action bars, deep links, reactions, send-as.
    /// Declared in `src/state/domains/chats/state.rs`.
    pub chats_state: ChatsState,
    pub histories: HashMap<i64, HistoryState>,
    /// History page requests issued for a window that has since been
    /// replaced (`reset_history_window`): their answers are dropped so an
    /// old page can't land in the new window and fake contiguity.
    pub(crate) stale_history_requests: HashSet<u64>,
    /// Bumped for every applied TDLib envelope: views that cache derived
    /// state (history rows) rebuild after any server-driven change.
    pub revision: u64,
    /// Messages: the menu, reports, links, AI drafts, limits, link previews, scheduling, forwarding, polls, drafts, sponsored rows, translation, export.
    /// Declared in `src/state/domains/messages/state.rs`.
    pub messages: MessagesState,
    /// Files, downloads, the shared-media gallery, the media library and map thumbnails.
    /// Declared in `src/state/domains/media/state.rs`.
    pub media: MediaState,
    /// Stickers, custom emoji, GIFs and reactions.
    /// Declared in `src/state/domains/stickers/state.rs`.
    pub stickers: StickersState,
    /// Groups and channels: members, admins, rights, invite links, join requests, boosts, communities, event logs.
    /// Declared in `src/state/domains/groups/state.rs`.
    pub groups: GroupsState,
    pub open_chat: Option<ChatId>,
    /// Phase 8.1: whether the OS considers our window focused. The UI sets
    /// this from `Window::is_window_active` on every render; it defaults to
    /// true so the reducer never notifies before the first paint measures it.
    pub app_active: bool,
    /// Notifications, storage, privacy, sessions, websites, the account and preferences.
    /// Declared in `src/state/domains/settings/state.rs`.
    pub settings: SettingsState,
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
    pub calls: CallsState,
    /// Phase 5.1: selected forum topic (`forum_topic_id`) of the open chat.
    /// `None` = topic list (or a non-forum chat). Reset by `open_chat`.
    pub open_topic: Option<i32>,
    /// Forum topics, comment threads and Saved Messages.
    /// Declared in `src/state/domains/threads/state.rs`.
    pub threads: ThreadsState,
    pub view_generation: ViewGeneration,
    pub requests: RequestRegistry,
    pub shutdown: ShutdownPhase,
    pub last_seq: u64,
    /// Bots: info, commands, games, inline queries, callback answers, login URLs, mini apps, reply keyboards.
    /// Declared in `src/state/domains/bots/state.rs`.
    pub bots: BotsState,
    /// Payments, receipts, Stars subscriptions, Premium and gifts.
    /// Declared in `src/state/domains/payments/state.rs`.
    pub payments: PaymentsState,
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
    pub stories: StoriesState,
    pub(crate) diagnostics: Arc<dyn DiagnosticSink>,
}
