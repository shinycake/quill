//! Connect driver: the listed downloads a low or full disk paused, kept
//! across a closed client or a restart. TDLib keeps them paused in its
//! Downloads list, under new file ids in the next instance, so the
//! message each came from is saved per account and finds them again.
use super::*;
use crate::ids::FileId;
use crate::state::{DiskSearch, MediaPurpose, RequestPurpose};
use crate::telegram::requests::search_active_file_downloads;

/// More than any Downloads list a disk pause leaves behind.
const SEARCH_LIMIT: i32 = 1000;

impl<S: JsonSender> ConnectDriver<S> {
    /// The disk paused `file_id`: save the message it came from.
    pub(super) fn save_disk_pause(&mut self, file_id: i32) {
        if let Some((chat, message)) = self.session.media.downloads.origin_of(file_id) {
            let saved = &mut self.session.settings.data_storage.disk_paused;
            saved.insert((chat.0, message));
        }
    }

    /// The disk's pause of `file_id` is undone.
    pub(super) fn forget_disk_pause(&mut self, file_id: i32) {
        if let Some((chat, message)) = self.session.media.downloads.origin_of(file_id) {
            let saved = &mut self.session.settings.data_storage.disk_paused;
            saved.remove(&(chat.0, message));
        }
    }

    /// On a healthy disk: saved pauses this session did not make were made
    /// by an earlier TDLib instance, so the Downloads list is searched for
    /// them, once per session.
    pub(super) fn search_disk_paused_elsewhere(&mut self) -> Result<(), ConnectSendError> {
        let keys = self.session.settings.data_storage.disk_paused.clone();
        if !self
            .session
            .media
            .disk_search
            .due(std::time::Instant::now())
            || keys.is_empty()
        {
            return Ok(());
        }
        let wait = self.session.media.disk_search.pause();
        let extra = self.session.request(
            RequestPurpose::Media(MediaPurpose::SearchDiskPausedDownloads),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&search_active_file_downloads(extra, SEARCH_LIMIT))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.media.disk_search = DiskSearch::Asked {
            request: extra,
            keys,
            wait,
        };
        Ok(())
    }

    /// After an ingest: resume what that search found, and save the
    /// pauses when they changed.
    pub(crate) fn resume_disk_paused_found(&mut self) -> Result<(), ConnectSendError> {
        let mut due = std::mem::take(&mut self.session.media.disk_resume_due);
        let mut result = Ok(());
        while let Some(id) = due.pop() {
            if let Err(err) = self.toggle_download_paused(FileId(id), false) {
                due.push(id);
                result = Err(err);
                break;
            }
        }
        self.session.media.disk_resume_due.extend(due);
        self.save_disk_pauses_if_changed();
        result
    }

    pub(super) fn save_disk_pauses_if_changed(&mut self) {
        let now = self.session.settings.data_storage.disk_paused.clone();
        if now != self.session.media.disk_paused_persisted && self.save_data_storage_prefs().is_ok()
        {
            self.session.media.disk_paused_persisted = now;
        }
    }
}
