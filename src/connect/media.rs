//! Connect driver: media downloads.
use super::*;
use crate::disk_guard::{DiskFloors, DiskState};
use crate::ids::{ChatId, FileId, MessageId, RequestId};
use crate::state::{DownloadOrigin, RequestPurpose};
use crate::story_preload::PreloadRequest;
use crate::telegram::requests::{
    add_file_to_downloads as add_file_to_downloads_request,
    cancel_download_file as cancel_download_file_request, download_file as download_file_request,
    download_file_range as download_file_range_request,
    remove_file_from_downloads as remove_file_from_downloads_request,
    toggle_download_is_paused as toggle_download_is_paused_request,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Auto-download photo thumbs in the open chat (`priority` 1). Skips secret/spoiler.
    pub fn maybe_download_open_thumbs(&mut self) -> Result<Vec<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(Vec::new());
        }
        let mut extras = self.maybe_request_map_thumbs()?;
        // The badge of a message sent with an effect needs the effect's
        // description (its icon) first.
        let effect_ids = self.session.message_effect_ids_in_view();
        if !effect_ids.is_empty() {
            extras.extend(self.load_message_effects(effect_ids)?);
        }
        let panels = self.session.inline_result_file_ids_to_download();
        for file_id in panels
            .into_iter()
            .chain(self.session.gif_panel_file_ids_to_download())
            .chain(self.session.info_cover_file_ids_to_download())
        {
            if let Some(extra) = self.download_file(file_id, INLINE_RESULTS_DOWNLOAD_PRIORITY)? {
                extras.push(extra);
            }
        }
        let ids = self.session.thumb_file_ids_to_download();
        for file_id in ids {
            if let Some(extra) = self.download_file(file_id, THUMB_DOWNLOAD_PRIORITY)? {
                extras.push(extra);
            }
        }
        Ok(extras)
    }

    /// [`Self::maybe_download_open_thumbs`] for the main window's chat, then
    /// for each chat shown in a window of its own: a message arriving there
    /// needs its thumbnails, and its effect described and downloaded, as
    /// much as one in the main window's chat.
    pub fn maybe_download_shown_thumbs(&mut self) -> Result<Vec<RequestId>, ConnectSendError> {
        let mut extras = self.maybe_download_open_thumbs()?;
        let windows: Vec<ChatId> = self.session.window_chats.iter().copied().collect();
        for chat_id in windows {
            extras.extend(
                self.with_window_chat(chat_id, |driver| driver.maybe_download_open_thumbs())?,
            );
        }
        Ok(extras)
    }

    /// Ask TDLib for the map tile of every location in the open chat that
    /// has none yet (`getMapThumbnailFile`). Each place is asked once; the
    /// answered file then downloads like a photo thumbnail.
    fn maybe_request_map_thumbs(&mut self) -> Result<Vec<RequestId>, ConnectSendError> {
        let mut extras = Vec::new();
        for (key, chat_id) in self.session.map_thumbs_to_request() {
            let extra = self
                .session
                .request(RequestPurpose::GetMapThumbnailFile, Some(chat_id));
            self.session.media.map_thumbs.expect(extra, key);
            let json = crate::telegram::requests::get_map_thumbnail_file(
                extra,
                key.latitude(),
                key.longitude(),
                crate::state::MAP_THUMB_ZOOM,
                crate::state::MAP_THUMB_WIDTH,
                crate::state::MAP_THUMB_HEIGHT,
                crate::state::MAP_THUMB_SCALE,
                chat_id,
            );
            match self.sender.send_json(&json) {
                Ok(()) => extras.push(extra),
                Err(err) => {
                    self.session.requests.take(extra);
                    self.session.media.map_thumbs.unsent(extra);
                    return Err(err);
                }
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
        let session = &mut self.session;
        session
            .media
            .open_chat_media_downloads
            .retain(|id| session.media.downloads.is_requested(*id));
        let mut extras = Vec::new();
        for file_id in ids {
            if let Some(extra) = self.download_file(file_id, AUTO_MEDIA_DOWNLOAD_PRIORITY)? {
                self.session
                    .media
                    .open_chat_media_downloads
                    .insert(file_id.0);
                extras.push(extra);
            }
        }
        Ok(extras)
    }

    /// Parity slice: auto-download chat-list avatar photos
    /// (`chat.photo.small`, the cheap 160px thumbnail, for every chat type).
    /// Runs on every ingest but only looks at avatars that became due
    /// (`Session::take_due_chat_list_photos`); `should_download` dedupes
    /// in-flight and completed files, so each photo is requested at most
    /// once until it lands, and `updateChatPhoto` re-arms the new file id.
    pub fn maybe_download_chat_list_photos(&mut self) -> Result<Vec<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(Vec::new());
        }
        // MED3: data saver pauses all automatic downloads (the per-kind
        // media-type grid governs chat media; avatars are display chrome).
        if self.session.settings.media_prefs.data_saver {
            return Ok(Vec::new());
        }
        let ids = self.session.take_due_chat_list_photos();
        let mut extras = Vec::new();
        for (index, file_id) in ids.iter().enumerate() {
            match self.download_file(*file_id, THUMB_DOWNLOAD_PRIORITY) {
                Ok(Some(extra)) => extras.push(extra),
                Ok(None) => {}
                Err(err) => {
                    // Not sent: keep this and the rest due for the next ingest.
                    self.session
                        .media
                        .avatar_downloads_due
                        .extend(ids.get(index..).unwrap_or_default().iter().map(|id| id.0));
                    return Err(err);
                }
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
        self.send_auto_download(file_id, priority, 0, DownloadOrigin::Auto)
    }

    /// The story viewer's preloads around the current story (tdesktop
    /// `preloadNext`): cancels the running preloads the plan dropped,
    /// except the files in `keep` (the current story's), as tdesktop's
    /// `StoryPreload` cancels when it is replaced or the viewer closes,
    /// then sends the plan. An empty plan and `keep` cancel them all.
    pub fn sync_story_preloads(
        &mut self,
        plan: &[PreloadRequest],
        keep: &[FileId],
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let stale: Vec<FileId> = self
            .session
            .media
            .downloads
            .preload_ids()
            .map(FileId)
            .filter(|id| !keep.contains(id) && !plan.iter().any(|r| r.file_id == *id))
            .collect();
        for file_id in stale {
            self.cancel_download(file_id)?;
        }
        for request in plan {
            self.send_auto_download(
                request.file_id,
                request.priority,
                request.limit,
                DownloadOrigin::Preload,
            )?;
        }
        Ok(())
    }

    /// `downloadFile` of the first `limit` bytes (0 = the whole file) for
    /// an automatic path. No-op if already local, in flight or stalled,
    /// except that an automatic download over a story preload, running or
    /// ended, asks again: TDLib replaces the running request's limit and
    /// priority, and the intent turns automatic, so dropping the preload
    /// no longer cancels it.
    fn send_auto_download(
        &mut self,
        file_id: FileId,
        priority: i32,
        limit: i64,
        origin: DownloadOrigin,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let over_preload = origin != DownloadOrigin::Preload
            && self.session.media.downloads.is_preload(file_id.0)
            && self
                .session
                .media
                .files
                .get(&file_id.0)
                .is_none_or(|file| file.needs_download());
        if !over_preload
            && (!self.session.should_download(file_id)
                || self.session.media.downloads.is_stalled_auto(file_id.0))
        {
            return Ok(None);
        }
        let extra = self.session.request_download(file_id);
        let origin = if over_preload {
            DownloadOrigin::AutoOverPreload
        } else {
            origin
        };
        self.session.media.downloads.begin(file_id.0, origin);
        match self.sender.send_json(&download_file_range_request(
            extra, file_id, priority, limit,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.unstick_download(file_id.0);
                Err(err)
            }
        }
    }

    /// User-initiated download (history rows, viewer, manager retry) via
    /// the persistent file-download list (`addFileToDownloads`) so it can
    /// be paused/resumed/cancelled through the list API. Tracked in
    /// `Session::user_downloads` (surfaces in the downloads manager).
    /// `origin` is the (chat, message) the file belongs to. Cached history resolves missing origins;
    /// files without a source message use `downloadFile`.
    pub fn download_user_file(
        &mut self,
        file_id: FileId,
        origin: Option<(ChatId, i64)>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // A preload asked for a prefix at a low priority, and an automatic
        // download that took one over runs at its own; TDLib replaces the
        // limit and priority of a running download with the new request's,
        // so the whole file is asked for again.
        let preloaded = self.session.media.downloads.is_story_preloaded(file_id.0)
            && self
                .session
                .media
                .files
                .get(&file_id.0)
                .is_none_or(|file| file.needs_download());
        // Asked for again right after a cancel: the cancel is undone.
        let undoes_cancel = self.session.media.downloads.is_cancelling(file_id.0)
            && self
                .session
                .media
                .files
                .get(&file_id.0)
                .is_none_or(|file| file.needs_download());
        if !preloaded && !undoes_cancel && !self.session.should_download(file_id) {
            return Ok(None);
        }
        let origin = origin
            .filter(|(chat, message)| chat.0 != 0 && *message > 0)
            .or_else(|| {
                self.session
                    .histories
                    .values()
                    .flat_map(|history| history.messages.values())
                    .find(|message| {
                        message.id.0 > 0
                            && match &message.content {
                                crate::telegram::envelope::MessageContent::Photo(photo) => {
                                    photo.sizes.iter().any(|size| size.file_id == file_id)
                                }
                                crate::telegram::envelope::MessageContent::Video(media) => {
                                    media.file_id == file_id
                                }
                                crate::telegram::envelope::MessageContent::Animation(media) => {
                                    media.file_id == file_id
                                }
                                crate::telegram::envelope::MessageContent::Document(media) => {
                                    media.file_id == file_id
                                }
                                crate::telegram::envelope::MessageContent::Audio(media) => {
                                    media.file_id == file_id
                                }
                                crate::telegram::envelope::MessageContent::VoiceNote(media) => {
                                    media.file_id == file_id
                                }
                                crate::telegram::envelope::MessageContent::VideoNote(media) => {
                                    media.file_id == file_id
                                }
                                _ => false,
                            }
                    })
                    .map(|message| (message.chat_id, message.id.0))
            });
        let extra = self.session.request_download(file_id);
        let request = if let Some((chat_id, message_id)) = origin {
            self.session.begin_user_download(file_id);
            self.session.media.downloads.note_chat(file_id.0, chat_id);
            add_file_to_downloads_request(
                extra,
                file_id,
                chat_id,
                MessageId(message_id),
                USER_DOWNLOAD_PRIORITY,
            )
        } else {
            // Picker stickers and thumbnails have no source message. TDLib's
            // persistent download list requires one; use the direct file API.
            self.session.begin_download(file_id);
            download_file_request(extra, file_id, USER_DOWNLOAD_PRIORITY)
        };
        match self.sender.send_json(&request) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                // The request never reached TDLib: record the failure
                // like an error response so the row offers Retry.
                self.session
                    .media
                    .downloads
                    .end_as(file_id.0, crate::state::DownloadOrigin::User);
                self.session.unstick_download(file_id.0);
                Err(err)
            }
        }
    }

    pub fn note_disk(
        &mut self,
        state: DiskState,
        free: Option<u64>,
        floors: &DiskFloors,
    ) -> Result<(), ConnectSendError> {
        self.session.media.disk_holds_auto_downloads = state != DiskState::Healthy;
        let (DiskState::Low, Some(free), true) = (state, free, self.chats_path_active()) else {
            return Ok(());
        };
        let room = free.saturating_sub(floors.close);
        let media = &self.session.media;
        let over: Vec<FileId> = media
            .downloads
            .user_ids()
            .filter(|id| !media.downloads.is_paused(*id))
            .filter(|id| {
                media
                    .files
                    .get(id)
                    .is_some_and(|file| file.download_left() > room)
            })
            .map(FileId)
            .collect();
        for file_id in over {
            self.pause_download(file_id)?;
        }
        Ok(())
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
        if file_id.0 == 0 || !self.session.media.downloads.is_user(file_id.0) {
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
        let requested = self.session.media.downloads.is_requested(file_id.0);
        // A transfer TDLib started without a request of ours is cancelled
        // the same way (tdesktop `cancelDownloadFile`).
        let untracked = !requested
            && self
                .session
                .media
                .files
                .get(&file_id.0)
                .is_some_and(|file| {
                    file.local.is_downloading_active && !file.local.is_downloading_completed
                });
        if file_id.0 == 0 || !(requested || untracked) {
            return Ok(false);
        }
        let extra = self
            .session
            .request(RequestPurpose::CancelDownloadFile, None);
        let payload = if self.session.media.downloads.is_user(file_id.0) {
            remove_file_from_downloads_request(extra, file_id, false)
        } else {
            cancel_download_file_request(extra, file_id, false)
        };
        match self.sender.send_json(&payload) {
            Ok(()) => {
                if untracked {
                    self.session.media.downloads.cancel_untracked(file_id.0);
                }
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
