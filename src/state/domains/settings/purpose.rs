//! Request purposes for settings: privacy, notifications, sessions, storage, proxy and the account.
use crate::state::request_purpose::flat_purposes;
use crate::state::*;

/// In-flight requests for settings: privacy, notifications, sessions, storage, proxy and the account; wrapped as
/// [`RequestPurpose::Settings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPurpose {
    ExportAccount,
    /// `setChatNotificationSettings`. Response is `ok`; mute via
    /// `updateChatNotificationSettings`.
    SetChatNotificationSettings,
    /// Parity slice: `getSavedNotificationSounds`. Response is
    /// `notificationSounds`; drives the sound picker and custom-sound
    /// playback.
    GetSavedNotificationSounds,
    /// Parity slice: `getScopeNotificationSettings`. Response is
    /// `scopeNotificationSettings`; the scope is correlated via
    /// `PendingRequest::scope`.
    GetScopeNotificationSettings,
    /// Parity slice: `setScopeNotificationSettings`. Response is `ok`;
    /// the new defaults arrive as `updateScopeNotificationSettings`.
    SetScopeNotificationSettings,
    /// Parity slice: `setReactionNotificationSettings`. Response is `ok`;
    /// the new values arrive as `updateReactionNotificationSettings`.
    SetReactionNotificationSettings,
    /// Parity slice: `getChatNotificationSettingsExceptions`. Response is
    /// `chats` (the exception chat ids); the scope is correlated via
    /// `PendingRequest::scope`.
    GetChatNotificationSettingsExceptions,
    /// Parity slice: `resetAllNotificationSettings`. Response is `ok`;
    /// the new values arrive as `updateScopeNotificationSettings` /
    /// `updateChatNotificationSettings` (the ok arm drops the cached
    /// scope settings so they refetch fresh).
    ResetAllNotificationSettings,
    /// Slice A3: `getActiveSessions`. Response is `sessions`; the list is
    /// replaced from the authoritative answer (never optimistic).
    GetActiveSessions,
    ConfirmDeviceLogin,
    /// Slice A3: `terminateSession`. Response is `ok`; the list is
    /// refetched from the authoritative answer (never optimistic).
    TerminateSession {
        session_id: i64,
    },
    /// Slice A3: `terminateAllOtherSessions`. Response is `ok`; the list
    /// is refetched from the authoritative answer (never optimistic).
    TerminateAllOtherSessions,
    /// Slice A4: `toggleSessionCanAcceptSecretChats`. Response is `ok`;
    /// the sessions list is refetched from the authoritative answer
    /// (never optimistic) — same as a terminate.
    ToggleSessionSecretChats {
        session_id: i64,
    },
    /// Slice A4: `toggleSessionCanAcceptCalls`. Response is `ok`; the
    /// sessions list is refetched from the authoritative answer (never
    /// optimistic).
    ToggleSessionCalls {
        session_id: i64,
    },
    /// Slice A4: `getConnectedWebsites`. Response is
    /// `connectedWebsites`; the list is replaced from the authoritative
    /// answer (never optimistic).
    GetConnectedWebsites,
    /// `parity:proxy-settings`: `getProxies`. Response is `addedProxies`.
    GetProxies,
    /// `parity:proxy-settings`: `addProxy` / `editProxy` / `enableProxy`
    /// / `disableProxy` / `removeProxy`. The answer (`addedProxy` /
    /// `ok`) only marks the list stale; the authoritative `getProxies`
    /// refetch replaces it (never optimistic).
    MutateProxy,
    /// `parity:proxy-settings`: `pingProxy`. Response is `seconds`; an
    /// error means the proxy is not available.
    PingProxy {
        proxy_id: i32,
    },
    /// `parity:proxy-settings`: `setOption("prefer_ipv6")`. Response is
    /// `ok`; `on` is the value that was requested.
    SetPreferIpv6 {
        on: bool,
    },
    /// Slice A4: `disconnectWebsite`. Response is `ok`; the list is
    /// refetched from the authoritative answer (never optimistic).
    DisconnectWebsite {
        website_id: i64,
    },
    /// Slice A4: `disconnectAllWebsites`. Response is `ok`; the list is
    /// refetched from the authoritative answer (never optimistic).
    DisconnectAllWebsites,
    /// The message menu's "Save for Notifications"
    /// (`addSavedNotificationSound`). Response is `notificationSound`; the
    /// saved list refetches.
    AddSavedNotificationSound,
    /// `getSupportUser` (Settings > Ask a Question). Response is `user`;
    /// the id is parked in `Session::support_user_ready` and the driver
    /// opens the chat.
    GetSupportUser,
    /// Slice S3: `getUserPrivacySettingRules` for a Privacy-screen key.
    /// Response is `userPrivacySettingRules`.
    GetPrivacyRules {
        key: PrivacySettingKey,
    },
    /// Slice S3: `setUserPrivacySettingRules` for a Privacy-screen key.
    /// Response is `ok`; the new detail is applied optimistically at
    /// send time.
    SetPrivacyRules {
        key: PrivacySettingKey,
    },
    /// B13: `getNewChatPrivacySettings`. Response is
    /// `newChatPrivacySettings`.
    GetNewChatPrivacy,
    /// B13: `setNewChatPrivacySettings`. Response is `ok`; applied
    /// optimistically at send time.
    SetNewChatPrivacy {
        previous_allow: bool,
    },
    /// B13: `setGiftSettings`. Response is `ok`; applied optimistically.
    SetGiftSettings,
    /// B13: `setInactiveSessionTtl`. Response is `ok`; applied
    /// optimistically.
    SetInactiveSessionTtl,
    /// B13: `setOption(ignore_sensitive_content_restrictions)`. The truth
    /// arrives as `updateOption`.
    SetSensitiveContent,
    /// `setOption(disable_contact_registered_notifications)`. The truth
    /// arrives as `updateOption`.
    SetContactJoinedNotifications,
    /// B13: `getNetworkStatistics`. Response is `networkStatistics`.
    GetNetworkStatistics,
    /// B13: `resetNetworkStatistics`. Response is `ok`.
    ResetNetworkStatistics,
    /// B13: `getRecoveryEmailAddress` used to verify a typed password.
    CheckRememberedPassword,
    /// B13: `hideSuggestedAction(suggestedActionCheckPassword)`.
    HideCheckPasswordSuggestion,
    /// `hideSuggestedAction` for another chat-list suggestion. Response
    /// is `ok`; the update that follows removes the action.
    HideSuggestedAction {
        action: &'static str,
    },
    /// `hideContactCloseBirthdays`. Response is `ok`.
    HideContactCloseBirthdays,
    /// Slice S3: `getReadDatePrivacySettings`. Response is
    /// `readDatePrivacySettings`.
    GetReadDatePrivacy,
    /// Slice S3: `setReadDatePrivacySettings`. Response is `ok`;
    /// applied optimistically at send time.
    SetReadDatePrivacy,
    /// Slice S3: `getBlockedMessageSenders` page. Response is
    /// `messageSenders`.
    GetBlockedSenders {
        offset: i32,
    },
    /// Slice S3: `setMessageSenderBlockList`. Response is `ok`; an
    /// unblock is applied optimistically at send time.
    SetSenderBlockList {
        user_id: i64,
        block: bool,
    },
    /// `getDefaultMessageAutoDeleteTime`. Response is `messageAutoDeleteTime`,
    /// stored in `Session::default_auto_delete_secs`.
    GetDefaultAutoDelete,
    /// `setDefaultMessageAutoDeleteTime`. Response is `ok`; the confirmed
    /// `seconds` are stored (the server accepted exactly this value).
    SetDefaultAutoDelete {
        seconds: i32,
    },
    /// Phase S2: `getStorageStatistics`. Response is `storageStatistics`;
    /// aggregated by file type into `Session::storage_stats` (TGX
    /// `SettingsCacheController` / `TGStorageStats` style, including the
    /// "Secret media and files" category for `fileTypeSecret`). Slice S4:
    /// requested with `chat_limit` 50 — per-chat rows feed the usage
    /// screen's per-chat breakdown.
    GetStorageStatistics,
    /// Slice S4: `setAutoDownloadSettings` for one network type.
    /// Response is `ok`; the confirmed sent settings are applied then
    /// (the setAccountTtl precedent — the server confirmed the write of
    /// exactly this value, not an optimistic guess). Carries the sent
    /// settings because the `ok` carries none.
    SetAutoDownloadSettings {
        network: NetworkKind,
        settings: AutoDownloadNetSettings,
    },
    /// Batch 6: `optimizeStorage` ("Clear cache", per-type and per-chat
    /// clears). Response is `storageStatistics` of the deleted files;
    /// the cached usage stats are dropped and refetched so the screen
    /// shows the post-clear numbers.
    OptimizeStorage,
    /// Batch 6: a `setOption` for a storage limit (`storage_max_*`,
    /// `use_storage_optimizer`). Response is `ok`; TDLib echoes the new
    /// value as `updateOption`.
    SetStorageOption,
    /// Batch 4: `setOption("online")`. Response is `ok`; nothing to apply.
    SetOnline,
    /// Batch 4: `confirmSession` / `terminateSession` for the
    /// new-login alert ("Yes, it's me" / "No, it's not me!").
    ReviewUnconfirmedSession {
        confirmed: bool,
    },
    /// Batch 4: `acceptTermsOfService`.
    AcceptTermsOfService,
    /// Slice S4: `getAutoDownloadSettingsPresets`. Response is
    /// `autoDownloadSettingsPresets`; seeds the local per-network
    /// settings once (TDLib has no getter for the current values).
    GetAutoDownloadSettingsPresets,
    /// Slice A7: `getAccountTtl`. Response is `accountTtl`, stored in
    /// `Session::account_ttl_days`.
    GetAccountTtl,
    /// Slice A7: `setAccountTtl`. Response is `ok`; the confirmed `days`
    /// are stored (the server confirmed the write of exactly this value).
    SetAccountTtl {
        days: i32,
    },
    /// Slice A7: `deleteAccount`. Response is `ok`; the authoritative
    /// account teardown arrives via TDLib's own
    /// `updateAuthorizationState` → `Closed` (already handled by
    /// `set_auth`) — never faked client-side.
    DeleteAccount,
    /// `getLanguagePackInfo` for a `setlanguage` link. Response is
    /// `languagePackInfo`.
    GetLanguagePackInfo,
}

