//! Connect driver: privacy, notifications, sessions, storage, account.
use super::*;
use crate::data_settings::{AutoDownloadNetSettings, NetworkKind, save_data_storage_prefs};
use crate::ids::{ChatId, RequestId};
use crate::notify::NotificationSoundKind;
use crate::privacy::{PrivacyKeyState, PrivacyRuleDetail};
use crate::settings::{
    load_preferences, save_badge_prefs, save_call_prefs, save_language_prefs, save_preferences,
};
use crate::state::RequestPurpose;
use crate::telegram::envelope::{
    ChatNotificationSettings, MUTE_FOREVER, NotificationSettingsScope,
    ReactionNotificationSettings, ScopeNotificationSettings,
};
use crate::telegram::requests::{
    delete_account, disconnect_all_websites, disconnect_website, get_account_ttl,
    get_active_sessions, get_chat_notification_settings_exceptions, get_connected_websites,
    get_saved_notification_sounds, get_scope_notification_settings, get_storage_statistics,
    reset_all_notification_settings, set_account_ttl, set_chat_notification_settings,
    set_message_sender_block_list, set_reaction_notification_settings,
    set_scope_notification_settings, terminate_all_other_sessions, terminate_session,
    toggle_session_can_accept_calls, toggle_session_can_accept_secret_chats,
};
use crate::telegram::requests_data_settings::{
    get_auto_download_settings_presets, remove_all_files_from_downloads, set_auto_download_settings,
};
use crate::telegram::requests_privacy::{
    PrivacySettingKey, get_blocked_message_senders, get_privacy_rules,
    get_read_date_privacy_settings, set_privacy_rules, set_read_date_privacy_settings,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Slice S3: fetch one Privacy-screen rule
    /// (`getUserPrivacySettingRules`, schema 1.8.67, :15620).
    pub fn fetch_privacy_rules(&mut self, key: PrivacySettingKey) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.privacy.insert(key, PrivacyKeyState::Loading);
        let extra = self
            .session
            .request(RequestPurpose::GetPrivacyRules { key }, None);
        if let Err(err) = self
            .sender
            .send_json(&get_privacy_rules(extra, key.td_type()))
        {
            self.session.requests.take(extra);
            self.session.privacy.insert(key, PrivacyKeyState::Failed);
            return Err(err);
        }
        Ok(())
    }

    /// Slice S3: change one Privacy-screen rule
    /// (`setUserPrivacySettingRules`, schema 1.8.67, :15617). Applied
    /// optimistically; the `ok` / error response confirms or fails it.
    pub fn set_privacy_rules(
        &mut self,
        key: PrivacySettingKey,
        detail: PrivacyRuleDetail,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetPrivacyRules { key }, None);
        let rules = detail.recompose();
        if let Err(err) = self
            .sender
            .send_json(&set_privacy_rules(extra, key.td_type(), rules))
        {
            self.session.requests.take(extra);
            self.session.privacy.insert(key, PrivacyKeyState::Failed);
            return Err(err);
        }
        self.session
            .privacy
            .insert(key, PrivacyKeyState::Ready(detail));
        Ok(extra)
    }

    /// Slice S3: fetch the read-date privacy setting
    /// (`getReadDatePrivacySettings`, schema 1.8.67, :15626).
    pub fn fetch_read_date_privacy(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.read_date_loading = true;
        self.session.read_date_error = false;
        let extra = self
            .session
            .request(RequestPurpose::GetReadDatePrivacy, None);
        if let Err(err) = self
            .sender
            .send_json(&get_read_date_privacy_settings(extra))
        {
            self.session.requests.take(extra);
            self.session.read_date_loading = false;
            self.session.read_date_error = true;
            return Err(err);
        }
        Ok(())
    }

    /// Slice S3: change the read-date privacy setting
    /// (`setReadDatePrivacySettings`, schema 1.8.67, :15623). Applied
    /// optimistically.
    pub fn set_read_date_privacy(&mut self, show: bool) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetReadDatePrivacy, None);
        if let Err(err) = self
            .sender
            .send_json(&set_read_date_privacy_settings(extra, show))
        {
            self.session.requests.take(extra);
            self.session.read_date_loading = false;
            self.session.read_date_error = true;
            return Err(err);
        }
        self.session.read_date_show = Some(show);
        self.session.read_date_loading = true;
        Ok(extra)
    }

    /// Slice S3: fetch the blocked-senders list
    /// (`getBlockedMessageSenders`, schema 1.8.67, :14505). Pages of
    /// 100; a later page continues from the loaded list length.
    pub fn fetch_blocked_senders(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let offset = self
            .session
            .blocked_senders
            .as_ref()
            .map(|list| list.len())
            .unwrap_or(0);
        self.session.blocked_loading = true;
        self.session.blocked_error = false;
        let extra = self.session.request(
            RequestPurpose::GetBlockedSenders {
                offset: offset as i32,
            },
            None,
        );
        if let Err(err) =
            self.sender
                .send_json(&get_blocked_message_senders(extra, offset as i32, 100))
        {
            self.session.requests.take(extra);
            self.session.blocked_loading = false;
            self.session.blocked_error = true;
            return Err(err);
        }
        Ok(())
    }

    /// Slice S3: unblock a user (`setMessageSenderBlockList` with a null
    /// block list, schema 1.8.67, :14492 — TGX `Tdlib.unblockSender`).
    /// Applied optimistically: the user leaves the list at send time.
    pub fn unblock_sender(&mut self, user_id: i64) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::SetSenderBlockList {
                user_id,
                block: false,
            },
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&set_message_sender_block_list(extra, user_id, false))
        {
            self.session.requests.take(extra);
            self.session.blocked_error = true;
            return Err(err);
        }
        if let Some(list) = self.session.blocked_senders.as_mut() {
            list.retain(|id| *id != user_id);
            self.session.blocked_total = self.session.blocked_total.saturating_sub(1);
        }
        Ok(extra)
    }

    /// Slice S3: block a user (`setMessageSenderBlockList` with
    /// `blockListMain`, schema 1.8.67, :14492). Applied optimistically:
    /// the user joins the list at send time.
    pub fn block_sender(&mut self, user_id: i64) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::SetSenderBlockList {
                user_id,
                block: true,
            },
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&set_message_sender_block_list(extra, user_id, true))
        {
            self.session.requests.take(extra);
            self.session.blocked_error = true;
            return Err(err);
        }
        let list = self.session.blocked_senders.get_or_insert_with(Vec::new);
        if !list.contains(&user_id) {
            list.push(user_id);
            self.session.blocked_total += 1;
        }
        Ok(extra)
    }

    /// notification settings and clears `use_default_mute_for` (Unigram).
    pub fn set_chat_mute_for(
        &mut self,
        chat_id: ChatId,
        mute_for: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !chat.supported() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let settings = chat.notification_settings.clone().with_mute_for(mute_for);
        self.send_notification_settings(chat_id, &settings)
    }

    /// Unmute (`mute_for` 0, not "use default").
    pub fn unmute_chat(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        self.set_chat_mute_for(chat_id, 0)
    }

    /// Mute forever (`i32::MAX`, tdesktop `kMuteForeverValue`).
    pub fn mute_chat_forever(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        self.set_chat_mute_for(chat_id, MUTE_FOREVER)
    }

    fn send_notification_settings(
        &mut self,
        chat_id: ChatId,
        settings: &ChatNotificationSettings,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::SetChatNotificationSettings, Some(chat_id));
        let json = set_chat_notification_settings(extra, chat_id, settings);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Parity slice: `getChatNotificationSettingsExceptions` (TDLib 1.8.67,
    /// line 13659) for a scope not yet loaded and not in flight — once per
    /// dialog open. `compare_sound=true` includes chats whose only
    /// non-default setting is the sound (the exceptions list view).
    pub fn maybe_fetch_notification_exceptions(
        &mut self,
        scope: NotificationSettingsScope,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.notification_exceptions.contains_key(&scope)
            || self
                .session
                .notification_exceptions_loading
                .contains(&scope)
            || self
                .session
                .requests
                .has_purpose_for_scope(RequestPurpose::GetChatNotificationSettingsExceptions, scope)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request_for_scope(RequestPurpose::GetChatNotificationSettingsExceptions, scope);
        self.session.notification_exceptions_loading.insert(scope);
        if let Err(err) = self
            .sender
            .send_json(&get_chat_notification_settings_exceptions(
                extra, scope, true,
            ))
        {
            self.session.requests.take(extra);
            self.session.notification_exceptions_loading.remove(&scope);
            return Err(err);
        }
        Ok(())
    }

    /// Parity slice: reset a chat's notification settings to the scope
    /// defaults (`setChatNotificationSettings` with every `use_default_*`
    /// flag set). The change arrives back as
    /// `updateChatNotificationSettings`, which prunes the chat from the
    /// cached exceptions list.
    pub fn reset_chat_notification_settings(
        &mut self,
        chat_id: ChatId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_notification_settings(chat_id, &ChatNotificationSettings::default())
    }

    /// Parity slice: set the chat's notification-sound exception.
    /// `use_default_sound = true` keeps the scope default; `sound_id = 0`
    /// disables sound (schema line 3350).
    pub fn set_chat_sound(
        &mut self,
        chat_id: ChatId,
        use_default_sound: bool,
        sound_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !chat.supported() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let mut settings = chat.notification_settings.clone();
        settings.use_default_sound = use_default_sound;
        settings.sound_id = sound_id;
        self.send_notification_settings(chat_id, &settings)
    }

    /// Parity slice: set the chat's message-preview exception.
    pub fn set_chat_show_preview(
        &mut self,
        chat_id: ChatId,
        show_preview: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !chat.supported() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let mut settings = chat.notification_settings.clone();
        settings.use_default_show_preview = false;
        settings.show_preview = show_preview;
        self.send_notification_settings(chat_id, &settings)
    }

    /// Parity slice: `getSavedNotificationSounds` once per Ready (guarded by
    /// loaded / in-flight). Drives the sound picker and custom-sound
    /// playback.
    pub fn maybe_fetch_notification_sounds(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.saved_sounds_loaded
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetSavedNotificationSounds)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetSavedNotificationSounds, None);
        match self.sender.send_json(&get_saved_notification_sounds(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Parity slice: refetch the saved-sound list after
    /// `updateSavedNotificationSounds` marked it stale.
    pub fn refresh_notification_sounds_if_stale(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.saved_sounds_stale {
            return Ok(None);
        }
        self.session.saved_sounds_loaded = false;
        self.maybe_fetch_notification_sounds()
    }

    /// Phase S2: `getStorageStatistics` for the storage-usage overlay —
    /// once per session unless forced (guarded by the cache and the
    /// in-flight purpose). Slice S4: `chat_limit` 50 — the Data &
    /// Storage screen's per-chat breakdown needs the per-chat rows
    /// (schema 1.8.67 line 15781).
    pub fn maybe_fetch_storage_statistics(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.storage_stats.is_some()
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetStorageStatistics)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetStorageStatistics, None);
        self.session.storage_stats_loading = true;
        match self.sender.send_json(&get_storage_statistics(extra, 50)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.storage_stats_loading = false;
                Err(err)
            }
        }
    }

    /// Phase S2: drop the cached storage stats so the next
    /// `maybe_fetch_storage_statistics` refetches (the overlay's Refresh).
    /// Also drops the in-flight request: otherwise the immediate refetch
    /// sees the stale purpose, no-ops, and the overlay shows "No storage
    /// data yet." until the old answer lands (late answers to the dropped
    /// `@extra` are ignored by the purpose match).
    pub fn refresh_storage_statistics(&mut self) {
        self.session.storage_stats = None;
        self.session.storage_stats_loading = false;
        self.session
            .requests
            .take_purpose(RequestPurpose::GetStorageStatistics);
    }

    /// Slice S4: fetch `getAutoDownloadSettingsPresets` once — only when
    /// no local `data_storage.json` was loaded (guarded by the seeded
    /// flag and the in-flight purpose). The reducer seeds the local
    /// per-network settings from the answer; TDLib has no getter for
    /// the *current* values, so this is the only server read.
    pub fn fetch_auto_download_presets(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.data_storage.seeded
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetAutoDownloadSettingsPresets)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetAutoDownloadSettingsPresets, None);
        self.session.auto_download_presets_loading = true;
        if let Err(err) = self
            .sender
            .send_json(&get_auto_download_settings_presets(extra))
        {
            self.session.requests.take(extra);
            self.session.auto_download_presets_loading = false;
            self.session.data_storage_error =
                Some("Couldn't load auto-download settings.".to_string());
            return Err(err);
        }
        Ok(())
    }

    /// Slice S4: push one network's auto-download settings
    /// (`setAutoDownloadSettings`, schema 1.8.67, :15820). Never
    /// applied optimistically: the reducer applies the sent settings
    /// only on the confirmed `ok` (the `ok` carries none, so they ride
    /// the request purpose — the setAccountTtl precedent, state.rs:8383,
    /// which applies on `ok` too), and the driver persists via
    /// `data_storage_dirty` on the next ingest. A send failure or
    /// TDLib error leaves the local prefs untouched.
    pub fn set_auto_download_settings(
        &mut self,
        network: NetworkKind,
        settings: AutoDownloadNetSettings,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::SetAutoDownloadSettings { network, settings },
            None,
        );
        let payload = set_auto_download_settings(extra, settings.to_json(), network.td_type());
        if let Err(err) = self.sender.send_json(&payload) {
            self.session.requests.take(extra);
            self.session.data_storage_error =
                Some("Couldn't save auto-download settings.".to_string());
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice S4: the "Use less data for calls" toggle
    /// (`autoDownloadSettings.use_less_data_for_calls`, schema 1.8.67,
    /// :9856). One toggle drives all three networks' settings (TGX
    /// keeps a single switch); each network keeps its own caps.
    /// Refuses while the local prefs are unseeded — pushing the
    /// all-off defaults would silently disable the user's
    /// auto-downloads account-wide.
    pub fn set_less_data_for_calls(&mut self, on: bool) -> Result<(), ConnectSendError> {
        if !self.session.data_storage.seeded {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        for network in NetworkKind::ALL {
            let mut settings = *self.session.data_storage.for_network(network);
            settings.use_less_data_for_calls = on;
            self.set_auto_download_settings(network, settings)?;
        }
        Ok(())
    }

    /// Slice S4: "Clear cache" (`removeAllFilesFromDownloads`, schema
    /// 1.8.67, :14056 — completed downloads dropped from the filesystem
    /// cache, in-flight downloads left alone). The confirmed `ok` drops
    /// the cached stats and refetches the post-clear numbers.
    pub fn clear_download_cache(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::RemoveAllFilesFromDownloads, None);
        if let Err(err) = self
            .sender
            .send_json(&remove_all_files_from_downloads(extra))
        {
            self.session.requests.take(extra);
            self.session.data_storage_error = Some("Couldn't clear the cache.".to_string());
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice S4: persist the per-network auto-download settings
    /// (`data_storage.json`) next to the account.
    pub fn save_data_storage_prefs(&mut self) -> std::io::Result<()> {
        save_data_storage_prefs(&self.paths, &self.session.data_storage)
    }

    /// Slice A3: `getActiveSessions` (schema 1.8.67, line 15102) — once
    /// per session unless the list was marked stale by a terminate or an
    /// explicit refresh (guarded by the cache and the in-flight purpose).
    /// `Ok(None)` = no request needed.
    pub fn maybe_fetch_active_sessions(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if (self.session.sessions.is_some() && !self.session.sessions_stale)
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetActiveSessions)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetActiveSessions, None);
        self.session.sessions_loading = true;
        self.session.sessions_error = None;
        match self.sender.send_json(&get_active_sessions(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.sessions_loading = false;
                Err(err)
            }
        }
    }

    /// Slice A3: refetch the sessions list after a terminate marked it
    /// stale — the reducer kept the old cache and marked it stale on the
    /// authoritative `ok` (the `refresh_notification_sounds_if_stale`
    /// pattern).
    pub fn refresh_active_sessions_if_stale(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.sessions_stale {
            return Ok(None);
        }
        self.maybe_fetch_active_sessions()
    }

    /// Slice A3: send `terminateSession` (schema 1.8.67, line 15105).
    /// One mutation at a time; the list is refetched from the
    /// authoritative `ok` response — never optimistic. A doomed request
    /// (no such session in the cache) is rejected before it leaves;
    /// TDLib is the authority for the rest.
    pub fn terminate_session(&mut self, session_id: i64) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.sessions_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .session
            .sessions
            .as_ref()
            .is_some_and(|s| s.iter().any(|s| s.id == session_id))
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.sessions_error = None;
        let extra = self
            .session
            .request(RequestPurpose::TerminateSession { session_id }, None);
        self.session.sessions_mutating = true;
        match self.sender.send_json(&terminate_session(extra, session_id)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.sessions_mutating = false;
                Err(err)
            }
        }
    }

    /// Slice A3: send `terminateAllOtherSessions` (schema 1.8.67, line
    /// 15108). One mutation at a time; the list is refetched from the
    /// authoritative `ok` response — never optimistic.
    pub fn terminate_all_other_sessions(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.sessions_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.sessions_error = None;
        let extra = self
            .session
            .request(RequestPurpose::TerminateAllOtherSessions, None);
        self.session.sessions_mutating = true;
        match self.sender.send_json(&terminate_all_other_sessions(extra)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.sessions_mutating = false;
                Err(err)
            }
        }
    }

    /// Slice A7: send `deleteAccount` (schema 1.8.67, line 15675).
    /// Guarded on the authorized chats path; one mutation at a time.
    /// The password rides the request JSON only — never stored on the
    /// session or diagnostics (the A2 `password_op_send` rule).
    pub fn delete_account(
        &mut self,
        reason: &str,
        password: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.account_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.account_error = None;
        let extra = self.session.request(RequestPurpose::DeleteAccount, None);
        self.session.account_mutating = true;
        match self
            .sender
            .send_json(&delete_account(extra, reason, password))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.account_mutating = false;
                Err(err)
            }
        }
    }

    /// Slice A7: send `getAccountTtl` (schema 1.8.67, line 15669). The
    /// cached value is reused and an in-flight fetch is never duplicated
    /// (`Ok(None)` = no request needed). The `password_op_send` fetch
    /// pattern, minus the password bookkeeping.
    pub fn get_account_ttl(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.account_ttl_days.is_some() || self.session.account_ttl_loading {
            return Ok(None);
        }
        self.session.account_error = None;
        let extra = self.session.request(RequestPurpose::GetAccountTtl, None);
        self.session.account_ttl_loading = true;
        match self.sender.send_json(&get_account_ttl(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.account_ttl_loading = false;
                Err(err)
            }
        }
    }

    /// Slice A7: send `setAccountTtl` (schema 1.8.67, line 15666). One
    /// mutation at a time; the confirmed days land from the
    /// authoritative `ok` (never an optimistic write).
    pub fn set_account_ttl(&mut self, days: i32) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.account_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.account_error = None;
        let extra = self
            .session
            .request(RequestPurpose::SetAccountTtl { days }, None);
        self.session.account_mutating = true;
        match self.sender.send_json(&set_account_ttl(extra, days)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.account_mutating = false;
                Err(err)
            }
        }
    }

    /// Slice A4: send `toggleSessionCanAcceptSecretChats` (schema 1.8.67,
    /// line 15117). The toggled value is the negation of the cached
    /// flag — one mutation at a time; the list is refetched from the
    /// authoritative `ok` response, never optimistically. A doomed
    /// request (no such session in the cache) is rejected before it
    /// leaves; TDLib is the authority for the rest.
    pub fn toggle_session_can_accept_secret_chats(
        &mut self,
        session_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_session_toggle(session_id, ToggleSessionKind::SecretChats)
    }

    /// Slice A4: send `toggleSessionCanAcceptCalls` (schema 1.8.67, line
    /// 15114) — the `toggleSessionCanAcceptSecretChats` twin.
    pub fn toggle_session_can_accept_calls(
        &mut self,
        session_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_session_toggle(session_id, ToggleSessionKind::Calls)
    }

    /// Slice A4: shared send path for the two per-session toggles.
    fn send_session_toggle(
        &mut self,
        session_id: i64,
        kind: ToggleSessionKind,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.sessions_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        let flags = self
            .session
            .sessions
            .as_ref()
            .and_then(|s| s.iter().find(|s| s.id == session_id))
            .map(|s| (s.can_accept_secret_chats, s.can_accept_calls));
        let Some((can_accept_secret_chats, can_accept_calls)) = flags else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let (extra, json) = match kind {
            ToggleSessionKind::SecretChats => {
                let extra = self.session.request(
                    RequestPurpose::ToggleSessionSecretChats { session_id },
                    None,
                );
                let value = !can_accept_secret_chats;
                (
                    extra,
                    toggle_session_can_accept_secret_chats(extra, session_id, value),
                )
            }
            ToggleSessionKind::Calls => {
                let extra = self
                    .session
                    .request(RequestPurpose::ToggleSessionCalls { session_id }, None);
                let value = !can_accept_calls;
                (
                    extra,
                    toggle_session_can_accept_calls(extra, session_id, value),
                )
            }
        };
        self.session.sessions_error = None;
        self.session.sessions_mutating = true;
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.sessions_mutating = false;
                Err(err)
            }
        }
    }

    /// Slice A4: `getConnectedWebsites` (schema 1.8.67, line 15124) —
    /// guarded-once like `maybe_fetch_active_sessions` (cached state
    /// reused, in-flight fetch deduped).
    pub fn maybe_fetch_connected_websites(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if (self.session.connected_websites.is_some() && !self.session.websites_stale)
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetConnectedWebsites)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetConnectedWebsites, None);
        self.session.connected_websites_loading = true;
        self.session.websites_error = None;
        match self.sender.send_json(&get_connected_websites(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.connected_websites_loading = false;
                Err(err)
            }
        }
    }

    /// Slice A4: refetch the websites list after a disconnect marked it
    /// stale — the reducer kept the old cache and marked it stale on the
    /// authoritative `ok` (the `refresh_active_sessions_if_stale`
    /// pattern).
    pub fn refresh_connected_websites_if_stale(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.websites_stale {
            return Ok(None);
        }
        self.maybe_fetch_connected_websites()
    }

    /// Slice A4: send `disconnectWebsite` (schema 1.8.67, line 15127).
    /// One mutation at a time; the list is refetched from the
    /// authoritative `ok` response — never optimistic. A doomed request
    /// (no such website in the cache) is rejected before it leaves;
    /// TDLib is the authority for the rest.
    pub fn disconnect_website(&mut self, website_id: i64) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.websites_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .session
            .connected_websites
            .as_ref()
            .is_some_and(|s| s.iter().any(|s| s.id == website_id))
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.websites_error = None;
        let extra = self
            .session
            .request(RequestPurpose::DisconnectWebsite { website_id }, None);
        self.session.websites_mutating = true;
        match self
            .sender
            .send_json(&disconnect_website(extra, website_id))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.websites_mutating = false;
                Err(err)
            }
        }
    }

    /// Slice A4: send `disconnectAllWebsites` (schema 1.8.67, line
    /// 15130). One mutation at a time; the list is refetched from the
    /// authoritative `ok` response — never optimistic.
    pub fn disconnect_all_websites(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.websites_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .session
            .connected_websites
            .as_ref()
            .is_some_and(|s| !s.is_empty())
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.websites_error = None;
        let extra = self
            .session
            .request(RequestPurpose::DisconnectAllWebsites, None);
        self.session.websites_mutating = true;
        match self.sender.send_json(&disconnect_all_websites(extra)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.websites_mutating = false;
                Err(err)
            }
        }
    }

    /// Parity slice: `getScopeNotificationSettings` for the scopes not yet
    /// loaded and not in flight — once per Ready.
    pub fn maybe_fetch_scope_notification_settings(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        for scope in NotificationSettingsScope::ALL {
            if self
                .session
                .scope_notification_settings
                .contains_key(&scope)
                || self.session.scope_settings_loading.contains(&scope)
                || self
                    .session
                    .requests
                    .has_purpose_for_scope(RequestPurpose::GetScopeNotificationSettings, scope)
            {
                continue;
            }
            let extra = self
                .session
                .request_for_scope(RequestPurpose::GetScopeNotificationSettings, scope);
            self.session.scope_settings_loading.insert(scope);
            if let Err(err) = self
                .sender
                .send_json(&get_scope_notification_settings(extra, scope))
            {
                self.session.requests.take(extra);
                self.session.scope_settings_loading.remove(&scope);
                return Err(err);
            }
        }
        Ok(())
    }

    /// Parity slice: `setScopeNotificationSettings` for one scope (full
    /// object; callers copy the current scope settings and change one
    /// field). The new values arrive as `updateScopeNotificationSettings`.
    pub fn send_scope_notification_settings(
        &mut self,
        scope: NotificationSettingsScope,
        settings: &ScopeNotificationSettings,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_scope(RequestPurpose::SetScopeNotificationSettings, scope);
        match self
            .sender
            .send_json(&set_scope_notification_settings(extra, scope, settings))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Parity slice: `setReactionNotificationSettings` (full object; callers
    /// copy the current settings and change one field). No getter exists —
    /// the new values arrive as `updateReactionNotificationSettings`.
    pub fn send_reaction_notification_settings(
        &mut self,
        settings: &ReactionNotificationSettings,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetReactionNotificationSettings, None);
        match self
            .sender
            .send_json(&set_reaction_notification_settings(extra, settings))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Parity slice: `resetAllNotificationSettings` — resets all chat and
    /// scope notification settings to their default values. The new values
    /// arrive as `updateScopeNotificationSettings` /
    /// `updateChatNotificationSettings`; the ok arm drops the cached scope
    /// settings so the next fetch shows server-confirmed defaults.
    pub fn reset_all_notification_settings(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ResetAllNotificationSettings, None);
        match self
            .sender
            .send_json(&reset_all_notification_settings(extra))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Parity slice: resolve a notification sound to a playable form.
    /// A custom id missing from the saved list falls back to the default
    /// tone (per the `getSavedNotificationSounds` schema comment).
    pub fn resolve_notification_sound(&mut self, kind: NotificationSoundKind) -> SoundResolution {
        use SoundResolution as R;
        let sound_id = match kind {
            NotificationSoundKind::Default => return R::DefaultTone,
            NotificationSoundKind::Custom(id) => id,
        };
        let Some(entry) = self
            .session
            .saved_notification_sounds
            .iter()
            .find(|s| s.id == sound_id)
        else {
            return R::DefaultTone;
        };
        let file_id = entry.sound.id;
        if let Some(path) = self.session.file(file_id).and_then(|f| f.usable_path()) {
            return R::FilePath(path.into());
        }
        // Not local yet: mark the file as a notification sound, request
        // playback on completion, and start the download (deduped).
        self.session.sound_file_ids.insert(file_id.0, sound_id);
        self.session.pending_sound_downloads.insert(sound_id);
        let _ = self.download_file(file_id, USER_DOWNLOAD_PRIORITY);
        R::Pending
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase C2i: persist the call preferences edited from the Calls
    /// tab (same account-scoped dir as the other settings files).
    pub fn save_call_prefs(&mut self) -> std::io::Result<()> {
        save_call_prefs(&self.paths, &self.session.call_prefs)
    }

    /// MED1: persist media prefs (`media_prefs.json`) next to the account.
    pub fn save_media_prefs(&mut self) -> std::io::Result<()> {
        crate::settings::save_media_prefs(&self.paths, &self.session.media_prefs)
    }

    /// Slice parity:chatlist-badge-settings: persist badge-counter prefs
    /// (`badge_prefs.json`) next to the account.
    pub fn save_badge_prefs(&mut self) -> std::io::Result<()> {
        save_badge_prefs(&self.paths, &self.session.badge_prefs)
    }

    /// Parity slice: persist the in-app notification sounds toggle
    /// (`prefs.json`). Loads the existing prefs first so the other
    /// fields survive the write.
    pub fn save_inapp_sounds_enabled(&mut self) -> std::io::Result<()> {
        let mut prefs = load_preferences(&self.paths);
        prefs.inapp_sounds_enabled = self.session.inapp_sounds_enabled;
        save_preferences(&self.paths, &prefs)
    }

    /// Parity slice (platform-custom-keybindings): load the user's shortcut
    /// overrides (`prefs.json`).
    pub fn load_custom_keybindings(&self) -> Vec<crate::settings::CustomKeybinding> {
        load_preferences(&self.paths).custom_keybindings
    }

    /// Parity slice (platform-custom-keybindings): persist one shortcut
    /// override (`prefs.json`); an empty keystroke resets to the default.
    pub fn save_custom_keybinding(
        &mut self,
        custom: crate::settings::CustomKeybinding,
    ) -> std::io::Result<()> {
        let mut prefs = load_preferences(&self.paths);
        if custom.keystroke.is_empty() {
            prefs.custom_keybindings.retain(|c| c.id != custom.id);
        } else if let Some(existing) = prefs
            .custom_keybindings
            .iter_mut()
            .find(|c| c.id == custom.id)
        {
            existing.keystroke = custom.keystroke;
        } else {
            prefs.custom_keybindings.push(custom);
        }
        save_preferences(&self.paths, &prefs)
    }

    /// Slice parity:settings-language: persist the app language pref
    /// (`language_prefs.json`) next to the account.
    pub fn save_language_prefs(&mut self) -> std::io::Result<()> {
        save_language_prefs(&self.paths, &self.session.language_prefs)
    }
}
