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
    /// The composer's `@` suggestions for the open chat.
    pub mention_search: Option<MentionSearch>,
    /// The emoji/sticker panel's set contents (`getStickerSet`, lazily).
    pub media_library: MediaLibrary,
    /// Reaction options for the message whose reaction picker is open.
    pub message_reaction_options: Option<MessageReactionOptions>,
    /// `updateActiveEmojiReactions`: the emoji reactions Telegram offers.
    pub active_reactions: Vec<String>,
    /// `updateDefaultReactionType`: the quick reaction (double-click).
    pub default_reaction: Option<ReactionChoice>,
    /// The reaction options of the open picker are out of date.
    pub reaction_options_stale: bool,
    /// What the open message context menu may offer (`messageProperties`).
    pub message_menu_actions:
        Option<(ChatId, MessageId, crate::telegram::envelope::MessageActions)>,
    /// The message menu's Report flow (`reportChat` with message ids).
    pub message_report: Option<MessageReportFlow>,
    /// Viewers, read date and reactors of the message the menu is open on.
    pub message_audience: Option<MessageAudience>,
    /// A reaction chip was right-clicked: its "who reacted" tab opens as
    /// soon as the message's audience is loaded.
    pub wanted_reactor_tab: Option<(ChatId, MessageId, crate::telegram::envelope::ReactionType)>,
    /// The sticker set the message menu's "View Sticker Set" opened.
    pub sticker_set_view: Option<StickerSetView>,
    /// The pack of the custom emoji the user just tapped in a message.
    pub custom_emoji_preview: Option<CustomEmojiPreview>,
    /// Titles of the emoji packs a message uses, by set id, for the menu's
    /// "This message contains emoji from X pack" footer.
    pub emoji_pack_titles: HashMap<i64, String>,
    /// One-shot result of an admin moderation call from the delete box
    /// (ban, delete all, report spam); the UI drains it into the status
    /// note.
    pub message_action_note: Option<String>,
    /// Groups and channels: members, admins, rights, invite links, join requests, boosts, communities, event logs.
    /// Declared in `src/state/domains/groups/state.rs`.
    pub groups: GroupsState,
    /// M1: parsed `messageLink.link` from the last `getMessageLink` response
    /// (one-shot; the UI copies it to the clipboard and clears it).
    pub message_link_result: Option<String>,
    /// `messageLink.is_public` of that answer: a public link works for
    /// anyone, a private one only for chat members (Telegram Desktop
    /// words the copied-toast differently).
    pub message_link_public: bool,
    /// M1 fix-up: one-shot; set when "Share link" is gated off by
    /// `messageProperties.can_get_link == false` or the `getMessageLink`
    /// request errors. The UI drains it into the status note so the
    /// click never silently does nothing.
    pub message_link_error: Option<String>,
    /// Slice msg-richtext-ai-tools: one-shot `fixTextWithAi` /
    /// `composeTextWithAi` answer for the open chat's composer. The UI
    /// drains it (replacing the draft) on the next frame; the chat id
    /// guards against applying to a chat the user has since left.
    pub ai_composer_text: Option<(ChatId, String)>,
    /// Slice msg-richtext-ai-tools: one-shot `composeRichMessageWithAi`
    /// / `createRichMessageWithAi` / `fixRichMessageWithAi` answer for
    /// the open chat's composer. Same drain contract as
    /// `ai_composer_text`. The UI writes the blocks back as editor markup
    /// (`blocks_to_markup`); the note distinguishes create / fix / rewrite.
    pub ai_composer_blocks: Option<(ChatId, RichMessageContent, &'static str)>,
    /// Slice msg-richtext-ai-tools: one-shot; set when an AI request
    /// errors. The UI drains it into the status note so the click never
    /// silently does nothing.
    pub ai_error: Option<String>,
    /// MED4: `getOption("message_caption_length_max")` via `updateOption`
    /// (TDLib 1.8.67, `schema/td_api.tl:10926`); default 1024 is TDLib's
    /// compiled default. Guards caption edits and media-send captions.
    pub message_caption_length_max: i32,
    /// R8: `getOption("message_text_length_max")` via `updateOption`; 4096 is
    /// the compiled default (Premium raises it). Plain text sends are cut
    /// into several messages at this size (tdesktop `CutPart`); edits over
    /// it are refused.
    pub message_text_length_max: i32,
    /// MED4: one-shot `getWebPageInstantView` answer for the IV reader.
    /// The UI drains it (opens the reader) and clears it.
    pub instant_view: Option<InstantViewPage>,
    /// MED4: one-shot fallback URL when `getWebPageInstantView` errors
    /// (TDLib 404s when the page has no Instant View). The UI drains it
    /// into the browser — TGX behaves the same.
    pub instant_view_fallback_url: Option<String>,
    /// MED4: pending `getWebPageInstantView` URLs by `RequestId`
    /// (`RequestPurpose` stays `Copy`, so the URL rides here).
    pub instant_view_urls: HashMap<RequestId, String>,
    /// MED4b: composer `getLinkPreview` prefetch state — the chip reads
    /// this. Replaced on every new request; cleared when the composer's
    /// detected URL changes away or the composer is submitted.
    pub composer_preview: Option<ComposerLinkPreview>,
    /// MED4b: pending `getLinkPreview` URLs by `RequestId` (same
    /// `Copy`-purpose pattern as `instant_view_urls`).
    pub composer_preview_urls: HashMap<RequestId, String>,
    /// MED2 fix-up: one-shot; set when TDLib refuses a `recognizeSpeech`
    /// request. The UI drains it into the status note — previously the
    /// error fell into the `_ => {}` swallower and the user saw
    /// "transcription requested" followed by silence.
    pub recognize_speech_error: Option<String>,
    /// M1 fix-up: one-shot; set when a `resendMessages` request errors.
    /// The UI drains it into the status note — previously the error fell
    /// into the `_ => {}` swallower and the user saw "retrying send…"
    /// followed by silence.
    pub resend_error: Option<String>,
    pub send_permission_error: Option<String>,
    /// Q1: one-shot; "Too many attempts. Try again in N seconds." after a
    /// user action (send, edit, join, ...) hit a rate limit. The UI drains
    /// it into the status note; the composer text is left untouched.
    pub flood_notice: Option<String>,
    /// M1: `getChatScheduledMessages` results — the chat's scheduled sends,
    /// with `scheduling_state` showing the planned send time.
    pub scheduled_messages: Vec<ParsedMessage>,
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
    /// Phase B1: secret-chat records keyed by `secret_chat_id`
    /// (`updateSecretChat` / `getSecretChat` answers). Kept at the
    /// session level because `updateSecretChat` is guaranteed to arrive
    /// *before* the chat identifier is returned (schema 1.8.67, line
    /// 10740), so a secret chat's chat summary may not exist yet when
    /// its state does. The full record (including `key_hash`) is kept
    /// for the B2 key-verification UI.
    pub secret_chat_states: HashMap<i32, ParsedSecretChat>,
    /// Phase B1: secret_chat_ids whose state still needs a `getSecretChat`
    /// fetch (e.g. a secret chat loaded from the local DB with no state
    /// seen yet). Drained by the driver's `maybe_fetch_secret_chat_states`.
    pub secret_chat_fetch_queue: Vec<i32>,
    /// One-to-one calls, group calls, recent calls and call privacy.
    /// Declared in `src/state/domains/calls/state.rs`.
    pub calls: CallsState,
    /// `getSupportUser` answer waiting for the driver to open the chat
    /// (Settings > Ask a Question).
    pub support_user_ready: Option<i64>,
    /// Replied-to messages outside the loaded window, keyed by the
    /// replying message `(chat_id, message_id)`.
    pub reply_targets: HashMap<(i64, i64), ReplyTarget>,
    /// Phase 5.1: selected forum topic (`forum_topic_id`) of the open chat.
    /// `None` = topic list (or a non-forum chat). Reset by `open_chat`.
    pub open_topic: Option<i32>,
    pub pending_bot_messages: HashMap<(i64, i32), PendingBotMessage>,
    pub pending_bot_period_secs: u64,
    /// Forum topics, comment threads and Saved Messages.
    /// Declared in `src/state/domains/threads/state.rs`.
    pub threads: ThreadsState,
    /// `poll.id` → `(chat_id, message_id)` of rows loaded with that poll,
    /// so `updatePoll` (which carries no chat or message id) touches only
    /// its rows. Entries can be stale; `apply_update_poll` re-checks and
    /// prunes them.
    pub(crate) poll_messages: HashMap<i64, HashSet<(i64, i64)>>,
    pub view_generation: ViewGeneration,
    pub requests: RequestRegistry,
    pub shutdown: ShutdownPhase,
    pub last_seq: u64,
    /// In-flight `forwardMessages` (dest / source / requested count).
    pub in_flight_forward: Option<ForwardFlight>,
    /// Further `forwardMessages` in flight while the share box sends to
    /// several chats at once (`in_flight_forward` holds the first).
    pub queued_forward_flights: Vec<ForwardFlight>,
    /// Share box search (local `searchChats` + `searchChatsOnServer`).
    pub share_search: ShareSearch,
    /// Last `forwardMessages` outcome for the dest picker success surface.
    pub last_forward: Option<ForwardResult>,
    /// Last `callbackQueryAnswer` to an inline keyboard callback-button press
    /// (Phase 3.2). The UI takes it on the next poll and shows the answer in
    /// the status line (URL answers open in the OS browser).
    pub last_callback_answer: Option<CallbackQueryAnswer>,
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
    /// Payments, receipts, Stars subscriptions, Premium and gifts.
    /// Declared in `src/state/domains/payments/state.rs`.
    pub payments: PaymentsState,
    /// Notification-tone limits (`notification_sound_*_max` options).
    pub tone_limits: crate::message_menu::ToneLimits,
    /// B1: force-reply target set when an incoming message carrying
    /// force-reply markup (`replyMarkupForceReply`, or `force_reply` on an
    /// inline / show-keyboard markup) arrives. The UI drains it on the
    /// next render: composer gets the reply-to and focus.
    pub pending_force_reply: Option<ForceReplyTarget>,
    /// TDLib `file.id` → latest `file` / `localFile` snapshot.
    pub files: HashMap<i32, ParsedFile>,
    /// `downloadFile` in flight (until completed, undownloadable, idle, or error).
    pub downloading: HashSet<i32>,
    /// Subset of `downloading` the user explicitly started (history rows,
    /// viewer) — these surface in the downloads manager. Automatic
    /// thumbs/avatars/sounds are not tracked here.
    pub user_downloads: HashSet<i32>,
    /// Subset of `user_downloads` currently paused via
    /// `toggleDownloadIsPaused`. Pause is list state (`updateFileDownload`)
    /// tracked separately from the in-flight download above.
    pub paused_downloads: HashSet<i32>,
    /// `downloadFile` requests TDLib answered with an error (file id → still
    /// in `downloading` until unstuck; the UI shows "failed — retry").
    /// Cleared when a new download starts or the file completes.
    pub failed_downloads: HashSet<i32>,
    /// Automatic (non-user) downloads TDLib refused or stopped without
    /// completing. The automatic path skips them so each ingest does not
    /// re-send `downloadFile`; an explicit user download or completion
    /// clears the mark (Telegram X `TdlibFilesManager.onFileUpdate` treats
    /// a stopped download as paused until asked again).
    pub stalled_auto_downloads: HashSet<i32>,
    /// Automatic downloads of full media in the open chat (MED3), whose
    /// progress the history draws: their `updateFile`s redraw at once
    /// (`redraw_need`). Pruned to `downloading` on each pass.
    pub open_chat_media_downloads: HashSet<i32>,
    /// Avatar file id → number of chats / users showing it (chat-list
    /// photos and contact `photo_small`), kept where those ids are set.
    pub(crate) avatar_file_refs: HashMap<i32, u32>,
    /// Avatar file ids whose download state may have changed since the
    /// driver last looked (`take_due_chat_list_photos`).
    pub(crate) avatar_downloads_due: BTreeSet<i32>,
    /// Re-check every avatar once (new session, any auth change: requests
    /// were invalidated and files may have been cleared).
    pub(crate) avatar_rescan: bool,
    /// Recently completed downloads (file ids, most recent last, capped) for
    /// the downloads manager's "recent" list. Recorded only when a file was
    /// in `downloading` and its `updateFile` shows completion — pre-existing
    /// local files don't count.
    pub completed_downloads: VecDeque<i32>,
    /// Downloads manager panel open (right side, next to the info panel).
    pub downloads_panel_open: bool,
    /// `@extra` → `file.id` until the download unsticks (survives `file@extra` consuming pending).
    pub(crate) download_extras: HashMap<u64, i32>,
    pub search: SearchState,
    pub chat_search: ChatSearchState,
    /// Each chat's pinned messages, newest first, as last fetched
    /// (`RequestPurpose::GetPinnedMessages`). Absent until fetched; the
    /// pinned bar then falls back to pinned rows in loaded history.
    pub pinned_messages: HashMap<i64, Vec<HistoryMessage>>,
    /// The oldest unread mention/reaction found for the corner buttons;
    /// the driver takes it and jumps (`ConnectDriver::ingest`).
    pub(crate) unread_jump: Option<MessageId>,
    /// The calendar box ("Jump to date"), when open.
    pub history_calendar: Option<HistoryCalendar>,
    /// A resolved date jump the driver has not started yet.
    pub(crate) date_jump: Option<(MessageId, DateJumpMode)>,
    /// A started date jump waiting for its window to load.
    pub(crate) date_jump_pending: Option<(MessageId, DateJumpMode)>,
    /// One-shot note for a date jump that found nothing.
    pub date_jump_note: Option<String>,
    /// Slice media-shared-gallery: per-chat shared-media gallery state
    /// (Media / Files / Music / Links / Voice / GIFs tabs).
    pub shared_media: SharedMediaState,
    /// Installed regular sticker sets + the loaded `stickerSet` for the picker.
    pub stickers: StickerPanel,
    pub emoji: EmojiPanel,
    /// Saved animations (`getSavedAnimations`) for the GIF picker.
    pub gifs: GifPanel,
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
    /// Slice B2: `getBotSimilarBots` results (schema 1.8.67, line 11640)
    /// for the similar-bots section of the bot profile, keyed by bot
    /// user id. The `users` ids resolve to names via `Session::users`.
    pub similar_bots: HashMap<i64, SimilarBotsFetch>,
    /// B10: chat-id lists behind the profile panels, keyed by
    /// `(kind, user/chat id)`: groups in common, similar channels and
    /// the suitable personal channels. Chat objects themselves arrive via
    /// `updateNewChat` before the `chats` answer.
    pub profile_chat_lists: HashMap<(ProfileChatsKind, i64), ProfileChatsFetch>,
    /// B10: profile photo galleries (`getUserProfilePhotos`) by user id.
    pub user_profile_photos: HashMap<i64, ProfilePhotosFetch>,
    /// Slice bots-games: games seen via `messageGame` in a bot's chat,
    /// keyed by bot user id. Only short names TDLib actually delivered
    /// are cached — the bot info panel's Send buttons never offer an
    /// invented short name.
    pub bot_games: HashMap<i64, Vec<GameInfo>>,
    /// Slice bots-games: high-score panels, keyed by
    /// `(chat_id, message_id)`. Present = panel open; `None` = request in
    /// flight (the panel shows a loading row); `Some` = loaded rows.
    pub game_scores: HashMap<(i64, i64), Option<Vec<GameHighScore>>>,
    /// Cached `getCommands` results for the default scope (a null `scope`
    /// selects `botCommandScopeDefault`, Phase 3.3), keyed by bot user id. Presence records "fetched"
    /// so the driver never retries — including when the response was an
    /// `error` (user sessions; `getCommands` is annotated "for bots only").
    pub bot_commands: HashMap<i64, Vec<BotCommand>>,
    /// Composer text changed since the last persisted draft. Remote
    /// `updateChatDraftMessage` must not replace it (schema comment).
    pub(crate) draft_dirty: HashSet<i64>,
    /// Send succeeded; UI clears the server draft if the composer is still empty.
    pub draft_clears: Vec<ChatId>,
    /// Sponsored messages per chat (`getChatSponsoredMessages`).
    pub sponsored: HashMap<i64, ChatSponsoredMessages>,
    /// Set once Telegram confirmed `reportSponsoredResultAdsHidden`: no ads
    /// are fetched or shown for the rest of this session.
    pub sponsored_hidden: bool,
    /// In-flight sponsored-message report waiting on an option choice.
    pub sponsored_report: Option<SponsoredReportFlight>,
    /// Report target chosen by the user (chat + sponsored message id); cleared
    /// when the flow resolves.
    pub(crate) sponsored_report_target: Option<(ChatId, i64)>,
    /// Last `reportChatSponsoredMessage` outcome note.
    pub last_sponsored_report: Option<SponsoredReportOutcome>,
    /// Own user id: TDLib's `my_id` option (pushed after authorization),
    /// or a `getMe` answer. `None` until either arrives.
    pub my_user_id: Option<i64>,
    /// Static map tiles of location / venue messages.
    pub map_thumbs: MapThumbs,
    /// Phase 6: user directory from `updateUser`, keyed by user id. Feeds
    /// the contacts list and the user info panel.
    pub users: HashMap<i64, ParsedUser>,
    /// Phase 6: `getContacts` result — user ids, in server order. `None`
    /// until the first `users` response; `contacts_error` records a failed
    /// fetch so the UI can offer a retry.
    pub contacts: Option<Vec<i64>>,
    pub contacts_error: bool,
    /// Slice A6: outcome line for contacts mutations (`removeContacts`,
    /// `importContacts`, `clearImportedContacts`) — set on ok and on
    /// error, cleared by the next mutation; shown in the contacts
    /// settings section.
    pub contacts_notice: Option<String>,
    /// Phase 6: cached `getUserFullInfo` bios, keyed by user id. Presence
    /// records "fetched" so the driver never refetches.
    pub user_full_infos: HashMap<i64, UserFullInfoData>,
    /// Message counts per `searchMessagesFilter*` (index into
    /// `MEDIA_COUNT_FILTERS`) for chats whose info panel was opened.
    pub chat_media_counts: HashMap<i64, HashMap<u8, i32>>,
    /// Translation state (`translateText` / `translateMessageText`, the
    /// chat translate bar).
    pub translate: TranslateState,
    /// Bot reply keyboards as TDLib reports them, and recent inline bots.
    pub reply_keyboards: ReplyKeyboardState,
    /// Stories, the story tray, albums, archive, posting, viewers and close friends.
    /// Declared in `src/state/domains/stories/state.rs`.
    pub stories: StoriesState,
    /// `parity:platform-chat-export` — in-progress chat history export.
    /// The driver pages `getChatHistory` into this; the UI surfaces the
    /// result (path or error) and clears it.
    pub chat_export: Option<crate::chat_export::ChatExportState>,
    /// B4: `getPollVoters` fetch state for the poll-voters dialog, keyed
    /// by (chat id, message id, 0-based option index). One page per key.
    pub poll_voters: HashMap<(i64, i64, i32), PollVotersFetch>,
    /// B15: `getPollVoteStatistics` fetch state, keyed by (chat id,
    /// message id).
    pub poll_stats: HashMap<(i64, i64), PollStatsFetch>,
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
    /// B7: `updateActiveEmojiReactions` — the emoji usable as reactions.
    pub active_emoji_reactions: Vec<String>,
    /// Phase 6: the open user / supergroup info panel, if any.
    pub open_info_panel: Option<InfoPanelTarget>,
    /// A5: latest `checkChatUsername` verdict for the edit-profile
    /// dialog: (checked username text, result). Written by the driver
    /// before `apply` takes the pending request; the dialog only shows it
    /// when it matches the current input text (stale verdicts ignored).
    pub username_check: Option<(String, UsernameCheckResult)>,
    /// A5: username text of the in-flight `checkChatUsername`.
    pub username_check_pending: Option<String>,
    /// A5: last profile-edit request failure
    /// (`setName`/`setBio`/`setUsername`/`checkChatUsername`/
    /// `reorderActiveUsernames`/`toggleUsernameIsActive`/
    /// `setProfilePhoto`/`deleteProfilePhoto`), shown in the
    /// edit-profile dialog. Cleared when the dialog opens.
    pub profile_edit_error: Option<String>,
    pub(crate) diagnostics: Arc<dyn DiagnosticSink>,
}