flat_purposes!(Settings(SettingsPurpose) {
    ExportAccount,
    SetChatNotificationSettings,
    GetSavedNotificationSounds,
    GetScopeNotificationSettings,
    SetScopeNotificationSettings,
    SetReactionNotificationSettings,
    GetChatNotificationSettingsExceptions,
    ResetAllNotificationSettings,
    GetActiveSessions,
    ConfirmDeviceLogin,
    TerminateAllOtherSessions,
    GetConnectedWebsites,
    GetProxies,
    MutateProxy,
    DisconnectAllWebsites,
    AddSavedNotificationSound,
    GetSupportUser,
    GetNewChatPrivacy,
    SetGiftSettings,
    SetInactiveSessionTtl,
    SetSensitiveContent,
    SetContactJoinedNotifications,
    GetNetworkStatistics,
    ResetNetworkStatistics,
    CheckRememberedPassword,
    HideCheckPasswordSuggestion,
    HideContactCloseBirthdays,
    GetReadDatePrivacy,
    SetReadDatePrivacy,
    GetDefaultAutoDelete,
    GetStorageStatistics,
    OptimizeStorage,
    SetStorageOption,
    SetOnline,
    AcceptTermsOfService,
    GetAutoDownloadSettingsPresets,
    GetAccountTtl,
    DeleteAccount,
    GetLanguagePackInfo,
});
