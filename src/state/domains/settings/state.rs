//! Notifications, storage, privacy, sessions, websites, the account and preferences: the `settings` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct SettingsState {
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
    /// `parity:proxy-settings`: TDLib's proxy list, ping results and the
    /// auto-switch / IPv6 preferences.
    pub proxy: crate::proxy::ProxyState,
    pub account_export: Option<crate::account_export::AccountExport>,
}

impl SettingsState {
    pub(crate) fn new() -> Self {
        Self {
            hide_notification_previews: true,
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
            scope_notification_settings: HashMap::new(),
            notification_exceptions: HashMap::new(),
            notification_exceptions_loading: HashSet::new(),
            scope_settings_loading: HashSet::new(),
            reaction_notification_settings: None,
            storage_stats: None,
            storage_stats_loading: false,
            data_storage: DataStoragePrefs::default(),
            data_storage_dirty: false,
            auto_download_presets_loading: false,
            data_storage_error: None,
            storage_freed: None,
            storage_clearing: false,
            storage_limits: Default::default(),
            privacy_data: Default::default(),
            notices: AccountNotices::default(),
            sessions: None,
            sessions_loading: false,
            sessions_mutating: false,
            device_login_result: None,
            sessions_error: None,
            sessions_stale: false,
            account_ttl_days: None,
            account_ttl_loading: false,
            account_mutating: false,
            account_error: None,
            connected_websites: None,
            connected_websites_loading: false,
            websites_mutating: false,
            websites_error: None,
            websites_stale: false,
            sound_file_ids: HashMap::new(),
            pending_sound_downloads: HashSet::new(),
            pending_sound_plays: Vec::new(),
            privacy: HashMap::new(),
            read_date_show: None,
            read_date_loading: false,
            read_date_error: false,
            blocked_senders: None,
            blocked_total: 0,
            blocked_loading: false,
            blocked_error: false,
            media_prefs: MediaPrefs::default(),
            contact_prefs: ContactPrefs::default(),
            badge_prefs: BadgePrefs::default(),
            language_prefs: LanguagePrefs::default(),
            proxy: Default::default(),
            account_export: None,
        }
    }
}
