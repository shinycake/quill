//! TDLib updates and answers for settings: privacy, notifications, sessions, storage, proxy and the account.
mod parse;

use crate::data_settings::{AutoDownloadNetSettings, StorageChatStats};
use crate::ids::ChatId;
use crate::privacy::PrivacyRule;
use crate::telegram::envelope::*;
pub(crate) use parse::parse_settings_payload;

/// Payloads for settings: privacy, notifications, sessions, storage, proxy and the account; wrapped as
/// [`EnvelopePayload::Settings`].
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsPayload {
    /// `updateNotificationGroup` (schema 1.8.67, line 10685), reduced to
    /// what clearing shown notifications needs: the chat, how many
    /// notifications remain in the group and how many were added.
    UpdateNotificationGroup {
        chat_id: ChatId,
        total_count: i32,
        added_count: usize,
        removed_count: usize,
    },
    /// `languagePackInfo` — the `getLanguagePackInfo` answer.
    LanguagePackInfo(LanguagePackInfoData),
    /// `updateActiveNotifications` (schema 1.8.67, line 10688): chats that
    /// still have notifications from a previous launch.
    UpdateActiveNotifications { chat_ids: Vec<ChatId> },
    /// Phase C2i: `userPrivacySettingRules` — `getUserPrivacySettingRules`.
    /// Slice S3: now carries the parsed rule details (exception user ids),
    /// not just constructor names.
    UserPrivacySettingRules { rules: Vec<PrivacyRule> },
    /// Slice S3: `updateUserPrivacySettingRules` (schema 1.8.67, :10871) —
    /// rules changed on another device; `setting` is the
    /// `UserPrivacySetting` constructor name.
    UpdateUserPrivacySettingRules {
        setting: String,
        rules: Vec<PrivacyRule>,
    },
    /// Slice S3: `readDatePrivacySettings` (schema 1.8.67, :9026) —
    /// the `getReadDatePrivacySettings` answer.
    ReadDatePrivacySettings { show_read_date: bool },
    /// Slice S3: `messageSenders` (schema 1.8.67, :14505) — the
    /// `getBlockedMessageSenders` answer; only user senders are kept.
    BlockedMessageSenders {
        total_count: i32,
        sender_ids: Vec<i64>,
        /// Every sender, users and chats (video-chat "join as" choices).
        senders: Vec<MessageSender>,
    },
    /// `notificationSounds` — `getSavedNotificationSounds` response.
    NotificationSounds { sounds: Vec<NotificationSound> },
    /// `storageStatistics` — `getStorageStatistics` response (Phase S2).
    /// Aggregated by file type across chats; stored in
    /// `Session::storage_stats` when the pending purpose is
    /// `GetStorageStatistics`.
    StorageStatistics {
        total_size: i64,
        by_file_type: Vec<StorageFileTypeStats>,
        /// Slice S4: per-chat rows (`chat_limit` > 0).
        by_chat: Vec<StorageChatStats>,
    },
    /// Slice S4: `autoDownloadSettingsPresets` — the
    /// `getAutoDownloadSettingsPresets` response (schema 1.8.67, line
    /// 9862: `autoDownloadSettingsPresets low:autoDownloadSettings
    /// medium:autoDownloadSettings high:autoDownloadSettings =
    /// AutoDownloadSettingsPresets;`). Seeds the local per-network
    /// settings once (the reducer marks them dirty for the driver to
    /// persist).
    AutoDownloadSettingsPresets {
        low: AutoDownloadNetSettings,
        medium: AutoDownloadNetSettings,
        high: AutoDownloadNetSettings,
    },
    /// Batch 4: `updateUnconfirmedSession` — the first unconfirmed login
    /// (`None` when none is left) and how many there are.
    UpdateUnconfirmedSession {
        session: Option<UnconfirmedLogin>,
        count: i32,
    },
    /// Batch 6: `resetPasswordResult*` — the `resetPassword` answer.
    ResetPasswordResult(ResetPasswordOutcome),
    /// Slice A2: `passwordState` — the `getPasswordState` /
    /// `setPassword` / `setRecoveryEmailAddress` /
    /// `resendRecoveryEmailAddressCode` /
    /// `cancelRecoveryEmailAddressVerification` response (schema
    /// 1.8.67, line 273). Stored in `Session::password_state` when the
    /// pending purpose is `PasswordStateOp`.
    PasswordState { state: PasswordState },
    /// Slice A3: `sessions` — `getActiveSessions` response (schema
    /// 1.8.67, lines 9147/15102). Stored in `Session::sessions` when the
    /// pending purpose is `GetActiveSessions`.
    DeviceLoginResult {
        result: crate::auth::DeviceLoginResult,
    },
    Sessions {
        sessions: Vec<ParsedSession>,
        /// B13: `sessions.inactive_session_ttl_days` (schema 1.8.67,
        /// :9150) — days of inactivity before sessions are terminated.
        inactive_session_ttl_days: Option<i32>,
    },
    /// B13: `newChatPrivacySettings` — `getNewChatPrivacySettings` answer.
    NewChatPrivacySettings(crate::privacy::NewChatPrivacy),
    /// B13: `networkStatistics` — `getNetworkStatistics` answer.
    NetworkStatistics(crate::network_usage::NetworkUsage),
    /// B13: `recoveryEmailAddress` — the answer that proves a typed
    /// password right (`getRecoveryEmailAddress`).
    RecoveryEmailAddress,
    /// B13: `updateSuggestedActions` (schema 1.8.67, :11070): constructor
    /// names of the added and removed actions.
    UpdateSuggestedActions {
        added: Vec<String>,
        removed: Vec<String>,
    },
    /// `updateContactCloseBirthdays` (schema 1.8.67): contacts whose
    /// birthday is yesterday, today or tomorrow.
    UpdateContactCloseBirthdays {
        users: Vec<crate::chatlist_suggestions::CloseBirthday>,
    },
    /// `parity:proxy-settings`: `addedProxies` — `getProxies` answer.
    AddedProxies {
        proxies: Vec<crate::proxy::ProxyEntry>,
    },
    /// `parity:proxy-settings`: `addedProxy` — `addProxy` / `editProxy`
    /// answer (`None`: an unknown proxy type).
    AddedProxy {
        proxy: Option<crate::proxy::ProxyEntry>,
    },
    /// Slice A7: `accountTtl` — `getAccountTtl` response (schema 1.8.67,
    /// line 9053). Stored in `Session::account_ttl_days` when the
    /// pending purpose is `GetAccountTtl`.
    AccountTtl { days: i32 },
    /// Slice A4: `connectedWebsites` — `getConnectedWebsites` response
    /// (schema 1.8.67, lines 9171/15124). Stored in
    /// `Session::connected_websites` when the pending purpose is
    /// `GetConnectedWebsites`.
    ConnectedWebsites { websites: Vec<ParsedWebsite> },
    /// `updateSavedNotificationSounds` — the saved-sound list changed;
    /// the reducer marks the cached list stale (schema line 10947).
    UpdateSavedNotificationSounds { sound_ids: Vec<i64> },
    /// `scopeNotificationSettings` — `getScopeNotificationSettings` response.
    ScopeNotificationSettings {
        scope: NotificationSettingsScope,
        settings: ScopeNotificationSettings,
    },
    /// `updateScopeNotificationSettings` — a scope's defaults changed
    /// (schema line 10668).
    UpdateScopeNotificationSettings {
        scope: NotificationSettingsScope,
        settings: ScopeNotificationSettings,
    },
    /// `updateReactionNotificationSettings` — reaction and poll-vote
    /// notification settings changed (schema line 10671). No getter
    /// exists; the update stream is the source of truth.
    UpdateReactionNotificationSettings {
        settings: ReactionNotificationSettings,
    },
}
