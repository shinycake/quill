//! The Session reducer: central client state and constructor.
use super::*;
use crate::telegram::envelope::{ChatAccent, ChatBackground, EmojiChatTheme};

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
    /// Phase 7.1: folder list from `updateChatFolders` (schema 1.8.67 line
    /// 10606). Empty until TDLib pushes the update (after authorization).
    pub chat_folders: Vec<ChatFolderInfo>,
    /// Parity slice: `are_tags_enabled` from `updateChatFolders` (schema
    /// 1.8.67 line 10606). When true, chat rows render folder-name tag
    /// chips; toggled via `toggleChatFolderTags` (`:13376`).
    pub are_folder_tags_enabled: bool,
    /// Parity slice: cached full `chatFolder` specs from `getChatFolder`
    /// responses (keyed by folder id) — edit dialog prefill and the
    /// remove-from-folder chain. Stale entries are dropped when the folder
    /// is edited or deleted.
    pub folder_specs: HashMap<i32, ChatFolderSpec>,
    /// Parity slice: `getChatListsToAddChat` results per chat id — the chat
    /// lists a chat may be added to. Drives the per-chat folder picker as
    /// the schema intends (`:13347` doc comment).
    pub chat_lists_for_add: HashMap<i64, Vec<ChatList>>,
    /// Parity slice: folder ids whose `loadChats(chatListFolder)` paging hit
    /// 404 — no "load more" for these folders.
    pub folder_chats_exhausted: HashSet<i32>,
    /// Parity slice: queued remove-from-folder intents `(chat_id,
    /// folder_id)`. The driver sends `getChatFolder`, then `editChatFolder`
    /// with the chat dropped from the spec — there is no
    /// `removeChatFromList` in 1.8.67.
    pub folder_remove_queue: Vec<(ChatId, i32)>,
    /// Parity slice: `getChatFolderChatsToLeave` results per folder id —
    /// chats the delete-confirm dialog offers to leave with the folder.
    pub folder_chats_to_leave: HashMap<i32, Vec<i64>>,
    /// `getChatFolderInviteLinks` results (and creates/edits applied on top)
    /// per folder id — the Share Folder dialog's link list.
    pub folder_invite_links: HashMap<i32, Vec<ChatFolderInviteLink>>,
    /// `getChatsForChatFolderInviteLink` results per folder id — the chats a
    /// link may include.
    pub folder_link_chats: HashMap<i32, Vec<i64>>,
    /// `getRecommendedChatFolders` result; `None` until fetched.
    pub recommended_folders: Option<Vec<RecommendedChatFolder>>,
    /// One-shot: a folder link create/edit was applied (the dialog returns
    /// to its list). Drained by the UI.
    pub folder_link_saved: bool,
    /// One-shot: a Share Folder / recommended-folder request failed; the
    /// text is shown in the dialog. Drained by the UI.
    pub folder_share_error: Option<String>,
    /// The `addlist` link being checked by the "Add folder" dialog.
    pub folder_invite_link: Option<String>,
    /// `checkChatFolderInviteLink` answer for `folder_invite_link`.
    pub folder_invite_info: Option<ChatFolderInviteLinkInfo>,
    /// The "Add folder" dialog's failure text (check or add).
    pub folder_invite_error: Option<String>,
    /// `addChatFolderByInviteLink` confirmed.
    pub folder_invite_done: bool,
    /// `getChatFolderNewChats` answers per shared folder: chats its owner
    /// added since the user last looked (the "N new chats" bar).
    pub folder_new_chats: HashMap<i32, Vec<i64>>,
    /// When each folder's new chats were last asked for (TDLib wants one
    /// call per `chat_folder_new_chats_update_period`).
    pub folder_new_chats_asked: HashMap<i32, std::time::Instant>,
    /// Folder limits from TDLib options and `getPremiumLimit`.
    pub folder_limits: crate::folder_limits::FolderLimits,
    /// One-shot: a folder request failed on a limit; the UI shows the box.
    /// Drained by the UI.
    pub folder_limit_hit: Option<crate::folder_limits::FolderLimitKind>,
    /// `getInstalledBackgrounds` answer for the current theme; `None` until
    /// fetched.
    pub installed_backgrounds: Option<Vec<Background>>,
    /// The account's default wallpaper per theme (`false` light, `true`
    /// dark), from `updateDefaultBackground` and `setDefaultBackground`.
    pub default_backgrounds: HashMap<bool, Background>,
    /// Which theme the pending `setDefaultBackground` was for.
    pub background_set_for_dark: bool,
    /// `chat.background` / `updateChatBackground`, by chat id.
    pub chat_backgrounds: HashMap<i64, ChatBackground>,
    /// `chat.theme` (emoji theme name) / `updateChatTheme`, by chat id.
    pub chat_theme_names: HashMap<i64, String>,
    /// `updateEmojiChatThemes`: the themes a private chat can pick.
    pub emoji_chat_themes: Vec<EmojiChatTheme>,
    /// `searchBackground` answer for a `bg/` link; `None` until it arrives.
    pub searched_background: Option<Background>,
    /// One-shot: a wallpaper request failed; shown in Appearance.
    pub background_error: Option<String>,
    /// Acknowledged (`ok`) chat-look requests (`setChatTheme`,
    /// `setChatBackground`, `deleteChatBackground`); the chat colors dialog
    /// waits for them before it closes.
    pub chat_look_oks: u32,
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
    /// Transfer-ownership gate, transfer progress and the "next owner" lookup.
    pub ownership: OwnershipState,
    /// Own status and admin rights in each basic group (`updateBasicGroup`).
    pub basic_group_own: HashMap<i64, BasicGroupOwn>,
    /// Member counts from `updateSupergroup` / `updateBasicGroup` (the
    /// header's fallback before full info loads), keyed by group id.
    pub supergroup_member_counts: HashMap<i64, i32>,
    pub basic_group_member_counts: HashMap<i64, i32>,
    /// `updateChatOnlineMemberCount`, keyed by chat id.
    pub chat_online_counts: HashMap<i64, i32>,
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
    /// Slice CL1: `getOption("pinned_chat_count_max")` /
    /// `getOption("pinned_archived_chat_count_max")` via `updateOption`
    /// (schema 1.8.67, line 13674). Defaults 5 / 100 are TDLib's
    /// compiled defaults; the server raises them for Premium. Used for
    /// the client-side pin-limit pre-check (TGX `ChatsController`
    /// `PinTooMuchWarn` / `ErrorPinnedChatsLimit` behavior).
    pub pinned_chat_count_max: i32,
    pub pinned_archived_chat_count_max: i32,
    /// Slice CL1: one-shot error from a refused chat-list action
    /// (`toggleChatIsPinned`, `toggleChatIsMarkedAsUnread`,
    /// `deleteChatHistory`). The UI drains it into the status note so a
    /// refused action never looks like it worked.
    pub chat_action_error: Option<String>,
    /// Slice CL3: one-shot `reportChat` outcome (`reportChatResultOk`
    /// vs option/text/messages required). The UI drains it into the
    /// status note next to `chat_action_error`.
    pub report_chat_outcome: Option<String>,
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
    /// Slice G1 fix-up: one-shot; set when an invite-link mutation
    /// (create/edit/revoke/replace-primary) errors. The UI drains it into
    /// the status note — the previously loaded list is kept, not wiped.
    pub invite_link_error: Option<String>,
    /// M1: `getChatScheduledMessages` results — the chat's scheduled sends,
    /// with `scheduling_state` showing the planned send time.
    pub scheduled_messages: Vec<ParsedMessage>,
    pub open_chat: Option<ChatId>,
    /// Phase 8.1: whether the OS considers our window focused. The UI sets
    /// this from `Window::is_window_active` on every render; it defaults to
    /// true so the reducer never notifies before the first paint measures it.
    pub app_active: bool,
    /// Phase 8.1: mirror of `settings::Preferences::hide_notification_previews`
    /// (default true). There is no settings UI yet, so the value lives on the
    /// session for the reducer to apply.
    pub hide_notification_previews: bool,
    /// Parity slice: mirror of
    /// `settings::Preferences::inapp_sounds_enabled` (default true) —
    /// tdesktop's "Play sounds" toggle. Loaded from `prefs.json` at
    /// connect time; the notification defaults dialog writes through.
    pub inapp_sounds_enabled: bool,
    /// Mirror of `settings::Preferences::desktop_notifications` (tdesktop
    /// `desktopNotify`, toggled from Settings or the tray menu).
    pub desktop_notifications: bool,
    /// Phase 8.1: notifications decided by the reducer, drained by the UI for
    /// OS dispatch. Same-chat bursts coalesce into one entry ("N new messages").
    pub pending_notifications: Vec<QueuedNotification>,
    /// Chats whose OS notification should be withdrawn (read elsewhere or
    /// removed by TDLib); drained by the UI, which dismisses the toast.
    pub pending_notification_clears: Vec<ChatId>,
    /// A notification-worthy message arrived: the UI bounces the Dock icon /
    /// flashes the taskbar (when the user wants it and the OS is not in
    /// Do Not Disturb). Set independently of the "Desktop notifications"
    /// switch, like tdesktop's alert.
    pub pending_attention: bool,
    /// TDLib option `disable_contact_registered_notifications`: the inverse
    /// of tdesktop's "Contact joined Telegram" event switch.
    pub disable_contact_registered_notifications: bool,
    /// Chats with an OS notification we showed (or TDLib reports active
    /// from a previous launch); only these produce a clear.
    pub shown_notification_chats: std::collections::HashSet<ChatId>,
    /// `getDefaultMessageAutoDeleteTime` cache, seconds (0 = off).
    pub default_auto_delete_secs: Option<i32>,
    /// A default auto-delete fetch or write is in flight.
    pub default_auto_delete_busy: bool,
    /// Honest one-line failure of the last default auto-delete request.
    pub default_auto_delete_error: Option<String>,
    /// Parity slice: `getSavedNotificationSounds` cache (titles / durations
    /// for the sound picker; `sound` files download on demand).
    pub saved_notification_sounds: Vec<NotificationSound>,
    /// Parity slice: the saved-sound list has been fetched at least once.
    pub saved_sounds_loaded: bool,
    /// Parity slice: `updateSavedNotificationSounds` arrived since the last
    /// fetch — the driver refetches on the next ingest.
    pub saved_sounds_stale: bool,
    /// Parity slice: `getScopeNotificationSettings` results per scope; used
    /// for `use_default_*` fallback (e.g. default sound) and the scope
    /// defaults settings view.
    pub scope_notification_settings: HashMap<NotificationSettingsScope, ScopeNotificationSettings>,
    /// Parity slice: `getChatNotificationSettingsExceptions` answers —
    /// chat ids with non-default notification settings per scope (the
    /// exceptions list view).
    pub notification_exceptions: HashMap<NotificationSettingsScope, Vec<i64>>,
    /// Parity slice: scopes with a `getChatNotificationSettingsExceptions`
    /// in flight.
    pub notification_exceptions_loading: HashSet<NotificationSettingsScope>,
    /// Parity slice: scopes with a `getScopeNotificationSettings` in flight.
    pub scope_settings_loading: HashSet<NotificationSettingsScope>,
    /// Parity slice: `updateReactionNotificationSettings` cache. No getter
    /// exists — this stays `None` until the first update arrives.
    pub reaction_notification_settings: Option<ReactionNotificationSettings>,
    /// Phase S2: cached `getStorageStatistics` answer (aggregated by file
    /// type, TGX `TGStorageStats` style); drives the storage-usage overlay,
    /// including the "Secret media and files" category.
    pub storage_stats: Option<StorageStats>,
    /// Phase S2: a `getStorageStatistics` round trip is in flight.
    pub storage_stats_loading: bool,
    /// Slice S4: per-network auto-download settings, the local source of
    /// truth (TDLib has no getter for the current values). Loaded from
    /// `data_storage.json` at session setup; seeded once from
    /// `getAutoDownloadSettingsPresets` when unseeded.
    pub data_storage: DataStoragePrefs,
    /// Slice S4: the presets answer seeded `data_storage` — the driver
    /// persists it on the next ingest (the reducer cannot touch the
    /// filesystem).
    pub data_storage_dirty: bool,
    /// Slice S4: a `getAutoDownloadSettingsPresets` round trip is in flight.
    pub auto_download_presets_loading: bool,
    /// Slice S4: last Data & Storage failure, shown on the screen
    /// (failures surface there, never as toasts — the S3 pattern).
    pub data_storage_error: Option<String>,
    /// Batch 6: bytes the last confirmed `optimizeStorage` freed — the
    /// screen shows "{size} freed on your device!" until reopened.
    pub storage_freed: Option<i64>,
    /// Batch 6: an `optimizeStorage` round trip is in flight.
    pub storage_clearing: bool,
    /// Batch 6: the local storage limits TDLib reports (`updateOption`).
    pub storage_limits: crate::storage_limits::StorageLimits,
    /// B13: new-chat privacy, inactive-session TTL, 18+ option, network
    /// usage and the remember-password check.
    pub privacy_data: PrivacyData,
    /// Batch 4: new-login alert, service popups and terms of service.
    pub notices: AccountNotices,
    /// Account-level sync updates: silent default, downloads, dice,
    /// freeze, speech quota, live shares, age verification.
    pub sync: UpdatesSync,
    /// Batch 6: two-step recovery / reset / login-email flow state.
    pub twofa_flow: TwofaFlow,
    /// Slice A2: cached `getPasswordState` / `setPassword` /
    /// `setRecoveryEmailAddress` answer; drives the two-step
    /// verification overlay. Replaced only by our own
    /// `PasswordStateOp` answers — never mutated optimistically.
    pub password_state: Option<PasswordState>,
    /// Slice A2: a 2FA management round trip is in flight (fetch or
    /// mutation); the overlay shows progress and disables submits.
    pub password_state_loading: bool,
    /// Slice A2: honest one-line failure of the last 2FA management
    /// request (TDLib's actual error, classified — never a fake
    /// success). Cleared on the next attempt and on success.
    pub password_op_error: Option<String>,
    /// Slice A3: cached `getActiveSessions` answer (TGX `SessionsInfo`
    /// style); drives the Active Sessions overlay. Incomplete login
    /// attempts (`is_password_pending`) render in their own section.
    pub sessions: Option<Vec<ParsedSession>>,
    /// Slice A3: a `getActiveSessions` round trip is in flight.
    pub sessions_loading: bool,
    /// Slice A3: a `terminateSession` / `terminateAllOtherSessions` round
    /// trip is in flight — terminate buttons stay disabled meanwhile.
    pub sessions_mutating: bool,
    pub device_login_result: Option<crate::auth::DeviceLoginResult>,
    /// Slice A3: honest one-line failure of the last sessions fetch or
    /// terminate (classified from the TDLib error code, never the native
    /// message). Cleared on the next successful fetch.
    pub sessions_error: Option<String>,
    /// Slice A3: a terminate succeeded — the old cache stays visible and
    /// is refetched from the authoritative answer on the next ingest
    /// (the `saved_sounds_stale` pattern); never an optimistic delete.
    pub sessions_stale: bool,
    /// Slice A7: cached `getAccountTtl` answer, in days — drives the
    /// self-destruct-if-away picker (UI half ships post-Phase-9).
    pub account_ttl_days: Option<i32>,
    /// `getCountries` rows for the sign-in picker (`None` until answered).
    pub countries: Option<Vec<crate::phone::Country>>,
    /// Uppercase ISO code from `getCountryCode`: the default country guess.
    pub guessed_country_iso: Option<String>,
    /// Slice A7: a `getAccountTtl` round trip is in flight.
    pub account_ttl_loading: bool,
    /// Slice A7: a `deleteAccount` / `setAccountTtl` round trip is in
    /// flight — the account surfaces stay disabled meanwhile.
    pub account_mutating: bool,
    /// Slice A7: honest one-line failure of the last account-lifecycle
    /// op (classified from the TDLib error code, never the native
    /// message). Cleared on the next attempt and on success.
    pub account_error: Option<String>,
    /// Slice A8: the target number a change-number code was sent to
    /// (`authenticationCodeInfo` answer) — drives the code-entry step of
    /// the change-number flow. The code itself is never stored (the A2
    /// rule: secrets ride the request JSON only).
    pub change_number_phone: Option<String>,
    /// Slice A8: server-specified timeout (seconds) before a resend is
    /// allowed, from the same `authenticationCodeInfo` answer.
    pub change_number_timeout: Option<i32>,
    /// Slice A8: a `sendPhoneNumberCode` / `resendPhoneNumberCode` round
    /// trip is in flight.
    pub change_number_loading: bool,
    /// Slice A8: a `checkPhoneNumberCode` round trip is in flight.
    pub change_number_checking: bool,
    /// Slice A8: honest one-line failure of the last change-number op
    /// (classified, never the native message). Cleared on the next
    /// attempt and on success.
    pub change_number_error: Option<String>,
    /// Slice A4: cached `getConnectedWebsites` answer (TGX
    /// `SettingsWebsitesController` style); drives the Connected Websites
    /// overlay.
    pub connected_websites: Option<Vec<ParsedWebsite>>,
    /// Slice A4: a `getConnectedWebsites` round trip is in flight.
    pub connected_websites_loading: bool,
    /// Slice A4: a `disconnectWebsite` / `disconnectAllWebsites` round trip
    /// is in flight — disconnect buttons stay disabled meanwhile.
    pub websites_mutating: bool,
    /// Slice A4: honest one-line failure of the last websites fetch or
    /// disconnect (classified from the TDLib error code, never the native
    /// message). Cleared on the next successful fetch.
    pub websites_error: Option<String>,
    /// Slice A4: a disconnect succeeded — the old cache stays visible and
    /// is refetched from the authoritative answer on the next ingest
    /// (the `saved_sounds_stale` pattern); never an optimistic delete.
    pub websites_stale: bool,
    /// Parity slice: downloaded-file id → notification sound id, for files
    /// fetched as notification sounds.
    pub sound_file_ids: HashMap<i32, i64>,
    /// Parity slice: sound ids with playback requested whose file is not
    /// local yet. When the file completes, its path lands in
    /// `pending_sound_plays`.
    pub pending_sound_downloads: HashSet<i64>,
    /// Parity slice: local sound-file paths the UI should play, drained by
    /// `flush_notifications`. The reducer never spawns processes.
    pub pending_sound_plays: Vec<std::path::PathBuf>,
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
    /// Phase C1: the tracked live call, if any. **Signaling only** —
    /// TDLib transports no audio/video (official clients use
    /// libtgvoip); real media transport is the C2 spike.
    pub active_call: Option<ActiveCall>,
    /// Phase C1: summary of the most recently ended call, driving the
    /// call-end screen and the optional 1–5 rating card
    /// (`callStateDiscarded.need_rating`).
    pub call_summary: Option<CallSummary>,
    /// Phase C1: last async call-request error (e.g. `createCall`
    /// rejected), shown on the call overlay and cleared when
    /// dismissed. Never a secret.
    pub call_error: Option<String>,
    /// Phase C3a: last async group-call request error (e.g.
    /// `joinVideoChat` rejected), shown on the group-call overlay and
    /// cleared when dismissed. Never a secret.
    pub group_call_error: Option<String>,
    /// Phase C1: incoming calls that arrived while another call was
    /// active — the driver discards them (busy) via `discardCall`.
    /// Entries are `(call_id, user_id, is_video)` so the decline
    /// reports the actual call kind rather than a hardcoded one.
    pub call_busy_decline_queue: Vec<(i32, i64, bool)>,
    /// Phase C2i: incoming calls auto-declined while busy, kept as
    /// `(user_id, is_video)` so the UI can say so honestly instead of
    /// declining silently. Drained by the UI banner.
    pub call_busy_declined: Vec<(i64, bool)>,
    /// Swap prompt: the first incoming call that arrived while another
    /// call was active, awaiting the user's decision — `(call_id,
    /// user_id, is_video)`. Further incoming calls while the prompt is
    /// open go to `call_busy_decline_queue` (auto-declined busy).
    pub call_swap_pending: Option<(i32, i64, bool)>,
    /// Swap prompt: the user chose "end current & answer" — the
    /// pending incoming call's `(call_id, is_video)`, accepted by the
    /// driver once the active call's terminal update lands (TDLib
    /// allows a single active call, so `acceptCall` waits for the
    /// discard to complete).
    pub call_swap_accept_queued: Option<(i32, bool)>,
    /// Phase C2i: recent calls from `searchCallMessages` (server-side
    /// history, schema 1.8.67 :11903) for the Recent-calls tab, newest
    /// first.
    pub recent_calls: Vec<ParsedMessage>,
    /// `next_offset` from the last `foundMessages` page; empty starts
    /// (or restarts) the list.
    pub recent_calls_offset: String,
    /// A `searchCallMessages` page is in flight.
    pub recent_calls_loading: bool,
    /// The last `searchCallMessages` request failed.
    pub recent_calls_error: bool,
    /// A `deleteAllCallMessages` request is in flight.
    pub recent_calls_clearing: bool,
    /// What the chat-list suggestions block shows from.
    pub suggestions: crate::chatlist_suggestions::SuggestionFacts,
    /// Phase C2i: "who can call me"
    /// (`userPrivacySettingAllowCalls`, schema 1.8.67 :9006).
    pub call_privacy_allow_calls: Option<PrivacyWho>,
    /// `getSupportUser` answer waiting for the driver to open the chat
    /// (Settings > Ask a Question).
    pub support_user_ready: Option<i64>,
    /// Phase C2i: peer-to-peer calls
    /// (`userPrivacySettingAllowPeerToPeerCalls`, schema 1.8.67 :9009).
    pub call_privacy_p2p: Option<PrivacyWho>,
    /// A privacy get/set round-trip is in flight (see
    /// `call_privacy_pending` — fetch sends two gets, so this clears
    /// only when the last response lands).
    pub call_privacy_loading: bool,
    /// Outstanding call-privacy get/set round-trips.
    pub call_privacy_pending: u8,
    /// The last privacy get/set failed.
    pub call_privacy_error: bool,
    /// Slice S3: per-key rule state for the Privacy screen
    /// (`userPrivacySettingShowStatus`, `ShowPhoneNumber`,
    /// `ShowProfilePhoto`, `ShowLinkInForwardedMessages`,
    /// `AllowChatInvites`; schema 1.8.67, :8981-:9003). Present only
    /// after a fetch was attempted — absent means never requested.
    pub privacy: HashMap<PrivacySettingKey, PrivacyKeyState>,
    /// Slice S3: `readDatePrivacySettings.show_read_date` (schema 1.8.67,
    /// :9026) — `None` while never fetched.
    pub read_date_show: Option<bool>,
    /// A read-date get/set round-trip is in flight.
    pub read_date_loading: bool,
    /// The last read-date get/set failed.
    pub read_date_error: bool,
    /// Slice S3: blocked user ids from `getBlockedMessageSenders`
    /// (schema 1.8.67, :14505); `None` while never fetched.
    pub blocked_senders: Option<Vec<i64>>,
    /// `total_count` from the last `messageSenders` answer; more pages
    /// exist while `blocked_senders.len() < blocked_total`.
    pub blocked_total: i32,
    /// A blocked-senders page is in flight.
    pub blocked_loading: bool,
    /// The last blocked-senders get/set failed.
    pub blocked_error: bool,
    /// Phase C2i: local call preferences (confirm-before-calling,
    /// less-data), persisted via `settings::CallPrefs`. The driver
    /// loads them at startup; the UI saves on toggle.
    pub call_prefs: CallPrefs,
    /// MED1: local media preferences (remember-media-grouping),
    /// persisted via `settings::MediaPrefs`. Loaded at startup like
    /// `call_prefs`; the UI saves on toggle.
    pub media_prefs: MediaPrefs,
    /// Slice A6: local contacts preferences (sync toggle), persisted
    /// via `settings::ContactPrefs`. Loaded at startup like
    /// `call_prefs`; the UI saves on toggle. Client-side only — TDLib
    /// 1.8.67 has no contact-sync switch (verified concept-level; TGX
    /// implements sync client-side in `TdlibContactManager`).
    pub contact_prefs: ContactPrefs,
    /// Slice parity:chatlist-badge-settings: local badge-counter
    /// preferences (include muted/archived, messages-vs-chats),
    /// persisted via `settings::BadgePrefs`. Loaded at startup like
    /// `call_prefs`; the UI saves on toggle.
    pub badge_prefs: BadgePrefs,
    /// TDLib's authoritative unread totals for the main and archive chat
    /// lists (`updateUnreadMessageCount` / `updateUnreadChatCount`); the
    /// badge uses these instead of summing the (paginated) loaded chats.
    pub unread_totals: UnreadTotals,
    /// `updateUnreadChatCount` for each chat folder (tab counters).
    pub folder_unread_chats: HashMap<i32, UnreadPair>,
    /// Slice parity:settings-language: the app language tag sent in
    /// `setTdlibParameters`, persisted via `settings::LanguagePrefs`.
    /// Loaded at startup like `call_prefs`; the UI saves on change.
    pub language_prefs: LanguagePrefs,
    /// Phase C3a: the tracked group call / voice chat, if any.
    /// **Signaling only** — TDLib transports no audio/video; the
    /// `joinVideoChat` response payload is stored (`join_payload`) and
    /// never consumed (real media transport is the C2 program).
    pub active_group_call: Option<ActiveGroupCall>,
    /// Phase C3a: group-call ids whose full `groupCall` still needs a
    /// `getGroupCall` fetch (queued from the `createVideoChat`
    /// `groupCallId` answer). Drained by the driver.
    pub group_call_fetch_queue: Vec<i32>,
    /// Replied-to messages outside the loaded window, keyed by the
    /// replying message `(chat_id, message_id)`.
    pub reply_targets: HashMap<(i64, i64), ReplyTarget>,
    /// stories-live-play: the story viewer's "Join live" asked for this
    /// group call; the driver issues `join_video_chat` once the
    /// `getGroupCall` answer has created the unjoined tracker.
    pub pending_live_story_join: Option<LiveStoryJoinIntent>,
    /// Phase 5.1: selected forum topic (`forum_topic_id`) of the open chat.
    /// `None` = topic list (or a non-forum chat). Reset by `open_chat`.
    pub open_topic: Option<i32>,
    pub pending_bot_messages: HashMap<(i64, i32), PendingBotMessage>,
    pub pending_bot_period_secs: u64,
    /// Phase 5.1: cached `forumTopics` per forum chat id (first page only).
    pub forum_topics: HashMap<i64, Vec<ForumTopic>>,
    /// Phase 5.1: per-topic histories keyed by `(chat_id, forum_topic_id)`.
    pub topic_histories: HashMap<(i64, i32), TopicHistory>,
    /// `chat.view_as_topics` / `updateChatViewAsTopics`, by chat id: a
    /// forum shown as topics, Saved Messages shown as chats.
    pub chat_view_as_topics: HashMap<i64, bool>,
    /// `getForumTopicDefaultIcons`: the custom emoji a topic may use.
    pub forum_topic_icons: Vec<StickerItem>,
    /// Saved Messages sublists, tags and the open sublist / tag filter.
    pub saved: SavedMessagesState,
    /// Subsection tabs: supergroup ids with `supergroup.has_forum_tabs`
    /// (schema 1.8.67, line 2746), from `updateSupergroup` / `getSupergroup`.
    pub forum_tabs_supergroups: HashSet<i64>,
    /// `poll.id` → `(chat_id, message_id)` of rows loaded with that poll,
    /// so `updatePoll` (which carries no chat or message id) touches only
    /// its rows. Entries can be stale; `apply_update_poll` re-checks and
    /// prunes them.
    pub(crate) poll_messages: HashMap<i64, HashSet<(i64, i64)>>,
    pub view_generation: ViewGeneration,
    pub requests: RequestRegistry,
    pub chats_exhausted: bool,
    /// `loadChats(chatListArchive)` answered 404 — the archive is fully
    /// loaded (paging starts once the main list is exhausted).
    pub archive_chats_exhausted: bool,
    pub shutdown: ShutdownPhase,
    pub last_seq: u64,
    /// Last classified error for phone / code / password submit. Never a secret.
    pub last_auth_error: Option<AuthRequestError>,
    /// In-flight `forwardMessages` (dest / source / requested count).
    pub in_flight_forward: Option<ForwardFlight>,
    /// Further `forwardMessages` in flight while the share box sends to
    /// several chats at once (`in_flight_forward` holds the first).
    pub queued_forward_flights: Vec<ForwardFlight>,
    /// `chat.message_sender_id` / `updateChatMessageSender`: the "send as"
    /// identity selected per chat (absent when the user cannot change it).
    pub chat_message_sender: HashMap<i64, MessageSender>,
    /// `getChatAvailableMessageSenders` answers per chat.
    pub send_as_options: HashMap<i64, Vec<AvailableMessageSender>>,
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
    /// Slice P1: the in-flight payment request context (`getPaymentForm`,
    /// `validateOrderInfo`, `sendPaymentForm`, `getPaymentReceipt`).
    pub payment_request: Option<PaymentRequest>,
    /// Slice P1: the fetched `paymentForm`, shown in the checkout dialog.
    pub payment_form: Option<PaymentFormData>,
    pub marketplace_gift: Option<crate::marketplace::GiftPurchase>,
    pub gift_text_length_max: Option<usize>,
    /// Notification-tone limits (`notification_sound_*_max` options).
    pub tone_limits: crate::message_menu::ToneLimits,
    /// Slice P1: `getPaymentForm` is in flight (dialog shows a spinner).
    pub payment_form_loading: bool,
    /// Slice P1: the validated order info + shipping options from
    /// `validateOrderInfo`.
    pub payment_validated: Option<ValidatedOrderInfoData>,
    /// Slice P1: the chosen shipping option id (default: the first).
    pub payment_shipping_id: Option<String>,
    /// Slice P1: the fetched `paymentReceipt`, shown in the receipt dialog.
    pub payment_receipt: Option<PaymentReceiptData>,
    /// Slice P1: receipt dialog visibility.
    pub payment_receipt_open: bool,
    /// Slice P1: latest payment error / outcome note, shown in the
    /// checkout dialog (never a secret — order fields and credentials are
    /// never echoed here).
    pub payment_note: Option<String>,
    /// Slice P1: `sendPaymentForm` is in flight — the Pay button shows
    /// "Processing…" and is disabled until the `paymentResult` answer (or
    /// error) lands, so a double-click can't submit twice.
    pub payment_sending: bool,
    /// Slice P1: a failed `getPaymentReceipt`, drained into the status
    /// note by `poll_live` — the checkout dialog (which renders
    /// `payment_note`) may be closed when the receipt fetch fails.
    pub payment_receipt_error: Option<String>,
    /// Slice P1: `paymentResult.verification_url` from a non-successful
    /// `sendPaymentForm` — the UI takes it on the next poll and opens it
    /// in the OS browser (3-D Secure and similar).
    pub payment_verification_url: Option<String>,
    /// Slice `parity:bots-payment-recurring`: the fetched
    /// `starSubscriptions`, shown in the Subscriptions dialog.
    pub star_subscriptions: Option<StarSubscriptionsData>,
    /// Slice `parity:bots-payment-recurring`: `getStarSubscriptions` is in
    /// flight (dialog shows a spinner).
    pub star_subscriptions_loading: bool,
    /// Slice `parity:bots-payment-recurring`: latest subscriptions error,
    /// shown in the dialog (never a secret — ids are opaque TDLib strings).
    pub star_subscriptions_error: Option<String>,
    /// Slice `parity:bots-payment-recurring`: pagination offset for the
    /// next `getStarSubscriptions` page (empty = no more pages).
    pub star_subscriptions_offset: String,
    /// Slice `parity:bots-payment-recurring`: a cancel/rejoin mutation
    /// landed — the list refetches on the next pump (the
    /// `sessions_stale` pattern; never optimistic).
    pub star_subscriptions_stale: bool,
    /// Slice `parity:bots-payment-recurring`: an `editStarSubscription` /
    /// `reuseStarSubscription` is in flight — the dialog disables its
    /// action buttons until the `ok` (or error) lands.
    pub star_subscriptions_mutating: bool,
    /// Slice `parity:bots-payment-recurring`: the Subscriptions dialog is
    /// on screen.
    pub subscriptions_open: bool,
    /// Premium / Stars / received-gifts hub state (`crate::premium_hub`).
    pub hub: crate::premium_hub::PremiumHub,
    /// Slice `parity:bots-payment-recurring`: subscription id awaiting
    /// cancel confirmation in the dialog.
    pub subscription_cancel_confirm: Option<String>,
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
    /// TDLib's `is_premium` option: the account's current Premium state.
    /// `None` until the option arrives.
    pub premium_option: Option<bool>,
    /// Slice CL2: archive auto-settings from `getArchiveChatListSettings`
    /// (schema 1.8.67, line 13421). `None` until the first fetch; the
    /// archive-settings panel fetches on open (TGX
    /// `SettingsArchiveChatListController` does the same).
    pub archive_chat_list_settings: Option<ArchiveChatListSettings>,
    /// Slice CL2: the archive-settings panel is fetching its truth.
    pub archive_settings_loading: bool,
    /// Slice CL2: the archive-settings panel is open.
    pub archive_settings_open: bool,
    /// Phase 6: user directory from `updateUser`, keyed by user id. Feeds
    /// the contacts list and the user info panel.
    pub users: HashMap<i64, ParsedUser>,
    /// Slice A12: accent palette from `updateProfileAccentColors`
    /// (schema 1.8.67, line 10964) — full `profileAccentColor` entries
    /// for swatch rendering.
    pub profile_accent_colors: Vec<ProfileAccentColor>,
    /// Name-color palette from `updateAccentColors` (sender names).
    pub name_accent_colors: Vec<crate::telegram::NameAccentColor>,
    /// Slice A12: ids `setProfileAccentColor` accepts, in server order —
    /// the edit-profile accent picker rows.
    pub available_accent_color_ids: Vec<i32>,
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
    /// Phase 6: cached `getSupergroupFullInfo`, keyed by supergroup id.
    /// Presence records "fetched".
    pub supergroup_full_infos: HashMap<i64, SupergroupFullInfoData>,
    /// Slice (communities backend core): communities by id, fed by
    /// `updateCommunity` (schema 1.8.67, line 10726),
    /// create-on-first-sight.
    pub communities: HashMap<i64, ParsedCommunity>,
    /// Slice (communities backend core): `communityFullInfo` cache, keyed
    /// by community id, fed by the `getCommunityFullInfo` answer and
    /// `updateCommunityFullInfo` (TDLib 1.8.68). Presence records
    /// "fetched".
    pub community_full_infos: HashMap<i64, ParsedCommunityFullInfo>,
    /// TDLib 1.8.68 community management: one-shot; set when
    /// `setCommunityName` / `setCommunityPhoto` / `setCommunityPermissions`
    /// / `deleteCommunity` errors. The UI drains it into the status note.
    pub community_error: Option<String>,
    /// Phase D2: `getChatStatistics` fetch state, keyed by chat id.
    pub chat_statistics: HashMap<i64, ChatStatisticsFetch>,
    /// Phase D3a: `getChatInviteLinks` fetch state, keyed by chat id.
    pub invite_links: HashMap<i64, InviteLinkFetch>,
    /// Phase D3a: `getChatJoinRequests` fetch state, keyed by chat id.
    pub join_requests: HashMap<i64, JoinRequestFetch>,
    /// B8: the search query the cached `join_requests` list was fetched
    /// with (absent = no query), keyed by chat id.
    pub join_request_queries: HashMap<i64, String>,
    /// B8: newest join-request page request per chat; replies with any
    /// other id are stale (an older search) and dropped.
    pub join_request_latest: HashMap<i64, RequestId>,
    /// B8: `getChatInviteLinks` with `is_revoked = true`, keyed by chat id.
    pub revoked_invite_links: HashMap<i64, InviteLinkFetch>,
    /// Another admin's invite links (owner only), by chat id.
    pub admin_invite_links: HashMap<i64, AdminLinksState>,
    /// Pending join requests of the invite link whose details are open.
    pub link_join_requests: HashMap<i64, LinkRequestsState>,
    /// `getChatBoosts` list of the open tab, by chat id.
    pub chat_boost_lists: HashMap<i64, BoostsListState>,
    /// `getChatBoostLink` answer `(link, is_public)`, by chat id.
    pub chat_boost_links: HashMap<i64, (String, bool)>,
    /// `supergroup.usernames`, by supergroup id.
    pub supergroup_username_lists: HashMap<i64, crate::telegram::envelope::SupergroupUsernames>,
    /// B8: `getChatInviteLinkCounts` (owner only), keyed by chat id.
    pub invite_link_counts: HashMap<i64, InviteLinkCountsFetch>,
    /// B8: members of the invite link whose details are open, by chat id.
    pub invite_link_members: HashMap<i64, InviteLinkMembersState>,
    /// B8: in-flight `deleteRevokedChatInviteLink` requests, request id to
    /// `(chat id, link)`; a purpose is `Copy` so the link rides here.
    pub revoked_link_deletions: HashMap<RequestId, (i64, String)>,
    /// Phase D3a: latest `updateChatPendingJoinRequests` total per chat
    /// (schema 1.8.67, line 10555). The full request list still needs
    /// `getChatJoinRequests`; this is only the badge count.
    pub pending_join_request_counts: HashMap<i64, i32>,
    /// Batch 8: `chatJoinRequestsInfo.user_ids` (the newest requesters)
    /// from the same update, for the requests bar's avatars.
    pub pending_join_request_users: HashMap<i64, Vec<i64>>,
    /// Batch 8: `chat.action_bar` / `updateChatActionBar` per chat — the
    /// Add contact / Block / Report spam / Share phone strip.
    pub chat_action_bars: HashMap<i64, ChatActionBar>,
    /// Phase D3c: `getChatEventLog` fetch state, keyed by chat id.
    pub event_logs: HashMap<i64, ChatEventLogFetch>,
    /// Slice G2: per-chat event-log filters (`chatEventLogFilters`,
    /// schema 1.8.67, line 7956). Absent = all event types (the schema's
    /// `null`).
    pub event_log_filters: HashMap<i64, ChatEventLogFilterSet>,
    /// Slice G2: per-chat event-log text search (the `query` parameter of
    /// `getChatEventLog`, schema 1.8.67, line 15252). Absent = no search.
    pub event_log_queries: HashMap<i64, String>,
    /// Per-chat admin filter: the `user_ids` of `getChatEventLog` (the server
    /// filters, so the admin list stays complete). Empty or absent = everyone.
    pub event_log_users: HashMap<i64, Vec<i64>>,
    /// Slice G2: `supergroup.sign_messages` (schema 1.8.67, line 2746),
    /// keyed by supergroup id. Drives the channel "Sign messages" toggle.
    pub supergroup_sign_messages: HashMap<i64, bool>,
    /// Slice G2: `supergroup.show_message_sender` (schema 1.8.67, line
    /// 2746), keyed by supergroup id. Drives the "Show authors" toggle.
    pub supergroup_show_message_sender: HashMap<i64, bool>,
    /// Slice G2: `supergroupFullInfo.has_aggressive_anti_spam_enabled`
    /// (schema 1.8.67, line 2792), keyed by supergroup id.
    pub supergroup_anti_spam_enabled: HashMap<i64, bool>,
    /// Slice G2: `supergroupFullInfo.can_toggle_aggressive_anti_spam`
    /// (schema 1.8.67, line 2792), keyed by supergroup id. Gates the
    /// anti-spam toggle.
    pub supergroup_can_toggle_anti_spam: HashMap<i64, bool>,
    /// Slice G2: the viewer's `rights.can_manage_topics` per supergroup
    /// (schema 1.8.67, line 1092). Gates forum topic management.
    pub supergroup_manage_topics_right: HashMap<i64, bool>,
    /// Slice G2: the viewer's `rights.can_change_info` per supergroup
    /// (schema 1.8.67, line 1092). `toggleSupergroupSignMessages`
    /// requires this right.
    pub supergroup_change_info_right: HashMap<i64, bool>,
    /// Slice G2: the viewer's `rights.can_send_welcome_messages` per
    /// supergroup (schema 1.8.67, line 1090). Gates welcome-message
    /// management.
    pub supergroup_send_welcome_right: HashMap<i64, bool>,
    /// Slice G2: `chat.has_welcome_messages` (schema 1.8.67, line 3627)
    /// / `updateChatHasWelcomeMessages` (line 10600), keyed by chat id.
    pub chat_has_welcome_messages: HashMap<i64, bool>,
    /// Chats whose content is protected (`chat.has_protected_content`,
    /// schema 1.8.67 line 3598): no saving, forwarding or copying.
    pub protected_chats: HashSet<i64>,
    /// Chats with scheduled messages (`chat.has_scheduled_messages`,
    /// `updateChatHasScheduledMessages`); drives the composer's
    /// scheduled-messages button.
    pub scheduled_chats: HashSet<i64>,
    /// Translation state (`translateText` / `translateMessageText`, the
    /// chat translate bar).
    pub translate: TranslateState,
    /// Bot reply keyboards as TDLib reports them, and recent inline bots.
    pub reply_keyboards: ReplyKeyboardState,
    /// Slice G2: the welcome-message pack per chat
    /// (`updateChatWelcomeMessages`, schema 1.8.67, line 10649).
    pub welcome_messages: HashMap<i64, Vec<ParsedWelcomeMessage>>,
    /// Slice G2: `loadChatWelcomeMessages` fetch state, keyed by chat id.
    pub welcome_message_fetches: HashMap<i64, WelcomeMessagesFetch>,
    /// Slice G2: `(level, boost_count)` from `getChatBoostStatus`
    /// (schema 1.8.67, lines 13917/6943), keyed by chat id.
    pub chat_boost_status: HashMap<i64, (i32, i32)>,
    /// Name color and reply emoji of chats that have one
    /// (`chat.accent_color_id`, `updateChatAccentColors`).
    pub chat_accents: HashMap<i64, ChatAccent>,
    /// Stories replied to that `getStory` was already asked for, so a
    /// deleted one is not requested again on every refresh.
    pub story_reply_attempted: HashSet<(i64, i32)>,
    /// Slice G2: available boost slot ids from `getAvailableChatBoostSlots`
    /// (schema 1.8.67, line 13914), keyed by chat id. The driver consumes
    /// them to chain `boostChat` once per boost intent.
    pub boost_slots_by_chat: HashMap<i64, Vec<i32>>,
    /// Slice G2: the chat id of a pending user boost intent — set by
    /// `request_chat_boost`, consumed by the driver's `boostChat` chain.
    pub boost_intent: Option<i64>,
    /// Slice G2: channel-comments viewer — the latest
    /// `getMessageThreadHistory` result (channel post → comment thread).
    pub thread: Option<ThreadView>,
    /// Slice CL: chat-list peek preview — the latest `getChatHistory`
    /// result for one unopened chat (`parity:chatlist-chat-preview`).
    pub chat_preview_fetch: Option<PreviewHistoryFetch>,
    /// `parity:platform-chat-export` — in-progress chat history export.
    /// The driver pages `getChatHistory` into this; the UI surfaces the
    /// result (path or error) and clears it.
    pub chat_export: Option<crate::chat_export::ChatExportState>,
    pub account_export: Option<crate::account_export::AccountExport>,
    /// Parity slice: first active username per supergroup (`supergroup`
    /// object / `updateSupergroup`, schema 1.8.67 line 2746), keyed by
    /// supergroup id. Feeds the channel/supergroup header's @username.
    pub supergroup_usernames: HashMap<i64, String>,
    /// Chat-row title badge: `supergroup.verification_status` per
    /// supergroup (`updateSupergroup`), keyed by supergroup id.
    pub supergroup_verification: HashMap<i64, crate::peer_badge::VerificationStatus>,
    /// Phase A1: the viewer's own `chatMemberStatus*` per supergroup
    /// (`supergroup.status` / `updateSupergroup`, schema 1.8.67 line 2746).
    /// Drives the slow-mode bypass (admins/creators are exempt) and gates
    /// the admin slow-mode control. Absent = unknown (gated, no bypass).
    pub supergroup_member_status: HashMap<i64, ChannelMemberStatus>,
    /// Phase A1: the viewer's `rights.can_restrict_members` per supergroup
    /// from own `chatMemberStatusAdministrator` (schema 1.8.67, lines
    /// 2500/`chatAdministratorRights` 1092). `setChatSlowModeDelay` requires
    /// 13551). Absent = unknown, treated as lacking the right.
    pub supergroup_restrict_right: HashMap<i64, bool>,
    /// Phase D3a: the viewer's `rights.can_invite_users` per supergroup
    /// from own `chatMemberStatusAdministrator` (schema 1.8.67, line
    /// 1092). Invite-link management requires this right (or creator
    /// status). Absent = unknown, treated as lacking the right.
    pub supergroup_invite_right: HashMap<i64, bool>,
    /// Phase D3b: `getChatAdministrators` fetch state, keyed by chat id.
    pub admin_lists: HashMap<i64, AdminListFetch>,
    /// Phase D3b / slice G1: `getSupergroupMembers` fetch state for the
    /// promote member picker and the member-management dialog, keyed by
    /// (chat id, filter). One page per filter.
    pub supergroup_members: HashMap<(i64, MemberListFilter), SupergroupMembersFetch>,
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
    /// `parity:platform-deep-links`: the single active deep-link flow
    /// (launch link → `getDeepLinkInfo` → follow-up → open chat).
    pub deep_link: Option<DeepLinkState>,
    /// Generation counter for deep-link request correlation.
    pub deep_link_seq: u64,
    /// The link text being resolved by `getInternalLinkType` (the proxy
    /// hand-off needs it back).
    pub deep_link_original: String,
    /// `parity:proxy-settings`: TDLib's proxy list, ping results and the
    /// auto-switch / IPv6 preferences.
    pub proxy: crate::proxy::ProxyState,
    /// Slice G1: `getBasicGroupFullInfo` fetch state (the member list for
    /// basic groups), keyed by chat id. Reuses `SupergroupMembersFetch`
    /// (Loading / Loaded / Failed).
    pub basic_group_members: HashMap<i64, SupergroupMembersFetch>,
    /// Slice G1: `supergroup.join_by_request` (schema 1.8.67, lines
    /// 2733/2746), keyed by supergroup id. Drives the "Approve new
    /// members" toggle.
    pub supergroup_join_by_request: HashMap<i64, bool>,
    /// B7: `supergroup.join_to_send_messages` (schema 1.8.67, line 2746),
    /// keyed by supergroup id.
    pub supergroup_join_to_send: HashMap<i64, bool>,
    /// B7: the viewer's own `basicGroup.status`, keyed by basic group id.
    pub basic_group_status: HashMap<i64, ChannelMemberStatus>,
    /// B7: `can_change_info` of the viewer's administrator status in a
    /// basic group.
    pub basic_group_change_info_right: HashMap<i64, bool>,
    /// B7: `basicGroup.is_active` (false after the upgrade).
    pub basic_group_active: HashMap<i64, bool>,
    /// B7: `chat.available_reactions` / `updateChatAvailableReactions`,
    /// keyed by chat id.
    pub chat_available_reactions: HashMap<i64, crate::telegram::envelope::ChatAvailableReactions>,
    /// B7: `updateActiveEmojiReactions` — the emoji usable as reactions.
    pub active_emoji_reactions: Vec<String>,
    /// B7: steps waiting for a group-admin request to succeed.
    pub admin_followups: Vec<(RequestId, AdminFollowup)>,
    /// B7: finished basic group upgrades, `(old chat id, new chat id)`.
    pub chat_upgrades: Vec<(i64, i64)>,
    /// Slice G1: `supergroup.is_broadcast_group` (schema 1.8.67, lines
    /// 2736/2746), keyed by supergroup id. Set by
    /// `toggleSupergroupIsBroadcastGroup` (one-way upgrade).
    pub supergroup_is_broadcast: HashMap<i64, bool>,
    /// Slice G1: `addChatMembers` failure count from the last add
    /// attempt — `failedToAddMembers.failed_to_add_members.len()` for the
    /// bulk path (schema 1.8.67, line 3640), or the accumulated per-user
    /// `addChatMember` `failedToAddMembers` counts plus error responses
    /// for basic groups — keyed by chat id.
    /// Reset when a new add starts; cleared when the dialog closes.
    pub add_members_failed: HashMap<i64, i32>,
    /// Slice G1 fix-up: chat ids whose member-list caches were dropped by
    /// an `updateChatMember` while a member dialog may be open. One-shot;
    /// the UI drains it and refetches the open dialog's page.
    pub member_list_stale: Vec<i64>,
    /// Slice G1: last member-action failure for the member-management
    /// dialog (`setChatMemberTag` / `setChatMemberStatus`), keyed by
    /// chat id. The dialog reads the member-list fetch states, not
    /// `admin_lists`, so action failures need their own slot to be
    /// visible where the action was taken. Cleared when the dialog
    /// opens.
    pub member_action_error: HashMap<i64, String>,
    /// Phase D3b: one administrator's parsed `chatAdministratorRights`
    /// fetch state, keyed by (chat_id, user_id). Filled by `getChatMember`
    /// (purpose `GetAdminRights`); drives the edit-rights dialog.
    pub admin_rights: HashMap<(i64, i64), AdminRightsFetch>,
    /// Phase D3b: the viewer's `rights.can_promote_members` per supergroup
    /// from own `chatMemberStatusAdministrator` (schema 1.8.67, line
    /// 1092). Admin management requires this right (or creator status).
    /// Absent = unknown, treated as lacking the right.
    pub supergroup_promote_right: HashMap<i64, bool>,
    /// Slice G1: own `rights.can_manage_tags` per supergroup (schema
    /// 1.8.67, line 1092) — gates custom-title changes for others.
    pub supergroup_manage_tags_right: HashMap<i64, bool>,
    /// Phase 6: the open user / supergroup info panel, if any.
    pub open_info_panel: Option<InfoPanelTarget>,
    /// Phase 9.1: active stories per chat from `updateChatActiveStories` /
    /// `getChatActiveStories` (TDLib 1.8.67, `schema/td_api.tl:6776-6783`),
    /// keyed by chat id. Entries whose `list` is not `Main` (archived or
    /// not shown in any story list) are dropped on insert.
    pub story_tray: HashMap<i64, ChatActiveStoriesView>,
    /// Phase 9.1: full story objects from `getStory` (and `updateStory`
    /// updates), keyed by `(poster_chat_id, story_id)`. The viewer
    /// prefetches every story in a tray entry before opening.
    pub stories: HashMap<(i64, i32), ParsedStory>,
    /// Phase 9.2+: custom-emoji reactions the story picker can offer —
    /// `getStoryAvailableReactions` response (`availableReactions`,
    /// `schema/td_api.tl:13802`). Emoji, custom-emoji, and paid rows.
    pub story_available_reactions: Option<Vec<StoryAvailableReactionView>>,
    /// Phase 9.2+: sticker visuals for the picker's custom-emoji
    /// reactions — the `getCustomEmojiStickers` response, keyed by
    /// sticker id (= custom emoji id).
    pub story_custom_emoji_stickers: HashMap<i64, StickerItem>,
    /// Phase 9.2: poster chat ids whose active stories the driver should
    /// refresh with `getChatActiveStories`. Filled by the reducer on
    /// `updateStoryPostSucceeded` (a story posted from another client goes
    /// live — e.g. our own) and drained by the UI each render, like
    /// `pending_story_open`.
    pub story_tray_refresh: HashSet<i64>,
    /// Phase 9.3: story-posting round-trip state — `canPostStory`
    /// eligibility plus the `postStory` pending/succeeded/failed outcome
    /// the composer renders.
    pub story_post: StoryPostState,
    /// Phase 9.5: paginated viewers list for the story currently open in
    /// the viewer (`getStoryInteractions` pages, `Session::story_viewers`
    /// accumulates them). `None` when the panel is closed or the viewer
    /// moved to a different story.
    pub story_viewers: Option<StoryViewersState>,
    /// Statistics and public forwards of the story open in the viewer.
    pub story_insights: Option<StoryInsightsState>,
    /// The public story search (hashtag, location or venue) and its pages.
    pub story_search: Option<StorySearchState>,
    /// Phase 9.5: the in-progress `reportStory` flow for the story open in
    /// the viewer — the reason picker and the optional details step.
    /// `None` when no report is in flight.
    pub story_report: Option<StoryReportFlow>,
    /// Phase 9.5: story stealth-mode state from `updateStoryStealthMode`
    /// (TDLib 1.8.67, `schema/td_api.tl:10919`); 0/0 = disabled, no
    /// cooldown — the schema exposes no getter, so this only ever
    /// reflects updates TDLib has pushed.
    pub story_stealth: StoryStealthMode,
    /// Phase 9.5: last `activateStoryStealthMode` error (e.g. Premium
    /// required), cleared when a new activation is sent.
    pub story_stealth_error: Option<String>,
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
    /// Phase 9.5: chat ids from the last `getChatsToPostStories` answer —
    /// the composer's "post as" picker (channels/supergroups where the
    /// user has the `can_post_stories` admin right).
    pub story_post_as_chats: Vec<i64>,
    /// Phase 9.5: posted-story management round-trip state (edit /
    /// cover / privacy) rendered as one status line.
    pub story_manage: StoryManageState,
    /// Phase 9.7: `getChatStoryAlbums` results per chat (`storyAlbum` rows;
    /// covers dropped in the parser — name-only list).
    pub story_albums: HashMap<i64, Vec<ParsedStoryAlbum>>,
    /// Phase 9.7: `(chat_id, album_id)` → story ids of an opened album
    /// (`getStoryAlbumStories` pages accumulate; stories live in
    /// `Session::stories`).
    pub story_album_stories: HashMap<(i64, i32), Vec<i32>>,
    /// Phase 9.7: `getChatPostedToChatPageStories` results per chat —
    /// story ids, `pinned_story_ids` (first page only), and the server
    /// total for the "Load more" gate.
    pub chat_page_stories: HashMap<i64, ChatPageStories>,
    /// Phase 9.7: `getChatArchivedStories` pages per chat (accumulated;
    /// `next_from_story_id` is the smallest loaded id, `None` until the
    /// first page lands).
    pub archived_stories: HashMap<i64, ArchivedStories>,
    /// Phase 9.7: honest status of the latest album/pin mutation on the
    /// story page (`Sending` at send time, `Succeeded` / `Failed` when
    /// the TDLib answer lands). The story page renders it as its status
    /// line.
    pub story_page_op: Option<StoryPageOp>,
    /// B14: the close-friends list (`getCloseFriends` / `setCloseFriends`);
    /// `None` until loaded.
    pub close_friends: Option<Vec<i64>>,
    /// B14: ids sent by an in-flight `setCloseFriends`, applied on `ok`.
    pub close_friends_pending: Option<Vec<i64>>,
    /// Phase 9.1: `loadActiveStories(storyListMain)` was issued. A retry is
    /// allowed (the flag is reset) if the attempt failed.
    pub stories_active_loaded: bool,
    pub(crate) diagnostics: Arc<dyn DiagnosticSink>,
}
