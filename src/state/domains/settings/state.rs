//! Notifications, storage, privacy, sessions, websites, the account and preferences: the `settings` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::fetch::Fetch;
use crate::state::*;

/// The read-date privacy setting (`show_read_date`) and its round trip.
/// A get and a set are told apart, so a get answered after a set went out
/// never overwrites the set value: only the answer to the request named
/// here lands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReadDate {
    /// Never fetched.
    #[default]
    Unknown,
    /// `getReadDatePrivacySettings` is in flight; `known` is the value
    /// held meanwhile.
    Getting {
        request: RequestId,
        known: Option<bool>,
    },
    /// The server's value.
    Known(bool),
    /// `setReadDatePrivacySettings` is in flight; `show` is the value it
    /// sent, shown at once.
    Setting { request: RequestId, show: bool },
    /// The last get or set failed. A failed get keeps `known`; a failed
    /// set drops the optimistic value (the next fetch restores the truth).
    Failed { known: Option<bool> },
}

impl ReadDate {
    /// The value to show, if one is held.
    pub fn show(self) -> Option<bool> {
        match self {
            Self::Known(show) | Self::Setting { show, .. } => Some(show),
            Self::Getting { known, .. } | Self::Failed { known } => known,
            Self::Unknown => None,
        }
    }

    /// A get or set is in flight.
    pub fn in_flight(self) -> bool {
        matches!(self, Self::Getting { .. } | Self::Setting { .. })
    }

    /// A set is in flight (a get must not be sent over it).
    pub fn setting(self) -> bool {
        matches!(self, Self::Setting { .. })
    }

    /// A get went out as `request`.
    pub fn start_get(&mut self, request: RequestId) {
        *self = Self::Getting {
            request,
            known: self.show(),
        };
    }

    /// A set went out as `request`.
    pub fn start_set(&mut self, request: RequestId, show: bool) {
        *self = Self::Setting { request, show };
    }

    /// A send was refused: nothing is in flight for it. A request already
    /// out stays.
    pub fn send_refused(&mut self) {
        if !self.in_flight() {
            *self = Self::Failed { known: self.show() };
        }
    }

    /// The get `request` answered.
    pub fn got(&mut self, request: RequestId, show: bool) {
        if matches!(*self, Self::Getting { request: r, .. } if r == request) {
            *self = Self::Known(show);
        }
    }

    /// The set `request` was confirmed.
    pub fn set_confirmed(&mut self, request: RequestId) {
        if let Self::Setting { request: r, show } = *self
            && r == request
        {
            *self = Self::Known(show);
        }
    }

    /// `request` failed (a get keeps the value, a set drops it).
    pub fn request_failed(&mut self, request: RequestId) {
        match *self {
            Self::Getting { request: r, known } if r == request => {
                *self = Self::Failed { known };
            }
            Self::Setting { request: r, .. } if r == request => {
                *self = Self::Failed { known: None };
            }
            _ => {}
        }
    }
}

/// A write against a fetched list (sessions, connected websites).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ListEdit {
    /// No write in flight and the list is current.
    #[default]
    Idle,
    /// A write is in flight: its buttons stay disabled. `refetch` is set
    /// when the list went stale before or while the write was out, so a
    /// failed write still leaves the refetch owed.
    InFlight { refetch: bool },
    /// A write succeeded: the cache stays visible and is refetched from
    /// the authoritative answer on the next ingest (never an optimistic
    /// change). Also set when an update says the list changed elsewhere.
    Refetch,
}

impl ListEdit {
    pub fn in_flight(self) -> bool {
        matches!(self, Self::InFlight { .. })
    }

    /// The list is out of date and should be asked for again.
    pub fn needs_refetch(self) -> bool {
        self == Self::Refetch
    }

    /// A write went out. A refetch owed before it stays owed.
    pub fn begin_write(&mut self) {
        *self = Self::InFlight {
            refetch: matches!(self, Self::Refetch | Self::InFlight { refetch: true }),
        };
    }

    /// The write succeeded: the list is refetched from the answer.
    pub fn write_succeeded(&mut self) {
        *self = Self::Refetch;
    }

