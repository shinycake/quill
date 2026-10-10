//! Connect driver: storage statistics, auto-download presets and storage limits.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
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
        if self.session.settings.storage_stats.is_some()
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
        self.session.settings.storage_stats_loading = true;
        match self.sender.send_json(&get_storage_statistics(extra, 50)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.settings.storage_stats_loading = false;
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
        self.session.settings.storage_stats = None;
        self.session.settings.storage_stats_loading = false;
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
        if self.session.settings.data_storage.seeded
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
        self.session.settings.auto_download_presets_loading = true;
        if let Err(err) = self
            .sender
            .send_json(&get_auto_download_settings_presets(extra))
        {
            self.session.requests.take(extra);
            self.session.settings.auto_download_presets_loading = false;
            self.session.settings.data_storage_error =
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
            RequestPurpose::Settings(SettingsPurpose::SetAutoDownloadSettings {
                network,
                settings,
            }),
            None,
        );
        let payload = set_auto_download_settings(extra, settings.to_json(), network.td_type());
        if let Err(err) = self.sender.send_json(&payload) {
            self.session.requests.take(extra);
            self.session.settings.data_storage_error =
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
        if !self.session.settings.data_storage.seeded {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        for network in NetworkKind::ALL {
            let mut settings = *self.session.settings.data_storage.for_network(network);
            settings.use_less_data_for_calls = on;
            self.set_auto_download_settings(network, settings)?;
        }
        Ok(())
    }

    /// Batch 6: "Clear cache" and its per-type / per-chat variants
    /// (`optimizeStorage`, schema 1.8.67, :15799). `file_types` are
    /// `FileType` constructor names (empty = every type except thumbnails,
    /// profile photos, stickers and wallpapers), `chat_ids` the chats to
    /// clear (empty = all). The answer carries the statistics of the
    /// deleted files, shown as "{size} freed on your device!"; the usage
    /// numbers are refetched. One clear at a time.
    pub fn clear_storage(
        &mut self,
        file_types: &[&str],
        chat_ids: &[i64],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.settings.storage_clearing {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::OptimizeStorage, None);
        let params = OptimizeStorage {
            file_types,
            chat_ids,
            ..OptimizeStorage::everything(50)
        };
        self.session.settings.storage_freed = None;
        self.session.settings.storage_clearing = true;
        self.session.settings.data_storage_error = None;
        if let Err(err) = self.sender.send_json(&optimize_storage(extra, &params)) {
            self.session.requests.take(extra);
            self.session.settings.storage_clearing = false;
            self.session.settings.data_storage_error =
                Some("Couldn't clear the cache.".to_string());
            return Err(err);
        }
        Ok(extra)
    }

    /// Batch 6: apply the local storage limits (tdesktop "Total size
    /// limit" / "Clear files older than"): the four TDLib options
    /// `crate::storage_limits::options_for` lists. `None` = no limit of
    /// that kind; neither switches TDLib's storage optimizer off.
    pub fn set_storage_limits(
        &mut self,
        size: Option<i64>,
        keep: Option<i64>,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        for (name, value) in crate::storage_limits::options_for(size, keep) {
            let extra = self.session.request(RequestPurpose::SetStorageOption, None);
            let json = match value {
                crate::storage_limits::StorageOptionValue::Boolean(on) => {
                    set_option_boolean(extra, name, on)
                }
                crate::storage_limits::StorageOptionValue::Integer(n) => {
                    set_option_integer(extra, name, Some(n))
                }
            };
            if let Err(err) = self.sender.send_json(&json) {
                self.session.requests.take(extra);
                self.session.settings.data_storage_error =
                    Some("Couldn't save the storage limits.".to_string());
                return Err(err);
            }
        }
        Ok(())
    }

    /// Slice S4: persist the per-network auto-download settings
    /// (`data_storage.json`) next to the account.
    pub fn save_data_storage_prefs(&mut self) -> std::io::Result<()> {
        save_data_storage_prefs(&self.paths, &self.session.settings.data_storage)
    }
}
