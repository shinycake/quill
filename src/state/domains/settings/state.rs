//! Notifications, storage, privacy, sessions, websites, the account and preferences: the `settings` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::fetch::Fetch;
use crate::state::*;

/// The read-date privacy round-trip. A get and a set are told apart, so
/// a get answered after a set went out never overwrites the set value:
/// only the answer to the request named here lands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReadDateStage {
    /// Nothing in flight; the last request (if any) succeeded.
    #[default]
    Idle,
    /// `getReadDatePrivacySettings` is in flight.
    Getting(RequestId),
    /// `setReadDatePrivacySettings` is in flight; `read_date_show` holds
    /// the value it sent.
    Setting(RequestId),
    /// The last get or set failed.
    Failed,
}

impl ReadDateStage {
    /// A get or set is in flight.
    pub fn in_flight(self) -> bool {
        matches!(self, Self::Getting(_) | Self::Setting(_))
    }
}

pub struct SettingsState {
    /// Mirror of `settings::Preferences::notify_view` (tdesktop "Name" /
    /// "Text"), loaded from `prefs.json` at connect time.
    pub notify_view: crate::notify::NotifyView,
    /// Mirror of `settings::Preferences::notify_photo_previews`: a photo
    /// message's notification shows the photo.
    pub notify_photo_previews: bool,
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
    /// "Upload Sound": the file in flight, or how it ended until the
    /// screen has reacted.
    pub tone_upload: crate::tone_upload::ToneUpload,
    /// Parity slice: `getScopeNotificationSettings` results per scope; used
    /// for `use_default_*` fallback (e.g. default sound) and the scope
    /// defaults settings view.
    pub scope_notification_settings: HashMap<NotificationSettingsScope, ScopeNotificationSettings>,
    /// Parity slice: `getChatNotificationSettingsExceptions` answers —
    /// chat ids with non-default notification settings per scope (the
    /// exceptions list view). No entry: never asked, or dropped as stale
    /// by `updateChatNotificationSettings`, so the page asks again; a
    /// failure stays failed until its Retry.
    pub notification_exceptions: HashMap<NotificationSettingsScope, Fetch<Vec<i64>>>,
    /// Parity slice: scopes with a `getScopeNotificationSettings` in flight.
    pub scope_settings_loading: HashSet<NotificationSettingsScope>,
    /// Parity slice: `updateReactionNotificationSettings` cache. No getter
    /// exists — this stays `None` until the first update arrives.
    pub reaction_notification_settings: Option<ReactionNotificationSettings>,
    /// `setScopeNotificationSettings` saves in flight: the edit shows at
    /// once (the next edit starts from it) and the answer confirms or
    /// undoes it.
    pub scope_saves: crate::state::optimistic::OptimisticSaves<
        NotificationSettingsScope,
        ScopeNotificationSettings,
    >,
    /// The same for `setReactionNotificationSettings`.
    pub reaction_saves: crate::state::optimistic::OptimisticSaves<(), ReactionNotificationSettings>,
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
    /// This session sent the first cache limits; the account counts as
    /// seeded once TDLib echoes them (`storage_limits.chosen()`).
    pub storage_limits_seed_sent: bool,
    /// B13: new-chat privacy, inactive-session TTL, 18+ option, network
    /// usage and the remember-password check.
    pub privacy_data: PrivacyData,
    /// Batch 4: new-login alert, service popups and terms of service.
    pub notices: AccountNotices,
    /// Slice A3: cached `getActiveSessions` answer (TGX `SessionsInfo`
    /// style); drives the Active Sessions overlay. Incomplete login
    /// attempts (`is_password_pending`) render in their own section.
    ///
    /// One value for the fetch: in flight, answered, or failed with the
    /// honest one-line error (classified from the TDLib error code, never the
    /// native message) of the last fetch or terminate. The cache stays
    /// visible meanwhile and the next successful fetch clears the error.
    pub sessions: Fetch<Vec<ParsedSession>>,
    /// Slice A3: a `terminateSession` / `terminateAllOtherSessions` round
    /// trip is in flight — terminate buttons stay disabled meanwhile.
    pub sessions_mutating: bool,
    pub device_login_result: Option<crate::auth::DeviceLoginResult>,
    /// Slice A3: a terminate succeeded — the old cache stays visible and
    /// is refetched from the authoritative answer on the next ingest
    /// (the `saved_sounds_stale` pattern); never an optimistic delete.
    pub sessions_stale: bool,
    /// Slice A7: cached `getAccountTtl` answer, in days — drives the
    /// self-destruct-if-away picker (UI half ships post-Phase-9).
    pub account_ttl_days: Option<i32>,
    /// Slice A7: a `getAccountTtl` round trip is in flight.
    pub account_ttl_loading: bool,
    /// Slice A7: a `deleteAccount` / `setAccountTtl` round trip is in
    /// flight — the account surfaces stay disabled meanwhile.
    pub account_mutating: bool,
    /// Slice A7: honest one-line failure of the last account-lifecycle
    /// op (classified from the TDLib error code, never the native
    /// message). Cleared on the next attempt and on success.
    pub account_error: Option<String>,
    /// Slice A4: cached `getConnectedWebsites` answer (TGX
    /// `SettingsWebsitesController` style); drives the Connected Websites
    /// overlay.
    pub connected_websites: Option<Vec<ParsedWebsite>>,
    /// The account's login passkeys (`crate::passkeys`).
    pub passkeys: crate::passkeys::PasskeysState,
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
    /// Parity slice: sound ids with a preview requested whose file is not
    /// local yet, and when it was asked. When the file completes within
    /// `SOUND_PREVIEW_WINDOW`, its path lands in `pending_sound_plays`; a
    /// later completion is not a preview anyone is waiting for.
    pub pending_sound_downloads: HashMap<i64, std::time::Instant>,
    /// Parity slice: local sound-file paths the UI should play, drained by
    /// `flush_notifications`. The reducer never spawns processes.
    pub pending_sound_plays: Vec<std::path::PathBuf>,
    /// Slice S3: per-key rule state for the Privacy screen
    /// (`userPrivacySettingShowStatus`, `ShowPhoneNumber`,
    /// `ShowProfilePhoto`, `ShowLinkInForwardedMessages`,
    /// `AllowChatInvites`; schema 1.8.67, :8981-:9003). Present only
    /// after a fetch was attempted — absent means never requested.
    pub privacy: HashMap<PrivacySettingKey, PrivacyKeyState>,
    /// Slice S3: `readDatePrivacySettings.show_read_date` (schema 1.8.67,
    /// :9026) — `None` while never fetched.
    pub read_date_show: Option<bool>,
    /// Which read-date request is in flight, or how the last one ended.
    pub read_date_stage: ReadDateStage,
    /// Slice S3: blocked user ids from `getBlockedMessageSenders`
    /// (schema 1.8.67, :14505); idle while never fetched.
    /// A blocked-senders page in flight or a failed get/set shows in the
    /// same value; the loaded ids stay meanwhile.
    pub blocked_senders: Fetch<Vec<i64>>,
    /// `total_count` from the last `messageSenders` answer; more pages
    /// exist while `blocked_senders.len() < blocked_total`.
    pub blocked_total: i32,
    /// Changes whenever the blocked list restarts or is dropped. A page
    /// answers under the generation it was asked in and only extends that
    /// list.
    pub blocked_generation: u32,
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
    /// Slice parity:settings-language: the app language tag sent in
    /// `setTdlibParameters`, persisted via `settings::LanguagePrefs`.
    /// Loaded at startup like `call_prefs`; the UI saves on change.
    pub language_prefs: LanguagePrefs,
    /// Custom device name sent as `device_model` at startup.
    pub device_prefs: crate::settings::DevicePrefs,
    /// `parity:proxy-settings`: TDLib's proxy list, ping results and the
    /// auto-switch / IPv6 preferences.
    pub proxy: crate::proxy::ProxyState,
    /// The latest `pingProxy` sent for each ping slot (a list entry, or
    /// the link box's). Only its answer fills the slot: an older ping of
    /// another server (the previous link, the entry before an edit) is
    /// dropped.
    pub(crate) proxy_ping_requests: HashMap<i32, RequestId>,
    /// The running or just-finished account export ("Export Telegram
    /// data"); the UI shows its progress and clears it.
    pub account_export: Option<crate::export::AccountExport>,
    /// The running or just-finished chat export; the UI shows its progress
    /// and clears it.
    pub chat_export: Option<crate::export::ChatExport>,
    /// Running "Save as HTML" exports of rich messages
    /// (`parity:menu-save-selection-html`); the driver removes each one
    /// when the UI takes its outcome.
    pub rich_html_exports: Vec<crate::rich_html::RichHtmlExport>,
    /// Cache copies the exports downloaded for themselves.
    pub export_files: ExportFiles,
    /// The `setlanguage` link whose box is open.
    pub language_link: Option<LanguageLinkLookup>,
    /// Settings → Language: packs, the switch in flight, strings to install.
    pub language: LanguageState,
    /// Quick reply shortcuts and their messages.
    pub quick_replies: QuickRepliesState,
    /// Telegram Business settings: time zones and saves in flight.
    pub business: BusinessSettingsState,
    /// Links to chat, the connected chatbot and the Business limits.
    pub business_extra: BusinessExtraState,
}

