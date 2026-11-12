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
        extra: Option<RequestId>,
        _seq: u64,
    ) {
        match payload {
            SettingsPayload::LanguagePackInfo(info) => self.apply_language_pack_info(info, pending),
            SettingsPayload::Passkeys(payload) => self.apply_passkeys(payload, pending),
            SettingsPayload::Language(payload) => self.apply_language_payload(payload, pending),
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
                self.settings.notified.mark_active(chat_ids);
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
                        RequestPurpose::Calls(CallsPurpose::GetCallPrivacyRules { setting })
                            if pending.is_some_and(|p| {
                                self.calls.privacy_asks.answered(setting, p.id)
                            }) =>
                        {
                            let names: Vec<String> = rules.iter().map(|r| r.name.clone()).collect();
                            let who = PrivacyWho::from_rule_names(&names);
                            match setting {
                                CallPrivacySetting::AllowCalls => {
                                    self.calls.privacy_allow_calls = who
                                }
                                CallPrivacySetting::PeerToPeer => self.calls.privacy_p2p = who,
                            }
                        }
                        RequestPurpose::Settings(SettingsPurpose::GetPrivacyRules { key }) => {
                            let detail = PrivacyRuleDetail::from_rules(&rules);
                            if self.settings.finish_privacy_load(
                                key,
                                pending.map(|p| p.id),
                                detail.clone(),
                            ) {
                                self.mirror_call_privacy(key, &detail);
                            }
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
                    // A save of ours in flight is settled by its own answer;
                    // this update is mostly its echo.
                    let saving = matches!(
                        self.settings.privacy.get(&key),
                        Some(PrivacyKeyState::Saving { .. })
                    );
                    if !saving {
                        self.mirror_call_privacy(key, &detail);
                        self.settings
                            .privacy
                            .insert(key, PrivacyKeyState::Ready(detail));
                    }
                }
            }
            // Slice S3: `readDatePrivacySettings` answer.
            SettingsPayload::ReadDatePrivacySettings { show_read_date } => {
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetReadDatePrivacy
                {
                    self.settings.read_date.got(pending.id, show_read_date);
                }
            }
            // Slice S3: `messageSenders` answer — one blocked-senders
            // page. Later pages append; a refetch restarts at 0.
            SettingsPayload::BlockedMessageSenders {
                total_count,
                sender_ids,
                senders,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPaidReactionSenders) {
                    if let Some(chat) = pending.and_then(|p| p.chat_id) {
                        self.payments.paid.set_senders(chat.0, &senders);
                    }
                } else if let Some(RequestPurpose::Calls(
                    CallsPurpose::GetVideoChatAvailableParticipants { group_call_id },
                )) = pending.map(|p| p.purpose)
                {
                    self.set_group_call_join_as_options(
                        group_call_id,
                        pending.map(|p| p.id),
                        senders,
                    );
                } else if let Some(RequestPurpose::Calls(CallsPurpose::CreateVideoChatJoinAs {
                    ..
                })) = pending.map(|p| p.purpose)
                {
                    if let Some(join_as) = self.calls.create_join_as.as_mut() {
                        join_as.accept(extra, senders);
                    }
                } else if let Some(RequestPurpose::Settings(SettingsPurpose::GetBlockedSenders {
                    offset,
                    generation,
                })) = pending.map(|p| p.purpose)
                {
                    self.settings
                        .apply_blocked_page(offset, generation, total_count, sender_ids);
                }
            }
            SettingsPayload::NotificationSounds { sounds } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::AddSavedNotificationSound) {
                    self.mark_saved_sounds_stale();
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::UploadSavedNotificationSound)
                {
                    self.finish_tone_upload(pending.map(|p| p.id), sounds);
                // Parity slice: `getSavedNotificationSounds` answer.
                } else if pending.map(|p| p.purpose)
                    == Some(RequestPurpose::GetSavedNotificationSounds)
                    && pending.is_some_and(|p| self.settings.saved_sounds.waits_for(p.id))
                {
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
                    self.settings
                        .saved_sounds
                        .answered(pending.map(|p| p.id), sounds);
                }
            }
            SettingsPayload::UpdateSavedNotificationSounds { .. } => {
                // The list changed server-side; refetch on the next ingest.
                self.mark_saved_sounds_stale();
            }
            SettingsPayload::StorageStatistics {
                total_size,
                by_file_type,
                by_chat,
            } => {
                // Phase S2: `getStorageStatistics` answer — only our own
                // in-flight request writes the cache (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStorageStatistics) {
                    self.settings.storage_stats.answered(
                        pending.map(|p| p.id),
                        StorageStats {
                            total_size,
                            by_file_type,
                            by_chat,
                        },
                    );
                }
                // Batch 6: `optimizeStorage` answers with the statistics
                // of the files it deleted. Drop the usage cache so the
                // driver refetches the post-clear numbers on this same
                // ingest.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::OptimizeStorage) {
                    self.settings.storage_clear = StorageClear::Freed(total_size);
                    self.settings.storage_stats = Default::default();
                }
            }
            SettingsPayload::AutoDownloadSettingsPresets { low, medium, high } => {
                // Slice S4: `getAutoDownloadSettingsPresets` answer —
                // only our own in-flight request seeds the local
                // per-network settings (matched by `@extra`), and only
                // when nothing was loaded from disk. The driver persists
                // the seed on the next ingest (`data_storage_dirty`).
                if self
                    .settings
                    .data_storage
                    .seeded
                    .waits_for(pending.map(|p| p.id))
                {
                    self.settings
                        .data_storage
                        .seed_from_presets(low, medium, high);
                    self.settings.data_storage_dirty = true;
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
                    self.auth_state.settle_password_op(pending.map(|p| p.id));
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
                    let has_password = state.has_password;
                    self.auth_state.password_state = Some(state);
                    self.auth_state.settle_password_op(pending.map(|p| p.id));
                    self.route_gift_export_on_password(has_password);
                    self.route_gram_withdrawal_on_password(has_password);
                    if let Some(RequestPurpose::Auth(AuthPurpose::PasswordStateOp { op })) =
                        pending.map(|p| p.purpose)
                    {
                        self.apply_password_state_op(op);
                    }
                }
            }
            SettingsPayload::DeviceLoginResult { result } => {
                if let Some(asked) =
                    pending.filter(|p| p.purpose == RequestPurpose::ConfirmDeviceLogin)
                {
                    self.settings.device_login = DeviceLogin::Done {
                        request: asked.id,
                        result,
                    };
                    if result != crate::auth::DeviceLoginResult::Failed {
                        self.settings.sessions_op.changed(pending.map(|p| p.id));
                        self.settings.sessions.clear_error();
                    } else {
                        self.settings.sessions_op.failed(pending.map(|p| p.id));
                        self.settings
                            .sessions
                            .fail("Telegram returned an invalid device session.");
                    }
                }
            }
            SettingsPayload::UpdateContactCloseBirthdays { users } => {
                // A new list (also an empty one) re-enables the suggestion.
                self.chat_list.suggestions.close_birthdays = users;
                self.chat_list.suggestions.show_birthdays();
            }
            SettingsPayload::AddedProxies { proxies } => {
                self.apply_added_proxies(pending, proxies);
            }
            SettingsPayload::AddedProxy { .. } => self.apply_added_proxy(pending),
            SettingsPayload::WebProxyCarrierOpened { carrier } => {
                self.apply_web_proxy_carrier(carrier)
            }
            SettingsPayload::WebProxyCarrierClosed { carrier_id } => {
                self.apply_web_proxy_carrier_closed(carrier_id)
            }
            // The driver relays it before the reducer runs; nothing to keep.
            SettingsPayload::WebProxyCarrierData { .. } => {}
            payload @ (SettingsPayload::NewChatPrivacySettings(_)
            | SettingsPayload::CanSendMessageToUser(_)
            | SettingsPayload::StarCount(_)
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
                if let Some(asked) = pending
                    && asked.purpose == RequestPurpose::GetActiveSessions
                    && self.settings.sessions.waits_for(asked.id)
                {
                    self.resolve_unconfirmed_entries(&sessions);
                    self.settings.sessions.answered(Some(asked.id), sessions);
                    if inactive_session_ttl_days.is_some() {
                        self.settings.privacy_data.inactive_session_ttl_days =
                            inactive_session_ttl_days;
                    }
                }
            }
            SettingsPayload::AccountTtl { days } => {
                // Slice A7: `getAccountTtl` answer — only our own
                // in-flight request writes the cache (matched by
                // `@extra`). Authoritative: replaces the cached days and
                // clears any stale error.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetAccountTtl) {
                    self.settings
                        .account_ttl
                        .answered(pending.map(|p| p.id), days);
                }
            }
            SettingsPayload::ConnectedWebsites { websites } => {
                // Slice A4: `getConnectedWebsites` answer — only our own
                // in-flight request writes the cache (matched by `@extra`).
                // The answer is authoritative: it replaces the list and
                // clears any stale error. No optimistic mutation ever
                // happens client-side.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetConnectedWebsites) {
                    self.settings
                        .connected_websites
                        .answered(pending.map(|p| p.id), websites);
                }
            }
            SettingsPayload::ScopeNotificationSettings { settings } => {
                // Parity slice: `getScopeNotificationSettings` answer; the
                // scope is correlated via the pending request (the response
                // carries no scope field).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetScopeNotificationSettings)
                    && let Some(scope) = pending.and_then(|p| p.scope)
                {
                    self.settings.scope_notification_settings.answered(
                        scope,
                        pending.map(|p| p.id),
                        settings,
                    );
                }
            }
            SettingsPayload::UpdateScopeNotificationSettings { scope, settings } => {
                // Parity slice: scope defaults changed (or our own
                // `setScopeNotificationSettings` was confirmed). While a save
                // of ours is out the update is its echo, and what shows is
                // already the newest edit.
                if !self.settings.scope_saves.is_saving(&scope) {
                    self.settings
                        .scope_notification_settings
                        .insert(scope, settings);
                }
            }
            SettingsPayload::UpdateReactionNotificationSettings { settings } => {
                // Parity slice: no getter exists — the update stream is the
                // source of truth (it also confirms our own
                // `setReactionNotificationSettings`).
                if !self.settings.reaction_saves.is_saving(&()) {
                    self.settings.reaction_notification_settings = Some(settings);
                }
            }
            SettingsPayload::QuickReplies(update) => self.apply_quick_replies(update),
            SettingsPayload::TimeZones(zones) => self.apply_time_zones(zones, extra),
            SettingsPayload::BusinessChatLinks(links) => self.apply_chat_links(links, extra),
            SettingsPayload::BusinessChatLink(link) => self.apply_chat_link(link, extra),
            SettingsPayload::BusinessConnectedBot(bot) => self.apply_connected_bot(bot, pending),
        }
    }
}