    /// The write failed or never left: back to where the list stood, so a
    /// refetch owed meanwhile is still asked for.
    pub fn write_failed(&mut self) {
        *self = match *self {
            Self::InFlight { refetch: true } | Self::Refetch => Self::Refetch,
            _ => Self::Idle,
        };
    }

    /// The refetch answered or failed: the list is current again (or the
    /// user refreshes by hand). A write in flight stays in flight.
    pub fn refetched(&mut self) {
        if *self == Self::Refetch {
            *self = Self::Idle;
        }
    }

    /// Something changed elsewhere: ask again. A write that is out keeps
    /// the need, so its failure does not forget it.
    pub fn mark_stale(&mut self) {
        match self {
            Self::Idle => *self = Self::Refetch,
            Self::InFlight { refetch } => *refetch = true,
            Self::Refetch => {}
        }
    }
}

/// The device-link round trip (`confirmQrCodeAuthentication`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeviceLogin {
    #[default]
    Idle,
    /// The confirmation this request carries is in flight.
    InFlight(RequestId),
    /// That request ended; the screen that sent it takes the result once.
    Done {
        request: RequestId,
        result: crate::auth::DeviceLoginResult,
    },
}

impl DeviceLogin {
    pub fn in_flight(self) -> bool {
        matches!(self, Self::InFlight(_))
    }

    /// The result of the confirmation `asked` sent, once: the stage returns
    /// to `Idle`. A result for another request is not this asker's.
    pub fn take_result_for(&mut self, asked: RequestId) -> Option<crate::auth::DeviceLoginResult> {
        match *self {
            Self::Done { request, result } if request == asked => {
                *self = Self::Idle;
                Some(result)
            }
            _ => None,
        }
    }
}

/// How the last `setAutoDownloadSettings` ended, by the request that asked:
/// the editor that sent it closes on `Saved` and reopens on `Refused`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoDownloadAnswer {
    Saved(RequestId),
    Refused(RequestId),
}

/// The local storage clear (`optimizeStorage`): one at a time. `Clearing`
/// leaves on its answer (`Freed`) or its error (`Failed`, the line the page
/// shows); `Freed` and `Failed` stay until the screen is reopened or the
/// next clear starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum StorageClear {
    #[default]
    Idle,
    Clearing,
    /// Bytes the last confirmed clear freed: the screen shows "{size}
    /// freed on your device!".
    Freed(i64),
    /// The clear did not go: why, for the page.
    Failed(String),
}

impl StorageClear {
    pub fn is_clearing(&self) -> bool {
        matches!(self, StorageClear::Clearing)
    }

    pub fn freed(&self) -> Option<i64> {
        match self {
            StorageClear::Freed(bytes) => Some(*bytes),
            StorageClear::Idle | StorageClear::Clearing | StorageClear::Failed(_) => None,
        }
    }

    pub fn failed(&self) -> Option<&str> {
        match self {
            StorageClear::Failed(why) => Some(why),
            StorageClear::Idle | StorageClear::Clearing | StorageClear::Freed(_) => None,
        }
    }
}