impl SettingsState {
    /// The blocked list starts over (a fresh fetch, or its data was
    /// dropped): pages asked for the old one no longer apply.
    pub fn restart_blocked_list(&mut self) {
        self.blocked_generation = self.blocked_generation.wrapping_add(1);
    }

    /// One blocked-senders page. Page 0 replaces the list; a later page
    /// extends the list it was asked for, without ids already there. A page
    /// of an older list, or one with no list left to extend, is dropped and
    /// changes nothing (not even the total).
    pub fn apply_blocked_page(
        &mut self,
        offset: i32,
        generation: u32,
        total_count: i32,
        sender_ids: Vec<i64>,
    ) {
        if generation != self.blocked_generation {
            return;
        }
        if offset == 0 {
            self.blocked_total = total_count;
            self.blocked_senders
                .succeed(dedup_ids(Vec::new(), sender_ids));
        } else if let Some(list) = self.blocked_senders.take_data() {
            self.blocked_total = total_count;
            self.blocked_senders.succeed(dedup_ids(list, sender_ids));
        }
    }

    pub fn export_links(&self) -> impl Iterator<Item = &crate::export::handle::WorkerLink> {
        let chat = self.chat_export.iter().map(|export| &export.link);
        let account = self.account_export.iter().map(|export| &export.link);
        chat.chain(account)
            .chain(self.rich_html_exports.iter().map(|export| &export.link))
    }

