//! Failed requests for settings: privacy, notifications, sessions, storage, proxy and the account.
use crate::state::*;

impl Session {
    /// Reacts to a failed settings request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_settings_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::GetLanguagePackInfo) => self.apply_language_pack_error(pending),
            // Slice S4: Data & Storage request failures surface
            // on the screen (the S3 pattern) — never as toasts.
            Some(RequestPurpose::Settings(SettingsPurpose::SetAutoDownloadSettings { .. })) => {
                self.settings.data_storage_error = Some(format!(
                    "Couldn't save auto-download settings: {}",
                    error_reason(err)
                ));
            }
            Some(RequestPurpose::OptimizeStorage) => {
                self.settings.storage_clearing = false;
                self.settings.data_storage_error =
                    Some(format!("Couldn't clear the cache: {}", error_reason(err)));
            }
            Some(RequestPurpose::SetStorageOption) => {
                self.settings.data_storage_error = Some(format!(
                    "Couldn't save the storage limits: {}",
                    error_reason(err)
                ));
            }
            // Batch 4: `setOption("online")` is fire-and-forget; the next
            // presence check sends it again.
            Some(RequestPurpose::SetOnline) => {}
            Some(RequestPurpose::Settings(SettingsPurpose::ReviewUnconfirmedSession {
                confirmed,
            })) => {
                self.finish_login_review(
                    confirmed,
                    Some(sessions_error_line("review the new login", err)),
                );
            }
            Some(RequestPurpose::AcceptTermsOfService) => {
                self.settings.notices.terms_in_flight = false;
                self.settings.notices.terms_error =
                    Some(sessions_error_line("accept the terms of service", err));
            }
            Some(RequestPurpose::GetAutoDownloadSettingsPresets) => {
                self.settings.auto_download_presets_loading = false;
                self.settings.data_storage_error = Some(format!(
                    "Couldn't load auto-download settings: {}",
                    error_reason(err)
                ));
            }
            // Slice S3: privacy-screen request failures surface
            // on the Privacy screen (the UI reads the state),
            // never as toasts.
            Some(RequestPurpose::Settings(SettingsPurpose::GetPrivacyRules { key })) => {
                self.settings.privacy.insert(key, PrivacyKeyState::Failed);
            }
            Some(RequestPurpose::Settings(SettingsPurpose::SetPrivacyRules { key })) => {
                self.settings.privacy.insert(key, PrivacyKeyState::Failed);
            }
            Some(RequestPurpose::GetReadDatePrivacy) => {
                self.settings.read_date_loading = false;
                self.settings.read_date_error = true;
            }
            Some(RequestPurpose::SetReadDatePrivacy) => {
                self.settings.read_date_loading = false;
                self.settings.read_date_error = true;
                // The optimistic value is dropped; the next
                // fetch restores the truth.
                self.settings.read_date_show = None;
            }
            Some(RequestPurpose::Settings(SettingsPurpose::GetBlockedSenders { .. })) => {
                self.settings.blocked_loading = false;
                self.settings.blocked_error = true;
            }
            Some(RequestPurpose::Settings(SettingsPurpose::SetSenderBlockList { .. })) => {
                self.settings.blocked_error = true;
                // Drop the optimistic list; the next fetch
                // restores the truth.
                self.settings.blocked_senders = None;
            }
            // Phase S2: a failed `getStorageStatistics` clears the
            // in-flight flag so the overlay shows "No storage data
            // yet." instead of spinning forever.
            Some(RequestPurpose::GetStorageStatistics) => {
                self.settings.storage_stats_loading = false;
            }
            // Slice A3: a failed sessions fetch or terminate
            // clears the in-flight flags and parks the honest,
            // classified error line on the overlay — never a fake
            // success, never an optimistic list change.
            // A failed stale-refetch also clears `sessions_stale`
            // so the next ingest does not retry the fetch and
            // flood state worsens; retry is user-driven via the
            // Refresh button. The old cache stays visible.
            Some(RequestPurpose::ConfirmDeviceLogin) => {
                self.settings.device_login_result = Some(crate::auth::DeviceLoginResult::Failed);
                self.settings.sessions_mutating = false;
                self.settings.sessions_error = Some(sessions_error_line("link the device", err));
            }
            Some(
                purpose @ (RequestPurpose::GetProxies
                | RequestPurpose::MutateProxy
                | RequestPurpose::Settings(SettingsPurpose::PingProxy { .. })
                | RequestPurpose::Settings(SettingsPurpose::SetPreferIpv6 { .. })),
            ) => self.apply_proxy_error(purpose, err),
            Some(
                purpose @ (RequestPurpose::GetNewChatPrivacy
                | RequestPurpose::Settings(SettingsPurpose::SetNewChatPrivacy { .. })
                | RequestPurpose::SetGiftSettings
                | RequestPurpose::SetInactiveSessionTtl
                | RequestPurpose::SetSensitiveContent
                | RequestPurpose::SetContactJoinedNotifications
                | RequestPurpose::GetNetworkStatistics
                | RequestPurpose::ResetNetworkStatistics
                | RequestPurpose::CheckRememberedPassword
                | RequestPurpose::Settings(SettingsPurpose::HideSuggestedAction {
                    ..
                })
                | RequestPurpose::HideContactCloseBirthdays),
            ) => self.apply_privacy_data_error(purpose, err),
            Some(RequestPurpose::GetActiveSessions) => {
                self.settings.sessions_loading = false;
                self.settings.sessions_stale = false;
                self.settings.sessions_error =
                    Some(sessions_error_line("load the sessions list", err));
            }
            Some(
                RequestPurpose::Settings(SettingsPurpose::TerminateSession { .. })
                | RequestPurpose::TerminateAllOtherSessions,
            ) => {
                self.settings.sessions_mutating = false;
                self.settings.sessions_error =
                    Some(sessions_error_line("terminate the session", err));
            }
            // Slice A4: a refused session toggle surfaces an honest
            // classified error and leaves the list untouched (the
            // toggled value is never applied optimistically).
            Some(
                RequestPurpose::Settings(SettingsPurpose::ToggleSessionSecretChats { .. })
                | RequestPurpose::Settings(SettingsPurpose::ToggleSessionCalls { .. }),
            ) => {
                self.settings.sessions_mutating = false;
                self.settings.sessions_error =
                    Some(sessions_error_line("change the session setting", err));
            }
            // Slice A7: a refused account-lifecycle op (TTL fetch
            // or set, account deletion) clears the in-flight
            // flags and parks the honest, classified error line —
            // never a fake success, never an optimistic change.
            // `sessions_error_line` is reused: it is a pure
            // (action, error-class) formatter, not session-bound.
            Some(RequestPurpose::GetDefaultAutoDelete) => {
                self.settings.default_auto_delete_busy = false;
                self.settings.default_auto_delete_error = Some(sessions_error_line(
                    "load the default auto-delete timer",
                    err,
                ));
            }
            Some(RequestPurpose::Settings(SettingsPurpose::SetDefaultAutoDelete { .. })) => {
                self.settings.default_auto_delete_busy = false;
                self.settings.default_auto_delete_error = Some(sessions_error_line(
                    "change the default auto-delete timer",
                    err,
                ));
            }
            Some(RequestPurpose::GetAccountTtl) => {
                self.settings.account_ttl_loading = false;
                self.settings.account_error = Some(sessions_error_line(
                    "load the account inactivity timer",
                    err,
                ));
            }
            Some(
                RequestPurpose::Settings(SettingsPurpose::SetAccountTtl { .. })
                | RequestPurpose::DeleteAccount,
            ) => {
                self.settings.account_mutating = false;
                self.settings.account_error = Some(sessions_error_line("update the account", err));
            }
            // Slice A4: a failed websites fetch or disconnect
            // clears the in-flight flags and parks the honest,
            // classified error line on the overlay.
            // A failed stale-refetch also clears `websites_stale`
            // (mirroring A3's sessions arm): otherwise the next
            // ingest retries the fetch and flood state worsens;
            // retry is user-driven via the Refresh button. The old
            // cache stays visible.
            Some(RequestPurpose::GetConnectedWebsites) => {
                self.settings.connected_websites_loading = false;
                self.settings.websites_stale = false;
                self.settings.websites_error =
                    Some(sessions_error_line("load the websites list", err));
            }
            Some(
                RequestPurpose::Settings(SettingsPurpose::DisconnectWebsite { .. })
                | RequestPurpose::DisconnectAllWebsites,
            ) => {
                self.settings.websites_mutating = false;
                self.settings.websites_error =
                    Some(sessions_error_line("disconnect the website", err));
            }
            _ => {}
        }
        // Parity slice: a failed `getScopeNotificationSettings` must
        // not leave the scope in `scope_settings_loading` — otherwise
        // `maybe_fetch_scope_notification_settings` skips it on every
        // later ingest and every "Defaults for all chats…" open.
        // Dropping it here means the next fetch retries.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetScopeNotificationSettings)
            && let Some(scope) = pending.and_then(|p| p.scope)
        {
            self.settings.scope_settings_loading.remove(&scope);
        }
        // Parity slice: a failed
        // `getChatNotificationSettingsExceptions` must not leave the
        // scope in `notification_exceptions_loading` — otherwise every
        // later dialog open skips the fetch and the exceptions stay
        // unfetchable. Dropping it here means the next open retries.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatNotificationSettingsExceptions)
            && let Some(scope) = pending.and_then(|p| p.scope)
        {
            self.settings.notification_exceptions_loading.remove(&scope);
        }
    }
}