pub struct SettingsState {
    /// See [`AutoDownloadAnswers`].
    pub auto_download_answers: AutoDownloadAnswers,
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
    /// A notification-worthy message arrived: the UI bounces the Dock icon /
    /// flashes the taskbar (when the user wants it and the OS is not in
    /// Do Not Disturb). Set independently of the "Desktop notifications"
    /// switch, like tdesktop's alert.
    pub pending_attention: bool,
    /// TDLib option `disable_contact_registered_notifications`: the inverse
    /// of tdesktop's "Contact joined Telegram" event switch.
    pub disable_contact_registered_notifications: bool,
    /// Chats with an OS notification we showed (or TDLib reports active
    /// from a previous launch), and those whose toast the UI withdraws
    /// (read elsewhere or removed by TDLib; drained by the UI, which
    /// dismisses the toast). Only shown chats produce a clear.
    pub notified: ShownNotifications,
    /// `getDefaultMessageAutoDeleteTime` (seconds, 0 = off): the fetch or
    /// write in flight, the cached value, or the honest one-line failure
    /// of the last request.
    pub default_auto_delete: Fetch<i32>,
    /// Parity slice: `getSavedNotificationSounds` cache (titles / durations
    /// for the sound picker; `sound` files download on demand).
    ///
    /// `Stale` means the list changed since the last fetch
    /// (`updateSavedNotificationSounds`, an upload, a removal): the driver
    /// refetches on the next ingest. A failed fetch
    /// waits for the next change.
    pub saved_sounds: Fetch<Vec<NotificationSound>>,
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
    pub storage_stats: Fetch<StorageStats>,
    /// Slice S4: per-network auto-download settings, the local source of
    /// truth (TDLib has no getter for the current values). Loaded from
    /// `data_storage.json` at session setup; seeded once from
    /// `getAutoDownloadSettingsPresets` when unseeded.
    pub data_storage: DataStoragePrefs,
    /// Slice S4: the presets answer seeded `data_storage` — the driver
    /// persists it on the next ingest (the reducer cannot touch the
    /// filesystem).
    pub data_storage_dirty: bool,
    /// Slice S4: last Data & Storage failure, shown on the screen
    /// (failures surface there, never as toasts — the S3 pattern).
    /// Slice S4: the failure line of each kind of Data & Storage save; it
    /// shows on the screen (never as a toast).
    pub data_storage_save: StorageSaveFailures,
    /// Batch 6: the `optimizeStorage` round trip and its result.
    pub storage_clear: StorageClear,
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
    ///
    /// One value for the fetch: in flight, answered, or failed with the
    /// honest one-line error (classified from the TDLib error code, never the
    /// native message) of the last fetch or terminate. The cache stays
    /// visible meanwhile and the next successful fetch clears the error.
    pub sessions: Fetch<Vec<ParsedSession>>,
    /// Slice A3: a `terminateSession` / `terminateAllOtherSessions` round
    /// trip in flight (terminate buttons stay disabled meanwhile), or a
    /// terminate that succeeded — the old cache stays visible and is
    /// refetched from the authoritative answer on the next ingest (the
    /// `saved_sounds_stale` pattern); never an optimistic delete.
    pub sessions_op: crate::state::fetch::ListOp,
    /// The device-link (`confirmQrCodeAuthentication`) round trip and how
    /// it ended until the screen has reacted.
    pub device_login: DeviceLogin,
    /// Slice A7: cached `getAccountTtl` answer, in days — drives the
    /// self-destruct-if-away picker (UI half ships post-Phase-9).
    /// A `getAccountTtl` or `setAccountTtl` round trip is in flight, the
    /// days are known, or the last try failed with the honest one-line
    /// failure (classified from the TDLib error code, never the native
    /// message); the known days stay. Cleared on the next attempt and on
    /// success.
    pub account_ttl: Fetch<i32>,
    /// Slice A7: a `deleteAccount` round trip is in flight — the account
    /// surfaces stay disabled meanwhile — or its honest one-line failure.
    pub account_delete: Fetch<()>,
    /// Slice A4: cached `getConnectedWebsites` answer (TGX
    /// `SettingsWebsitesController` style); drives the Connected Websites
    /// overlay.
    ///
    /// One value for the fetch: in flight, answered, or failed with the
    /// honest one-line error (classified from the TDLib error code, never
    /// the native message) of the last fetch or disconnect; the cache stays
    /// visible meanwhile and the next successful fetch clears the error.
    pub connected_websites: Fetch<Vec<ParsedWebsite>>,
    /// The account's login passkeys (`crate::passkeys`).
    pub passkeys: crate::passkeys::PasskeysState,
    /// Slice A4: a `disconnectWebsite` / `disconnectAllWebsites` round trip
    /// in flight (disconnect buttons stay disabled meanwhile), or a
    /// disconnect that succeeded — the old cache stays visible and is
    /// refetched from the authoritative answer on the next ingest (the
    /// `saved_sounds_stale` pattern); never an optimistic delete.
    pub websites_op: crate::state::fetch::ListOp,
    /// Parity slice: downloaded-file id → notification sound id, for files
    /// fetched as notification sounds.
    pub sound_file_ids: HashMap<i32, i64>,
    /// Parity slice: sound ids with a preview requested whose file is not
    /// local yet. When the file completes, its path lands in
    /// `pending_sound_plays`. The set belongs to the open sound picker: the
    /// UI calls `forget_sound_previews` once no picker is open, so a
    /// completion nobody waits for plays nothing.
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
    /// :9026) with the request in flight, or how the last one ended.
    pub read_date: ReadDate,
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
    /// The failure line of the last account-lifecycle op.
    pub fn account_error(&self) -> Option<&str> {
        self.account_ttl.error().or(self.account_delete.error())
    }