    pub(crate) fn new() -> Self {
        Self {
            quick_replies: QuickRepliesState::default(),
            business: BusinessSettingsState::default(),
            business_extra: BusinessExtraState::default(),
            notify_view: crate::notify::NotifyView::ShowPreview,
            notify_photo_previews: true,
            inapp_sounds_enabled: true,
            desktop_notifications: true,
            pending_notifications: Vec::new(),
            pending_notification_clears: Vec::new(),
            pending_attention: false,
            disable_contact_registered_notifications: false,
            shown_notification_chats: std::collections::HashSet::new(),
            default_auto_delete_secs: None,
            default_auto_delete_busy: false,
            default_auto_delete_error: None,
            saved_notification_sounds: Vec::new(),
            saved_sounds_loaded: false,
            saved_sounds_stale: false,
            tone_upload: crate::tone_upload::ToneUpload::Idle,
            scope_notification_settings: HashMap::new(),
            notification_exceptions: HashMap::new(),
            scope_settings_loading: HashSet::new(),
            reaction_notification_settings: None,
            scope_saves: Default::default(),
            reaction_saves: Default::default(),
            storage_stats: None,
            storage_stats_loading: false,
            data_storage: DataStoragePrefs::default(),
            data_storage_dirty: false,
            auto_download_presets_loading: false,
            data_storage_error: None,
            storage_freed: None,
            storage_clearing: false,
            storage_limits: Default::default(),
            storage_limits_seed_sent: false,
            privacy_data: Default::default(),
            notices: AccountNotices::default(),
            sessions: Fetch::Idle,
            sessions_mutating: false,
            device_login_result: None,
            sessions_stale: false,
            account_ttl_days: None,
            account_ttl_loading: false,
            account_mutating: false,
            account_error: None,
            connected_websites: None,
            passkeys: Default::default(),
            connected_websites_loading: false,
            websites_mutating: false,
            websites_error: None,
            websites_stale: false,
            sound_file_ids: HashMap::new(),
            pending_sound_downloads: HashMap::new(),
            pending_sound_plays: Vec::new(),
            privacy: HashMap::new(),
            read_date_show: None,
            read_date_stage: ReadDateStage::Idle,
            blocked_senders: Fetch::Idle,
            blocked_total: 0,
            blocked_generation: 0,
            media_prefs: MediaPrefs::default(),
            contact_prefs: ContactPrefs::default(),
            badge_prefs: BadgePrefs::default(),
            language_prefs: LanguagePrefs::default(),
            device_prefs: Default::default(),
            proxy: Default::default(),
            proxy_ping_requests: HashMap::new(),
            account_export: None,
            chat_export: None,
            rich_html_exports: Vec::new(),
            export_files: ExportFiles::default(),
            language_link: None,
            language: LanguageState::default(),
        }
    }
}

/// `list` followed by the ids of `more` it does not hold yet.
fn dedup_ids(mut list: Vec<i64>, more: Vec<i64>) -> Vec<i64> {
    for id in more {
        if !list.contains(&id) {
            list.push(id);
        }
    }
    list
}

/// How long a previewed sound's download is still worth playing (the user
/// pressed play on the picker and waits for it).
pub const SOUND_PREVIEW_WINDOW: std::time::Duration = std::time::Duration::from_secs(15);

impl SettingsState {
    /// A preview download finished at `now`: whether it is still wanted.
    /// Either way the request is over.
    pub fn take_sound_preview(&mut self, sound_id: i64, now: std::time::Instant) -> bool {
        self.pending_sound_downloads
            .remove(&sound_id)
            .is_some_and(|asked| now.saturating_duration_since(asked) <= SOUND_PREVIEW_WINDOW)
    }
}
