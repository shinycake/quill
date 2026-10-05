//! Connect driver: media downloads.
use super::*;
use crate::ids::{ChatId, FileId, MessageId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::requests::{
    add_file_to_downloads as add_file_to_downloads_request,
    cancel_download_file as cancel_download_file_request, download_file as download_file_request,
    remove_file_from_downloads as remove_file_from_downloads_request,
    toggle_download_is_paused as toggle_download_is_paused_request,
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
            if let Some(extra) = self.download_file(file_id, THUMB_DOWNLOAD_PRIORITY)? {
                extras.push(extra);
            }
        }
        Ok(extras)
    }

    /// MED3: auto-download full media in the open chat for the media types
    /// the user enabled per chat kind (TGX auto-download). These are one-shot
    /// automatic downloads, so they never enter the downloads manager's
    /// user lists.
    pub fn maybe_download_open_chat_media(&mut self) -> Result<Vec<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(Vec::new());
        }
        let ids = self.session.auto_download_media_file_ids();
        let mut extras = Vec::new();
        for file_id in ids {
            if let Some(extra) = self.download_file(file_id, AUTO_MEDIA_DOWNLOAD_PRIORITY)? {
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
            if let Some(extra) = self.download_file(file_id, THUMB_DOWNLOAD_PRIORITY)? {
                extras.push(extra);
            }
        }
        Ok(extras)
    }

    /// Send one-shot `downloadFile` (`synchronous: false`) for automatic
    /// downloads (thumbs, auto-downloaded media, avatars, sounds). No-op if
    /// already local or in flight. Never enters the downloads manager's
    /// user lists — pause only exists in the persistent download-list API.
    pub fn download_file(
        &mut self,
        file_id: FileId,
        priority: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.should_download(file_id)
            || self.session.stalled_auto_downloads.contains(&file_id.0)
        {
            return Ok(None);
        }
        let extra = self.session.request_download(file_id);
        self.session.begin_download(file_id);
        match self
            .sender
            .send_json(&download_file_request(extra, file_id, priority))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.abort_download(file_id);
                Err(err)
            }
        }
    }

    /// User-initiated download (history rows, viewer, manager retry) via
    /// the persistent file-download list (`addFileToDownloads`) so it can
    /// be paused/resumed/cancelled through the list API. Tracked in
    /// `Session::user_downloads` (surfaces in the downloads manager).
    /// `origin` is the (chat, message) the file belongs to — `(0, 0)` when
    /// unknown (e.g. manager retry).
    pub fn download_user_file(
        &mut self,
        file_id: FileId,
        origin: Option<(ChatId, i64)>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.should_download(file_id) {
            return Ok(None);
        }
        let extra = self.session.request_download(file_id);
        self.session.begin_download(file_id);
        self.session.user_downloads.insert(file_id.0);
        let (chat_id, message_id) = origin.unwrap_or((ChatId(0), 0));
        match self.sender.send_json(&add_file_to_downloads_request(
            extra,
            file_id,
            chat_id,
            MessageId(message_id),
            USER_DOWNLOAD_PRIORITY,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                // The request never reached TDLib: record the failure
                // like an error response so the row offers Retry.
                self.session.failed_downloads.insert(file_id.0);
                self.session.abort_download(file_id);
                Err(err)
            }
        }
    }

    /// Pause a user-initiated (listed) download (`toggleDownloadIsPaused`).
    /// The pause state itself arrives on `updateFileDownload` — this only
    /// sends the request. Returns `Ok(false)` when the file isn't a
    /// tracked user download.
    pub fn pause_download(&mut self, file_id: FileId) -> Result<bool, ConnectSendError> {
        self.toggle_download_paused(file_id, true)
    }

    /// Resume a paused user-initiated download (`toggleDownloadIsPaused`).
    pub fn resume_download(&mut self, file_id: FileId) -> Result<bool, ConnectSendError> {
        self.toggle_download_paused(file_id, false)
    }

    fn toggle_download_paused(
        &mut self,
        file_id: FileId,
        is_paused: bool,
    ) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if file_id.0 == 0 || !self.session.user_downloads.contains(&file_id.0) {
            return Ok(false);
        }
        let extra = self
            .session
            .request(RequestPurpose::ToggleDownloadIsPaused, None);
        match self.sender.send_json(&toggle_download_is_paused_request(
            extra, file_id, is_paused,
        )) {
            Ok(()) => Ok(true),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Cancel an in-flight download. Listed (user-initiated) downloads are
    /// removed from the persistent download list
    /// (`removeFileFromDownloads(delete_from_cache:false)` — removal also
    /// stops the transfer); one-shot automatic downloads still use
    /// `cancelDownloadFile`. TDLib answers `Ok`; the subsequent
    /// `updateFile` (active=false) unsticks the download too. Returns
    /// `Ok(false)` when nothing was in flight.
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
        let payload = if self.session.user_downloads.contains(&file_id.0) {
            remove_file_from_downloads_request(extra, file_id, false)
        } else {
            cancel_download_file_request(extra, file_id, false)
        };
        match self.sender.send_json(&payload) {
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