    /// An account-lifecycle round trip is in flight: the surfaces stay
    /// disabled.
    pub fn account_busy(&self) -> bool {
        self.account_ttl.is_loading() || self.account_delete.is_loading()
    }

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

    /// An export's request that will never be answered: its worker waits on
    /// the link for this answer, so the link is told the send failed.
    pub(crate) fn fail_lost_export(&mut self, purpose: RequestPurpose, id: RequestId) {
        match purpose {
            RequestPurpose::ExportAccount => {
                if let Some(export) = self.account_export.as_mut() {
                    export.link.fail_send(id);
                }
            }
            RequestPurpose::ExportChat => {
                if let Some(export) = self.chat_export.as_mut() {
                    export.link.fail_send(id);
                }
            }
            RequestPurpose::ExportRichHtml => {
                for export in &mut self.rich_html_exports {
                    export.link.fail_send(id);
                }
            }
            _ => {}
        }
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
            pending_attention: false,
            disable_contact_registered_notifications: false,
            notified: ShownNotifications::default(),
            default_auto_delete: Fetch::Idle,
            saved_sounds: Fetch::Idle,
            tone_upload: crate::tone_upload::ToneUpload::Idle,
            scope_notification_settings: HashMap::new(),
            notification_exceptions: HashMap::new(),
            scope_settings_loading: HashSet::new(),
            reaction_notification_settings: None,
            scope_saves: Default::default(),
            reaction_saves: Default::default(),
            storage_stats: Fetch::Idle,
            data_storage: DataStoragePrefs::default(),
            data_storage_dirty: false,
            auto_download_answers: AutoDownloadAnswers::default(),
            data_storage_save: StorageSaveFailures::default(),
            storage_clear: StorageClear::Idle,
            storage_limits: Default::default(),
            privacy_data: Default::default(),
            notices: AccountNotices::default(),
            sessions: Fetch::Idle,
            sessions_op: Default::default(),
            device_login: DeviceLogin::Idle,
            account_ttl: Fetch::Idle,
            account_delete: Fetch::Idle,
            connected_websites: Fetch::Idle,
            passkeys: Default::default(),
            websites_op: Default::default(),
            sound_file_ids: HashMap::new(),
            pending_sound_downloads: HashSet::new(),
            pending_sound_plays: Vec::new(),
            privacy: HashMap::new(),
            read_date: ReadDate::Unknown,
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

impl SettingsState {
    /// The device link is out: the sessions buttons stay disabled (a
    /// terminate in flight is `sessions_op`).
    pub fn sessions_busy(&self) -> bool {
        self.sessions_op.is_mutating() || self.device_login.in_flight()
    }

    /// The Data & Storage failure line: the presets fetch, the cache clear
    /// or the last save, each from its own stage.
    pub fn data_storage_error(&self) -> Option<&str> {
        self.data_storage
            .seeded
            .failure()
            .or_else(|| self.storage_clear.failed())
            .or_else(|| self.data_storage_save.error())
    }

    /// The saved notification sounds held.
    pub fn saved_notification_sounds(&self) -> &[NotificationSound] {
        self.saved_sounds.data().map_or(&[], Vec::as_slice)
    }

    /// The "freed" line goes when the screen is reopened; a clear still on
    /// its way is not forgotten.
    pub fn reset_storage_freed(&mut self) {
        if matches!(self.storage_clear, StorageClear::Freed(_)) {
            self.storage_clear = StorageClear::Idle;
        }
    }

    /// A preview download finished: whether a picker still waits for it.
    /// Either way the request is over.
    pub fn take_sound_preview(&mut self, sound_id: i64) -> bool {
        self.pending_sound_downloads.remove(&sound_id)
    }

    /// No sound picker is open: previews asked for, and any not played yet,
    /// are no longer wanted.
    pub fn forget_sound_previews(&mut self) {
        self.pending_sound_downloads.clear();
        self.pending_sound_plays.clear();
    }
}
