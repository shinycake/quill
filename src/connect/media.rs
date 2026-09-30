//! Connect driver: media downloads.
use super::*;
use crate::ids::{FileId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::requests::{
    cancel_download_file as cancel_download_file_request, download_file as download_file_request,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Auto-download photo thumbs in the open chat (`priority` 1). Skips secret/spoiler.
    pub fn maybe_download_open_thumbs(&mut self) -> Result<Vec<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(Vec::new());
        }
        let ids = self.session.thumb_file_ids_to_download();
        let mut extras = Vec::new();
        for file_id in ids {
            if let Some(extra) = self.download_file(file_id, THUMB_DOWNLOAD_PRIORITY, false)? {
                extras.push(extra);
            }
        }
        Ok(extras)
    }

    /// MED3: auto-download full media in the open chat for the media types
    /// the user enabled per chat kind (TGX auto-download). Runs with
    /// `user_initiated: false`, so these never enter the downloads manager's
    /// user lists.
    pub fn maybe_download_open_chat_media(&mut self) -> Result<Vec<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(Vec::new());
        }
        let ids = self.session.auto_download_media_file_ids();
        let mut extras = Vec::new();
        for file_id in ids {
            if let Some(extra) = self.download_file(file_id, AUTO_MEDIA_DOWNLOAD_PRIORITY, false)? {
                extras.push(extra);
            }
        }
        Ok(extras)
    }

    /// Parity slice: auto-download chat-list avatar photos
    /// (`chat.photo.small`, the cheap 160px thumbnail, for every chat type).
    /// Runs on every ingest; `should_download` dedupes in-flight and
    /// completed files, so each photo is requested at most once until it
    /// lands, and `updateChatPhoto` re-arms the new file id.
    pub fn maybe_download_chat_list_photos(&mut self) -> Result<Vec<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(Vec::new());
        }
        // MED3: data saver pauses all automatic downloads (the per-kind
        // media-type grid governs chat media; avatars are display chrome).
        if self.session.media_prefs.data_saver {
            return Ok(Vec::new());
        }
        let ids = self.session.chat_list_photo_file_ids();
        let mut extras = Vec::new();
        for file_id in ids {
            if let Some(extra) = self.download_file(file_id, THUMB_DOWNLOAD_PRIORITY, false)? {
                extras.push(extra);
            }
        }
        Ok(extras)
    }

    /// Send `downloadFile` (`synchronous: false`). No-op if already local or in flight.
    /// `user_initiated` marks downloads the user explicitly started (history
    /// rows, viewer) — those surface in the downloads manager; automatic
    /// thumbs/avatars/sounds don't.
    pub fn download_file(
        &mut self,
        file_id: FileId,
        priority: i32,
        user_initiated: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.should_download(file_id) {
            return Ok(None);
        }
        let extra = self.session.request_download(file_id);
        self.session.begin_download(file_id);
        if user_initiated {
            self.session.user_downloads.insert(file_id.0);
        }
        match self
            .sender
            .send_json(&download_file_request(extra, file_id, priority))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                if user_initiated {
                    // The request never reached TDLib: record the failure
                    // like an error response so the row offers Retry.
                    self.session.failed_downloads.insert(file_id.0);
                }
                self.session.abort_download(file_id);
                Err(err)
            }
        }
    }

    /// MED3: send `cancelDownloadFile` (`only_if_pending: false`) for an
    /// in-flight download — TGX's cancel button on downloading media.
    /// TDLib answers `Ok`; the subsequent `updateFile` (active=false)
    /// unsticks the download too. Returns `Ok(false)` when nothing was
    /// in flight. Note: `downloadFile` has no pause — pause exists only in
    /// the downloads-manager API (`addFileToDownloads`), which Quill
    /// doesn't use for inline media; "pause" is out of slice.
    pub fn cancel_download(&mut self, file_id: FileId) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if file_id.0 == 0 || !self.session.downloading.contains(&file_id.0) {
            return Ok(false);
        }
        let extra = self
            .session
            .request(RequestPurpose::CancelDownloadFile, None);
        match self
            .sender
            .send_json(&cancel_download_file_request(extra, file_id, false))
        {
            Ok(()) => {
                self.session.abort_download(file_id);
                Ok(true)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
}
