//! Failed requests for settings: privacy, notifications, sessions, storage, proxy and the account.
use crate::state::*;

impl Session {
    /// Reacts to a failed settings request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_settings_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        _seq: u64,
    ) {
        if self.apply_quick_replies_error(err, pending.map(|p| p.purpose))
            || self.apply_business_error(err, pending.map(|p| p.purpose))
            || self.apply_business_extra_error(err, pending.map(|p| p.purpose), extra)
            || self.apply_passkeys_error(err, pending.map(|p| (p.id, p.purpose)))
            || self.apply_tone_upload_error(err, pending.map(|p| p.purpose))
        {
            return;
        }
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::GetLanguagePackInfo) => self.apply_language_pack_error(pending),
            Some(RequestPurpose::Settings(SettingsPurpose::Language(purpose))) => {
                self.apply_language_error(err, purpose)
            }
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
                session_id,
                ..
            })) => {
                self.finish_login_review(
                    session_id,
                    Some(sessions_error_line("review the new login", err)),
                );
            }
            Some(RequestPurpose::AcceptTermsOfService) => {
                self.settings
                    .notices
                    .terms_accept
                    .fail(sessions_error_line("accept the terms of service", err));
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
                // Only the key's live load fails it; a save or an update
                // since then already settled the key.
                let live = matches!(
                    self.settings.privacy.get(&key),
                    Some(PrivacyKeyState::Loading(id)) if Some(*id) == pending.map(|p| p.id)
                );
                if live {
                    self.settings.privacy.insert(key, PrivacyKeyState::Failed);
                }
            }
            // A refused save puts back what the server kept (and the call
            // rules mirrored from it) and says why in the editor.
            Some(RequestPurpose::Settings(SettingsPurpose::SetPrivacyRules { key })) => {
                let error = sessions_error_line("save the privacy setting", err);
                if let Some(kept) =
                    self.settings
                        .fail_privacy_save(key, pending.map(|p| p.id), error)
                {
                    self.mirror_call_privacy(key, &kept);
                }
            }
            Some(RequestPurpose::GetReadDatePrivacy) => {
                if pending
                    .is_some_and(|p| self.settings.read_date_stage == ReadDateStage::Getting(p.id))
                {
                    self.settings.read_date_stage = ReadDateStage::Failed;
                }
            }
            Some(RequestPurpose::SetReadDatePrivacy) => {
                if pending
                    .is_some_and(|p| self.settings.read_date_stage == ReadDateStage::Setting(p.id))
                {
                    self.settings.read_date_stage = ReadDateStage::Failed;
                    // The optimistic value is dropped; the next
                    // fetch restores the truth.
                    self.settings.read_date_show = None;
                }
            }
            Some(RequestPurpose::Settings(SettingsPurpose::GetBlockedSenders {
                generation,
                ..
            })) => {
                // A page of an older list failing says nothing about this one.
                if generation == self.settings.blocked_generation {
                    self.settings
                        .blocked_senders
                        .fail(sessions_error_line("load the blocked users", err));
                }
            }
            Some(RequestPurpose::Settings(SettingsPurpose::SetSenderBlockList { .. })) => {
                // Drop the optimistic list; the next fetch
                // restores the truth.
                self.settings.restart_blocked_list();
                self.settings
                    .blocked_senders
                    .fail_dropping_data(sessions_error_line("change the block list", err));
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
                self.settings
                    .sessions
                    .fail(sessions_error_line("link the device", err));
            }
            Some(
                purpose @ (RequestPurpose::GetProxies
                | RequestPurpose::MutateProxy
                | RequestPurpose::Settings(SettingsPurpose::PingProxy { .. })
                | RequestPurpose::Settings(SettingsPurpose::SetPreferIpv6 { .. })),
            ) => self.apply_proxy_error(purpose, pending, err),
            Some(
                purpose @ (RequestPurpose::GetNewChatPrivacy
                | RequestPurpose::Settings(SettingsPurpose::SetNewChatPrivacy { .. })
                | RequestPurpose::CanSendMessageToUser
                | RequestPurpose::GetPaidMessageRevenue
                | RequestPurpose::AllowUnpaidMessages
                | RequestPurpose::SetDirectMessagesGroup
                | RequestPurpose::LoadDirectMessagesChat
                | RequestPurpose::SetGiftSettings
                | RequestPurpose::Settings(SettingsPurpose::SetInactiveSessionTtl {
                    ..
                })
                | RequestPurpose::SetSensitiveContent
                | RequestPurpose::SetContactJoinedNotifications
                | RequestPurpose::GetNetworkStatistics
                | RequestPurpose::ResetNetworkStatistics
                | RequestPurpose::CheckRememberedPassword
                | RequestPurpose::Settings(SettingsPurpose::HideSuggestedAction {
                    ..
                })
                | RequestPurpose::HideContactCloseBirthdays),
            ) => self.apply_privacy_data_error(purpose, pending, err),
            Some(RequestPurpose::GetActiveSessions) => {
                self.settings.sessions_stale = false;
                self.settings
                    .sessions
                    .fail(sessions_error_line("load the sessions list", err));
            }
            Some(
                RequestPurpose::Settings(SettingsPurpose::TerminateSession { .. })
                | RequestPurpose::TerminateAllOtherSessions,
            ) => {
                self.settings.sessions_mutating = false;
                self.settings
                    .sessions
                    .fail(sessions_error_line("terminate the session", err));
            }
            // Slice A4: a refused session toggle surfaces an honest
            // classified error and leaves the list untouched (the
            // toggled value is never applied optimistically).
            Some(
                RequestPurpose::Settings(SettingsPurpose::ToggleSessionSecretChats { .. })
                | RequestPurpose::Settings(SettingsPurpose::ToggleSessionCalls { .. }),
            ) => {
                self.settings.sessions_mutating = false;
                self.settings
                    .sessions
                    .fail(sessions_error_line("change the session setting", err));
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
        self.settle_notification_save(pending, true);
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
        // A failed `getChatNotificationSettingsExceptions` ends its
        // loading stage: the list says so, with a Retry, and nothing asks
        // again by itself.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatNotificationSettingsExceptions)
            && let Some(scope) = pending.and_then(|p| p.scope)
        {
            self.settings
                .notification_exceptions
                .entry(scope)
                .or_default()
                .fail(error_reason(err));
        }
    }
}
