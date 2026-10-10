//! Connect driver: construction, TDLib paths, kickoff, chat-list loading, close and logout.
use super::*;

/// TDLib's media cache folders inside the database directory (TDLib
/// `FileType` directories for stickers, thumbnails, profile photos and
/// wallpapers).
const TDLIB_DATABASE_MEDIA_DIRS: [&str; 4] =
    ["stickers", "thumbnails", "profile_photos", "wallpapers"];

impl<S: JsonSender> ConnectDriver<S> {
    pub fn new(
        session: Session,
        sender: S,
        credentials: TelegramCredentials,
        prepared: PreparedConnect,
    ) -> Self {
        Self {
            session,
            sender,
            call_engine: None,
            signaling_outbox: Arc::new(Mutex::new(VecDeque::new())),
            transport_outbox: Arc::new(Mutex::new(VecDeque::new())),
            video_state_outbox: Arc::new(Mutex::new(VecDeque::new())),
            screen_state_outbox: Arc::new(Mutex::new(VecDeque::new())),
            audio_state_outbox: Arc::new(Mutex::new(VecDeque::new())),
            call_audio: Default::default(),
            video_frame_slots: Arc::new(Mutex::new(HashMap::new())),
            group_video_frame_slots: Arc::new(Mutex::new(HashMap::new())),
            group_camera_state: HashMap::new(),
            selected_camera: None,
            call_devices_cache: Vec::new(),
            selected_devices: (None, None),
            call_connect_params: None,
            reconnect_attempts: 0,
            credentials,
            paths: prepared.paths,
            database_key: prepared.database_key,
            parameters_sent: false,
            search_debounce_token: 0,
            pending_typed_search: None,
            chat_search_debounce_token: 0,
            pending_typed_chat_search: None,
            outgoing_typing: None,
            outgoing_voice: None,
            draft_clock: DraftSaveClock::idle(),
            draft_save_token: 0,
            pending_draft: None,
            flood_retries: Vec::new(),
            flood_attempts: HashMap::new(),
            last_request_sweep: None,
        }
    }

    pub fn parameters_sent(&self) -> bool {
        self.parameters_sent
    }

    pub fn tdlib_files(&self) -> &Path {
        &self.paths.tdlib_files
    }

    /// Every folder TDLib downloads displayable media into. Besides
    /// `files_directory`, TDLib keeps stickers, thumbnails, profile photos
    /// and wallpapers under the database directory, as part of its cache.
    pub fn tdlib_media_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![self.paths.tdlib_files.clone()];
        roots.extend(
            TDLIB_DATABASE_MEDIA_DIRS
                .iter()
                .map(|dir| self.paths.tdlib_database.join(dir)),
        );
        roots
    }

    /// Kick the JSON client so authorization updates start flowing.
    pub fn kickoff(&mut self) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::GetAuthorizationState, None);
        self.sender.send_json(&get_authorization_state(extra))?;
        Ok(extra)
    }

    pub(crate) fn chats_path_active(&self) -> bool {
        matches!(self.session.auth, AuthorizationState::Ready)
            && matches!(self.session.shutdown, ShutdownPhase::Running)
    }

    /// First page on Ready; further pages only when a `loadChats` request returns ok.
    pub fn maybe_load_main_chats(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(None);
        }
        if self.session.chats_exhausted {
            return Ok(None);
        }
        if self.session.requests.has_purpose(RequestPurpose::LoadChats) {
            return Ok(None);
        }
        let extra = self.session.request(RequestPurpose::LoadChats, None);
        self.sender
            .send_json(&load_chats(extra, MAIN_CHAT_LOAD_LIMIT))?;
        Ok(Some(extra))
    }

    /// One `loadChats(chatListArchive)` page unless the archive is
    /// exhausted or a page is in flight. TDLib sends archived chats'
    /// positions only for the loaded part of the list (Telegram X
    /// `TdlibChatList.loadMore`).
    pub fn maybe_load_archive_chats(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || self.session.archive_chats_exhausted {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::LoadArchiveChats)
        {
            return Ok(None);
        }
        let extra = self.session.request(RequestPurpose::LoadArchiveChats, None);
        if let Err(err) = self
            .sender
            .send_json(&load_archive_chats(extra, MAIN_CHAT_LOAD_LIMIT))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Send `close` (not `logOut`). Callers must keep receiving until Closed.
    pub fn request_close(&mut self) -> Result<RequestId, ConnectSendError> {
        if matches!(
            self.session.auth,
            AuthorizationState::Closed | AuthorizationState::Closing
        ) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.begin_close();
        let extra = self.session.request(RequestPurpose::Close, None);
        self.sender.send_json(&close_request(extra))?;
        Ok(extra)
    }

    /// Slice auth-logout-warning: send `logOut` (not `close`). TDLib
    /// answers `ok`, then drives `Ready → authorizationStateLoggingOut →
    /// authorizationStateClosed` (`Session::set_auth` handles both); the
    /// UI restarts the live connection on Closed so the user lands back
    /// on the login screen. Only valid while authorized.
    pub fn request_logout(&mut self) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::Ready) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.begin_logout();
        let extra = self.session.request(RequestPurpose::LogOut, None);
        self.sender.send_json(&log_out(extra))?;
        Ok(extra)
    }
}
