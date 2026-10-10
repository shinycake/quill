//! Applies TDLib updates and answers for settings: privacy, notifications, sessions, storage, proxy and the account.
use crate::state::*;
use crate::telegram::envelope::SettingsPayload;

impl Session {
    /// Applies one settings payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_settings_payload(
        &mut self,
        payload: SettingsPayload,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match payload {
            // `updateNotificationGroup` / `updateActiveNotifications`: a
            // group that emptied (read elsewhere, or removed) clears the
            // OS notifications we showed for the chat.
            SettingsPayload::UpdateNotificationGroup {
                chat_id,
                total_count,
                added_count,
                removed_count,
            } => {
                if total_count == 0 && added_count == 0 && removed_count > 0 {
                    self.clear_chat_notifications(chat_id);
                }
            }
            SettingsPayload::UpdateActiveNotifications { chat_ids } => {
                // Notifications of a previous launch: remember the chats so
                // a later read clears them too.
                self.settings.shown_notification_chats.extend(chat_ids);
            }
            // Phase C2i: `getUserPrivacySettingRules` answer — map the
            // rule list to the simple Everybody / Contacts / Nobody
            // choice (`None` when the account has custom rules the UI
            // cannot represent; the radios then show nothing selected).
            // Slice S3: the Privacy-screen keys keep the full parsed
            // detail (exception lists) instead.
            SettingsPayload::UserPrivacySettingRules { rules } => {
                if let Some(purpose) = pending.map(|p| p.purpose) {
                    match purpose {
                        RequestPurpose::Calls(CallsPurpose::GetCallPrivacyRules { setting }) => {
                            let names: Vec<String> = rules.iter().map(|r| r.name.clone()).collect();
                            let who = PrivacyWho::from_rule_names(&names);
                            match setting {
                                CallPrivacySetting::AllowCalls => {
                                    self.calls.privacy_allow_calls = who
                                }
                                CallPrivacySetting::PeerToPeer => self.calls.privacy_p2p = who,
                            }
                            self.privacy_roundtrip_done();
                        }
                        RequestPurpose::Settings(SettingsPurpose::GetPrivacyRules { key }) => {
                            let detail = PrivacyRuleDetail::from_rules(&rules);
                            self.mirror_call_privacy(key, &detail);
                            self.settings
                                .privacy
                                .insert(key, PrivacyKeyState::Ready(detail));
                        }
                        _ => {}
                    }
                }
            }
            // Slice S3: `updateUserPrivacySettingRules` (:10871) — rules
            // changed on another device; only refresh keys the Privacy
            // screen (or the C2i calls UI) owns.
            SettingsPayload::UpdateUserPrivacySettingRules { setting, rules } => {
                let detail = PrivacyRuleDetail::from_rules(&rules);
                if let Some(key) = PrivacySettingKey::all()
                    .into_iter()
                    .find(|k| k.td_type() == setting)
                {
                    self.mirror_call_privacy(key, &detail);
                    self.settings
                        .privacy
                        .insert(key, PrivacyKeyState::Ready(detail));
                }
            }
            // Slice S3: `readDatePrivacySettings` answer.
            SettingsPayload::ReadDatePrivacySettings { show_read_date } => {
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::GetReadDatePrivacy)
                ) {
                    self.settings.read_date_show = Some(show_read_date);
                    self.settings.read_date_loading = false;
                    self.settings.read_date_error = false;
                }
            }
            // Slice S3: `messageSenders` answer — one blocked-senders
            // page. Later pages append; a refetch restarts at 0.
            SettingsPayload::BlockedMessageSenders {
                total_count,
                sender_ids,
                senders,
            } => {
                if let Some(RequestPurpose::Calls(
                    CallsPurpose::GetVideoChatAvailableParticipants { group_call_id },
                )) = pending.map(|p| p.purpose)
                {
                    self.set_group_call_join_as_options(group_call_id, senders);
                } else if let Some(RequestPurpose::Settings(SettingsPurpose::GetBlockedSenders {
                    offset,
                })) = pending.map(|p| p.purpose)
                {
                    self.settings.blocked_total = total_count;
                    if offset == 0 {
                        self.settings.blocked_senders = Some(sender_ids);
                    } else if let Some(list) = self.settings.blocked_senders.as_mut() {
                        list.extend(sender_ids);
                    }
                    self.settings.blocked_loading = false;
                    self.settings.blocked_error = false;
                }
            }
            SettingsPayload::NotificationSounds { sounds } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::AddSavedNotificationSound) {
                    self.settings.saved_sounds_stale = true;
                }
                // Parity slice: `getSavedNotificationSounds` answer.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSavedNotificationSounds) {
                    let files: Vec<ParsedFile> = sounds.iter().map(|s| s.sound.clone()).collect();
                    self.remember_files(&files);
                    // Parity slice: a refetch replaces the saved list, so
                    // evict `sound_file_ids` entries for sounds that are no
                    // longer saved — stale file→sound mappings would
                    // otherwise accumulate forever.
                    let live_ids: HashSet<i64> = sounds.iter().map(|s| s.id).collect();
                    self.settings
                        .sound_file_ids
                        .retain(|_, sound_id| live_ids.contains(sound_id));
                    self.settings.saved_notification_sounds = sounds;
                    self.settings.saved_sounds_loaded = true;
                    self.settings.saved_sounds_stale = false;
                }
            }
            SettingsPayload::UpdateSavedNotificationSounds { .. } => {
                // The list changed server-side; refetch on the next ingest.
                self.settings.saved_sounds_stale = true;
            }
            SettingsPayload::StorageStatistics {
                total_size,
                by_file_type,
                by_chat,
            } => {
                // Phase S2: `getStorageStatistics` answer — only our own
                // in-flight request writes the cache (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStorageStatistics) {
                    self.settings.storage_stats = Some(StorageStats {
                        total_size,
                        by_file_type,
                        by_chat,
                    });
                    self.settings.storage_stats_loading = false;
                }
                // Batch 6: `optimizeStorage` answers with the statistics
                // of the files it deleted. Drop the usage cache so the
                // driver refetches the post-clear numbers on this same
                // ingest.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::OptimizeStorage) {
                    self.settings.storage_freed = Some(total_size);
                    self.settings.storage_clearing = false;
                    self.settings.storage_stats = None;
                    self.settings.storage_stats_loading = false;
                    self.settings.data_storage_error = None;
                }
            }
            SettingsPayload::AutoDownloadSettingsPresets { low, medium, high } => {
                // Slice S4: `getAutoDownloadSettingsPresets` answer —
                // only our own in-flight request seeds the local
                // per-network settings (matched by `@extra`), and only
                // when nothing was loaded from disk. The driver persists
                // the seed on the next ingest (`data_storage_dirty`).
                if pending.map(|p| p.purpose)
                    == Some(RequestPurpose::GetAutoDownloadSettingsPresets)
                    && !self.settings.data_storage.seeded
                {
                    self.settings
                        .data_storage
                        .seed_from_presets(low, medium, high);
                    self.settings.data_storage_dirty = true;
                    self.settings.auto_download_presets_loading = false;
                    self.settings.data_storage_error = None;
                }
            }
            SettingsPayload::UpdateUnconfirmedSession { session, count } => {
                self.apply_unconfirmed_session(session, count);
            }
            SettingsPayload::ResetPasswordResult(outcome) => {
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::Auth(AuthPurpose::PasswordStateOp {
                        op: PasswordOp::ResetPassword
                    }))
                ) {
                    self.auth_state.password_state_loading = false;
                    self.auth_state.password_op_error = None;
                    self.apply_reset_password_result(outcome);
                }
            }
            SettingsPayload::PasswordState { state } => {
                // Slice A2: `passwordState` answer — only our own
                // in-flight `PasswordStateOp` writes the cache (matched by
                // `@extra`). The response is authoritative: it replaces
                // the cached state and clears any stale error. No
                // optimistic mutation ever happens client-side.
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::Auth(AuthPurpose::PasswordStateOp { .. }))
                ) {
                    self.auth_state.password_state = Some(state);
                    self.auth_state.password_state_loading = false;
                    self.auth_state.password_op_error = None;
                    if let Some(RequestPurpose::Auth(AuthPurpose::PasswordStateOp { op })) =
                        pending.map(|p| p.purpose)
                    {
                        self.apply_password_state_op(op);
                    }
                }
            }
            SettingsPayload::DeviceLoginResult { result } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ConfirmDeviceLogin) {
                    self.settings.device_login_result = Some(result);
                    self.settings.sessions_mutating = false;
                    if result != crate::auth::DeviceLoginResult::Failed {
                        self.settings.sessions_stale = true;
                        self.settings.sessions_error = None;
                    } else {
                        self.settings.sessions_error =
                            Some("Telegram returned an invalid device session.".into());
                    }
                }
            }
            SettingsPayload::UpdateContactCloseBirthdays { users } => {
                // A new list (also an empty one) re-enables the suggestion.
                self.suggestions.close_birthdays = users;
                self.suggestions.birthdays_hidden = false;
            }
            SettingsPayload::AddedProxies { proxies } => {
                self.apply_added_proxies(pending, proxies);
            }
            SettingsPayload::AddedProxy { .. } => self.apply_added_proxy(pending),
            payload @ (SettingsPayload::NewChatPrivacySettings(_)
            | SettingsPayload::NetworkStatistics(_)
            | SettingsPayload::RecoveryEmailAddress
            | SettingsPayload::UpdateSuggestedActions { .. }) => {
                self.apply_privacy_data_payload(&payload, pending);
            }
            SettingsPayload::Sessions {
                sessions,
                inactive_session_ttl_days,
            } => {
                // Slice A3: `getActiveSessions` answer — only our own
                // in-flight request writes the cache (matched by `@extra`).
                // The answer is authoritative: it replaces the list and
                // clears any stale error. No optimistic mutation ever
                // happens client-side.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetActiveSessions) {
                    self.resolve_unconfirmed_entries(&sessions);
                    self.settings.sessions = Some(sessions);
                    if inactive_session_ttl_days.is_some() {
                        self.settings.privacy_data.inactive_session_ttl_days =
                            inactive_session_ttl_days;
                    }
                    self.settings.sessions_loading = false;
                    self.settings.sessions_error = None;
                    self.settings.sessions_stale = false;
                }
            }
            SettingsPayload::AccountTtl { days } => {
                // Slice A7: `getAccountTtl` answer — only our own
                // in-flight request writes the cache (matched by
                // `@extra`). Authoritative: replaces the cached days and
                // clears any stale error.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetAccountTtl) {
                    self.settings.account_ttl_days = Some(days);
                    self.settings.account_ttl_loading = false;
                    self.settings.account_error = None;
                }
            }
            SettingsPayload::ConnectedWebsites { websites } => {
                // Slice A4: `getConnectedWebsites` answer — only our own
                // in-flight request writes the cache (matched by `@extra`).
                // The answer is authoritative: it replaces the list and
                // clears any stale error. No optimistic mutation ever
                // happens client-side.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetConnectedWebsites) {
                    self.settings.connected_websites = Some(websites);
                    self.settings.connected_websites_loading = false;
                    self.settings.websites_error = None;
                    self.settings.websites_stale = false;
                }
            }
            SettingsPayload::ScopeNotificationSettings { settings, .. } => {
                // Parity slice: `getScopeNotificationSettings` answer; the
                // scope is correlated via the pending request (the response
                // carries no scope field).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetScopeNotificationSettings)
                    && let Some(scope) = pending.and_then(|p| p.scope)
                {
                    self.settings
                        .scope_notification_settings
                        .insert(scope, settings);
                    self.settings.scope_settings_loading.remove(&scope);
                }
            }
            SettingsPayload::UpdateScopeNotificationSettings { scope, settings } => {
                // Parity slice: scope defaults changed (or our own
                // `setScopeNotificationSettings` was confirmed).
                self.settings
                    .scope_notification_settings
                    .insert(scope, settings);
                self.settings.scope_settings_loading.remove(&scope);
            }
            SettingsPayload::UpdateReactionNotificationSettings { settings } => {
                // Parity slice: no getter exists — the update stream is the
                // source of truth (it also confirms our own
                // `setReactionNotificationSettings`).
                self.settings.reaction_notification_settings = Some(settings);
            }
        }
    }
}
