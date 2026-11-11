//! Connect driver: storage statistics, auto-download presets and storage limits.
use super::*;
use crate::data_settings::PresetsSeed;
use crate::state::{StorageClear, StorageSaveKind};

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase S2: `getStorageStatistics` for the storage-usage overlay —
    /// once per session unless forced (guarded by the cache and the
    /// in-flight purpose). Slice S4: `chat_limit` 50 — the Data &
    /// Storage screen's per-chat breakdown needs the per-chat rows
    /// (schema 1.8.67 line 15781).
    pub fn maybe_fetch_storage_statistics(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.settings.storage_stats.can_ask()
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetStorageStatistics)
        {
            return Ok(None);
        }
        // A refused ask ends in the failure the usage list draws with its
        // Retry, never in a list that waits for good.
        if !self.chats_path_active() {
            self.session
                .settings
                .storage_stats
                .fail("Couldn't load storage usage.");
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetStorageStatistics, None);
        match self.sender.send_json(&get_storage_statistics(extra, 50)) {
            Ok(()) => {
                self.session.settings.storage_stats.begin();
                Ok(Some(extra))
            }
            Err(err) => {
                self.session.requests.take(extra);
                self.session
                    .settings
                    .storage_stats
                    .fail("Couldn't load storage usage.");
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
        self.session.settings.storage_stats = Default::default();
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
        if !self.session.settings.data_storage.seeded.askable() {
            return Ok(());
        }
        if !self.chats_path_active() {
            self.session.settings.data_storage.seeded =
                PresetsSeed::Failed("Couldn't load auto-download settings.".to_owned());
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetAutoDownloadSettingsPresets, None);
        if let Err(err) = self
            .sender
            .send_json(&get_auto_download_settings_presets(extra))
        {
            self.session.requests.take(extra);
            self.session.settings.data_storage.seeded =
                PresetsSeed::Failed("Couldn't load auto-download settings.".to_owned());
            return Err(err);
        }
        self.session.settings.data_storage.seeded = PresetsSeed::Loading(extra);
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
        self.session
            .settings
            .data_storage_save
            .begin(StorageSaveKind::AutoDownload);
        self.send_auto_download_settings(network, settings)
    }

    /// One `setAutoDownloadSettings` of a batch: the batch cleared the
    /// earlier failure, this one may add its own.
    fn send_auto_download_settings(
        &mut self,
        network: NetworkKind,
        settings: AutoDownloadNetSettings,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            self.session.settings.data_storage_save.fail(
                StorageSaveKind::AutoDownload,
                "Couldn't save auto-download settings.",
            );
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
            self.session.settings.data_storage_save.fail(
                StorageSaveKind::AutoDownload,
                "Couldn't save auto-download settings.",
            );
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
        if !self.session.settings.data_storage.seeded.is_seeded() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Every network is sent even if one send fails, so a failure never
        // leaves the later networks out of a toggle that covers all three.
        let mut first_error = None;
        self.session
            .settings
            .data_storage_save
            .begin(StorageSaveKind::AutoDownload);
        for network in NetworkKind::ALL {
            let mut settings = *self.session.settings.data_storage.for_network(network);
            settings.use_less_data_for_calls = on;
            if let Err(err) = self.send_auto_download_settings(network, settings) {
                first_error.get_or_insert(err);
            }
        }
        first_error.map_or(Ok(()), Err)
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
        if self.session.settings.storage_clear.is_clearing() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.chats_path_active() {
            self.session.settings.storage_clear =
                StorageClear::Failed("Couldn't clear the cache.".to_string());
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::OptimizeStorage, None);
        let params = OptimizeStorage {
            file_types,
            chat_ids,
            ..OptimizeStorage::everything(50)
        };
        self.session.settings.storage_clear = StorageClear::Clearing;
        if let Err(err) = self.sender.send_json(&optimize_storage(extra, &params)) {
            self.session.requests.take(extra);
            self.session.settings.storage_clear =
                StorageClear::Failed("Couldn't clear the cache.".to_string());
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
        self.session
            .settings
            .data_storage_save
            .begin(StorageSaveKind::Limits);
        if !self.chats_path_active() {
            self.session
                .settings
                .data_storage_save
                .fail(StorageSaveKind::Limits, "Couldn't save the storage limits.");
            return Err(ConnectSendError::InvalidRequest);
        }
        // All four options go out even if one send fails; the first error
        // is the outcome.
        let mut first_error = None;
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
                self.session
                    .settings
                    .data_storage_save
                    .fail(StorageSaveKind::Limits, "Couldn't save the storage limits.");
                first_error.get_or_insert(err);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// TDLib keeps every cached file until the optimizer runs, and it only
    /// runs with a limit set. The first Ready of an account that never had
    /// limits sets tdesktop's starting ones, once: the flag stays, so a
    /// later "No limit" is never overwritten. The flag is saved only once
    /// TDLib holds limits ([`Self::note_storage_limits_seeded`]): a process
    /// that dies first, or an option TDLib refuses, sends them again on
    /// the next Ready (the options are idempotent).
    pub(crate) fn maybe_seed_storage_limits(&mut self) -> Result<(), ConnectSendError> {
        let settings = &self.session.settings;
        if settings.data_storage.storage_limits_seeded.is_seeded() {
            return Ok(());
        }
        if !settings.storage_limits.chosen() {
            self.set_storage_limits(
                Some(crate::storage_limits::DEFAULT_SIZE),
                Some(crate::storage_limits::DEFAULT_KEEP),
            )?;
        }
        self.note_storage_limits_seeded();
        Ok(())
    }

    /// After an ingest: TDLib's options say the account has limits (the
    /// `updateOption` echo of `use_storage_optimizer`, or an earlier
    /// choice), so it counts as seeded from now on.
    pub(crate) fn note_storage_limits_seeded(&mut self) {
        let settings = &self.session.settings;
        if settings.data_storage.storage_limits_seeded.is_seeded()
            || !settings.storage_limits.chosen()
        {
            return;
        }
        self.session.settings.data_storage.storage_limits_seeded = Seed::Seeded;
        let _ = self.save_data_storage_prefs();
    }

    /// Slice S4: persist the per-network auto-download settings
    /// (`data_storage.json`) next to the account.
    pub fn save_data_storage_prefs(&mut self) -> std::io::Result<()> {
        save_data_storage_prefs(&self.paths, &self.session.settings.data_storage)
    }
}
