//! Live TDLib connect gate: credentials + tdjson → setTdlibParameters → auth updates.
//! Never logs api_hash, phone numbers, or codes.

use crate::calls::engine::{
    CallEngine, ConnectParams, EngineError, GroupVideoSource, GroupVideoSourceGroup, MediaDevice,
    MediaDeviceKind, RemoteVideoState, RtcServer, TransportState, VideoFrame,
    group_offer_audio_source_id, video_wanted,
};
use crate::composer::{
    AttachmentKind, ComposerEdit, ComposerEditKind, ComposerSnapshot, DeleteConfirm,
    DraftSaveClock, DraftSaveStep, ForwardDraft, draft_text_to_store, schedule_draft_save,
};
use crate::credentials::TelegramCredentials;
use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::folders::spec_without_chat;
use crate::ids::{AccountKey, ChatId, FileId, MessageId, RequestId, TopicId};
use crate::lifecycle::{RestoreBlocker, plan_restore};
use crate::notify::NotificationSoundKind;
use crate::platform::{DatabaseKey, KeyDecision, SecretStore, load_or_create_key};
use crate::poll::{PollDraft, poll_answer_for_tap};
use crate::settings::{AccountPaths, default_app_root};
use crate::state::{
    AdminListFetch, AdminRightsFetch, CHAT_EVENT_LOG_PAGE_SIZE, ChatEventLogFetch,
    ChatSearchJumpNeed, ChatStatisticsFetch, ForwardFlight, InfoPanelTarget, InviteLinkFetch,
    JoinRequestFetch, MemberStatusChange, RequestPurpose, SearchStatus, Session, ShutdownPhase,
    SupergroupMembersFetch,
};
use crate::telegram::client::{LiveTdJson, OwnedEnvelope, ReceiveBridge};
use crate::telegram::envelope::{
    AuthorizationState, CallState, ChatAdminRights, ChatDraft, ChatFolderSpec, ChatKind,
    ChatNotificationSettings, EnvelopePayload, GroupCallVideoInfo, MUTE_FOREVER, MessageContent,
    MessageSender, NotificationSettingsScope, ParsedGroupCallParticipant, ReadyParams,
    ScopeNotificationSettings, StoryContentView,
};
use crate::telegram::ffi::{LibraryOrigin, TdJsonError, resolve_tdjson_path};
use crate::telegram::requests::{
    AnimationSend, GroupCallJoinParams, InputGroupCallRef, MessageSenderRef, PollSend,
    SetTdlibParameters, StickerSend, VideoNoteSend, VideoNoteThumbnailSend, VideoSend,
    VoiceNoteSend, accept_call_with_protocol, add_chat_to_list, add_chat_to_list_value,
    add_contact, add_message_reaction, add_recently_found_chat, ban_group_call_participants,
    chat_member_status_administrator_json, chat_member_status_member_json,
    check_authentication_code, check_authentication_password, click_chat_sponsored_message,
    close_chat, close_request, close_secret_chat as close_secret_chat_request, close_story,
    create_call_with_protocol, create_chat_folder, create_chat_invite_link, create_new_secret_chat,
    create_video_chat, decline_group_call_invitation, delete_chat_folder, delete_messages,
    delete_story, discard_call as discard_call_request, download_file as download_file_request,
    edit_chat_folder, edit_chat_invite_link, edit_message_caption, edit_message_text,
    end_group_call, end_group_call_recording, end_group_call_screen_sharing, forward_messages,
    get_authorization_state, get_callback_query_answer, get_chat_active_stories,
    get_chat_administrators, get_chat_event_log, get_chat_folder, get_chat_history,
    get_chat_invite_links, get_chat_join_requests, get_chat_lists_to_add_chat, get_chat_member,
    get_chat_sponsored_messages, get_chat_statistics, get_commands, get_contacts, get_forum_topics,
    get_group_call, get_installed_sticker_sets, get_me, get_saved_animations,
    get_saved_notification_sounds, get_scope_notification_settings, get_secret_chat,
    get_sticker_set, get_story, get_story_available_reactions, get_supergroup,
    get_supergroup_full_info, get_supergroup_members, get_user_full_info,
    get_video_chat_invite_link, get_video_chat_rtmp_url, input_message_photo, input_message_video,
    invite_group_call_participant, join_chat, join_group_call, join_video_chat, leave_chat,
    leave_group_call, load_active_stories, load_chats, load_chats_list,
    load_group_call_participants, open_chat, open_message_content, open_story, pin_chat_message,
    process_chat_join_request, remove_message_reaction, reorder_chat_folders,
    replace_video_chat_rtmp_url, report_chat_sponsored_message, revoke_chat_invite_link,
    revoke_group_call_invite_link, search_chat_messages, search_chats, search_messages,
    search_public_chats, search_recently_found_chats, send_animation, send_call_debug_information,
    send_call_rating, send_call_signaling_data, send_chat_action, send_chat_action_kind,
    send_document, send_group_call_message, send_message_album, send_photo, send_poll,
    send_sticker, send_text, send_text_story_reply, send_video, send_video_note, send_voice_note,
    set_authentication_phone_number, set_chat_draft_message, set_chat_member_status,
    set_chat_message_auto_delete_time, set_chat_notification_settings, set_chat_slow_mode_delay,
    set_group_call_participant_volume_level, set_poll_answer, set_scope_notification_settings,
    set_story_reaction, set_video_chat_title, start_group_call_recording,
    start_group_call_screen_sharing, supergroup_members_filter_recent_json,
    supergroup_members_filter_search_json, toggle_chat_folder_tags,
    toggle_group_call_are_messages_allowed, toggle_group_call_is_my_video_enabled,
    toggle_group_call_is_my_video_paused, toggle_group_call_participant_is_hand_raised,
    toggle_group_call_participant_is_muted, toggle_video_chat_mute_new_participants,
    unpin_chat_message, view_messages, view_sponsored_chat,
};
use crate::voice::VoiceDraft;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How a [`NotificationSoundKind`] resolves to something playable
/// (parity slice: notification sounds).
pub enum SoundResolution {
    /// Play the app default tone (ffplay-synthesized).
    DefaultTone,
    /// Play this downloaded MP3.
    FilePath(std::path::PathBuf),
    /// The sound file is being downloaded; it plays on completion via
    /// `Session::pending_sound_plays`.
    Pending,
}

/// Unigram `OutputChatActionManager` default delay and `ChatTextBox` keystroke
/// gap before another `SendChatAction` (`chatActionTyping`). tdesktop resends
/// every 5s (`kSendMyTypingInterval`); Quill follows the TDLib client's 4s.
pub const OUTGOING_TYPING_INTERVAL_MS: u64 = 4_000;

/// How many chats to ask TDLib to load per `loadChats` page.
pub const MAIN_CHAT_LOAD_LIMIT: i32 = 100;
/// Page size for `getChatHistory`.
pub const HISTORY_PAGE_SIZE: i32 = 50;
/// `downloadFile.priority` for automatic photo thumbs (schema: 1–32).
pub const THUMB_DOWNLOAD_PRIORITY: i32 = 1;
/// `downloadFile.priority` when the user opens media.
pub const USER_DOWNLOAD_PRIORITY: i32 = 32;
/// `searchChats.limit` / `searchMessages.limit` (Unigram messages page is 20).
pub const SEARCH_LIMIT: i32 = 20;
/// `searchRecentlyFoundChats.limit` — schema/Unigram cap is 50.
pub const RECENT_SEARCH_LIMIT: i32 = 50;
/// tdesktop `kSearchRequestDelay` / `AutoSearchTimeout` (config.h): 900 ms.
/// ComposeSearch `requestSearchDelayed` uses the same `AutoSearchTimeout`.
pub const SEARCH_DEBOUNCE: Duration = Duration::from_millis(900);
/// tdesktop `kSaveDraftTimeout` — quiet time before `setChatDraftMessage`.
pub const DRAFT_SAVE_DEBOUNCE: Duration = Duration::from_millis(1_000);
/// tdesktop `kSearchPerPage` (`api_messages_search.cpp`).
pub const CHAT_SEARCH_LIMIT: i32 = 50;
/// Unigram `LoadMessageSliceImpl`: `GetChatHistory(chatId, maxId, -25, 50)`.
pub const HISTORY_AROUND_OFFSET: i32 = -25;
/// Phase 5.1: `getForumTopics.limit` — first page of the topic list.
pub const FORUM_TOPICS_LIMIT: i32 = 100;
/// Phase 5.1: `searchChatMessages.limit` for per-topic history pages.
pub const TOPIC_HISTORY_PAGE_SIZE: i32 = 50;
/// Unigram `LoadMessageSliceImpl` page size around the jump target.
pub const HISTORY_AROUND_LIMIT: i32 = 50;

/// In-flight global search extras (official empty = recents; typed = chats + messages).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchFlight {
    Recents(RequestId),
    /// `searchChats` + `searchMessages` + `searchPublicChats` extras.
    Query(RequestId, RequestId, RequestId),
}

/// Empty/open recents send immediately; typed queries wait for [`SEARCH_DEBOUNCE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchQueryOutcome {
    Sent(SearchFlight),
    Debounced { token: u64 },
    Unchanged,
}

/// In-chat search extras (`searchChatMessages`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSearchFlight {
    Query(RequestId),
}

/// Empty in-chat query clears immediately; typed queries wait for [`SEARCH_DEBOUNCE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSearchQueryOutcome {
    Sent(ChatSearchFlight),
    Debounced { token: u64 },
    Unchanged,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectBlocker {
    MissingCredentials,
    MissingTdjson,
    MissingKeyAgainstExistingDb,
    LockedStore,
    StoreError,
    TdjsonLoad,
}

impl ConnectBlocker {
    pub fn user_message(&self) -> &'static str {
        match self {
            ConnectBlocker::MissingCredentials => {
                "set TELEGRAM_API_ID / TELEGRAM_API_HASH (or local .env)"
            }
            ConnectBlocker::MissingTdjson => {
                "tdjson not found — set QUILL_TDJSON_PATH to libtdjson, or bundle it next to the executable (see docs/native-bundle.md). Homebrew paths are never searched."
            }
            ConnectBlocker::MissingKeyAgainstExistingDb => {
                "database encryption key missing for an existing TDLib database — restore the key or remove the local database"
            }
            ConnectBlocker::LockedStore => "secret store is locked or unavailable",
            ConnectBlocker::StoreError => "secret store error",
            ConnectBlocker::TdjsonLoad => {
                "tdjson library found but failed to load (missing symbols or wrong arch)"
            }
        }
    }

    /// One-token label for `--connect-smoke` (no secrets).
    pub fn slug(&self) -> &'static str {
        match self {
            ConnectBlocker::MissingCredentials => "missing-credentials",
            ConnectBlocker::MissingTdjson => "missing-tdjson",
            ConnectBlocker::MissingKeyAgainstExistingDb => "missing-key",
            ConnectBlocker::LockedStore => "locked-store",
            ConnectBlocker::StoreError => "store-error",
            ConnectBlocker::TdjsonLoad => "tdjson-load",
        }
    }
}

impl From<RestoreBlocker> for ConnectBlocker {
    fn from(value: RestoreBlocker) -> Self {
        match value {
            RestoreBlocker::MissingCredentials => ConnectBlocker::MissingCredentials,
            RestoreBlocker::MissingKeyAgainstExistingDb => {
                ConnectBlocker::MissingKeyAgainstExistingDb
            }
            RestoreBlocker::LockedStore => ConnectBlocker::LockedStore,
            RestoreBlocker::StoreError => ConnectBlocker::StoreError,
        }
    }
}

/// Outcome of the pre-flight gate (no native load yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectGate {
    Blocked(ConnectBlocker),
    Ready { tdjson: LibraryOrigin },
}

/// Classify credentials + tdjson availability. Does not open a client.
pub fn evaluate_gate(credentials_present: bool) -> ConnectGate {
    if !credentials_present {
        return ConnectGate::Blocked(ConnectBlocker::MissingCredentials);
    }
    match resolve_tdjson_path() {
        Some(tdjson) => ConnectGate::Ready { tdjson },
        None => ConnectGate::Blocked(ConnectBlocker::MissingTdjson),
    }
}

/// Local paths + database key ready for `setTdlibParameters`.
#[derive(Debug)]
pub struct PreparedConnect {
    pub account: AccountKey,
    pub paths: AccountPaths,
    pub database_key: DatabaseKey,
    pub database_exists: bool,
}

/// Resolve account paths and DB key. Credentials must already be validated.
pub fn prepare_connect<S: SecretStore + ?Sized>(
    app_root: &Path,
    account: AccountKey,
    store: &S,
    credentials: &TelegramCredentials,
) -> Result<PreparedConnect, ConnectBlocker> {
    let plan = plan_restore(
        app_root,
        account.clone(),
        store,
        Some(credentials.api_id),
        Some(credentials.api_hash.as_str()),
    )
    .map_err(ConnectBlocker::from)?;
    let key =
        load_or_create_key(store, &plan.account, plan.database_exists).map_err(|e| match e {
            KeyDecision::MissingAgainstExistingDb => ConnectBlocker::MissingKeyAgainstExistingDb,
            KeyDecision::Locked => ConnectBlocker::LockedStore,
            KeyDecision::Store(_) => ConnectBlocker::StoreError,
        })?;
    std::fs::create_dir_all(&plan.paths.tdlib_database).map_err(|_| ConnectBlocker::StoreError)?;
    std::fs::create_dir_all(&plan.paths.tdlib_files).map_err(|_| ConnectBlocker::StoreError)?;
    Ok(PreparedConnect {
        account: plan.account,
        paths: plan.paths,
        database_key: key,
        database_exists: plan.database_exists,
    })
}

/// Build `setTdlibParameters` from loaded credentials + local paths/key.
/// The returned JSON includes `api_hash`; callers must not log it.
pub fn build_set_tdlib_parameters(
    credentials: &TelegramCredentials,
    paths: &AccountPaths,
    database_key: &DatabaseKey,
) -> SetTdlibParameters {
    SetTdlibParameters {
        use_test_dc: std::env::var("QUILL_USE_TEST_DC")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        database_directory: paths.tdlib_database.to_string_lossy().into_owned(),
        files_directory: paths.tdlib_files.to_string_lossy().into_owned(),
        database_encryption_key_b64: database_key.tdlib_base64(),
        api_id: credentials.api_id,
        api_hash: credentials.api_hash.clone(),
        device_model: "Desktop".into(),
        system_version: std::env::consts::OS.into(),
        application_version: env!("CARGO_PKG_VERSION").into(),
        system_language_code: "en".into(),
    }
}

/// Outbound JSON (live `td_send` or test recorder). Must not log request bodies.
pub trait JsonSender: Send {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectSendError {
    InvalidRequest,
    Native,
}

/// Records outbound JSON for unit/replay tests (no network).
#[derive(Default)]
pub struct RecordingSender {
    pub sent: Mutex<Vec<String>>,
}

impl RecordingSender {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.sent.lock().expect("recording sender").clone()
    }
}

impl JsonSender for RecordingSender {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        self.sent
            .lock()
            .expect("recording sender")
            .push(request.to_string());
        Ok(())
    }
}

impl JsonSender for Arc<RecordingSender> {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        (**self).send_json(request)
    }
}

/// Live `td_send` wrapper.
pub struct LiveSender {
    api: Arc<crate::telegram::ffi::TdJson>,
    client_id: i32,
}

impl LiveSender {
    pub fn from_live(live: &LiveTdJson) -> Self {
        Self {
            api: live.api.clone(),
            client_id: live.client_id,
        }
    }
}

impl JsonSender for LiveSender {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        self.api.send(self.client_id, request).map_err(|e| match e {
            TdJsonError::InvalidRequest => ConnectSendError::InvalidRequest,
            _ => ConnectSendError::Native,
        })
    }
}

/// Phase C2b: worker-safe queue for engine-emitted signaling.
type SignalingOutbox = Arc<Mutex<VecDeque<(i32, Vec<u8>)>>>;
type TransportOutbox = Arc<Mutex<VecDeque<(i32, TransportState)>>>;
/// Phase C2e: worker-safe queue for engine-emitted peer camera states.
type VideoStateOutbox = Arc<Mutex<VecDeque<(i32, RemoteVideoState)>>>;
/// Phase C2e: latest decoded video frame per (call id, is_local); only
/// the newest frame is kept, so the UI never sees a backlog.
type VideoFrameSlots = Arc<Mutex<HashMap<(i32, bool), VideoFrame>>>;

/// Phase C2g: latest decoded group video frame per (group call id,
/// participant user id, is_screen).
type GroupVideoFrameSlots = Arc<Mutex<HashMap<(i32, i64, bool), VideoFrame>>>;

/// Session + outbound sender that auto-replies to `WaitTdlibParameters`.
pub struct ConnectDriver<S: JsonSender> {
    pub session: Session,
    sender: S,
    call_engine: Option<Box<dyn CallEngine>>,
    signaling_outbox: SignalingOutbox,
    transport_outbox: TransportOutbox,
    /// Phase C2e: engine-emitted peer camera states (worker thread ->
    /// driver pump).
    video_state_outbox: VideoStateOutbox,
    /// Phase C2e: latest decoded video frame per (call id, is_local).
    video_frame_slots: VideoFrameSlots,
    /// Phase C2g: latest decoded group video frame per (group call id,
    /// participant user id, is_screen); only the newest frame is kept.
    group_video_frame_slots: GroupVideoFrameSlots,
    /// Phase C2g: last camera-enabled value issued to the engine per
    /// group call id; the engine is only re-issued on change.
    group_camera_state: HashMap<i32, bool>,
    /// Phase C2e: selected camera device id; `None` means the engine
    /// default. No devices are fabricated, so this can legitimately be
    /// unset.
    selected_camera: Option<String>,
    /// Phase C2c: last-enumerated audio devices and the selected
    /// (microphone, speaker) ids. `None` means the engine default; no
    /// devices are fabricated, so this can legitimately be empty.
    call_devices_cache: Vec<MediaDevice>,
    selected_devices: (Option<String>, Option<String>),
    call_connect_params: Option<ConnectParams>,
    reconnect_attempts: usize,
    credentials: TelegramCredentials,
    paths: AccountPaths,
    database_key: DatabaseKey,
    parameters_sent: bool,
    search_debounce_token: u64,
    pending_typed_search: Option<(u64, String)>,
    chat_search_debounce_token: u64,
    pending_typed_chat_search: Option<(u64, String)>,
    /// Last `chatActionTyping` we sent (Unigram `_lastTypingTime`).
    outgoing_typing: Option<OutgoingTyping>,
    /// Last `chatActionRecordingVoiceNote` (Unigram record button).
    outgoing_voice: Option<OutgoingTyping>,
    /// tdesktop `saveDraft` clock for the open composer.
    draft_clock: DraftSaveClock,
    draft_save_token: u64,
    pending_draft: Option<PendingDraft>,
}

struct OutgoingTyping {
    chat_id: ChatId,
    last_sent_ms: u64,
}

struct PendingDraft {
    token: u64,
    chat_id: ChatId,
    text: String,
    reply_to: Option<MessageId>,
}

/// Result of noting a composer edit. UI arms a timer only for `Debounced`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftSaveOutcome {
    Debounced { token: u64, delay: Duration },
    Sent,
    Skipped,
}

/// Phase C2g: build the engine's incoming-video subscription set from
/// the tracked participants' `video_info` / `screen_sharing_video_info`
/// (TDLib 1.8.67, `schema/td_api.tl:7163`). Skips the local user, paused
/// channels, and channels without a usable endpoint/ssrc — the engine
/// diffs this set against its subscriptions on every pump.
fn group_video_sources(participants: &[ParsedGroupCallParticipant]) -> Vec<GroupVideoSource> {
    fn one(user_id: i64, info: &Option<GroupCallVideoInfo>, out: &mut Vec<GroupVideoSource>) {
        let Some(info) = info else { return };
        if info.is_paused || info.endpoint_id.is_empty() {
            return;
        }
        let ssrc_groups: Vec<GroupVideoSourceGroup> = info
            .source_groups
            .iter()
            .filter(|group| !group.source_ids.is_empty())
            .map(|group| GroupVideoSourceGroup {
                semantics: group.semantics.clone(),
                ssrcs: group.source_ids.clone(),
            })
            .collect();
        if ssrc_groups.is_empty() {
            return;
        }
        out.push(GroupVideoSource {
            user_id,
            endpoint: info.endpoint_id.clone(),
            ssrc_groups,
        });
    }

    let mut sources = Vec::new();
    for participant in participants {
        if participant.is_current_user {
            continue;
        }
        let MessageSender::User { user_id } = participant.participant_id else {
            continue;
        };
        one(user_id, &participant.video_info, &mut sources);
        one(
            user_id,
            &participant.screen_sharing_video_info,
            &mut sources,
        );
    }
    sources
}

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
        }
    }

    pub fn parameters_sent(&self) -> bool {
        self.parameters_sent
    }

    pub fn tdlib_files(&self) -> &Path {
        &self.paths.tdlib_files
    }

    /// Phase C2b: install the driver-thread call engine and route its
    /// worker-thread signaling/transport emissions into the driver
    /// queues. Devices are fetched on demand (`refresh_call_devices`),
    /// never here.
    pub fn set_call_engine(&mut self, mut engine: Box<dyn CallEngine>) {
        let outbox = self.signaling_outbox.clone();
        engine.set_signaling_emitted_callback(Arc::new(move |call_id, data| {
            outbox
                .lock()
                .expect("call signaling outbox")
                .push_back((call_id, data));
        }));
        let transport_outbox = self.transport_outbox.clone();
        engine.set_transport_state_callback(Arc::new(move |call_id, state| {
            transport_outbox
                .lock()
                .expect("call transport outbox")
                .push_back((call_id, state));
        }));
        // Phase C2e: peer camera states and decoded frames ride the same
        // worker-thread -> driver-pump path as transport and signaling.
        let video_state_outbox = self.video_state_outbox.clone();
        engine.set_remote_video_state_callback(Arc::new(move |call_id, state| {
            video_state_outbox
                .lock()
                .expect("call video state outbox")
                .push_back((call_id, state));
        }));
        let video_frame_slots = self.video_frame_slots.clone();
        let group_video_frame_slots = self.group_video_frame_slots.clone();
        engine.set_video_frame_callback(Arc::new(move |call_id, frame| {
            // Phase C2g: group-call frames carry the participant they were
            // subscribed for; they live in their own slots so the 1:1
            // (call id, is_local) keys stay untouched.
            if let Some(user_id) = frame.participant_user_id {
                group_video_frame_slots
                    .lock()
                    .expect("group video frame slots")
                    .insert((call_id, user_id, frame.is_screen), frame);
            } else {
                video_frame_slots
                    .lock()
                    .expect("call video frame slots")
                    .insert((call_id, frame.is_local), frame);
            }
        }));
        self.call_engine = Some(engine);
    }

    /// Phase C2c: whether a call engine is installed at all.
    pub fn has_call_engine(&self) -> bool {
        self.call_engine.is_some()
    }

    /// Phase C2c: last-enumerated audio devices; empty when the engine
    /// reported none (or is missing/unavailable).
    pub fn call_devices(&self) -> &[MediaDevice] {
        &self.call_devices_cache
    }

    /// Phase C2c: selected (microphone, speaker) device ids; `(None,
    /// None)` is the engine default. No fabricated devices: nothing is
    /// auto-selected from enumeration.
    pub fn selected_call_devices(&self) -> (Option<&str>, Option<&str>) {
        (
            self.selected_devices.0.as_deref(),
            self.selected_devices.1.as_deref(),
        )
    }

    /// Phase C2c: best-effort device re-enumeration. A missing or
    /// unavailable engine leaves the cache untouched.
    pub fn refresh_call_devices(&mut self) {
        if let Some(engine) = self.call_engine.as_deref()
            && let Ok(devices) = engine.media_devices()
        {
            self.call_devices_cache = devices;
        }
    }

    /// Phase C2g: whether the engine enumerates a screen-capture
    /// source. The presentation handshake needs one; without it the UI
    /// offers no screen-share control ("No screen source available").
    pub fn group_call_screen_source_available(&self) -> bool {
        self.call_devices_cache
            .iter()
            .any(|device| device.kind == MediaDeviceKind::Screen)
    }

    /// Phase C2g: whether the native call engine is installed and
    /// available, for honest group-call audio/video state copy.
    pub fn group_call_engine_available(&self) -> bool {
        self.call_engine
            .as_ref()
            .is_some_and(|engine| engine.is_available())
    }

    /// Phase C2b: advertise the engine's protocol only when an engine is
    /// installed AND available; otherwise fall back to the honest
    /// signaling-only shape.
    fn engine_protocol_json(&self) -> serde_json::Value {
        self.call_engine
            .as_ref()
            .filter(|engine| engine.is_available())
            .map(|engine| engine.protocol().to_json())
            .unwrap_or_else(crate::telegram::requests::call_protocol)
    }

    fn call_connect_params(&self, is_outgoing: bool, ready: &ReadyParams) -> ConnectParams {
        let library_versions = self
            .call_engine
            .as_ref()
            .filter(|engine| engine.is_available())
            .map(|engine| engine.protocol().library_versions)
            .unwrap_or_default();
        let is_video = self.session.active_call.as_ref().is_some_and(|call| {
            // A camera toggle before the transport existed is stored in
            // `camera_on` and must survive into the connect params (a
            // camera-off toggle means the call is negotiated without
            // video). At initial connect `camera_on == is_video`, so
            // nothing changes there.
            call.is_video && call.camera_on
        });
        let (video_enabled, default_camera) = video_wanted(is_video, &self.call_devices_cache);
        let camera_input = video_enabled
            .then(|| self.selected_camera.clone().or(default_camera))
            .flatten();
        ConnectParams {
            encryption_key: ready.encryption_key.clone(),
            is_outgoing,
            servers: ready
                .servers
                .iter()
                .map(|server| RtcServer {
                    id: server.id,
                    ipv4: server.ipv4.clone(),
                    ipv6: server.ipv6.clone(),
                    port: server.port,
                    username: server.username.clone(),
                    password: server.password.clone(),
                    turn: server.turn,
                    stun: server.stun,
                    tcp: server.tcp,
                    peer_tag: server.peer_tag.clone(),
                })
                .collect(),
            library_versions,
            p2p_allowed: ready.allow_p2p,
            mic_input: self.selected_devices.0.clone(),
            speaker_input: self.selected_devices.1.clone(),
            // Phase C2e: a video call negotiates video only when a camera
            // exists; the user's camera pick wins over the first
            // enumerated camera. The connect path refreshes the device
            // cache before calling this (cheap no-op when the engine is
            // missing).
            video_enabled,
            camera_input,
        }
    }

    /// Phase C2e: honest 1:1 video readiness — the active call is a video
    /// call, the engine is available, and a camera exists. The UI uses
    /// this for disabled states instead of guessing.
    pub fn call_video_ready(&self) -> bool {
        let is_video = self
            .session
            .active_call
            .as_ref()
            .is_some_and(|call| call.is_video);
        self.call_engine
            .as_ref()
            .is_some_and(|engine| engine.is_available())
            && video_wanted(is_video, &self.call_devices_cache).0
    }

    /// Phase C2e: newest decoded frame for a call; `is_local` selects the
    /// local preview (`true`) or the peer camera (`false`). `None` when
    /// no frame has arrived yet.
    pub fn latest_video_frame(&self, call_id: i32, is_local: bool) -> Option<VideoFrame> {
        self.video_frame_slots
            .lock()
            .expect("call video frame slots")
            .get(&(call_id, is_local))
            .cloned()
    }

    /// Phase C2g: newest decoded group video frame for a participant;
    /// `screen` selects the screen-share stream (`true`) or the camera
    /// (`false`). `None` when no frame has arrived yet.
    pub fn latest_group_video_frame(
        &self,
        group_call_id: i32,
        user_id: i64,
        screen: bool,
    ) -> Option<VideoFrame> {
        self.group_video_frame_slots
            .lock()
            .expect("group video frame slots")
            .get(&(group_call_id, user_id, screen))
            .cloned()
    }

    /// Phase C2e: selected camera device id; `None` is the engine default.
    pub fn selected_call_camera(&self) -> Option<&str> {
        self.selected_camera.as_deref()
    }

    /// Phase C2e: camera toggle through the native engine. The engine is
    /// called first and its error propagates *without* flipping the
    /// session flag (mirrors `set_call_muted`); without a connected
    /// transport the intent is only stored (it applies on connect).
    pub fn set_call_camera(&mut self, call_id: i32, enabled: bool) -> Result<(), EngineError> {
        let Some(call) = self.session.active_call.as_mut() else {
            return Err(EngineError::NoActiveCall);
        };
        if call.id != call_id {
            return Err(EngineError::NoSuchCall(call_id));
        }
        if call.transport.is_some()
            && let Some(engine) = self.call_engine.as_deref_mut()
            && engine.is_available()
        {
            engine.set_camera_enabled(call.id, enabled, self.selected_camera.as_deref())?;
        }
        call.camera_on = enabled;
        Ok(())
    }

    /// Phase C2e: pick the camera device id (`None` = engine default).
    /// The selection is stored (so it applies on connect) and forwarded
    /// to the native engine with the current camera intent only when a
    /// transport is already connected; a failed forward propagates
    /// before the stored selection changes.
    pub fn select_call_camera(&mut self, camera: Option<String>) -> Result<(), EngineError> {
        let camera_on = self
            .session
            .active_call
            .as_ref()
            .is_some_and(|call| call.camera_on);
        if let Some(call) = self.session.active_call.as_ref()
            && call.transport.is_some()
            && let Some(engine) = self.call_engine.as_deref_mut()
            && engine.is_available()
        {
            engine.set_camera_enabled(call.id, camera_on, camera.as_deref())?;
        }
        self.selected_camera = camera;
        Ok(())
    }

    /// Kick the JSON client so authorization updates start flowing.
    pub fn kickoff(&mut self) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::GetAuthorizationState, None);
        self.sender.send_json(&get_authorization_state(extra))?;
        Ok(extra)
    }

    pub fn ingest(&mut self, owned: OwnedEnvelope) -> Result<(), ConnectSendError> {
        let was_ready = matches!(self.session.auth, AuthorizationState::Ready);
        let active_call_before = self.session.active_call.as_ref().map(|call| call.id);
        let active_group_call_before = self.session.active_group_call.as_ref().map(|call| call.id);
        let bridge_signaling = match &owned.envelope.payload {
            EnvelopePayload::UpdateNewCallSignalingData { call_id, data } => {
                Some((*call_id, data.clone()))
            }
            _ => None,
        };
        // Continue paging only when this envelope completes an in-flight loadChats
        // with ok. Unrelated ingest ticks and non-404 errors must not re-issue.
        let load_chats_ok = matches!(owned.envelope.payload, EnvelopePayload::Ok)
            && owned.envelope.extra.is_some_and(|id| {
                self.session.requests.purpose(id) == Some(RequestPurpose::LoadChats)
            });
        // Parity slice: a folder `loadChats` page completing with ok pages
        // on (until a 404 marks the folder exhausted in the reducer).
        // Captured before `apply` takes the pending request.
        let folder_load_ok: Option<i32> = match owned.envelope.payload {
            EnvelopePayload::Ok => owned.envelope.extra.and_then(|id| {
                (self.session.requests.purpose(id) == Some(RequestPurpose::LoadFolderChats))
                    .then(|| self.session.requests.folder_id_for(id))
                    .flatten()
            }),
            _ => None,
        };
        let view_purpose = owned
            .envelope
            .extra
            .and_then(|id| self.session.requests.purpose(id));
        let view_after = match &owned.envelope.payload {
            EnvelopePayload::Messages(_) | EnvelopePayload::UpdateNewMessage(_) => true,
            EnvelopePayload::Ok | EnvelopePayload::Error(_)
                if view_purpose == Some(RequestPurpose::ViewMessages) =>
            {
                true
            }
            _ => false,
        };
        let thumbs_after = matches!(
            owned.envelope.payload,
            EnvelopePayload::Messages(_)
                | EnvelopePayload::UpdateNewMessage(_)
                | EnvelopePayload::UpdateMessageContent { .. }
                | EnvelopePayload::UpdateFile(_)
                | EnvelopePayload::File(_)
        );
        let chat_search_hits = matches!(
            owned.envelope.payload,
            EnvelopePayload::FoundChatMessages { .. }
        );
        self.session.apply(owned);
        self.pump_call_engine(active_call_before, bridge_signaling)?;
        self.pump_group_call_transport(active_group_call_before)?;
        self.maybe_send_parameters()?;
        self.maybe_probe_channel_membership()?;
        let became_ready = !was_ready && matches!(self.session.auth, AuthorizationState::Ready);
        if became_ready || load_chats_ok {
            self.maybe_load_main_chats()?;
        }
        if let Some(folder_id) = folder_load_ok {
            self.maybe_load_folder_chats(folder_id)?;
        }
        // Parity slice: queued remove-from-folder edits go out once the
        // `getChatFolder` spec arrives.
        self.maybe_finish_folder_removals()?;
        if became_ready {
            // Phase 9.1: the story tray needs `updateChatActiveStories`
            // updates; one `loadActiveStories(storyListMain)` per Ready.
            self.maybe_load_active_stories()?;
            // Parity slice: saved notification sounds (picker + custom-sound
            // playback) and per-scope default settings (`use_default_*`
            // fallback), once per Ready.
            let _ = self.maybe_fetch_notification_sounds();
            let _ = self.maybe_fetch_scope_notification_settings();
        }
        // Parity slice: `updateSavedNotificationSounds` may have marked the
        // list stale between ingests.
        let _ = self.refresh_notification_sounds_if_stale();
        if view_after {
            self.maybe_view_open_messages()?;
        }
        if thumbs_after || self.session.stickers.open || self.session.gifs.open {
            self.maybe_download_open_thumbs()?;
        }
        // Parity slice: chat-list avatars download on every ingest; each
        // photo is requested at most once (in-flight / completed dedupe).
        self.maybe_download_chat_list_photos()?;
        // Phase B1: secret chats whose state never arrived via
        // `updateSecretChat` (e.g. loaded from the local DB) resolve it
        // through the offline `getSecretChat`.
        let _ = self.maybe_fetch_secret_chat_states();
        // Phase C1: incoming calls that arrived while another call was
        // active are declined (busy).
        let _ = self.maybe_decline_busy_calls();
        // Phase C3a: freshly created voice chats get their full
        // `groupCall` via `getGroupCall`.
        let _ = self.maybe_fetch_group_calls();
        // Phase C2f: a dropped group call (`need_rejoin`) auto-rejoins
        // with the C2d attempt discipline (max 3).
        let _ = self.maybe_auto_rejoin_group_call();
        self.maybe_load_selected_sticker_set()?;
        self.maybe_refresh_saved_animations()?;
        if chat_search_hits {
            // Unigram ChatSearchViewModel: first hit → LoadMessageSliceAsync.
            self.jump_selected_chat_search_hit()?;
        }
        Ok(())
    }

    /// Phase C2b: synchronize reducer call lifecycle/signaling with the
    /// engine and flush engine-emitted bytes through TDLib.
    fn pump_call_engine(
        &mut self,
        active_call_before: Option<i32>,
        bridge_signaling: Option<(i32, Vec<u8>)>,
    ) -> Result<(), ConnectSendError> {
        let active_call_after = self
            .session
            .active_call
            .as_ref()
            .map(|call| (call.id, call.user_id, call.is_outgoing, call.ready.clone()));
        let tracked_call_id = active_call_after
            .as_ref()
            .map(|(call_id, _, _, _)| *call_id);

        if tracked_call_id != active_call_before && tracked_call_id.is_some() {
            self.call_connect_params = None;
            self.reconnect_attempts = 0;
        }

        if let Some((call_id, user_id, is_outgoing, _)) = &active_call_after
            && Some(*call_id) != active_call_before
            && let Some(engine) = self.call_engine.as_deref_mut()
        {
            // TDLib remains the source of truth; engine startup failures must
            // not erase the reducer's honest signaling-only call state.
            if let Err(err) = engine.start_call(*call_id, *user_id, *is_outgoing)
                && let Some(call) = self.session.active_call.as_mut()
            {
                call.transport_error = Some(err.to_string());
            }
        }
        if tracked_call_id.is_none()
            && let Some(call_id) = active_call_before
        {
            if let Some(summary) = self
                .session
                .call_summary
                .as_mut()
                .filter(|summary| summary.call_id == call_id)
            {
                summary.reconnect_attempts = self.reconnect_attempts;
            }
            if let Some(engine) = self.call_engine.as_deref_mut() {
                let _ = engine.hangup(call_id);
            }
            // Phase C2e: drop any retained frames for the ended call so
            // the UI cannot render a stale picture.
            self.video_frame_slots
                .lock()
                .expect("call video frame slots")
                .retain(|(slot_call_id, _), _| *slot_call_id != call_id);
            self.call_connect_params = None;
            self.reconnect_attempts = 0;
        }
        if let Some((call_id, data)) = bridge_signaling
            && tracked_call_id == Some(call_id)
            && let Some(engine) = self.call_engine.as_deref_mut()
        {
            // Gate on the reducer-tracked call: the reducer drops signaling
            // for unknown or ended calls, and this bridge follows the same
            // gate. The session queue remains the honest diagnostic record
            // if the optional engine rejects or cannot consume these bytes.
            let _ = engine.send_signaling_data(call_id, &data);
        }

        // Phase C2c: connect the native audio transport exactly once per
        // call — gated on the reducer-tracked transport state, not on a
        // separate attempted set. A Ready call without an encryption key
        // cannot build the native encryption parameters; the call stays
        // up (TDLib owns signaling) but carries no sound.
        if let Some((call_id, _, is_outgoing, Some(ready))) = &active_call_after
            && self
                .session
                .active_call
                .as_ref()
                .is_some_and(|call| call.transport.is_none())
        {
            let call_id = *call_id;
            let is_outgoing = *is_outgoing;
            if ready.encryption_key.is_empty() {
                if let Some(call) = self.session.active_call.as_mut() {
                    call.transport = Some(TransportState::Failed);
                    call.transport_error =
                        Some("call became ready without an encryption key".into());
                }
            } else {
                // Refresh devices before connecting so the native engine
                // sees the current device ids.
                self.refresh_call_devices();
                let params = self.call_connect_params(is_outgoing, ready);
                self.call_connect_params = Some(params.clone());
                self.reconnect_attempts = 0;
                let result = self
                    .call_engine
                    .as_deref_mut()
                    .ok_or(crate::calls::engine::EngineError::Unavailable)
                    .and_then(|engine| engine.connect(call_id, &params));
                let pre_muted = self
                    .session
                    .active_call
                    .as_ref()
                    .is_some_and(|call| call.muted);
                if result.is_ok()
                    && pre_muted
                    && let Some(engine) = self.call_engine.as_deref_mut()
                {
                    // A mute requested before the transport existed applies
                    // once it does; failure here is non-fatal (the unmute
                    // path can retry).
                    let _ = engine.set_muted(call_id, true);
                }
                if let Some(call) = self.session.active_call.as_mut() {
                    match result {
                        Ok(()) => {
                            call.transport = Some(TransportState::Connecting);
                            call.transport_error = None;
                        }
                        Err(err) => {
                            call.transport = Some(TransportState::Failed);
                            call.transport_error = Some(err.to_string());
                        }
                    }
                }
            }
        }

        loop {
            let update = self
                .transport_outbox
                .lock()
                .expect("call transport outbox")
                .pop_front();
            let Some((call_id, state)) = update else {
                break;
            };
            if !self
                .session
                .active_call
                .as_ref()
                .is_some_and(|call| call.id == call_id)
            {
                continue;
            }
            if state == TransportState::Failed {
                let failure = if self.reconnect_attempts >= 3 {
                    Some("reconnect attempts exhausted".to_string())
                } else if !self
                    .call_engine
                    .as_ref()
                    .is_some_and(|engine| engine.is_available())
                {
                    Some("call engine is unavailable for reconnect".to_string())
                } else if let Some(params) = self.call_connect_params.as_ref() {
                    self.reconnect_attempts += 1;
                    if let Some(call) = self.session.active_call.as_mut() {
                        call.transport = Some(TransportState::Reconnecting);
                        call.transport_error = None;
                    }
                    match self
                        .call_engine
                        .as_deref_mut()
                        .expect("available engine")
                        .connect(call_id, params)
                    {
                        // The retry is in flight: leave the call in
                        // `Reconnecting` so the UI can show it. The
                        // engine's own state callbacks move it to
                        // `Connecting`/`Connected` (or back to `Failed`)
                        // when they arrive.
                        Ok(()) => None,
                        Err(err) => Some(err.to_string()),
                    }
                } else {
                    Some("no retained call parameters for reconnect".to_string())
                };
                if let Some(error) = failure
                    && let Some(call) = self.session.active_call.as_mut()
                {
                    call.transport = Some(TransportState::Failed);
                    call.transport_error = Some(error);
                }
            } else if let Some(call) = self.session.active_call.as_mut() {
                if state == TransportState::Connected {
                    self.reconnect_attempts = 0;
                }
                call.transport = Some(state);
                call.transport_error = None;
            }
        }

        // Phase C2e: peer camera state follows the same gate as
        // transport — only the reducer-tracked active call is updated;
        // emissions for an unknown or ended call are dropped.
        loop {
            let update = self
                .video_state_outbox
                .lock()
                .expect("call video state outbox")
                .pop_front();
            let Some((call_id, state)) = update else {
                break;
            };
            if let Some(call) = self
                .session
                .active_call
                .as_mut()
                .filter(|call| call.id == call_id)
            {
                call.remote_video = state;
            }
        }

        loop {
            let emitted = self
                .signaling_outbox
                .lock()
                .expect("call signaling outbox")
                .pop_front();
            let Some((call_id, data)) = emitted else {
                break;
            };
            let extra = self
                .session
                .request(RequestPurpose::SendCallSignalingData, None);
            let payload = send_call_signaling_data(extra, call_id, &data);
            if let Err(err) = self.sender.send_json(&payload) {
                // Keep request bookkeeping honest and return the unsent
                // bytes to the head of the outbox so a later ingest retries
                // them in order instead of dropping them.
                self.session.requests.take(extra);
                self.signaling_outbox
                    .lock()
                    .expect("call signaling outbox")
                    .push_front((call_id, data));
                return Err(err);
            }
        }

        Ok(())
    }

    /// Phase C2g: native group-call transport lifecycle, after every
    /// reducer update. Finishes the `ntg_connect` handshake once the
    /// `joinVideoChat` `Text` answer arrives, keeps the outgoing camera
    /// and the incoming video subscriptions in sync with the tracked
    /// participants, finishes the presentation handshake, and tears the
    /// transport down when the tracked call goes away.
    fn pump_group_call_transport(
        &mut self,
        active_group_call_before: Option<i32>,
    ) -> Result<(), ConnectSendError> {
        let active_group_call_after = self.session.active_group_call.as_ref().map(|call| call.id);
        if active_group_call_after != active_group_call_before
            && let Some(before_id) = active_group_call_before
        {
            // The tracked call ended or was replaced: the native transport
            // must not linger, and stale frames must not render.
            if let Some(engine) = self.call_engine.as_deref_mut() {
                let _ = engine.leave_group_call(before_id);
            }
            self.group_video_frame_slots
                .lock()
                .expect("group video frame slots")
                .retain(|(slot_call_id, _, _), _| *slot_call_id != before_id);
            self.group_camera_state.remove(&before_id);
        }
        let Some(group_call_id) = active_group_call_after else {
            return Ok(());
        };
        let engine_available = self
            .call_engine
            .as_ref()
            .is_some_and(|engine| engine.is_available());
        if !engine_available {
            return Ok(());
        }
        // Finish the join handshake once the `joinVideoChat` answer is
        // stored on the tracked call.
        let join_answer = self
            .session
            .active_group_call
            .as_ref()
            .filter(|call| !call.transport_ready)
            .map(|call| {
                (
                    call.join_payload.clone(),
                    call.is_my_video_enabled && !call.is_my_video_paused,
                )
            });
        if let Some((answer, video_enabled)) = join_answer
            && !answer.is_empty()
        {
            let result = self
                .call_engine
                .as_deref_mut()
                .expect("available engine")
                .connect_group_call(group_call_id, &answer, video_enabled);
            let connected = result.is_ok();
            if let Some(call) = self.session.active_group_call.as_mut() {
                match result {
                    Ok(()) => {
                        call.transport_ready = true;
                        call.transport_error = None;
                    }
                    Err(err) => {
                        call.transport_error = Some(err.to_string());
                    }
                }
            }
            if connected {
                self.group_camera_state.insert(group_call_id, video_enabled);
                // Refresh the device cache on connect so the screen-share
                // availability gate sees the engine's real sources.
                self.refresh_call_devices();
            }
        }
        let transport_ready = self
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.transport_ready);
        if !transport_ready {
            return Ok(());
        }
        // Outgoing camera follows the TDLib video flags; re-issue only on
        // change (mirrors the 1:1 `set_call_camera` discipline).
        let wanted_camera = self
            .session
            .active_group_call
            .as_ref()
            .map(|call| call.is_my_video_enabled && !call.is_my_video_paused)
            .unwrap_or(false);
        if self.group_camera_state.get(&group_call_id) != Some(&wanted_camera) {
            let camera = self.selected_camera.clone();
            let result = self
                .call_engine
                .as_deref_mut()
                .expect("available engine")
                .set_group_camera(group_call_id, wanted_camera, camera.as_deref());
            match result {
                Ok(()) => {
                    self.group_camera_state.insert(group_call_id, wanted_camera);
                    if let Some(call) = self.session.active_group_call.as_mut() {
                        call.transport_error = None;
                    }
                }
                Err(err) => {
                    if let Some(call) = self.session.active_group_call.as_mut() {
                        call.transport_error = Some(err.to_string());
                    }
                }
            }
        }
        // Incoming video follows the participants' `video_info` /
        // `screen_sharing_video_info`; the engine diffs add/remove.
        let sources: Vec<GroupVideoSource> = self
            .session
            .active_group_call
            .as_ref()
            .map(|call| group_video_sources(&call.participants))
            .unwrap_or_default();
        if let Err(err) = self
            .call_engine
            .as_deref_mut()
            .expect("available engine")
            .sync_group_video(group_call_id, &sources)
            && let Some(call) = self.session.active_group_call.as_mut()
        {
            call.transport_error = Some(err.to_string());
        }
        // Finish the presentation handshake once the
        // `startGroupCallScreenSharing` answer is stored.
        let share_answer = self
            .session
            .active_group_call
            .as_ref()
            .filter(|call| call.screen_share_pending)
            .map(|call| call.screen_share_answer.clone());
        if let Some(answer) = share_answer
            && !answer.is_empty()
        {
            let result = self
                .call_engine
                .as_deref_mut()
                .expect("available engine")
                .connect_screen_share(group_call_id, &answer);
            if let Some(call) = self.session.active_group_call.as_mut() {
                match result {
                    Ok(()) => {
                        call.screen_share_pending = false;
                        call.screen_sharing = true;
                        call.screen_share_answer.clear();
                        call.transport_error = None;
                    }
                    Err(err) => {
                        call.screen_share_pending = false;
                        call.screen_share_answer.clear();
                        call.transport_error = Some(err.to_string());
                    }
                }
            }
        }
        // Phase C2g: reconcile the native presentation against the
        // tracked screen-share flags. A failed start request or a bad
        // answer clears the tracked flags without touching the engine,
        // so a stray initialized-but-unwanted presentation is stopped
        // here instead of lingering.
        let want_presentation = self
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.screen_sharing || call.screen_share_pending);
        if !want_presentation
            && let Some(engine) = self.call_engine.as_deref_mut()
            && engine.presentation_active(group_call_id)
        {
            let _ = engine.stop_screen_share(group_call_id);
        }
        Ok(())
    }

    fn maybe_send_parameters(&mut self) -> Result<(), ConnectSendError> {
        if self.parameters_sent {
            return Ok(());
        }
        if !matches!(self.session.auth, AuthorizationState::WaitTdlibParameters) {
            return Ok(());
        }
        let extra = self.session.request(RequestPurpose::SetParameters, None);
        let params = build_set_tdlib_parameters(&self.credentials, &self.paths, &self.database_key);
        // Contains api_hash — do not log `json`.
        let json = params.to_json(extra);
        self.sender.send_json(&json)?;
        self.parameters_sent = true;
        Ok(())
    }

    fn chats_path_active(&self) -> bool {
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

    /// Phase 7.1 (extended by the parity slice): `loadChats(chatListFolder)`
    /// when a folder tab is selected, so TDLib delivers the folder's chats /
    /// positions. Like the main list, folders page eagerly: each `ok`
    /// re-enters `maybe_load_folder_chats` (via `ingest`) until a 404 marks
    /// the folder exhausted in the reducer.
    pub fn load_folder_chats(&mut self, folder_id: i32) -> Result<RequestId, ConnectSendError> {
        self.maybe_load_folder_chats(folder_id)?
            .ok_or(ConnectSendError::InvalidRequest)
    }

    /// One `loadChats(chatListFolder)` page, unless the folder is exhausted
    /// or a page is already in flight for it.
    pub fn maybe_load_folder_chats(
        &mut self,
        folder_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(None);
        }
        if self.session.folder_chats_exhausted.contains(&folder_id) {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_folder(RequestPurpose::LoadFolderChats, folder_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_folder(RequestPurpose::LoadFolderChats, folder_id);
        if let Err(err) = self.sender.send_json(&load_chats_list(
            extra,
            serde_json::json!({ "@type": "chatListFolder", "chat_folder_id": folder_id }),
            MAIN_CHAT_LOAD_LIMIT,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Parity slice: `createChatFolder`. Response is `chatFolderInfo`
    /// (upserted by the reducer); the full list still arrives via
    /// `updateChatFolders`.
    pub fn create_chat_folder(
        &mut self,
        spec: &ChatFolderSpec,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::CreateChatFolder, None);
        if let Err(err) = self.sender.send_json(&create_chat_folder(extra, spec)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Parity slice: `editChatFolder`. Response is `chatFolderInfo`
    /// (upserted by the reducer). Drops the cached spec so the next edit
    /// refetches.
    pub fn edit_chat_folder(
        &mut self,
        folder_id: i32,
        spec: &ChatFolderSpec,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.folder_specs.remove(&folder_id);
        let extra = self
            .session
            .request_for_folder(RequestPurpose::EditChatFolder, folder_id);
        if let Err(err) = self
            .sender
            .send_json(&edit_chat_folder(extra, folder_id, spec))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Parity slice: `deleteChatFolder`. Response is `ok`; the reducer drops
    /// the tab on ok. `leave_chat_ids` are chats to leave with the folder
    /// (empty = keep every chat in the main list).
    pub fn delete_chat_folder(
        &mut self,
        folder_id: i32,
        leave_chat_ids: &[i64],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_folder(RequestPurpose::DeleteChatFolder, folder_id);
        if let Err(err) =
            self.sender
                .send_json(&delete_chat_folder(extra, folder_id, leave_chat_ids))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Parity slice: `reorderChatFolders` with the full new folder-id order
    /// (`main_chat_list_position` is always 0 — non-zero is Premium-only).
    /// The tab order is applied optimistically; `updateChatFolders`
    /// confirms (or corrects, on error).
    pub fn reorder_chat_folders(
        &mut self,
        folder_ids: &[i32],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ReorderChatFolders, None);
        if let Err(err) = self
            .sender
            .send_json(&reorder_chat_folders(extra, folder_ids))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        let order: std::collections::HashMap<i32, usize> = folder_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        self.session
            .chat_folders
            .sort_by_key(|f| order.get(&f.id).copied().unwrap_or(usize::MAX));
        Ok(extra)
    }

    /// Parity slice: `toggleChatFolderTags`. Flips
    /// `are_folder_tags_enabled` optimistically; `updateChatFolders`
    /// confirms.
    pub fn toggle_chat_folder_tags(
        &mut self,
        enabled: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ToggleChatFolderTags, None);
        if let Err(err) = self
            .sender
            .send_json(&toggle_chat_folder_tags(extra, enabled))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.are_folder_tags_enabled = enabled;
        Ok(extra)
    }

    /// Parity slice: `getChatFolder` for the edit dialog prefill (or the
    /// remove-from-folder chain). The full spec lands in
    /// `Session::folder_specs`, keyed by folder id. In-flight deduped; the
    /// response always overwrites the cache.
    pub fn fetch_chat_folder(
        &mut self,
        folder_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_folder(RequestPurpose::GetChatFolder, folder_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_folder(RequestPurpose::GetChatFolder, folder_id);
        if let Err(err) = self.sender.send_json(&get_chat_folder(extra, folder_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Parity slice: `getChatFolderChatsToLeave` for the delete-confirm
    /// dialog (schema 1.8.67 line 13367 — chats suggested to leave with the
    /// folder). In-flight deduped per folder.
    pub fn fetch_chat_folder_chats_to_leave(
        &mut self,
        folder_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_folder(RequestPurpose::GetChatFolderChatsToLeave, folder_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_folder(RequestPurpose::GetChatFolderChatsToLeave, folder_id);
        if let Err(err) = self.sender.send_json(&{
            serde_json::json!({
                "@type": "getChatFolderChatsToLeave",
                "@extra": extra.as_extra(),
                "chat_folder_id": folder_id,
            })
            .to_string()
        }) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Parity slice: `getChatListsToAddChat` (schema 1.8.67 line 13347 —
    /// "Returns chat lists to which the chat can be added. This is an
    /// offline method"). Drives the per-chat folder picker as the schema
    /// intends. In-flight deduped per chat.
    pub fn fetch_chat_lists_to_add_chat(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatListsToAddChat, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatListsToAddChat, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_lists_to_add_chat(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Parity slice: `addChatToList` with `chatListFolder` (schema 1.8.67
    /// lines 13352 + 3524). Membership confirms via `updateChatPosition` /
    /// added-to-list updates, like archive.
    pub fn add_chat_to_folder(
        &mut self,
        chat_id: ChatId,
        folder_id: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::AddChatToList, Some(chat_id));
        let json = add_chat_to_list_value(
            extra,
            chat_id,
            serde_json::json!({ "@type": "chatListFolder", "chat_folder_id": folder_id }),
        );
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Parity slice: remove a chat from a folder. There is no
    /// `removeChatFromList` in 1.8.67 — removal is `editChatFolder` with the
    /// chat dropped from the spec (and added to `excluded_chat_ids` when it
    /// would still match the folder's filter flags). Queues the intent and
    /// fetches the full spec; `maybe_finish_folder_removals` (called from
    /// `ingest`) sends the edit once the spec arrives.
    pub fn remove_chat_from_folder(
        &mut self,
        chat_id: ChatId,
        folder_id: i32,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .session
            .folder_remove_queue
            .contains(&(chat_id, folder_id))
        {
            self.session.folder_remove_queue.push((chat_id, folder_id));
        }
        // Skip the fetch when the spec is already cached (the edit dialog
        // keeps it fresh); otherwise the edit waits for `getChatFolder`.
        if !self.session.folder_specs.contains_key(&folder_id) {
            self.fetch_chat_folder(folder_id)?;
        }
        self.maybe_finish_folder_removals()?;
        Ok(())
    }

    /// Send `editChatFolder` for queued remove-from-folder intents whose
    /// full spec is cached and which have no edit already in flight.
    fn maybe_finish_folder_removals(&mut self) -> Result<(), ConnectSendError> {
        let queue = std::mem::take(&mut self.session.folder_remove_queue);
        let mut still_pending = Vec::new();
        for (chat_id, folder_id) in queue {
            let Some(spec) = self.session.folder_specs.get(&folder_id).cloned() else {
                still_pending.push((chat_id, folder_id));
                continue;
            };
            if self
                .session
                .requests
                .has_purpose_for_folder(RequestPurpose::EditChatFolder, folder_id)
            {
                still_pending.push((chat_id, folder_id));
                continue;
            }
            let Some(chat) = self.session.chats.get(&chat_id.0) else {
                continue;
            };
            let user_id = match &chat.kind {
                ChatKind::Private { user_id } | ChatKind::Secret { user_id, .. } => Some(user_id.0),
                _ => None,
            };
            let user = user_id.and_then(|id| self.session.users.get(&id));
            let edited = spec_without_chat(&spec, chat, user);
            let extra = self
                .session
                .request_for_folder(RequestPurpose::EditChatFolder, folder_id);
            if let Err(err) = self
                .sender
                .send_json(&edit_chat_folder(extra, folder_id, &edited))
            {
                self.session.requests.take(extra);
                // Restore the current intent so a transient send failure
                // retries on the next ingest instead of silently dropping it.
                still_pending.push((chat_id, folder_id));
                self.session.folder_remove_queue = still_pending;
                return Err(err);
            }
        }
        self.session.folder_remove_queue = still_pending;
        Ok(())
    }

    /// Select a chat, inform TDLib it is open, and request history.
    /// Returns `None` if history is already complete or a history request
    /// is already in flight.
    pub fn select_chat(&mut self, chat_id: ChatId) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.open_chat == Some(chat_id) {
            self.maybe_probe_channel_membership()?;
            self.maybe_fetch_bot_info()?;
            self.maybe_fetch_bot_commands()?;
            self.maybe_view_open_messages()?;
            self.maybe_download_open_thumbs()?;
            self.fetch_sponsored_messages(chat_id)?;
            // Phase 5.1: re-selecting an open forum chat also resolves /
            // loads topics (the first select may have raced `is_forum`).
            self.maybe_fetch_supergroup_forum(chat_id)?;
            // Parity slice: channel/supergroup header extras.
            self.maybe_fetch_supergroup_profile(chat_id)?;
            self.maybe_fetch_supergroup_full_info_for_header(chat_id)?;
            self.maybe_fetch_forum_topics(chat_id)?;
            return self.fetch_history();
        }
        self.cancel_outgoing_typing()?;
        self.close_open_chat()?;
        self.draft_clock = DraftSaveClock::idle();
        self.pending_draft = None;
        self.session.open_chat(chat_id);
        // Channels are ungated since Phase 2.2: they follow the normal
        // openChat / history path; sponsored rows fetch for every channel.
        self.send_open_chat(chat_id)?;
        self.maybe_probe_channel_membership()?;
        self.maybe_fetch_bot_info()?;
        self.maybe_fetch_bot_commands()?;
        self.maybe_view_open_messages()?;
        self.maybe_download_open_thumbs()?;
        self.fetch_sponsored_messages(chat_id)?;
        // Phase 5.1: forum supergroups resolve `is_forum` from the
        // `supergroup` object (`chatTypeSupergroup` has no forum flag), then
        // load their topic list.
        self.maybe_fetch_supergroup_forum(chat_id)?;
        // Parity slice: channel/supergroup header extras.
        self.maybe_fetch_supergroup_profile(chat_id)?;
        self.maybe_fetch_supergroup_full_info_for_header(chat_id)?;
        self.maybe_fetch_forum_topics(chat_id)?;
        self.fetch_history()
    }

    /// Phase 5.1: `getSupergroup` for a non-channel supergroup whose forum
    /// status is still unknown. Fires once (deduped by cache + in-flight
    /// purpose); the `supergroup` response and `updateSupergroup` both
    /// populate `ChatSummary::is_forum`. No-op for channels, non-supergroups,
    /// and already-resolved chats.
    fn maybe_fetch_supergroup_forum(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat)
                if matches!(
                    chat.kind,
                    ChatKind::Supergroup {
                        is_channel: false,
                        ..
                    }
                ) && chat.is_forum.is_none() =>
            {
                match chat.kind {
                    ChatKind::Supergroup { supergroup_id, .. } => supergroup_id,
                    _ => return Ok(()),
                }
            }
            _ => return Ok(()),
        };
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetSupergroup, chat_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetSupergroup, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_supergroup(extra, supergroup_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Parity slice: `getSupergroup` for the channel/supergroup header's
    /// @username (and `is_forum` for non-channels). Fires once per
    /// supergroup — deduped by the `supergroup_usernames` cache (which
    /// spontaneous `updateSupergroup` updates also fill) and the in-flight
    /// `GetSupergroup` purpose, so it never doubles
    /// `maybe_fetch_supergroup_forum`'s request.
    fn maybe_fetch_supergroup_profile(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup { supergroup_id, .. }
                    if !self
                        .session
                        .supergroup_usernames
                        .contains_key(&supergroup_id) =>
                {
                    supergroup_id
                }
                _ => return Ok(()),
            },
            None => return Ok(()),
        };
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetSupergroup, chat_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetSupergroup, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_supergroup(extra, supergroup_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Parity slice: `getSupergroupFullInfo` for the channel/supergroup
    /// header (description snippet, subscriber/member count, linked
    /// discussion group). Deduped by the cache + in-flight purpose inside
    /// `fetch_supergroup_full_info`.
    fn maybe_fetch_supergroup_full_info_for_header(
        &mut self,
        chat_id: ChatId,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup { supergroup_id, .. } => supergroup_id,
                _ => return Ok(()),
            },
            None => return Ok(()),
        };
        self.fetch_supergroup_full_info(supergroup_id).map(|_| ())
    }

    /// Phase 5.1: `getForumTopics` (first page) for a known forum supergroup.
    /// Fires once per chat (deduped by cache + in-flight purpose). No-op
    /// until `is_forum` resolves true.
    fn maybe_fetch_forum_topics(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        if !self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.is_forum_chat())
        {
            return Ok(());
        }
        if self.session.forum_topics.contains_key(&chat_id.0) {
            return Ok(());
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetForumTopics, chat_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetForumTopics, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_forum_topics(
            extra,
            chat_id,
            "",
            0,
            MessageId(0),
            0,
            FORUM_TOPICS_LIMIT,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Phase 5.1: select a forum topic. The topic's history is fetched with
    /// `searchChatMessages` (`topic_id = messageTopicForum`, empty query)
    /// and rendered by the same history component as chat history.
    pub fn select_topic(
        &mut self,
        forum_topic_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.is_forum_chat())
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.select_topic(chat_id, forum_topic_id);
        self.fetch_topic_history()
    }

    /// Phase 5.1: leave the topic view, back to the forum's topic list.
    pub fn deselect_topic(&mut self) {
        self.session.deselect_topic();
    }

    /// Phase 5.1: page the open topic's history (`searchChatMessages` with
    /// `topic_id`). First page starts at `from_message_id` 0; later pages
    /// continue from the response's `next_from_message_id`.
    pub fn fetch_topic_history(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let Some(forum_topic_id) = self.session.open_topic else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let key = (chat_id.0, forum_topic_id);
        if self
            .session
            .topic_histories
            .get(&key)
            .is_some_and(|h| h.loaded_complete)
        {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetTopicHistory, chat_id)
        {
            return Ok(None);
        }
        let from = self
            .session
            .topic_histories
            .get(&key)
            .map(|h| h.next_from_message_id)
            .unwrap_or(MessageId(0));
        let extra = self.session.request_for_topic(
            RequestPurpose::GetTopicHistory,
            Some(chat_id),
            forum_topic_id,
        );
        let topic = TopicId::Forum {
            forum_topic_id: forum_topic_id as i64,
        };
        match self.sender.send_json(&search_chat_messages(
            extra,
            chat_id,
            &topic,
            "",
            from,
            0,
            TOPIC_HISTORY_PAGE_SIZE,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Own membership probe for the open broadcast channel: `getMe` once, then
    /// `getChatMember`. Drives the composer gate and the join/leave affordance
    /// (`ChannelMemberStatus`). No-op for non-channels.
    fn maybe_probe_channel_membership(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let Some(chat_id) = self.session.open_chat else {
            return Ok(());
        };
        if !self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.is_channel())
        {
            return Ok(());
        }
        let Some(my_id) = self.session.my_user_id else {
            if self.session.requests.has_purpose(RequestPurpose::GetMe) {
                return Ok(());
            }
            let extra = self.session.request(RequestPurpose::GetMe, None);
            return self
                .sender
                .send_json(&get_me(extra))
                .map(|_| ())
                .inspect_err(|_| {
                    self.session.requests.take(extra);
                });
        };
        if self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.my_member_status.is_some())
        {
            return Ok(());
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatMember, chat_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatMember, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_member(extra, chat_id, my_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Phase 3.1: lazy `getUserFullInfo` for the open bot chat. Fires once
    /// per uncached bot (deduped by cache + in-flight purpose); the
    /// `userFullInfo` response and `updateUserFullInfo` both populate
    /// `Session::bot_info`. No-op for non-bot chats.
    fn maybe_fetch_bot_info(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let Some(chat_id) = self.session.open_chat else {
            return Ok(());
        };
        let Some(user_id) = self.session.bot_user_id_for_chat(chat_id) else {
            return Ok(());
        };
        if self.session.bot_info.contains_key(&user_id) {
            return Ok(());
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetUserFullInfo, chat_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetUserFullInfo, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_user_full_info(extra, user_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Phase 3.3: `getCommands` for the open bot chat's global commands.
    /// A null scope selects the default scope (`botCommandScopeDefault`,
    /// schema 1.8.67 line 10360) with an empty language code. Fires once
    /// per bot (deduped by cache and in-flight purpose) alongside the
    /// `getUserFullInfo` fetch. The schema annotates `getCommands`
    /// "for bots only" (TDLib 1.8.67 line 14953), so a user session gets
    /// an `error` answer — recorded as an empty command set by
    /// `Session::apply`, with no retry on later chat selections; the `/`
    /// menu then shows the `botInfo` commands only.
    fn maybe_fetch_bot_commands(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let Some(chat_id) = self.session.open_chat else {
            return Ok(());
        };
        let Some(user_id) = self.session.bot_user_id_for_chat(chat_id) else {
            return Ok(());
        };
        if self.session.bot_commands.contains_key(&user_id) {
            return Ok(());
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetCommands, chat_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetCommands, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_commands(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// `joinChat` for a public channel. Own membership updates arrive via
    /// `updateChatMember`; the response also flips status optimistically.
    pub fn join_channel(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::JoinChat, Some(chat_id));
        if let Err(err) = self.sender.send_json(&join_chat(extra, chat_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// `leaveChat` for a channel. Own membership updates arrive via
    /// `updateChatMember`; the `ok` response flips status optimistically.
    pub fn leave_channel(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::LeaveChat, Some(chat_id));
        if let Err(err) = self.sender.send_json(&leave_chat(extra, chat_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Phase B1: `createNewSecretChat` for a user. Gated on a known
    /// non-bot user — the affordance lives on the user profile panel, and
    /// secret chats are 1:1 E2E sessions (bots are cloud-side actors).
    /// The new chat arrives as `updateNewChat` (chatTypeSecret); its state
    /// arrives as `updateSecretChat`.
    pub fn start_secret_chat(&mut self, user_id: i64) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let user = self.session.user(user_id);
        let is_bot = user.is_some_and(|u| u.is_bot);
        let is_self = self.session.my_user_id.is_some_and(|me| me == user_id);
        if user.is_none() || is_bot || is_self {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::CreateNewSecretChat, None);
        if let Err(err) = self
            .sender
            .send_json(&create_new_secret_chat(extra, user_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C1: `createCall` for a user. Gated on a known non-bot
    /// user (like `createNewSecretChat`) and on no call already being
    /// active. Phase C1b: `is_video: true` starts video-call
    /// *signaling* — media transport is still Phase C2, so the call
    /// carries no audio or video; the UI says so. The `callId` answer
    /// starts tracking the outgoing call; its states arrive as
    /// `updateCall`.
    pub fn start_call(
        &mut self,
        user_id: i64,
        is_video: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.active_call.is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let user = self.session.user(user_id);
        let is_bot = user.is_some_and(|u| u.is_bot);
        let is_self = self.session.my_user_id.is_some_and(|me| me == user_id);
        if user.is_none() || is_bot || is_self {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::CreateCall { is_video }, user_id);
        let protocol = self.engine_protocol_json();
        if let Err(err) = self.sender.send_json(&create_call_with_protocol(
            extra, user_id, is_video, &protocol,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C1: `acceptCall` for the tracked incoming call. Requires an
    /// incoming `Pending` call — answering a call that already moved on
    /// is rejected here, not sent.
    pub fn accept_call(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let call_id = match &self.session.active_call {
            Some(call) if !call.is_outgoing && matches!(call.state, CallState::Pending { .. }) => {
                call.id
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(RequestPurpose::AcceptCall, None);
        let protocol = self.engine_protocol_json();
        if let Err(err) = self
            .sender
            .send_json(&accept_call_with_protocol(extra, call_id, &protocol))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(engine) = self.call_engine.as_deref_mut() {
            // TDLib remains the call-state source of truth; this only marks
            // the already-tracked engine-side call accepted.
            let _ = engine.accept_call(call_id);
        }
        Ok(extra)
    }

    /// Phase C1: `discardCall` for the tracked call (decline an
    /// incoming call / hang up an active one). Marks the call
    /// `HangingUp` optimistically; the discard states arrive as
    /// `updateCall`. `duration` is the connected time in seconds.
    pub fn discard_call(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (call_id, duration_secs, is_video) = match &self.session.active_call {
            Some(call) => (call.id, call.connected_secs() as i32, call.is_video),
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(RequestPurpose::DiscardCall, None);
        if let Err(err) = self.sender.send_json(&discard_call_request(
            extra,
            call_id,
            false,
            duration_secs,
            is_video,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(call) = self.session.active_call.as_mut() {
            call.state = CallState::HangingUp;
        }
        if let Some(engine) = self.call_engine.as_deref_mut() {
            // Prompt teardown; the later terminal updateCall repeats this
            // idempotently through `pump_call_engine`.
            let _ = engine.hangup(call_id);
        }
        Ok(extra)
    }

    /// Phase C2c: real mute through the native engine. When an available
    /// engine is installed and the transport exists, the engine is
    /// called first and its error propagates *without* flipping the
    /// session flag; otherwise the flag is stored (it applies to the
    /// transport on connect).
    pub fn set_call_muted(&mut self, muted: bool) -> Result<(), EngineError> {
        let Some(call) = self.session.active_call.as_mut() else {
            return Err(EngineError::NoActiveCall);
        };
        if call.transport.is_some()
            && let Some(engine) = self.call_engine.as_deref_mut()
            && engine.is_available()
        {
            // Engine first: a failed native call must not flip the flag.
            engine.set_muted(call.id, muted)?;
        }
        call.muted = muted;
        Ok(())
    }

    /// Phase C2c: pick the (microphone, speaker) device ids. `None`
    /// means the engine default. The pair is always stored; it is
    /// forwarded to the native engine only when a transport is already
    /// connected, and a failed forward propagates before the stored
    /// selection changes.
    pub fn select_call_devices(
        &mut self,
        mic: Option<String>,
        speaker: Option<String>,
    ) -> Result<(), EngineError> {
        let selection = (mic, speaker);
        if let Some(call) = self.session.active_call.as_ref()
            && call.transport.is_some()
            && let Some(engine) = self.call_engine.as_deref_mut()
        {
            engine.select_devices(call.id, selection.0.as_deref(), selection.1.as_deref())?;
        }
        self.selected_devices = selection;
        Ok(())
    }

    /// Phase C1: `sendCallRating` for the last ended call (the 1–5
    /// rating card, `callStateDiscarded.need_rating`). Marks the summary
    /// so the card can show "Thanks" while the `ok` confirms.
    pub fn send_call_rating(&mut self, rating: i32) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let call_id = match &self.session.call_summary {
            Some(summary) if summary.need_rating && !summary.rating_sent => summary.call_id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let rating = rating.clamp(1, 5);
        let extra = self.session.request(RequestPurpose::SendCallRating, None);
        if let Err(err) = self
            .sender
            .send_json(&send_call_rating(extra, call_id, rating))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(summary) = self.session.call_summary.as_mut() {
            summary.rating_sent = true;
        }
        Ok(extra)
    }

    fn call_debug_information(&self) -> Result<String, ConnectSendError> {
        let summary = self
            .session
            .call_summary
            .as_ref()
            .filter(|summary| summary.need_debug_information && !summary.debug_information_sent)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let engine_available = self
            .call_engine
            .as_ref()
            .is_some_and(|engine| engine.is_available());
        let transport = summary.final_transport.map(|state| match state {
            TransportState::Connecting => "connecting",
            TransportState::Reconnecting => "reconnecting",
            TransportState::Connected => "connected",
            TransportState::Failed => "failed",
            TransportState::Closed => "closed",
        });
        let mut payload = serde_json::json!({
            "app": env!("CARGO_PKG_NAME"),
            "app_version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
            "engine_available": engine_available,
            "call_id": summary.call_id,
            "duration_secs": summary.duration_secs,
            "had_audio": summary.had_audio,
            "final_transport_state": transport,
            "reconnect_attempts": summary.reconnect_attempts,
            "muted": summary.muted,
            "microphone_device_id": self.selected_devices.0,
            "speaker_device_id": self.selected_devices.1,
        });
        if engine_available {
            let protocol = self
                .call_engine
                .as_ref()
                .expect("available engine")
                .protocol();
            payload["engine_protocol"] = serde_json::json!({
                "udp_p2p": protocol.udp_p2p,
                "udp_reflector": protocol.udp_reflector,
                "min_layer": protocol.min_layer,
                "max_layer": protocol.max_layer,
                "library_versions": protocol.library_versions,
            });
        }
        Ok(payload.to_string())
    }

    /// Phase C2d: upload real local call diagnostics for the last discarded
    /// call (`schema/td_api.tl:14237`).
    pub fn send_call_debug_information(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let call_id = self
            .session
            .call_summary
            .as_ref()
            .map(|summary| summary.call_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let debug_information = self.call_debug_information()?;
        let extra = self
            .session
            .request(RequestPurpose::SendCallDebugInformation, None);
        if let Err(err) = self.sender.send_json(&send_call_debug_information(
            extra,
            call_id,
            &debug_information,
        )) {
            self.session.requests.take(extra);
            if let Some(summary) = self.session.call_summary.as_mut() {
                summary.debug_information_error = Some(match err {
                    ConnectSendError::InvalidRequest => {
                        "Could not upload diagnostics: invalid request".into()
                    }
                    ConnectSendError::Native => {
                        "Could not upload diagnostics: TDLib send failed".into()
                    }
                });
            }
            return Err(err);
        }
        if let Some(summary) = self.session.call_summary.as_mut() {
            summary.debug_information_sent = true;
            summary.debug_information_error = None;
        }
        Ok(extra)
    }

    /// Phase C3a / C2h: `createVideoChat` — start a voice chat on a
    /// group or channel (schema 1.8.67, :14256). Signaling only: the
    /// chat-bound creation path. The `groupCallId` answer queues a
    /// `getGroupCall` fetch; live state arrives as `updateGroupCall`.
    /// `start_date`: Unix timestamp, 0 = start immediately; otherwise
    /// at least 10s and at most 8 days in the future (schema). Empty
    /// title falls back to the chat title (schema).
    pub fn start_video_chat(
        &mut self,
        chat_id: i64,
        title: String,
        start_date: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_group_or_channel = self.session.chats.get(&chat_id).is_some_and(|chat| {
            matches!(
                chat.kind,
                ChatKind::BasicGroup { .. } | ChatKind::Supergroup { .. }
            )
        });
        if !is_group_or_channel {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.active_group_call.is_some() || self.session.active_call.is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let title = title.trim().to_string();
        if title.chars().count() > 64 {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Schema :14256 — scheduled start must be ≥10s and ≤8d out.
        let start_date = if start_date == 0 {
            0
        } else {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if !(now + 10..=now + 8 * 86400).contains(&start_date) {
                return Err(ConnectSendError::InvalidRequest);
            }
            start_date.min(i32::MAX as i64) as i32
        };
        let extra = self
            .session
            .request(RequestPurpose::CreateVideoChat { chat_id }, None);
        if let Err(err) = self.sender.send_json(&create_video_chat(
            extra, chat_id, &title, start_date, false,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `joinVideoChat` for a chat-bound voice chat (schema
    /// 1.8.67, :14292). Joins as self; the TDLib `Text` answer is stored
    /// on the tracked call and consumed by the driver pump to finish the
    /// native handshake.
    /// Phase C2g: the native group transport is created first so the
    /// join carries the real tgcalls offer (`ntg_create_call`) as its
    /// payload, with `audio_source_id` parsed from the offer SDP. When
    /// no engine is available (or the offer fails), the join still goes
    /// out with the honest no-device params — signaling-only, as before.
    pub fn join_video_chat(&mut self, group_call_id: i32) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Never join while a 1:1 call is active. A tracked group call is
        // fine to join when it is the same call and not yet joined (the
        // normal flow: getGroupCall creates the unjoined tracker, then the
        // overlay's Join button calls this). Reject a different tracked call
        // or one already joined.
        if self.session.active_call.is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_muted_self = match &self.session.active_group_call {
            Some(call) if call.id == group_call_id && !call.is_joined => call.is_muted_self,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        // Phase C2g: the native group context is created inside
        // `group_join_params` so the join carries the real tgcalls offer.
        let params = self.group_join_params(group_call_id, is_muted_self);
        let extra = self
            .session
            .request(RequestPurpose::JoinVideoChat { group_call_id }, None);
        if let Err(err) =
            self.sender
                .send_json(&join_video_chat(extra, group_call_id, None, &params, ""))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `getGroupCall` for a known call id (schema 1.8.67,
    /// :14274). Used to start tracking a voice chat found via a chat's
    /// `video_chat` affordance.
    pub fn fetch_group_call(&mut self, group_call_id: i32) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetGroupCall { group_call_id }, None);
        if let Err(err) = self.sender.send_json(&get_group_call(extra, group_call_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: drain `Session::group_call_fetch_queue` — `getGroupCall`
    /// for freshly created voice chats. Called from `ingest`.
    fn maybe_fetch_group_calls(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let queued: Vec<i32> = std::mem::take(&mut self.session.group_call_fetch_queue);
        for group_call_id in queued {
            if let Err(err) = self.fetch_group_call(group_call_id) {
                // Best-effort: re-queue for the next ingest tick; the
                // `updateGroupCall` backstop still tracks the call.
                let _ = err;
                self.session.group_call_fetch_queue.push(group_call_id);
            }
        }
        Ok(())
    }

    /// Phase C2f: rejoin after `need_rejoin` (schema 1.8.67, line 7154
    /// docs: "user was kicked from the call because of network loss and
    /// the call needs to be rejoined"). Same attempt discipline as the
    /// C2d 1:1 reconnect: at most 3 attempts with identical join params
    /// (current self-mute state, honest no-device). A failed attempt
    /// re-arms `reconnecting` via the `JoinVideoChat` error arm so the
    /// driver's auto-rejoin retries; `manual` (the UI Rejoin button)
    /// resets the counter — explicit user intent starts the attempts
    /// over.
    pub fn rejoin_group_call(&mut self, manual: bool) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if manual && let Some(call) = self.session.active_group_call.as_mut() {
            call.rejoin_attempts = 0;
        }
        let (group_call_id, is_muted) = match &self.session.active_group_call {
            Some(call) if call.reconnecting && call.rejoin_attempts < 3 => {
                (call.id, call.is_muted_self)
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self
            .session
            .request(RequestPurpose::JoinVideoChat { group_call_id }, None);
        // Review fix: the new native context starts unconnected, and
        // `create_group_call` below replaces the old media entry — a
        // live presentation would be orphaned (still capturing on the
        // native side). Snapshot it before the entry is replaced.
        let orphaned_presentation = self
            .call_engine
            .as_ref()
            .is_some_and(|engine| engine.presentation_active(group_call_id));
        let params = self.group_join_params(group_call_id, is_muted);
        if let Err(err) =
            self.sender
                .send_json(&join_video_chat(extra, group_call_id, None, &params, ""))
        {
            self.session.requests.take(extra);
            // The attempt never went out: re-arm so the next ingest
            // retries instead of stranding the call.
            if let Some(call) = self.session.active_group_call.as_mut() {
                call.reconnecting = true;
            }
            return Err(err);
        }
        if orphaned_presentation && let Some(engine) = self.call_engine.as_deref_mut() {
            let _ = engine.stop_screen_share(group_call_id);
        }
        if let Some(call) = self.session.active_group_call.as_mut() {
            call.rejoin_attempts += 1;
            call.reconnecting = false;
            // Review fix: the fresh native context must run the join
            // handshake again — reset the transport gate, the stale
            // answer, and the stale screen-share state so the pump
            // doesn't drop the new `joinVideoChat` answer.
            call.transport_ready = false;
            call.join_payload.clear();
            call.screen_sharing = false;
            call.screen_share_pending = false;
            call.screen_share_answer.clear();
        }
        Ok(extra)
    }

    /// Phase C2f: auto-rejoin a dropped group call (`need_rejoin`) —
    /// one attempt per ingest tick while the tracked call still wants
    /// reconnecting and attempts remain (the `rejoin_group_call`
    /// guard caps at 3, the C2d discipline).
    fn maybe_auto_rejoin_group_call(&mut self) -> Result<(), ConnectSendError> {
        let wants = self
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.reconnecting && call.rejoin_attempts < 3);
        if wants {
            let _ = self.rejoin_group_call(false);
        }
        Ok(())
    }

    /// Phase C2g: build `joinVideoChat` params for a (re)join. Creates
    /// the native group context first so the payload is the real tgcalls
    /// offer; degrades to the honest no-device params when no engine is
    /// available or the offer fails (the join is never blocked).
    fn group_join_params(&mut self, group_call_id: i32, is_muted: bool) -> GroupCallJoinParams {
        let is_my_video_enabled = self
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.is_my_video_enabled);
        // The native call key is the chat id; resolve it through the
        // chat's `video_chat` association (schema `groupCall` carries no
        // chat id).
        let chat_id = self
            .session
            .chats
            .values()
            .find(|chat| {
                chat.video_chat
                    .as_ref()
                    .is_some_and(|video_chat| video_chat.group_call_id == group_call_id)
            })
            .map(|chat| chat.id.0);
        let offer = match (chat_id, self.call_engine.as_deref_mut()) {
            (Some(chat_id), Some(engine)) if engine.is_available() => {
                Some(engine.create_group_call(group_call_id, chat_id))
            }
            _ => None,
        };
        match offer {
            Some(Ok(payload)) => GroupCallJoinParams {
                audio_source_id: group_offer_audio_source_id(&payload),
                payload,
                is_muted,
                is_my_video_enabled,
            },
            Some(Err(err)) => {
                if let Some(call) = self.session.active_group_call.as_mut() {
                    call.transport_error = Some(err.to_string());
                }
                let mut params = GroupCallJoinParams::honest_no_device();
                params.is_muted = is_muted;
                params.is_my_video_enabled = is_my_video_enabled;
                params
            }
            None => {
                let mut params = GroupCallJoinParams::honest_no_device();
                params.is_muted = is_muted;
                params.is_my_video_enabled = is_my_video_enabled;
                params
            }
        }
    }

    /// Phase C3a: `leaveGroupCall` (schema 1.8.67, :14458). Drops the
    /// tracked call; the `ok` confirms (state also handles the
    /// `!is_active` backstop).
    pub fn leave_group_call(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self
            .session
            .request(RequestPurpose::LeaveGroupCall { group_call_id }, None);
        if let Err(err) = self
            .sender
            .send_json(&leave_group_call(extra, group_call_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Phase C2g: tear the native group transport down eagerly; the
        // pump's before/after backstop covers server-driven ends.
        self.teardown_group_call_transport(group_call_id);
        Ok(extra)
    }

    /// Phase C2g: drop the native transport and retained frames for one
    /// group call id. Unknown ids succeed (idempotent cleanup).
    fn teardown_group_call_transport(&mut self, group_call_id: i32) {
        if let Some(engine) = self.call_engine.as_deref_mut() {
            let _ = engine.leave_group_call(group_call_id);
        }
        self.group_video_frame_slots
            .lock()
            .expect("group video frame slots")
            .retain(|(slot_call_id, _, _), _| *slot_call_id != group_call_id);
        self.group_camera_state.remove(&group_call_id);
    }

    /// Phase C3a: `endGroupCall` (schema 1.8.67, :14461). Gated on
    /// `groupCall.can_be_managed` (video chats).
    pub fn end_group_call(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.can_be_managed => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self
            .session
            .request(RequestPurpose::EndGroupCall { group_call_id }, None);
        if let Err(err) = self.sender.send_json(&end_group_call(extra, group_call_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Phase C2g: same native teardown as leaving; the server-driven
        // end also lands via the pump backstop.
        self.teardown_group_call_transport(group_call_id);
        Ok(extra)
    }

    /// Phase C2g: start or stop screen sharing in the tracked group
    /// call. Starting goes through the native presentation handshake:
    /// `ntg_init_presentation` yields the offer that
    /// `startGroupCallScreenSharing` (schema 1.8.67, :14303) carries;
    /// its `Text` answer is consumed by the driver pump
    /// (`ntg_connect(..., is_presentation=true)` + desktop capture).
    /// Stopping pairs `endGroupCallScreenSharing` (:14309) with
    /// `ntg_stop_presentation`. Requires the joined call and an
    /// available engine; without one the toggle is rejected.
    pub fn toggle_group_call_screen_share(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.is_joined => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let engine_available = self
            .call_engine
            .as_ref()
            .is_some_and(|engine| engine.is_available());
        if !engine_available {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Honest gate: the presentation handshake needs a screen-capture
        // source, and the native engine enumerates them via
        // `media_devices` (`MediaDeviceKind::Screen`).
        if !self.group_call_screen_source_available() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let sharing = self
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.screen_sharing || call.screen_share_pending);
        if sharing {
            let extra = self.session.request(
                RequestPurpose::EndGroupCallScreenSharing { group_call_id },
                None,
            );
            if let Err(err) = self
                .sender
                .send_json(&end_group_call_screen_sharing(extra, group_call_id))
            {
                self.session.requests.take(extra);
                return Err(err);
            }
            if let Some(engine) = self.call_engine.as_deref_mut()
                && let Err(err) = engine.stop_screen_share(group_call_id)
                && let Some(call) = self.session.active_group_call.as_mut()
            {
                call.transport_error = Some(err.to_string());
            }
            if let Some(call) = self.session.active_group_call.as_mut() {
                call.screen_sharing = false;
                call.screen_share_pending = false;
                call.screen_share_answer.clear();
            }
            return Ok(extra);
        }
        let offer = match self
            .call_engine
            .as_deref_mut()
            .expect("available engine")
            .start_screen_share(group_call_id)
        {
            Ok(offer) => offer,
            Err(err) => {
                if let Some(call) = self.session.active_group_call.as_mut() {
                    call.transport_error = Some(err.to_string());
                }
                return Err(ConnectSendError::InvalidRequest);
            }
        };
        let extra = self.session.request(
            RequestPurpose::StartGroupCallScreenSharing { group_call_id },
            None,
        );
        if let Err(err) = self.sender.send_json(&start_group_call_screen_sharing(
            extra,
            group_call_id,
            &offer,
        )) {
            self.session.requests.take(extra);
            if let Some(engine) = self.call_engine.as_deref_mut() {
                let _ = engine.stop_screen_share(group_call_id);
            }
            return Err(err);
        }
        if let Some(call) = self.session.active_group_call.as_mut() {
            call.screen_share_pending = true;
        }
        Ok(extra)
    }

    /// Phase C3a: local-only self mute toggle. There is no TDLib "mute
    /// self" for group calls outside the join parameters — the UI labels
    /// this honestly as local-only; the state rides on the next (re)join.
    pub fn toggle_group_call_self_mute(&mut self) {
        let muted = !self
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|c| c.is_muted_self);
        self.session.set_group_call_self_muted(muted);
    }

    /// Phase C3a: `toggleGroupCallIsMyVideoEnabled` (schema 1.8.67,
    /// :14414). Tracks the TDLib flag; Phase C2g applies it to the
    /// native transport in the driver pump (`set_group_camera`).
    pub fn toggle_group_call_my_video(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, enable) = match &self.session.active_group_call {
            Some(call) => (call.id, !call.is_my_video_enabled),
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self
            .session
            .request(RequestPurpose::ToggleGroupCallVideo { group_call_id }, None);
        if let Err(err) = self
            .sender
            .send_json(&toggle_group_call_is_my_video_enabled(
                extra,
                group_call_id,
                enable,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `toggleGroupCallIsMyVideoPaused` (schema 1.8.67,
    /// :14411). Signaling-only: tracks state, no camera (Phase C2).
    pub fn toggle_group_call_my_video_paused(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, pause) = match &self.session.active_group_call {
            Some(call) if call.is_my_video_enabled => (call.id, !call.is_my_video_paused),
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self
            .session
            .request(RequestPurpose::ToggleGroupCallVideo { group_call_id }, None);
        if let Err(err) = self.sender.send_json(&toggle_group_call_is_my_video_paused(
            extra,
            group_call_id,
            pause,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `toggleGroupCallParticipantIsMuted` (schema 1.8.67,
    /// :14431). The caller gates on the participant's
    /// `can_be_muted_for_all_users` / `can_be_unmuted_for_all_users`.
    pub fn toggle_group_call_participant_muted(
        &mut self,
        participant_id: MessageSender,
        mute: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let sender_ref = match participant_id {
            MessageSender::User { user_id } => MessageSenderRef::User(user_id),
            MessageSender::Chat { chat_id } => MessageSenderRef::Chat(chat_id),
        };
        let extra = self.session.request(
            RequestPurpose::ToggleGroupCallParticipantMute { group_call_id },
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&toggle_group_call_participant_is_muted(
                extra,
                group_call_id,
                &sender_ref,
                mute,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `toggleGroupCallParticipantIsHandRaised` (schema
    /// 1.8.67, :14444). Only the self hand can be raised; lowering
    /// others' hands requires `groupCall.can_be_managed` (gated by the
    /// caller).
    pub fn toggle_group_call_participant_hand(
        &mut self,
        participant_id: MessageSender,
        raise: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let sender_ref = match participant_id {
            MessageSender::User { user_id } => MessageSenderRef::User(user_id),
            MessageSender::Chat { chat_id } => MessageSenderRef::Chat(chat_id),
        };
        let extra = self.session.request(
            RequestPurpose::ToggleGroupCallParticipantHand { group_call_id },
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&toggle_group_call_participant_is_hand_raised(
                extra,
                group_call_id,
                &sender_ref,
                raise,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2f: `inviteGroupCallParticipant` (schema 1.8.67,
    /// :14375). `is_video` follows the tracked call's `is_video_chat`.
    pub fn invite_group_call_participant(
        &mut self,
        user_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, is_video) = match &self.session.active_group_call {
            Some(call) => (call.id, call.is_video_chat),
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::InviteGroupCallParticipant { group_call_id },
            None,
        );
        if let Err(err) = self.sender.send_json(&invite_group_call_participant(
            extra,
            group_call_id,
            user_id,
            is_video,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2f: `banGroupCallParticipants` (schema 1.8.67, :14385)
    /// for a single participant. Takes `user_ids` (int64 user ids —
    /// `messageSenderChat` participants cannot be banned); requires
    /// `groupCall.is_owned` — the owner can ban, not `can_be_managed`
    /// admins (that's "for video chats and live stories only").
    pub fn ban_group_call_participant(
        &mut self,
        user_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.is_owned => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::BanGroupCallParticipants { group_call_id },
            None,
        );
        if let Err(err) = self.sender.send_json(&ban_group_call_participants(
            extra,
            group_call_id,
            &[user_id],
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2f: `setGroupCallParticipantVolumeLevel` (schema 1.8.67,
    /// :14438). Clamps to the schema's 1-20000 (hundreds of percents)
    /// before sending.
    pub fn set_group_call_participant_volume(
        &mut self,
        participant_id: MessageSender,
        volume_level: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let sender_ref = match participant_id {
            MessageSender::User { user_id } => MessageSenderRef::User(user_id),
            MessageSender::Chat { chat_id } => MessageSenderRef::Chat(chat_id),
        };
        let extra = self.session.request(
            RequestPurpose::SetGroupCallParticipantVolumeLevel { group_call_id },
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&set_group_call_participant_volume_level(
                extra,
                group_call_id,
                &sender_ref,
                volume_level.clamp(1, 20000),
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2f: accept a `messageGroupCall` invitation via
    /// `joinGroupCall` (schema 1.8.67, line 5288: "Use joinGroupCall
    /// to accept the call"). Refuses while a 1:1 call is active, like
    /// the chat-bound join; the joined call is tracked via
    /// `updateGroupCall`.
    pub fn accept_group_call_invitation(
        &mut self,
        chat_id: i64,
        message_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.active_call.is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::JoinGroupCallInvitation, None);
        let input = InputGroupCallRef::Message {
            chat_id,
            message_id,
        };
        let params = GroupCallJoinParams::honest_no_device();
        if let Err(err) = self
            .sender
            .send_json(&join_group_call(extra, &input, &params))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2f: `declineGroupCallInvitation` (schema 1.8.67,
    /// :14380) — declines (or cancels, for the sender) a
    /// `messageGroupCall` invitation.
    pub fn decline_group_call_invitation(
        &mut self,
        chat_id: i64,
        message_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::DeclineGroupCallInvitation {
                chat_id,
                message_id,
            },
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&decline_group_call_invitation(extra, chat_id, message_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `toggleVideoChatMuteNewParticipants` (schema 1.8.67,
    /// :14317). Gated on `groupCall.can_toggle_mute_new_participants`.
    pub fn toggle_video_chat_mute_new(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, mute_new) = match &self.session.active_group_call {
            Some(call) if call.can_toggle_mute_new_participants => {
                (call.id, !call.mute_new_participants)
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::ToggleVideoChatMuteNew { group_call_id },
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&toggle_video_chat_mute_new_participants(
                extra,
                group_call_id,
                mute_new,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `setVideoChatTitle` (schema 1.8.67, :14312). Gated on
    /// `groupCall.can_be_managed`.
    pub fn set_video_chat_title(&mut self, title: String) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.can_be_managed => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let title = title.trim().to_string();
        if title.is_empty() || title.chars().count() > 64 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetVideoChatTitle { group_call_id }, None);
        if let Err(err) = self
            .sender
            .send_json(&set_video_chat_title(extra, group_call_id, &title))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `getVideoChatInviteLink` (schema 1.8.67, :14395). The
    /// `HttpUrl` answer is stored on the tracked call for the UI to
    /// show. `can_self_unmute: true` requires `can_be_managed` — the
    /// caller passes `call.can_be_managed`.
    pub fn fetch_video_chat_invite_link(
        &mut self,
        can_self_unmute: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::GetVideoChatInviteLink { group_call_id },
            None,
        );
        if let Err(err) = self.sender.send_json(&get_video_chat_invite_link(
            extra,
            group_call_id,
            can_self_unmute,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `revokeGroupCallInviteLink` (schema 1.8.67,
    /// :14396). Gated on `groupCall.can_be_managed` (video chats).
    pub fn revoke_video_chat_invite_link(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.can_be_managed => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::RevokeVideoChatInviteLink { group_call_id },
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&revoke_group_call_invite_link(extra, group_call_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `startGroupCallRecording` (schema 1.8.67, :14400).
    /// Gated on `groupCall.can_be_managed` and `is_video_chat`
    /// (schema: "for video chats only"). Recording state arrives as
    /// `updateGroupCall` (`record_duration` / `is_video_recorded`).
    pub fn start_group_call_recording(
        &mut self,
        title: String,
        record_video: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.can_be_managed && call.is_video_chat => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let title = title.trim().to_string();
        if title.chars().count() > 64 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::StartGroupCallRecording { group_call_id },
            None,
        );
        if let Err(err) = self.sender.send_json(&start_group_call_recording(
            extra,
            group_call_id,
            &title,
            record_video,
            false,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `endGroupCallRecording` (schema 1.8.67, :14407).
    /// Gated on `groupCall.can_be_managed`.
    pub fn stop_group_call_recording(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.can_be_managed => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::EndGroupCallRecording { group_call_id },
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&end_group_call_recording(extra, group_call_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `getVideoChatRtmpUrl` (schema 1.8.67, :14261) — the
    /// request is chat-bound, so resolve the chat from the tracked
    /// call. Gated on `groupCall.can_be_managed` (the schema's
    /// `can_manage_video_chats` admin right is the closest tracked
    /// flag; a 403 surfaces honestly via `group_call_error`).
    pub fn fetch_video_chat_rtmp_url(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let call = match &self.session.active_group_call {
            Some(call) if call.can_be_managed => call,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let call_id = call.id;
        let chat_id = self
            .session
            .chats
            .iter()
            .find(|(_, c)| {
                c.video_chat
                    .as_ref()
                    .is_some_and(|vc| vc.group_call_id == call_id)
            })
            .map(|(id, _)| *id);
        let Some(chat_id) = chat_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let extra = self
            .session
            .request(RequestPurpose::GetVideoChatRtmpUrl { chat_id }, None);
        if let Err(err) = self
            .sender
            .send_json(&get_video_chat_rtmp_url(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `replaceVideoChatRtmpUrl` (schema 1.8.67, :14264) —
    /// regenerates the RTMP URL + stream key. Requires owner
    /// privileges; `groupCall.is_owned` is the closest tracked flag
    /// and a 403 surfaces honestly.
    pub fn replace_video_chat_rtmp_url(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let call = match &self.session.active_group_call {
            Some(call) if call.is_owned => call,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let call_id = call.id;
        let chat_id = self
            .session
            .chats
            .iter()
            .find(|(_, c)| {
                c.video_chat
                    .as_ref()
                    .is_some_and(|vc| vc.group_call_id == call_id)
            })
            .map(|(id, _)| *id);
        let Some(chat_id) = chat_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let extra = self
            .session
            .request(RequestPurpose::ReplaceVideoChatRtmpUrl { chat_id }, None);
        if let Err(err) = self
            .sender
            .send_json(&replace_video_chat_rtmp_url(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `sendGroupCallMessage` (schema 1.8.67, :14335).
    /// Gated on `groupCall.can_send_messages` and
    /// `are_messages_allowed`. The echo arrives as
    /// `updateNewGroupCallMessage`; there is no history getter, so
    /// the UI shows the live feed only.
    pub fn send_group_call_message(&mut self, text: String) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.can_send_messages && call.are_messages_allowed => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let text = text.trim().to_string();
        // ponytail: the true cap is getOption
        // "group_call_message_text_length_max" (server-enforced);
        // 4096 chars is just a client-side sanity guard.
        if text.is_empty() || text.chars().count() > 4096 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SendGroupCallMessage { group_call_id }, None);
        if let Err(err) =
            self.sender
                .send_json(&send_group_call_message(extra, group_call_id, &text))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `toggleGroupCallAreMessagesAllowed` (schema 1.8.67,
    /// :14319). Gated on `can_toggle_are_messages_allowed`; flips the
    /// current `are_messages_allowed`.
    pub fn toggle_group_call_are_messages_allowed(
        &mut self,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, new_value) = match &self.session.active_group_call {
            Some(call) if call.can_toggle_are_messages_allowed => {
                (call.id, !call.are_messages_allowed)
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::ToggleGroupCallAreMessagesAllowed { group_call_id },
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&toggle_group_call_are_messages_allowed(
                extra,
                group_call_id,
                new_value,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `loadGroupCallParticipants` (schema 1.8.67, :14455)
    /// — page more participants (up to 100). Gated on
    /// `!loaded_all_participants`.
    pub fn load_more_group_call_participants(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) if !call.loaded_all_participants => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::LoadGroupCallParticipants { group_call_id },
            None,
        );
        if let Err(err) =
            self.sender
                .send_json(&load_group_call_participants(extra, group_call_id, 100))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C1: drain `Session::call_busy_decline_queue` — incoming
    /// calls that arrived while another call was active are declined
    /// (busy) with `discardCall`. Called from `ingest`.
    fn maybe_decline_busy_calls(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let queued: Vec<(i32, bool)> = std::mem::take(&mut self.session.call_busy_decline_queue);
        for (call_id, is_video) in queued {
            let extra = self.session.request(RequestPurpose::DiscardCall, None);
            if let Err(err) = self
                .sender
                .send_json(&discard_call_request(extra, call_id, false, 0, is_video))
            {
                self.session.requests.take(extra);
                return Err(err);
            }
        }
        Ok(())
    }

    /// Phase B1: `closeSecretChat` for a secret chat. The state change to
    /// `secretChatStateClosed` arrives as `updateSecretChat`; the composer
    /// hides then (the chat can never send again).
    pub fn close_secret_chat(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let secret_chat_id = self
            .session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| chat.secret_chat_id());
        let Some(secret_chat_id) = secret_chat_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let extra = self
            .session
            .request_for_secret_chat(RequestPurpose::CloseSecretChat, secret_chat_id);
        if let Err(err) = self
            .sender
            .send_json(&close_secret_chat_request(extra, secret_chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase B4: `setChatMessageAutoDeleteTime` (TDLib 1.8.67,
    /// `schema/td_api.tl:13454`) — the chat-level auto-delete or
    /// self-destruct (secret chats) timer. Value rule from the schema
    /// comment, enforced here (defense in depth — TDLib would 400 an
    /// out-of-rule value): secret chats accept any non-negative second
    /// value; other chats need 0 or a multiple of 86400 up to
    /// 365 * 86400. The new value arrives as
    /// `updateChatMessageAutoDeleteTime` (plus the service message in
    /// history); there is no optimistic state change.
    pub fn set_chat_message_auto_delete_time(
        &mut self,
        chat_id: ChatId,
        message_auto_delete_time: i32,
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
        let is_secret = matches!(chat.kind, ChatKind::Secret { .. });
        let valid = if message_auto_delete_time < 0 {
            false
        } else if is_secret {
            true
        } else {
            message_auto_delete_time == 0
                || (message_auto_delete_time % 86_400 == 0
                    && message_auto_delete_time <= 365 * 86_400)
        };
        if !valid {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetChatMessageAutoDeleteTime, Some(chat_id));
        let json = set_chat_message_auto_delete_time(extra, chat_id.0, message_auto_delete_time);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase B1: drain `Session::secret_chat_fetch_queue` — one
    /// `getSecretChat` (an offline method) per unknown secret-chat state,
    /// deduped against in-flight fetches. Called from `ingest`.
    fn maybe_fetch_secret_chat_states(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let queued: Vec<i32> = std::mem::take(&mut self.session.secret_chat_fetch_queue);
        for secret_chat_id in queued {
            if self
                .session
                .secret_chat_states
                .contains_key(&secret_chat_id)
            {
                continue;
            }
            let in_flight = self
                .session
                .requests
                .has_pending_for_secret_chat(RequestPurpose::GetSecretChat, secret_chat_id);
            if in_flight {
                self.session.secret_chat_fetch_queue.push(secret_chat_id);
                continue;
            }
            let extra = self
                .session
                .request_for_secret_chat(RequestPurpose::GetSecretChat, secret_chat_id);
            if let Err(err) = self
                .sender
                .send_json(&get_secret_chat(extra, secret_chat_id))
            {
                self.session.requests.take(extra);
                self.session.secret_chat_fetch_queue.push(secret_chat_id);
                return Err(err);
            }
        }
        Ok(())
    }

    /// Composer edit in a private chat. `delayed` follows tdesktop `saveDraft(true)`
    /// (1s quiet, 5s cap). `delayed == false` is Unigram's flush on leaving the chat.
    pub fn note_composer_draft(
        &mut self,
        chat_id: ChatId,
        text: &str,
        reply_to: Option<MessageId>,
        now_ms: u64,
        delayed: bool,
    ) -> Result<DraftSaveOutcome, ConnectSendError> {
        if !self.chats_path_active() || !self.session.accepts_composer_draft(chat_id) {
            return Ok(DraftSaveOutcome::Skipped);
        }
        let stored = draft_text_to_store(text, reply_to.is_some()).map(str::to_string);
        let reply_to = stored.as_ref().and(reply_to);
        if self.draft_matches(chat_id, stored.as_deref(), reply_to) {
            self.pending_draft = None;
            self.draft_clock = DraftSaveClock::idle();
            let existing = self
                .session
                .chats
                .get(&chat_id.0)
                .and_then(|chat| chat.draft.clone());
            self.session.store_composer_draft(chat_id, existing);
            return Ok(DraftSaveOutcome::Skipped);
        }
        self.session.mark_draft_dirty(chat_id);
        match schedule_draft_save(self.draft_clock, now_ms, delayed) {
            DraftSaveStep::Wait {
                delay_ms,
                started_ms,
            } => {
                self.draft_clock = DraftSaveClock {
                    started_ms: Some(started_ms),
                };
                self.draft_save_token = self.draft_save_token.saturating_add(1);
                let token = self.draft_save_token;
                self.pending_draft = Some(PendingDraft {
                    token,
                    chat_id,
                    text: stored.unwrap_or_default(),
                    reply_to,
                });
                Ok(DraftSaveOutcome::Debounced {
                    token,
                    delay: Duration::from_millis(delay_ms),
                })
            }
            DraftSaveStep::Write => {
                self.pending_draft = None;
                self.draft_clock = DraftSaveClock::idle();
                self.send_draft(chat_id, stored.as_deref(), reply_to)?;
                Ok(DraftSaveOutcome::Sent)
            }
        }
    }

    /// Timer fired. Sends only if `token` is still the latest quiet window.
    pub fn commit_debounced_draft(
        &mut self,
        token: u64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(pending) = self.pending_draft.take() else {
            return Ok(None);
        };
        if pending.token != token {
            self.pending_draft = Some(pending);
            return Ok(None);
        }
        self.draft_clock = DraftSaveClock::idle();
        let text = draft_text_to_store(&pending.text, pending.reply_to.is_some());
        let reply_to = text.and(pending.reply_to);
        if self.draft_matches(pending.chat_id, text, reply_to) {
            self.session.store_composer_draft(
                pending.chat_id,
                self.session
                    .chats
                    .get(&pending.chat_id.0)
                    .and_then(|chat| chat.draft.clone()),
            );
            return Ok(None);
        }
        self.send_draft(pending.chat_id, text, reply_to).map(Some)
    }

    /// Successful send: drop the draft when the composer is still idle.
    pub fn clear_draft_after_send(
        &mut self,
        chat_id: ChatId,
        composer_idle: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !composer_idle || self.session.draft_is_dirty(chat_id) {
            return Ok(None);
        }
        if !self.session.accepts_composer_draft(chat_id) {
            return Ok(None);
        }
        self.pending_draft = None;
        self.draft_clock = DraftSaveClock::idle();
        self.send_draft(chat_id, None, None).map(Some)
    }

    pub fn cancel_pending_draft(&mut self) {
        self.pending_draft = None;
        self.draft_clock = DraftSaveClock::idle();
    }

    fn draft_matches(
        &self,
        chat_id: ChatId,
        text: Option<&str>,
        reply_to: Option<MessageId>,
    ) -> bool {
        let current = self
            .session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| chat.draft.as_ref());
        match (current, text) {
            (None, None) => true,
            (Some(draft), Some(text)) => {
                draft.text == text && draft.reply_to_message_id == reply_to
            }
            _ => false,
        }
    }

    fn send_draft(
        &mut self,
        chat_id: ChatId,
        text: Option<&str>,
        reply_to: Option<MessageId>,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::SetChatDraftMessage, Some(chat_id));
        match self
            .sender
            .send_json(&set_chat_draft_message(extra, chat_id, text, reply_to))
        {
            Ok(()) => {
                let draft = text
                    .filter(|text| !text.trim().is_empty() || reply_to.is_some())
                    .map(|text| ChatDraft {
                        text: text.to_string(),
                        reply_to_message_id: reply_to,
                    });
                self.session.store_composer_draft(chat_id, draft);
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    fn close_open_chat(&mut self) -> Result<(), ConnectSendError> {
        let Some(prev) = self.session.open_chat else {
            return Ok(());
        };
        if !self
            .session
            .chats
            .get(&prev.0)
            .is_some_and(|chat| chat.supported())
        {
            return Ok(());
        }
        let extra = self.session.request(RequestPurpose::CloseChat, Some(prev));
        self.sender.send_json(&close_chat(extra, prev))?;
        Ok(())
    }

    fn send_open_chat(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::OpenChat, Some(chat_id));
        self.sender.send_json(&open_chat(extra, chat_id))?;
        Ok(extra)
    }

    /// `viewMessages` for loaded history in the open chat (TDLib 1.8.67).
    /// Unread counts change only when `updateChatReadInbox` arrives.
    pub fn maybe_view_open_messages(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(None);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Ok(None);
        };
        if !self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported())
        {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::ViewMessages, chat_id)
        {
            return Ok(None);
        }
        let ids = self.session.message_ids_to_view(chat_id);
        if ids.is_empty() {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::ViewMessages, Some(chat_id));
        match self
            .sender
            .send_json(&view_messages(extra, chat_id, &ids, true))
        {
            Ok(()) => {
                self.session.begin_viewing(chat_id, &ids);
                Ok(Some(extra))
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

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

    /// Parity slice: auto-download chat-list avatar photos
    /// (`chat.photo.small`, the cheap 160px thumbnail, for every chat type).
    /// Runs on every ingest; `should_download` dedupes in-flight and
    /// completed files, so each photo is requested at most once until it
    /// lands, and `updateChatPhoto` re-arms the new file id.
    pub fn maybe_download_chat_list_photos(&mut self) -> Result<Vec<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
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

    /// Send `downloadFile` (`synchronous: false`). No-op if already local or in flight.
    pub fn download_file(
        &mut self,
        file_id: FileId,
        priority: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.should_download(file_id) {
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

    /// Phase 6: open/close the contacts info panel. Live and demo sessions
    /// both keep the target in `Session`; the UI then fetches the panel
    /// data through the driver.
    pub fn set_info_panel(&mut self, target: Option<InfoPanelTarget>) {
        self.session.open_info_panel = target;
    }

    /// Phase 6: `getContacts` for the contacts tab. Fires once per list
    /// (deduped by a settled `Session::contacts` + in-flight purpose); a
    /// failed attempt clears its error flag on retry. The `users` response
    /// lands the id list; user objects arrive via `updateUser`.
    pub fn fetch_contacts(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.contacts.is_some()
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetContacts)
        {
            return Ok(None);
        }
        self.session.contacts_error = false;
        let extra = self.session.request(RequestPurpose::GetContacts, None);
        if let Err(err) = self.sender.send_json(&get_contacts(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase 6: user-scoped `getUserFullInfo` for the user info panel
    /// (opened from the contacts list, where there is no chat to resolve
    /// through). Deduped by the bio cache + in-flight user id; the id-less
    /// response correlates via `PendingRequest::user_id`.
    pub fn fetch_user_full_info(
        &mut self,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.user_full_infos.contains_key(&user_id)
            || self
                .session
                .requests
                .has_purpose_for_user(RequestPurpose::GetUserFullInfo, user_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::GetUserFullInfo, user_id);
        if let Err(err) = self.sender.send_json(&get_user_full_info(extra, user_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase 6: `getSupergroupFullInfo` for the group info panel. Deduped
    /// by cache + in-flight supergroup id; the id-less response correlates
    /// via `PendingRequest::supergroup_id`.
    pub fn fetch_supergroup_full_info(
        &mut self,
        supergroup_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .supergroup_full_infos
            .contains_key(&supergroup_id)
            || self
                .session
                .requests
                .has_purpose_for_supergroup(RequestPurpose::GetSupergroupFullInfo, supergroup_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, supergroup_id);
        if let Err(err) = self
            .sender
            .send_json(&get_supergroup_full_info(extra, supergroup_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D2: `getChatStatistics` (TDLib 1.8.67, line 15760). Sent only
    /// when `supergroupFullInfo.can_get_statistics` is true for the chat's
    /// supergroup (the schema gates the method on it). Idempotent: a
    /// cached `Loaded` result is kept until an explicit refresh clears it,
    /// and no second request goes out while one is in flight. Returns
    /// `Ok(None)` when nothing was sent.
    pub fn fetch_chat_statistics(
        &mut self,
        chat_id: ChatId,
        is_dark: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_get = self
            .session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| match chat.kind {
                ChatKind::Supergroup { supergroup_id, .. } => {
                    self.session.supergroup_full_infos.get(&supergroup_id)
                }
                _ => None,
            })
            .is_some_and(|info| info.can_get_statistics);
        if !can_get {
            return Ok(None);
        }
        if matches!(
            self.session.chat_statistics.get(&chat_id.0),
            Some(ChatStatisticsFetch::Loading | ChatStatisticsFetch::Loaded(_))
        ) || self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatStatistics, chat_id)
        {
            return Ok(None);
        }
        self.session
            .chat_statistics
            .insert(chat_id.0, ChatStatisticsFetch::Loading);
        let extra = self
            .session
            .request(RequestPurpose::GetChatStatistics, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_statistics(extra, chat_id.0, is_dark))
        {
            self.session.requests.take(extra);
            self.session.chat_statistics.remove(&chat_id.0);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D2: explicit refresh of `getChatStatistics` — clears the
    /// cached result and re-sends (the plain fetch keeps `Loaded`).
    pub fn refresh_chat_statistics(
        &mut self,
        chat_id: ChatId,
        is_dark: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.chat_statistics.remove(&chat_id.0);
        self.fetch_chat_statistics(chat_id, is_dark)
    }

    /// Phase D3a: `getChatInviteLinks` (TDLib 1.8.67, `schema/td_api.tl:14138`).
    /// Lists active invite links from all creators; only admins with the
    /// `can_invite_users` right may call it (TDLib errors otherwise).
    /// Idempotent: a cached `Loaded` result is kept until an explicit
    /// refresh clears it, and no second request goes out while one is in
    /// flight. Returns `Ok(None)` when nothing was sent.
    pub fn fetch_chat_invite_links(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_invite = self.session.chat_can_invite_users(chat_id);
        if !can_invite {
            return Ok(None);
        }
        if matches!(
            self.session.invite_links.get(&chat_id.0),
            Some(InviteLinkFetch::Loading | InviteLinkFetch::Loaded(_))
        ) || self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatInviteLinks, chat_id)
        {
            return Ok(None);
        }
        self.session
            .invite_links
            .insert(chat_id.0, InviteLinkFetch::Loading);
        let extra = self
            .session
            .request(RequestPurpose::GetChatInviteLinks, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_chat_invite_links(
            extra, chat_id.0, 0, false, 0, "", 100,
        )) {
            self.session.requests.take(extra);
            self.session.invite_links.remove(&chat_id.0);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3a: explicit refresh of `getChatInviteLinks` — clears the
    /// cached result and re-sends (the plain fetch keeps `Loaded`).
    pub fn refresh_chat_invite_links(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.invite_links.remove(&chat_id.0);
        self.fetch_chat_invite_links(chat_id)
    }

    /// Phase D3a: `createChatInviteLink` (TDLib 1.8.67,
    /// `schema/td_api.tl:14097`). Returns `Ok(None)` when the chat cannot
    /// be managed or another create request is already in flight. The new
    /// link arrives as the `chatInviteLink` response.
    pub fn create_chat_invite_link(
        &mut self,
        chat_id: ChatId,
        name: &str,
        expiration_date: i32,
        member_limit: i32,
        creates_join_request: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_invite = self.session.chat_can_invite_users(chat_id);
        if !can_invite {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::CreateChatInviteLink, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::CreateChatInviteLink, Some(chat_id));
        if let Err(err) = self.sender.send_json(&create_chat_invite_link(
            extra,
            chat_id.0,
            name,
            expiration_date,
            member_limit,
            creates_join_request,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3a: `editChatInviteLink` (TDLib 1.8.67,
    /// `schema/td_api.tl:14115`). Returns `Ok(None)` when the chat cannot
    /// be managed or another edit request is already in flight. The
    /// updated link arrives as the `chatInviteLink` response.
    pub fn edit_chat_invite_link(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
        name: &str,
        expiration_date: i32,
        member_limit: i32,
        creates_join_request: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_invite = self.session.chat_can_invite_users(chat_id);
        if !can_invite {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::EditChatInviteLink, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::EditChatInviteLink, Some(chat_id));
        if let Err(err) = self.sender.send_json(&edit_chat_invite_link(
            extra,
            chat_id.0,
            invite_link,
            name,
            expiration_date,
            member_limit,
            creates_join_request,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3a: `revokeChatInviteLink` (TDLib 1.8.67,
    /// `schema/td_api.tl:14152`). Revocation is the only delete path (no
    /// `deleteChatInviteLink` in 1.8.67). Returns `Ok(None)` when the chat
    /// cannot be managed or another revoke request is already in flight.
    /// The updated list arrives as the `chatInviteLinks` response.
    pub fn revoke_chat_invite_link(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_invite = self.session.chat_can_invite_users(chat_id);
        if !can_invite {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::RevokeChatInviteLink, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::RevokeChatInviteLink, Some(chat_id));
        if let Err(err) =
            self.sender
                .send_json(&revoke_chat_invite_link(extra, chat_id.0, invite_link))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3a: `getChatJoinRequests` (TDLib 1.8.67,
    /// `schema/td_api.tl:14174`). Lists pending requests across all invite
    /// links, unfiltered. Idempotent like `fetch_chat_invite_links`.
    /// Returns `Ok(None)` when nothing was sent.
    pub fn fetch_chat_join_requests(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_invite = self.session.chat_can_invite_users(chat_id);
        if !can_invite {
            return Ok(None);
        }
        if matches!(
            self.session.join_requests.get(&chat_id.0),
            Some(JoinRequestFetch::Loading | JoinRequestFetch::Loaded(_))
        ) || self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatJoinRequests, chat_id)
        {
            return Ok(None);
        }
        self.session
            .join_requests
            .insert(chat_id.0, JoinRequestFetch::Loading);
        let extra = self
            .session
            .request(RequestPurpose::GetChatJoinRequests, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_join_requests(extra, chat_id.0, "", "", 50))
        {
            self.session.requests.take(extra);
            self.session.join_requests.remove(&chat_id.0);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3a: explicit refresh of `getChatJoinRequests` — clears the
    /// cached result and re-sends (the plain fetch keeps `Loaded`).
    pub fn refresh_chat_join_requests(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.join_requests.remove(&chat_id.0);
        self.fetch_chat_join_requests(chat_id)
    }

    /// Phase D3a: `processChatJoinRequest` (TDLib 1.8.67,
    /// `schema/td_api.tl:14177`). Approves or declines one pending
    /// request. Duplicate submissions for the same user and chat are
    /// suppressed while a request is in flight; the `ok` response drops
    /// the request from the cached list.
    pub fn process_chat_join_request(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        approve: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_invite = self.session.chat_can_invite_users(chat_id);
        if !can_invite {
            return Ok(None);
        }
        let purpose = RequestPurpose::ProcessChatJoinRequest { user_id };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&process_chat_join_request(
            extra, chat_id.0, user_id, approve,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3b: `getChatAdministrators` (TDLib 1.8.67,
    /// `schema/td_api.tl:13632`). Only the owner or admins with
    /// `can_promote_members` may call it (TDLib errors otherwise).
    /// Idempotent: a cached `Loaded` result is kept until an explicit
    /// refresh or a membership change clears it, and no second request
    /// goes out while one is in flight. Returns `Ok(None)` when nothing
    /// was sent.
    pub fn fetch_chat_administrators(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_manage_admins(chat_id) {
            return Ok(None);
        }
        if matches!(
            self.session.admin_lists.get(&chat_id.0),
            Some(AdminListFetch::Loading | AdminListFetch::Loaded(_))
        ) || self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatAdministrators, chat_id)
        {
            return Ok(None);
        }
        self.session
            .admin_lists
            .insert(chat_id.0, AdminListFetch::Loading);
        let extra = self
            .session
            .request(RequestPurpose::GetChatAdministrators, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_administrators(extra, chat_id.0))
        {
            self.session.requests.take(extra);
            self.session.admin_lists.remove(&chat_id.0);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3b: explicit refresh of `getChatAdministrators` — clears the
    /// cached result and re-sends (the plain fetch keeps `Loaded`).
    pub fn refresh_chat_administrators(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.admin_lists.remove(&chat_id.0);
        self.fetch_chat_administrators(chat_id)
    }

    /// Phase D3c: `getChatEventLog` first page (TDLib 1.8.67,
    /// `schema/td_api.tl:15252`). Idempotent: a cached `Loaded` page is
    /// kept until an explicit refresh clears it, and no second request
    /// goes out while one is in flight. The driver no-ops unless the
    /// viewer may view the log (`chat_can_view_event_log` — TDLib
    /// "requires administrator rights"). Returns `Ok(None)` when nothing
    /// was sent.
    pub fn fetch_chat_event_log(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.fetch_chat_event_log_page(chat_id, 0)
    }

    /// Phase D3c: explicit refresh of `getChatEventLog` — clears the
    /// cached page and re-sends (the plain fetch keeps `Loaded`).
    pub fn refresh_chat_event_log(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.event_logs.remove(&chat_id.0);
        self.fetch_chat_event_log(chat_id)
    }

    /// Phase D3c: the next older page of the event log. The paging cursor
    /// is the oldest cached event's id (results arrive in decreasing
    /// event-id order); the driver no-ops unless a `Loaded` page reports
    /// `has_more` — a short page means the log is exhausted.
    pub fn fetch_chat_event_log_more(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let from_event_id = match self.session.event_logs.get(&chat_id.0) {
            Some(ChatEventLogFetch::Loaded(page)) if page.has_more => {
                page.events.last().map(|event| event.id).unwrap_or(0)
            }
            _ => return Ok(None),
        };
        self.fetch_chat_event_log_page(chat_id, from_event_id)
    }

    /// Phase D3c: one `getChatEventLog` page (`from_event_id` 0 = latest).
    /// Deduped on any in-flight `GetChatEventLog` for the chat, whatever
    /// its cursor.
    fn fetch_chat_event_log_page(
        &mut self,
        chat_id: ChatId,
        from_event_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_view_event_log(chat_id) {
            return Ok(None);
        }
        if from_event_id == 0
            && matches!(
                self.session.event_logs.get(&chat_id.0),
                Some(ChatEventLogFetch::Loading | ChatEventLogFetch::Loaded(_))
            )
        {
            return Ok(None);
        }
        if self.session.requests.has_event_log_in_flight(chat_id) {
            return Ok(None);
        }
        if from_event_id == 0 {
            self.session
                .event_logs
                .insert(chat_id.0, ChatEventLogFetch::Loading);
        }
        let extra = self.session.request(
            RequestPurpose::GetChatEventLog { from_event_id },
            Some(chat_id),
        );
        if let Err(err) = self.sender.send_json(&get_chat_event_log(
            extra,
            chat_id.0,
            "",
            from_event_id,
            CHAT_EVENT_LOG_PAGE_SIZE,
            None,
            &[],
        )) {
            self.session.requests.take(extra);
            if from_event_id == 0 {
                self.session.event_logs.remove(&chat_id.0);
            }
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3b: shared `setChatMemberStatus` sender (TDLib 1.8.67,
    /// `schema/td_api.tl:13592`). Gated on `can_promote_members` and
    /// deduped per (chat, user, kind). The `ok` response invalidates the
    /// cached admin list; the member change itself arrives as
    /// `updateChatMember`.
    fn send_set_chat_member_status(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        kind: MemberStatusChange,
        status: &serde_json::Value,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_manage_admins(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetChatMemberStatus { user_id, kind };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let member_id = MessageSenderRef::User(user_id).to_value();
        if let Err(err) = self.sender.send_json(&set_chat_member_status(
            extra, chat_id.0, &member_id, status,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3b: promote a member to administrator with the given rights
    /// (`chatMemberStatusAdministrator`, schema 1.8.67, line 2500).
    pub fn promote_chat_member(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        rights: &ChatAdminRights,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let status = chat_member_status_administrator_json(true, rights);
        self.send_set_chat_member_status(chat_id, user_id, MemberStatusChange::Promote, &status)
    }

    /// Phase D3b: edit an administrator's rights (same
    /// `chatMemberStatusAdministrator` shape as promote).
    pub fn edit_admin_rights(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        rights: &ChatAdminRights,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let status = chat_member_status_administrator_json(true, rights);
        self.send_set_chat_member_status(chat_id, user_id, MemberStatusChange::EditRights, &status)
    }

    /// Phase D3b: demote an administrator to a plain member
    /// (`chatMemberStatusMember`, schema 1.8.67, line 2504).
    pub fn demote_chat_member(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        // Phase D3b: the owner can never be demoted — the UI hides the
        // action, but enforce it at the driver level too so no future
        // caller can bypass it. Unknown/unloaded list → allow and let
        // TDLib reject as the backstop.
        let is_owner = matches!(
            self.session.admin_lists.get(&chat_id.0),
            Some(AdminListFetch::Loaded(list))
                if list.iter().any(|e| e.user_id == user_id && e.is_owner)
        );
        if is_owner {
            return Ok(None);
        }
        let status = chat_member_status_member_json();
        self.send_set_chat_member_status(chat_id, user_id, MemberStatusChange::Demote, &status)
    }

    /// Phase D3b: `getChatMember` for one administrator's current rights
    /// (schema 1.8.67, line 13622), backing the edit-rights dialog.
    /// Deduped per (chat, user); the answer lands in
    /// `Session::admin_rights`.
    pub fn fetch_admin_rights(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_manage_admins(chat_id) {
            return Ok(None);
        }
        if matches!(
            self.session.admin_rights.get(&(chat_id.0, user_id)),
            Some(AdminRightsFetch::Loading | AdminRightsFetch::Loaded(_))
        ) {
            return Ok(None);
        }
        let purpose = RequestPurpose::GetAdminRights { user_id };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        self.session
            .admin_rights
            .insert((chat_id.0, user_id), AdminRightsFetch::Loading);
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_member(extra, chat_id, user_id))
        {
            self.session.requests.take(extra);
            self.session.admin_rights.remove(&(chat_id.0, user_id));
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3b: explicit refresh of one administrator's rights — clears
    /// the cached result and re-sends (the plain fetch keeps `Loaded`).
    pub fn refresh_admin_rights(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.admin_rights.remove(&(chat_id.0, user_id));
        self.fetch_admin_rights(chat_id, user_id)
    }

    /// Phase D3b: `getSupergroupMembers` (TDLib 1.8.67,
    /// `schema/td_api.tl:15238`) for the promote member picker — recent
    /// members when `query` is empty, a search filter otherwise (schema
    /// lines 2559/2568). Only supergroup chats (incl. channels) have
    /// members to pick. Deduped like the other D3b fetches.
    pub fn fetch_supergroup_members(
        &mut self,
        chat_id: ChatId,
        query: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_manage_admins(chat_id) {
            return Ok(None);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup { supergroup_id, .. } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        if matches!(
            self.session.supergroup_members.get(&chat_id.0),
            Some(SupergroupMembersFetch::Loading | SupergroupMembersFetch::Loaded { .. })
        ) || self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetSupergroupMembers, chat_id)
        {
            return Ok(None);
        }
        self.session
            .supergroup_members
            .insert(chat_id.0, SupergroupMembersFetch::Loading);
        let extra = self
            .session
            .request(RequestPurpose::GetSupergroupMembers, Some(chat_id));
        let filter = if query.is_empty() {
            supergroup_members_filter_recent_json()
        } else {
            supergroup_members_filter_search_json(query)
        };
        if let Err(err) = self.sender.send_json(&get_supergroup_members(
            extra,
            supergroup_id,
            &filter,
            0,
            200,
        )) {
            self.session.requests.take(extra);
            self.session.supergroup_members.remove(&chat_id.0);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3b: explicit refresh of the member picker — clears the
    /// cached page and re-sends.
    pub fn refresh_supergroup_members(
        &mut self,
        chat_id: ChatId,
        query: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.supergroup_members.remove(&chat_id.0);
        self.fetch_supergroup_members(chat_id, query)
    }

    /// Phase A1: forced `getSupergroupFullInfo` refresh for the slow-mode
    /// gate. Unlike `fetch_supergroup_full_info` it ignores the "already
    /// fetched" cache: the schema (1.8.67, line 2759) warns no
    /// `updateSupergroupFullInfo` fires when only
    /// `slow_mode_delay_expires_in` changes, so a blocked send attempt
    /// re-reads the server value. In-flight requests are still deduped.
    pub fn refresh_supergroup_full_info(
        &mut self,
        supergroup_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_supergroup(RequestPurpose::GetSupergroupFullInfo, supergroup_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, supergroup_id);
        if let Err(err) = self
            .sender
            .send_json(&get_supergroup_full_info(extra, supergroup_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase A1: `setChatSlowModeDelay` (TDLib 1.8.67, line 13551) — the
    /// admin slow-mode control. `slow_mode_delay` must be one of 0, 5, 10,
    /// 30, 60, 300, 900, 3600. The new delay arrives via
    /// `updateSupergroupFullInfo`; `ok`/errors resolve through the pending
    /// request like other fire-and-forget setters.
    pub fn set_chat_slow_mode_delay(
        &mut self,
        chat_id: ChatId,
        slow_mode_delay: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetChatSlowModeDelay, Some(chat_id));
        if let Err(err) =
            self.sender
                .send_json(&set_chat_slow_mode_delay(extra, chat_id, slow_mode_delay))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase 6: `addContact` from the add-contact dialog. The reducer
    /// invalidates the contacts list on `ok`; the new contact row arrives
    /// via `updateUser`. `share_phone_number` stays `false` — sharing the
    /// user's own number is a privacy decision the dialog does not ask
    /// for (documented in DECISIONS.md).
    pub fn add_contact(
        &mut self,
        user_id: i64,
        phone_number: &str,
        first_name: &str,
        last_name: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::AddContact, user_id);
        if let Err(err) = self.sender.send_json(&add_contact(
            extra,
            user_id,
            phone_number,
            first_name,
            last_name,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase 6: download the small profile photo for the user info panel
    /// (thumb priority). No-op when the user has no photo, or the file is
    /// already local / in flight.
    pub fn download_user_photo(
        &mut self,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        // Prefer the photo from the cached `userFullInfo` (fetched for the
        // panel); fall back to the `user.profile_photo.small` file id.
        let file_id = self
            .session
            .user_full_info(user_id)
            .and_then(|info| info.photo_file_id)
            .map(FileId)
            .unwrap_or_else(|| {
                self.session
                    .user(user_id)
                    .map(|user| FileId(user.photo_small_file_id))
                    .unwrap_or(FileId(0))
            });
        self.download_file(file_id, THUMB_DOWNLOAD_PRIORITY)
    }

    /// Load another page of history for the open chat (`from_message_id` = oldest, or 0).
    pub fn fetch_history(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if self
            .session
            .histories
            .get(&chat_id.0)
            .is_some_and(|h| h.loaded_complete)
        {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetHistory, chat_id)
        {
            return Ok(None);
        }
        let from = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|h| h.oldest_id())
            .unwrap_or(MessageId(0));
        let extra = self
            .session
            .request(RequestPurpose::GetHistory, Some(chat_id));
        self.sender.send_json(&get_chat_history(
            extra,
            chat_id,
            from,
            0,
            HISTORY_PAGE_SIZE,
            false,
        ))?;
        Ok(Some(extra))
    }

    /// `getChatSponsoredMessages` for a channel chat (TDLib 1.8.67). Called
    /// when a channel is opened; rows render Sponsored / Recommended.
    /// The fetch already runs so the pipeline is proven with replay
    /// fixtures. Bot chats can also carry sponsored messages per the
    /// schema; they are not fetched yet (Phase 3).
    pub fn fetch_sponsored_messages(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_channel = self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            matches!(
                chat.kind,
                ChatKind::Supergroup {
                    is_channel: true,
                    ..
                }
            )
        });
        if !is_channel {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatSponsoredMessages, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatSponsoredMessages, Some(chat_id));
        match self
            .sender
            .send_json(&get_chat_sponsored_messages(extra, chat_id))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `reportChatSponsoredMessage` (TDLib 1.8.67). Empty `option_id` starts
    /// the flow; TDLib may answer `reportSponsoredResultOptionRequired`.
    /// Returns `None` when the row is missing or `can_be_reported` is false.
    pub fn report_sponsored_message(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
        option_id: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .begin_sponsored_report(chat_id, message_id)
            .is_none()
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::ReportChatSponsoredMessage, Some(chat_id));
        match self.sender.send_json(&report_chat_sponsored_message(
            extra, chat_id, message_id, option_id,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.dismiss_sponsored_report();
                Err(err)
            }
        }
    }

    /// `clickChatSponsoredMessage` (TDLib 1.8.67): the user opened a sponsored
    /// message's sponsor link/button (`is_media_click = false`) or its media
    /// (`is_media_click = true`). Fire-and-forget; the `ok` response is ignored.
    pub fn click_chat_sponsored_message(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
        is_media_click: bool,
        from_fullscreen: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ClickChatSponsoredMessage, Some(chat_id));
        match self.sender.send_json(&click_chat_sponsored_message(
            extra,
            chat_id,
            message_id,
            is_media_click,
            from_fullscreen,
        )) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `viewSponsoredChat` (TDLib 1.8.67): the user fully viewed a sponsored
    /// chat. The unique id comes from `sponsoredChat` search results.
    pub fn view_sponsored_chat(
        &mut self,
        sponsored_chat_unique_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ViewSponsoredChat, None);
        match self
            .sender
            .send_json(&view_sponsored_chat(extra, sponsored_chat_unique_id))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Open the sticker panel and load installed regular sets
    /// (`getInstalledStickerSets` + `stickerTypeRegular`).
    pub fn open_sticker_panel(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.stickers.open = true;
        self.session.stickers.failed = false;
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetInstalledStickerSets)
        {
            return Ok(None);
        }
        if !self.session.stickers.sets.is_empty() {
            return self.maybe_load_selected_sticker_set();
        }
        self.session.stickers.loading_sets = true;
        let extra = self
            .session
            .request(RequestPurpose::GetInstalledStickerSets, None);
        match self.sender.send_json(&get_installed_sticker_sets(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.stickers.loading_sets = false;
                Err(err)
            }
        }
    }

    pub fn close_sticker_panel(&mut self) {
        self.session.stickers.close();
    }

    pub fn select_sticker_set(
        &mut self,
        set_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.select_sticker_set(set_id);
        self.maybe_load_selected_sticker_set()
    }

    fn maybe_load_selected_sticker_set(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.stickers.open || !self.chats_path_active() {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetStickerSet)
        {
            return Ok(None);
        }
        let Some(set_id) = self.session.stickers.selected_needs_load() else {
            return Ok(None);
        };
        self.session.mark_sticker_set_loading();
        let extra = self.session.request(RequestPurpose::GetStickerSet, None);
        match self.sender.send_json(&get_sticker_set(extra, set_id)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.stickers.loading_set = false;
                Err(err)
            }
        }
    }

    /// Open the GIF panel and load `getSavedAnimations`.
    pub fn open_gif_panel(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.gifs.open = true;
        self.session.gifs.failed = false;
        self.maybe_refresh_saved_animations()
    }

    pub fn close_gif_panel(&mut self) {
        self.session.gifs.close();
    }

    fn maybe_refresh_saved_animations(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.gifs.open || !self.chats_path_active() {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetSavedAnimations)
        {
            return Ok(None);
        }
        let needs = self.session.gifs.animations.is_empty() || self.session.gifs.stale;
        if !needs {
            return Ok(None);
        }
        self.session.gifs.loading = true;
        self.session.gifs.stale = false;
        let extra = self
            .session
            .request(RequestPurpose::GetSavedAnimations, None);
        match self.sender.send_json(&get_saved_animations(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.gifs.loading = false;
                Err(err)
            }
        }
    }

    /// Phase 9.1: `loadActiveStories(storyListMain)` once per Ready. The
    /// loaded stories arrive as `updateChatActiveStories` updates and feed
    /// the story tray above the chat list.
    pub fn maybe_load_active_stories(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || self.session.stories_active_loaded {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::LoadActiveStories)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::LoadActiveStories, None);
        match self.sender.send_json(&load_active_stories(extra)) {
            Ok(()) => {
                self.session.stories_active_loaded = true;
                Ok(Some(extra))
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.1: refresh one chat's active stories (`getChatActiveStories`).
    /// The response is `chatActiveStories`, handled like the update.
    pub fn get_chat_active_stories(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatActiveStories, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatActiveStories, Some(chat_id));
        match self
            .sender
            .send_json(&get_chat_active_stories(extra, chat_id))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.1: fetch one story's full content (`getStory`). Deduped by
    /// the story cache + in-flight per-story requests; the `story` response
    /// is authoritative on `(poster_chat_id, id)`.
    pub fn get_story(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.stories.contains_key(&(chat_id.0, story_id))
            || self.session.requests.has_purpose_for_story(
                RequestPurpose::GetStory,
                chat_id,
                story_id,
            )
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_story(RequestPurpose::GetStory, chat_id, story_id);
        match self.sender.send_json(&get_story(extra, chat_id, story_id)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.1: `openStory` — the user opened a story for viewing.
    /// Fire-and-forget; the `ok` answer needs no handling.
    pub fn open_story(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::OpenStory, Some(chat_id));
        match self.sender.send_json(&open_story(extra, chat_id, story_id)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.1: `closeStory` — the user closed a story. Fire-and-forget.
    pub fn close_story(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::CloseStory, Some(chat_id));
        match self
            .sender
            .send_json(&close_story(extra, chat_id, story_id))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.2: `getStoryAvailableReactions` (row_size 10, within the
    /// schema's 5–25 range). The `availableReactions` response feeds the
    /// story viewer's reaction picker. Deduped while a request is
    /// in-flight; cached afterwards (`Session::story_available_reactions`).
    pub fn get_story_available_reactions(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.story_available_reactions.is_some()
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetStoryAvailableReactions)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetStoryAvailableReactions, None);
        match self
            .sender
            .send_json(&get_story_available_reactions(extra, 10))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.2: `setStoryReaction` — set (or with `None`, remove) the
    /// user's emoji reaction on a story. Gates: the story must be cached
    /// and must not be live (`setStoryReaction` is not supported for live
    /// stories, TDLib 1.8.67 `schema/td_api.tl:13809`). The reaction shows
    /// up via the follow-up `updateStory` (`story.chosen_reaction_type`).
    pub fn set_story_reaction(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        emoji: Option<&str>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let story = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .ok_or(ConnectSendError::InvalidRequest)?;
        if matches!(story.content, StoryContentView::Live) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if emoji.is_some_and(|emoji| emoji.trim().is_empty()) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = if emoji.is_some() {
            RequestPurpose::SetStoryReaction
        } else {
            RequestPurpose::RemoveStoryReaction
        };
        let extra = self.session.request_for_story(purpose, chat_id, story_id);
        match self
            .sender
            .send_json(&set_story_reaction(extra, chat_id, story_id, emoji))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.2: `deleteStory` — delete a story posted by the current
    /// user. Gated on `story.can_be_deleted`. The deletion lands as
    /// `updateStoryDeleted`.
    pub fn delete_story(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let deletable = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_be_deleted);
        if !deletable {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_story(RequestPurpose::DeleteStory, chat_id, story_id);
        match self
            .sender
            .send_json(&delete_story(extra, chat_id, story_id))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.2: reply to a story — `sendMessage` to the poster chat with
    /// `inputMessageReplyToStory`. Gated on `story.can_be_replied` and a
    /// non-empty message sent to a supported chat.
    pub fn send_story_reply(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        text: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let repliable = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_be_replied);
        if !repliable || text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra =
            self.session
                .request_for_story(RequestPurpose::SendStoryReply, chat_id, story_id);
        let json = send_text_story_reply(extra, chat_id, chat_id, story_id, text);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Parity slice 4: the forum topic a send to `chat_id` is addressed to.
    /// `Some` only when `chat_id` is the open chat and a topic is selected
    /// there; the `sendMessage` request then carries
    /// `topic_id = messageTopicForum{forum_topic_id}` (schema 1.8.67, lines
    /// 12200 and 3004). Story replies keep a null topic — they address the
    /// poster chat, never a topic.
    fn send_topic(&self, chat_id: ChatId) -> Option<i32> {
        if self.session.open_chat == Some(chat_id) {
            self.session.open_topic
        } else {
            None
        }
    }

    /// Parity slice 4: rejects sends into a closed forum topic. The
    /// composer is hidden there; this guards a stale-snapshot race.
    fn topic_send_is_closed(&self, chat_id: ChatId) -> bool {
        self.send_topic(chat_id).is_some_and(|_| {
            self.session
                .open_topic_info(chat_id)
                .is_some_and(|topic| topic.is_closed)
        })
    }

    /// `sendMessage` + `inputMessageAnimation` / `inputAnimation` / `inputFileId`.
    pub fn send_animation(
        &mut self,
        chat_id: ChatId,
        animation: AnimationSend,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported || animation.file_id.0 == 0 || self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        let animation = AnimationSend {
            topic_id,
            ..animation
        };
        let json = send_animation(extra, chat_id, animation);
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `sendMessage` + `inputMessageSticker` / `inputFileId` (Unigram compose).
    pub fn send_sticker(
        &mut self,
        chat_id: ChatId,
        sticker: StickerSend<'_>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported || sticker.file_id.0 == 0 || self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        let sticker = StickerSend {
            topic_id,
            ..sticker
        };
        let json = send_sticker(extra, chat_id, sticker);
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Send `sendMessage` for a snapshot frozen at composer submit.
    /// Caption / path are not logged.
    pub fn send_text_snapshot(
        &mut self,
        snapshot: &ComposerSnapshot,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_snapshot(snapshot)
    }

    /// Send text, photo, document, or local video via `sendMessage` (TDLib 1.8.67).
    pub fn send_snapshot(
        &mut self,
        snapshot: &ComposerSnapshot,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if snapshot.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if snapshot.is_media_album() {
            return self.send_album_snapshot(snapshot);
        }
        let chat_id = snapshot.chat_id();
        // Phase 2.3: channel posting is admin-gated (`can_post` derives the
        // right from own membership); the driver rejects non-admin channel
        // sends the same way the hidden composer does.
        let can_post = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_post());
        if !can_post {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Parity slice 4: never send into a closed forum topic (the
        // composer is hidden there; this guards a stale-snapshot race).
        if self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let caption = snapshot.caption();
        if snapshot.attachment.is_none() && caption.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let reply_to = snapshot.send_reply_to();
        // Validate the picked path before allocating `@extra`.
        let media_path = match snapshot.attachment.as_ref() {
            Some(att) => Some(
                att.send_path_str()
                    .ok_or(ConnectSendError::InvalidRequest)?,
            ),
            None => None,
        };
        let video_probe = match snapshot.attachment.as_ref() {
            Some(att) if att.kind == AttachmentKind::Video => Some(
                crate::video::probe_local_video(&att.path)
                    .map_err(|_| ConnectSendError::InvalidRequest)?,
            ),
            _ => None,
        };
        let video_note = match snapshot.attachment.as_ref() {
            Some(att) if att.kind == AttachmentKind::VideoNote => {
                let probe = crate::video::probe_local_video_note(&att.path)
                    .map_err(|_| ConnectSendError::InvalidRequest)?;
                let thumbnail = crate::video::write_video_note_thumbnail(&att.path).map(|thumb| {
                    VideoNoteThumbnailSend {
                        path: thumb.path.to_string_lossy().into_owned(),
                        width: thumb.width,
                        height: thumb.height,
                    }
                });
                Some(VideoNoteSend {
                    duration: probe.duration,
                    length: probe.length,
                    thumbnail,
                })
            }
            _ => None,
        };
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        // Parity slice 4: sends from a topic view address the open topic.
        let topic_id = self.send_topic(chat_id);
        // Phase B3: TDLib accepts `inputMessagePhoto`/`inputMessageVideo`
        // `self_destruct_type` only in `chatTypePrivate` chats (its runtime
        // check is `dialog_id.get_type() != DialogType::User` → 400, and the
        // schema says "private chats only"). The choice is stripped for
        // every other chat kind here (defense in depth — the composer
        // picker is gated the same way), so a stale snapshot can never
        // turn a secret-chat send into a 400.
        let self_destruct = self
            .session
            .chats
            .get(&chat_id.0)
            .filter(|chat| matches!(chat.kind, ChatKind::Private { .. }))
            .and(snapshot.self_destruct);
        // Contains caption / path — do not log `json`.
        let json = match (snapshot.attachment.as_ref(), media_path.as_deref()) {
            (Some(att), Some(path)) => match att.kind {
                AttachmentKind::Photo => send_photo(
                    extra,
                    chat_id,
                    topic_id,
                    path,
                    caption,
                    reply_to,
                    self_destruct,
                ),
                AttachmentKind::Document => {
                    send_document(extra, chat_id, topic_id, path, caption, reply_to)
                }
                AttachmentKind::Video => {
                    let probe = video_probe.ok_or_else(|| {
                        self.session.requests.take(extra);
                        ConnectSendError::InvalidRequest
                    })?;
                    send_video(
                        extra,
                        chat_id,
                        topic_id,
                        path,
                        &VideoSend {
                            duration: probe.duration,
                            width: probe.width,
                            height: probe.height,
                            supports_streaming: probe.supports_streaming,
                            self_destruct,
                        },
                        caption,
                        reply_to,
                    )
                }
                AttachmentKind::VideoNote => {
                    let note = video_note.ok_or_else(|| {
                        self.session.requests.take(extra);
                        ConnectSendError::InvalidRequest
                    })?;
                    send_video_note(extra, chat_id, topic_id, path, &note, reply_to)
                }
            },
            (None, None) => send_text(extra, chat_id, topic_id, caption, reply_to),
            _ => {
                self.session.requests.take(extra);
                return Err(ConnectSendError::InvalidRequest);
            }
        };
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `sendMessageAlbum` for 2–10 local photos and/or videos.
    fn send_album_snapshot(
        &mut self,
        snapshot: &ComposerSnapshot,
    ) -> Result<RequestId, ConnectSendError> {
        let chat_id = snapshot.chat_id();
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported || !snapshot.is_media_album() || self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let caption = snapshot.caption();
        let last = snapshot.album.len() - 1;
        // Phase B3: same private-chat gate as `send_snapshot` — the timer
        // applies per album item (the schema allows it per
        // `inputMessagePhoto`/`inputMessageVideo`).
        let self_destruct = self
            .session
            .chats
            .get(&chat_id.0)
            .filter(|chat| matches!(chat.kind, ChatKind::Private { .. }))
            .and(snapshot.self_destruct);
        let mut contents = Vec::with_capacity(snapshot.album.len());
        for (index, att) in snapshot.album.iter().enumerate() {
            let path = att
                .send_path_str()
                .ok_or(ConnectSendError::InvalidRequest)?;
            let item_caption = if index == last { caption } else { "" };
            let content = match att.kind {
                AttachmentKind::Photo => input_message_photo(&path, item_caption, self_destruct),
                AttachmentKind::Video => {
                    let probe = crate::video::probe_local_video(&att.path)
                        .map_err(|_| ConnectSendError::InvalidRequest)?;
                    input_message_video(
                        &path,
                        &VideoSend {
                            duration: probe.duration,
                            width: probe.width,
                            height: probe.height,
                            supports_streaming: probe.supports_streaming,
                            self_destruct,
                        },
                        item_caption,
                    )
                }
                AttachmentKind::Document | AttachmentKind::VideoNote => {
                    return Err(ConnectSendError::InvalidRequest);
                }
            };
            contents.push(content);
        }
        let reply_to = snapshot.send_reply_to();
        let extra = self
            .session
            .request(RequestPurpose::SendMessageAlbum, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        let json = send_message_album(extra, chat_id, topic_id, reply_to, contents);
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `sendMessage` + `inputMessageVoiceNote` for a finished local capture.
    pub fn send_voice_note(
        &mut self,
        draft: &VoiceDraft,
        caption: &str,
        reply_to: Option<MessageId>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported || self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let path = crate::local_path::pick_send_path(&draft.path)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let path = path.to_string_lossy().into_owned();
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        let json = send_voice_note(
            extra,
            chat_id,
            VoiceNoteSend {
                path: &path,
                duration: draft.duration_secs,
                waveform_b64: &draft.waveform_b64(),
                caption,
                reply_to,
                topic_id,
            },
        );
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                let _ = self.sync_voice_recording(false, 0);
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `openMessageContent` when playback of a voice note starts.
    pub fn open_voice_content(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::OpenMessageContent, Some(chat_id));
        let json = open_message_content(extra, chat_id, message_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 3.2: send a callback query for an inline keyboard button press
    /// (`getCallbackQueryAnswer`; TDLib answers with `callbackQueryAnswer`).
    /// Only real (non-pending) messages in a supported chat can be answered.
    pub fn send_callback_query(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        data: &[u8],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if message.pending || message.id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetCallbackQueryAnswer, Some(chat_id));
        let json = get_callback_query_answer(extra, chat_id, message_id, data);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// While the voice bar is recording, send `chatActionRecordingVoiceNote`
    /// at most every [`OUTGOING_TYPING_INTERVAL_MS`] (Unigram record action).
    /// Stopping sends `chatActionCancel`.
    pub fn sync_voice_recording(
        &mut self,
        active: bool,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        let chat_id = self
            .session
            .open_chat
            .filter(|id| self.typing_chat_allowed(*id));
        let Some(chat_id) = chat_id.filter(|_| active) else {
            return self.cancel_outgoing_voice();
        };
        let _ = self.cancel_outgoing_typing();
        if let Some(prev) = &self.outgoing_voice {
            if prev.chat_id != chat_id {
                self.cancel_outgoing_voice()?;
            } else if now_ms.saturating_sub(prev.last_sent_ms) < OUTGOING_TYPING_INTERVAL_MS {
                return Ok(());
            }
        }
        self.send_voice_action(chat_id, true, now_ms)
    }

    fn cancel_outgoing_voice(&mut self) -> Result<(), ConnectSendError> {
        let Some(prev) = self.outgoing_voice.take() else {
            return Ok(());
        };
        if !self.typing_chat_allowed(prev.chat_id) {
            return Ok(());
        }
        self.send_voice_action(prev.chat_id, false, 0)
    }

    fn send_voice_action(
        &mut self,
        chat_id: ChatId,
        recording: bool,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::SendChatAction, Some(chat_id));
        let json = send_chat_action_kind(
            extra,
            chat_id,
            if recording {
                "chatActionRecordingVoiceNote"
            } else {
                "chatActionCancel"
            },
        );
        match self.sender.send_json(&json) {
            Ok(()) => {
                self.outgoing_voice = recording.then_some(OutgoingTyping {
                    chat_id,
                    last_sent_ms: now_ms,
                });
                Ok(())
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Composer activity → `sendChatAction`.
    ///
    /// Unigram `ChatTextBox.OnTextChanged` calls `SetTyping(ChatActionTyping)`
    /// while the field is non-empty, at most every 4s. tdesktop
    /// `HistoryWidget::fieldChanged` does the same only when the field has
    /// sendable text and the user is not editing. Empty text, edit mode, send,
    /// and leaving the chat send `chatActionCancel` (Unigram `CancelTyping`;
    /// tdesktop stops the action with progress `-1` on chat close and on send).
    pub fn sync_outgoing_typing(
        &mut self,
        text: &str,
        editing: bool,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        let composing = !editing && !text.trim().is_empty();
        let chat_id = self
            .session
            .open_chat
            .filter(|id| self.typing_chat_allowed(*id));
        let Some(chat_id) = chat_id.filter(|_| composing) else {
            return self.cancel_outgoing_typing();
        };
        if let Some(prev) = &self.outgoing_typing {
            if prev.chat_id != chat_id {
                self.cancel_outgoing_typing()?;
            } else if now_ms.saturating_sub(prev.last_sent_ms) < OUTGOING_TYPING_INTERVAL_MS {
                return Ok(());
            }
        }
        self.send_outgoing_action(chat_id, true, now_ms)
    }

    fn typing_chat_allowed(&self, chat_id: ChatId) -> bool {
        self.chats_path_active()
            && self
                .session
                .chats
                .get(&chat_id.0)
                .is_some_and(|chat| chat.supported())
    }

    fn cancel_outgoing_typing(&mut self) -> Result<(), ConnectSendError> {
        let Some(prev) = self.outgoing_typing.take() else {
            return Ok(());
        };
        if !self.typing_chat_allowed(prev.chat_id) {
            return Ok(());
        }
        self.send_outgoing_action(prev.chat_id, false, 0)
    }

    fn send_outgoing_action(
        &mut self,
        chat_id: ChatId,
        typing: bool,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::SendChatAction, Some(chat_id));
        let json = send_chat_action(extra, chat_id, typing);
        match self.sender.send_json(&json) {
            Ok(()) => {
                self.outgoing_typing = typing.then_some(OutgoingTyping {
                    chat_id,
                    last_sent_ms: now_ms,
                });
                Ok(())
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Save an own-message edit via `editMessageText` or `editMessageCaption`.
    pub fn edit_snapshot(
        &mut self,
        edit: &ComposerEdit,
        text: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&edit.chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&edit.chat_id.0)
            .and_then(|history| history.messages.get(&edit.message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if ComposerEdit::from_own_content(
            message.chat_id,
            message.id,
            message.is_outgoing,
            message.pending,
            &message.content,
        )
        .is_none()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let caption = text.trim();
        if matches!(edit.kind, ComposerEditKind::Text) && caption.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::EditMessage, Some(edit.chat_id));
        let json = match edit.kind {
            ComposerEditKind::Text => {
                edit_message_text(extra, edit.chat_id, edit.message_id, caption)
            }
            ComposerEditKind::Caption => {
                edit_message_caption(extra, edit.chat_id, edit.message_id, caption, false)
            }
        };
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// After UI confirm (tdesktop `DeleteMessagesBox`), send `deleteMessages`.
    /// `revoke: true` matches official desktop default for own outgoing.
    pub fn delete_confirmed(
        &mut self,
        confirm: &DeleteConfirm,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&confirm.chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&confirm.chat_id.0)
            .and_then(|history| history.messages.get(&confirm.message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if DeleteConfirm::own(
            message.chat_id,
            message.id,
            message.is_outgoing,
            message.pending,
        )
        .is_none()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteMessages, Some(confirm.chat_id));
        let json = delete_messages(extra, confirm.chat_id, &[confirm.message_id], true);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Send `forwardMessages` after the dest picker chooses a supported chat.
    /// `send_copy: false` preserves official "Forwarded from" attribution.
    pub fn forward_messages(
        &mut self,
        dest: ChatId,
        draft: &ForwardDraft,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if draft.is_empty() || draft.message_ids.len() > 100 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let dest_ok = self
            .session
            .chats
            .get(&dest.0)
            .is_some_and(|chat| chat.supported());
        let from_ok = self
            .session
            .chats
            .get(&draft.from_chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !dest_ok || !from_ok {
            return Err(ConnectSendError::InvalidRequest);
        }
        for id in &draft.message_ids {
            let Some(message) = self
                .session
                .histories
                .get(&draft.from_chat_id.0)
                .and_then(|history| history.messages.get(&id.0))
            else {
                return Err(ConnectSendError::InvalidRequest);
            };
            if message.pending || id.0 <= 0 {
                return Err(ConnectSendError::InvalidRequest);
            }
        }
        let extra = self
            .session
            .request(RequestPurpose::ForwardMessages, Some(dest));
        self.session.in_flight_forward = Some(ForwardFlight {
            extra,
            dest_chat_id: dest,
            from_chat_id: draft.from_chat_id,
            requested: draft.message_ids.len(),
        });
        let json = forward_messages(extra, dest, draft.from_chat_id, &draft.message_ids);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.in_flight_forward = None;
                Err(err)
            }
        }
    }

    /// Add an emoji reaction (tdesktop InlineList / Unigram ReactionButton).
    /// `is_big: false`, `update_recent_reactions: true` — official picker click.
    pub fn add_message_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_message_reaction(chat_id, message_id, emoji, false)
    }

    /// Remove the current user's chosen emoji reaction.
    pub fn remove_message_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_message_reaction(chat_id, message_id, emoji, true)
    }

    /// Chip / picker toggle: chosen → `removeMessageReaction`, else add.
    pub fn toggle_message_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let chosen = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| message.chosen_emoji(emoji));
        self.send_message_reaction(chat_id, message_id, emoji, chosen)
    }

    fn send_message_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
        remove: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if emoji.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !message.can_react() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = if remove {
            RequestPurpose::RemoveMessageReaction
        } else {
            RequestPurpose::AddMessageReaction
        };
        let extra = self.session.request(purpose, Some(chat_id));
        let json = if remove {
            remove_message_reaction(extra, chat_id, message_id, emoji)
        } else {
            add_message_reaction(extra, chat_id, message_id, emoji, false, true)
        };
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Pin a history message (tdesktop / Unigram Pin). Official defaults:
    /// `disable_notification` false, `only_for_self` false.
    pub fn pin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_pin_chat_message(chat_id, message_id, false)
    }

    /// Unpin one pinned message (tdesktop PinnedBar cancel / Unpin).
    pub fn unpin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_pin_chat_message(chat_id, message_id, true)
    }

    /// Toggle pin for an already-sent message.
    pub fn toggle_pin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        let pinned = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| message.is_pinned);
        self.send_pin_chat_message(chat_id, message_id, pinned)
    }

    fn send_pin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        unpin: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !message.can_pin() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = if unpin {
            RequestPurpose::UnpinChatMessage
        } else {
            RequestPurpose::PinChatMessage
        };
        let extra = self.session.request(purpose, Some(chat_id));
        let json = if unpin {
            unpin_chat_message(extra, chat_id, message_id)
        } else {
            pin_chat_message(extra, chat_id, message_id, false, false)
        };
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 4.2: `setPollAnswer` for a poll-row tap. Guards mirror the other
    /// send methods: chats path active, supported chat, real non-pending
    /// message whose content is a votable `messagePoll`. The tap resolves to
    /// the full answer set via `poll_answer_for_tap` (tdesktop-style
    /// toggle/retract semantics); `None` there means no-op (closed poll,
    /// unchanged answer) and no request goes out. On send, the chosen marks
    /// flip locally right away; the server's `updatePoll` corrects the
    /// counts/percentages in place.
    pub fn send_poll_answer(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_index: usize,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let poll = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .filter(|message| !message.pending && message.id.0 > 0)
            .and_then(|message| match &message.content {
                MessageContent::Poll(poll) => Some(poll.poll.clone()),
                _ => None,
            })
            .ok_or(ConnectSendError::InvalidRequest)?;
        if !poll.can_vote() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let option_ids =
            poll_answer_for_tap(&poll, option_index).ok_or(ConnectSendError::InvalidRequest)?;
        // Capture the previous chosen marks: the optimistic flip below is
        // rolled back if the send fails (the server's `updatePoll` corrects
        // counts/percentages in place on success).
        let mut previous: Vec<bool> = Vec::new();
        if let Some(history) = self.session.histories.get_mut(&chat_id.0)
            && let Some(message) = history.messages.get_mut(&message_id.0)
            && let MessageContent::Poll(poll_content) = &mut message.content
        {
            let chosen: std::collections::HashSet<i32> = option_ids.iter().copied().collect();
            for (index, option) in poll_content.poll.options.iter_mut().enumerate() {
                previous.push(option.is_chosen);
                option.is_chosen = chosen.contains(&(index as i32));
            }
        }
        let extra = self
            .session
            .request(RequestPurpose::SetPollAnswer, Some(chat_id));
        let json = set_poll_answer(extra, chat_id, message_id, &option_ids);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                // The vote never left the client: restore the previous marks.
                if let Some(history) = self.session.histories.get_mut(&chat_id.0)
                    && let Some(message) = history.messages.get_mut(&message_id.0)
                    && let MessageContent::Poll(poll_content) = &mut message.content
                {
                    for (option, &was_chosen) in
                        poll_content.poll.options.iter_mut().zip(previous.iter())
                    {
                        option.is_chosen = was_chosen;
                    }
                }
                Err(err)
            }
        }
    }

    /// Phase 4.2: create a poll from the composer dialog via `sendMessage` +
    /// `inputMessagePoll` (TDLib 1.8.67). Guards: chats path active, chat can
    /// post (admin-gated channels, same as `send_snapshot`), valid draft
    /// (`PollDraft::validate`). Quiz polls are created as regular
    /// (`inputPollTypeRegular`) — quiz creation stays out of this slice.
    pub fn send_poll_draft(
        &mut self,
        chat_id: ChatId,
        draft: &PollDraft,
        reply_to: Option<MessageId>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if draft.validate().is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_post = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_post());
        if !can_post {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Parity slice 4: never send into a closed forum topic (the
        // composer is hidden there; this guards a stale-snapshot race).
        if self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let question = draft.question.trim().to_string();
        let options: Vec<String> = draft
            .usable_options()
            .into_iter()
            .map(str::to_string)
            .collect();
        let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        // Parity slice 4: sends from a topic view address the open topic.
        let topic_id = self.send_topic(chat_id);
        let json = send_poll(
            extra,
            chat_id,
            PollSend {
                question: &question,
                options: &option_refs,
                is_anonymous: draft.is_anonymous,
                allows_multiple_answers: draft.allows_multiple_answers,
                reply_to,
                topic_id,
            },
        );
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
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

    /// Move the chat to `chatListArchive` (`addChatToList`).
    pub fn archive_chat(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        self.send_add_chat_to_list(chat_id, true)
    }

    /// Move the chat back to `chatListMain` (`addChatToList`).
    pub fn unarchive_chat(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        self.send_add_chat_to_list(chat_id, false)
    }

    fn send_add_chat_to_list(
        &mut self,
        chat_id: ChatId,
        archive: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::AddChatToList, Some(chat_id));
        let json = add_chat_to_list(extra, chat_id, archive);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Send `setAuthenticationPhoneNumber` when auth is WaitPhoneNumber.
    /// Phone value is never stored on the session or diagnostics.
    pub fn submit_phone(&mut self, phone: &str) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitPhoneNumber) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let phone = phone.trim();
        if phone.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self.session.request(RequestPurpose::SetPhoneNumber, None);
        self.sender
            .send_json(&set_authentication_phone_number(extra, phone))?;
        Ok(extra)
    }

    /// Send `checkAuthenticationCode` when auth is WaitCode.
    /// The code is never stored on the session or diagnostics.
    pub fn submit_code(&mut self, code: &str) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitCode { .. }) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let code = code.trim();
        if code.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self
            .session
            .request(RequestPurpose::CheckAuthenticationCode, None);
        self.sender
            .send_json(&check_authentication_code(extra, code))?;
        Ok(extra)
    }

    /// Send `checkAuthenticationPassword` when auth is WaitPassword.
    /// The password is never stored on the session or diagnostics. Not trimmed
    /// (leading/trailing spaces can be significant).
    pub fn submit_password(&mut self, password: &str) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitPassword { .. }) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if password.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self
            .session
            .request(RequestPurpose::CheckAuthenticationPassword, None);
        self.sender
            .send_json(&check_authentication_password(extra, password))?;
        Ok(extra)
    }

    pub fn open_search(&mut self) -> Result<Option<SearchFlight>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.clear_typed_debounce();
        if self.session.search.open && !self.session.search.query.is_empty() {
            return Ok(None);
        }
        if self.session.search.open
            && self.session.search.recents
            && matches!(
                self.session.search.status,
                SearchStatus::Searching | SearchStatus::Ready | SearchStatus::Idle
            )
        {
            return Ok(None);
        }
        self.request_recents()
    }

    pub fn close_search(&mut self) {
        self.clear_typed_debounce();
        self.session.close_search();
    }

    /// Empty query: `searchRecentlyFoundChats` immediately (official Recent).
    /// Non-empty: debounce, then `searchChats` + `searchMessages` (`chat_list` null).
    pub fn set_search_query(
        &mut self,
        query: &str,
    ) -> Result<SearchQueryOutcome, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let trimmed = query.trim();
        if trimmed.is_empty() {
            self.clear_typed_debounce();
            if self.session.search.open
                && self.session.search.query.is_empty()
                && self.session.search.recents
                && matches!(
                    self.session.search.status,
                    SearchStatus::Searching | SearchStatus::Ready | SearchStatus::Idle
                )
            {
                return Ok(SearchQueryOutcome::Unchanged);
            }
            return Ok(match self.request_recents()? {
                Some(flight) => SearchQueryOutcome::Sent(flight),
                None => SearchQueryOutcome::Unchanged,
            });
        }
        if self
            .pending_typed_search
            .as_ref()
            .is_some_and(|(_, q)| q == trimmed)
            && self.session.search.query == trimmed
        {
            return Ok(SearchQueryOutcome::Unchanged);
        }
        if self.pending_typed_search.is_none()
            && self.session.search.query == trimmed
            && !self.session.search.recents
            && matches!(
                self.session.search.status,
                SearchStatus::Searching
                    | SearchStatus::Ready
                    | SearchStatus::Empty
                    | SearchStatus::Failed
            )
        {
            return Ok(SearchQueryOutcome::Unchanged);
        }
        let _search_gen = self.session.search.begin_query(trimmed);
        self.search_debounce_token = self.search_debounce_token.saturating_add(1);
        let token = self.search_debounce_token;
        self.pending_typed_search = Some((token, trimmed.to_string()));
        Ok(SearchQueryOutcome::Debounced { token })
    }

    /// Send the settled typed query if `token` is still the latest debounce.
    pub fn commit_debounced_search(
        &mut self,
        token: u64,
    ) -> Result<Option<SearchFlight>, ConnectSendError> {
        let Some((pending_token, query)) = self.pending_typed_search.clone() else {
            return Ok(None);
        };
        if pending_token != token {
            return Ok(None);
        }
        self.pending_typed_search = None;
        self.send_typed_search(&query)
    }

    fn clear_typed_debounce(&mut self) {
        self.pending_typed_search = None;
        self.search_debounce_token = self.search_debounce_token.saturating_add(1);
    }

    /// Correlate a failed typed search: drop the pending requests and mark
    /// all three searches errored so the status resolves instead of
    /// stranding the query in `Searching`.
    fn abort_typed_search(
        &mut self,
        chats_extra: RequestId,
        messages_extra: RequestId,
        public_extra: RequestId,
    ) {
        self.session.requests.take(chats_extra);
        self.session.requests.take(messages_extra);
        self.session.requests.take(public_extra);
        self.session.search.accept_chats(Vec::new(), true);
        self.session.search.accept_messages(Vec::new(), true);
        self.session.search.accept_public_chats(Vec::new(), true);
    }

    /// Typed query: `searchChats` + `searchPublicChats` + `searchMessages`
    /// (Phase 7.2 adds the public username lookup alongside the offline
    /// known-chat search).
    fn send_typed_search(
        &mut self,
        trimmed: &str,
    ) -> Result<Option<SearchFlight>, ConnectSendError> {
        let search_gen = self.session.search.generation;
        let chats_extra = self
            .session
            .request_search(RequestPurpose::SearchChats, search_gen);
        let messages_extra = self
            .session
            .request_search(RequestPurpose::SearchMessages, search_gen);
        let public_extra = self
            .session
            .request_search(RequestPurpose::SearchPublicChats, search_gen);
        if let Err(err) = self
            .sender
            .send_json(&search_chats(chats_extra, trimmed, SEARCH_LIMIT))
        {
            self.abort_typed_search(chats_extra, messages_extra, public_extra);
            return Err(err);
        }
        if let Err(err) =
            self.sender
                .send_json(&search_messages(messages_extra, trimmed, SEARCH_LIMIT))
        {
            self.abort_typed_search(chats_extra, messages_extra, public_extra);
            return Err(err);
        }
        match self
            .sender
            .send_json(&search_public_chats(public_extra, trimmed))
        {
            Ok(()) => Ok(Some(SearchFlight::Query(
                chats_extra,
                messages_extra,
                public_extra,
            ))),
            Err(err) => {
                self.abort_typed_search(chats_extra, messages_extra, public_extra);
                Err(err)
            }
        }
    }

    fn request_recents(&mut self) -> Result<Option<SearchFlight>, ConnectSendError> {
        let search_gen = self.session.search.begin_recents();
        let extra = self
            .session
            .request_search(RequestPurpose::SearchRecentlyFoundChats, search_gen);
        match self
            .sender
            .send_json(&search_recently_found_chats(extra, "", RECENT_SEARCH_LIMIT))
        {
            Ok(()) => Ok(Some(SearchFlight::Recents(extra))),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.search.accept_chats(Vec::new(), true);
                Err(err)
            }
        }
    }

    fn remember_found_chat(&mut self, chat_id: ChatId) {
        let extra = self
            .session
            .request(RequestPurpose::AddRecentlyFoundChat, Some(chat_id));
        if self
            .sender
            .send_json(&add_recently_found_chat(extra, chat_id))
            .is_err()
        {
            self.session.requests.take(extra);
        }
    }

    /// Open a chat from search via `addRecentlyFoundChat` then `openChat`.
    /// Flushes the leaving composer's draft before `select_chat` drops `pending_draft`.
    pub fn select_search_chat(
        &mut self,
        chat_id: ChatId,
        leaving_text: &str,
        leaving_reply: Option<MessageId>,
        now_ms: u64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.flush_leaving_composer(chat_id, leaving_text, leaving_reply, now_ms)?;
        self.remember_found_chat(chat_id);
        self.session.close_search();
        self.select_chat(chat_id)
    }

    /// Jump to a found message: upsert it into history, then `select_chat`.
    /// Same flush-before-drop as [`Self::select_search_chat`].
    pub fn select_search_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        leaving_text: &str,
        leaving_reply: Option<MessageId>,
        now_ms: u64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.flush_leaving_composer(chat_id, leaving_text, leaving_reply, now_ms)?;
        self.remember_found_chat(chat_id);
        self.session.promote_search_message(chat_id, message_id);
        self.session.close_search();
        self.select_chat(chat_id)
    }

    /// Persist the open chat's composer before a search result switches chats.
    fn flush_leaving_composer(
        &mut self,
        next_chat: ChatId,
        leaving_text: &str,
        leaving_reply: Option<MessageId>,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        let Some(prev) = self.session.open_chat else {
            return Ok(());
        };
        if prev == next_chat {
            return Ok(());
        }
        self.note_composer_draft(prev, leaving_text, leaving_reply, now_ms, false)?;
        Ok(())
    }

    /// tdesktop `searchInChat` when history is focused (`Command::Search` / Ctrl+F).
    pub fn open_chat_search(&mut self) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        Ok(self.session.open_chat_search())
    }

    pub fn close_chat_search(&mut self) {
        self.clear_chat_search_debounce();
        self.session.close_chat_search();
    }

    /// Empty query: clear immediately (tdesktop ComposeSearch skips empty).
    /// Non-empty: debounce `AutoSearchTimeout` (900 ms), then `searchChatMessages`.
    pub fn set_chat_search_query(
        &mut self,
        query: &str,
    ) -> Result<ChatSearchQueryOutcome, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_search.open && !self.session.open_chat_search() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let trimmed = query.trim();
        if trimmed.is_empty() {
            self.clear_chat_search_debounce();
            if self.session.chat_search.query.is_empty()
                && matches!(
                    self.session.chat_search.status,
                    SearchStatus::Idle | SearchStatus::Closed
                )
            {
                return Ok(ChatSearchQueryOutcome::Unchanged);
            }
            self.session.chat_search.clear_query();
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        if self
            .pending_typed_chat_search
            .as_ref()
            .is_some_and(|(_, q)| q == trimmed)
            && self.session.chat_search.query == trimmed
        {
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        if self.pending_typed_chat_search.is_none()
            && self.session.chat_search.query == trimmed
            && matches!(
                self.session.chat_search.status,
                SearchStatus::Searching
                    | SearchStatus::Ready
                    | SearchStatus::Empty
                    | SearchStatus::Failed
            )
        {
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        let _search_gen = self.session.chat_search.begin_query(trimmed);
        self.chat_search_debounce_token = self.chat_search_debounce_token.saturating_add(1);
        let token = self.chat_search_debounce_token;
        self.pending_typed_chat_search = Some((token, trimmed.to_string()));
        Ok(ChatSearchQueryOutcome::Debounced { token })
    }

    pub fn commit_debounced_chat_search(
        &mut self,
        token: u64,
    ) -> Result<Option<ChatSearchFlight>, ConnectSendError> {
        let Some((pending_token, query)) = self.pending_typed_chat_search.clone() else {
            return Ok(None);
        };
        if pending_token != token {
            return Ok(None);
        }
        self.pending_typed_chat_search = None;
        self.send_chat_search(&query)
    }

    fn clear_chat_search_debounce(&mut self) {
        self.pending_typed_chat_search = None;
        self.chat_search_debounce_token = self.chat_search_debounce_token.saturating_add(1);
    }

    fn send_chat_search(
        &mut self,
        trimmed: &str,
    ) -> Result<Option<ChatSearchFlight>, ConnectSendError> {
        let Some(chat_id) = self.session.chat_search.chat_id.or(self.session.open_chat) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let search_gen = self.session.chat_search.generation;
        let extra = self.session.request_chat_search(
            RequestPurpose::SearchChatMessages,
            chat_id,
            search_gen,
        );
        match self.sender.send_json(&search_chat_messages(
            extra,
            chat_id,
            &TopicId::None,
            trimmed,
            MessageId(0),
            0,
            CHAT_SEARCH_LIMIT,
        )) {
            Ok(()) => Ok(Some(ChatSearchFlight::Query(extra))),
            Err(err) => {
                self.session.requests.take(extra);
                self.session
                    .chat_search
                    .accept_hits(Vec::new(), 0, MessageId(0), true);
                Err(err)
            }
        }
    }

    pub fn jump_to_chat_search_message(
        &mut self,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        match self.session.begin_chat_search_jump(message_id) {
            ChatSearchJumpNeed::AlreadyReady | ChatSearchJumpNeed::Missing => Ok(None),
            ChatSearchJumpNeed::LoadAround => self.fetch_history_around(message_id),
        }
    }

    /// Quote-strip activation: same Unigram `LoadMessageSliceImpl` around-load
    /// + highlight pipeline as in-chat search jump (`getChatHistory` offset -25).
    pub fn jump_to_replied_message(
        &mut self,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.jump_to_chat_search_message(message_id)
    }

    pub fn jump_selected_chat_search_hit(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(message_id) = self
            .session
            .chat_search
            .selected_hit()
            .map(|hit| hit.message_id)
        else {
            return Ok(None);
        };
        self.jump_to_chat_search_message(message_id)
    }

    pub fn chat_search_newer(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(message_id) = self.session.chat_search.select_newer() else {
            return Ok(None);
        };
        self.jump_to_chat_search_message(message_id)
    }

    pub fn chat_search_older(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(message_id) = self.session.chat_search.select_older() else {
            return Ok(None);
        };
        self.jump_to_chat_search_message(message_id)
    }

    /// Unigram `GetChatHistory(chatId, maxId, -25, 50)` around the jump target.
    fn fetch_history_around(
        &mut self,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let extra = self.session.request_history_around(chat_id, message_id);
        match self.sender.send_json(&get_chat_history(
            extra,
            chat_id,
            message_id,
            HISTORY_AROUND_OFFSET,
            HISTORY_AROUND_LIMIT,
            false,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.chat_search.jump =
                    crate::state::ChatSearchJump::Missing { message_id };
                Err(err)
            }
        }
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
}

/// How long Drop / `--connect-smoke` waits for `authorizationStateClosed`.
pub const CLIENT_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Send `close` and ingest until Closed. Does not join a receive thread or
/// unload tdjson. Returns whether Closed was observed.
pub fn wait_closed<S: JsonSender>(
    driver: &mut ConnectDriver<S>,
    mut recv: impl FnMut(Duration) -> Option<OwnedEnvelope>,
    timeout: Duration,
) -> bool {
    if matches!(driver.session.auth, AuthorizationState::Closed) {
        return true;
    }
    let _ = driver.request_close();
    let deadline = Instant::now() + timeout;
    loop {
        if matches!(driver.session.auth, AuthorizationState::Closed) {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        let slice = deadline
            .saturating_duration_since(now)
            .min(Duration::from_millis(50));
        if let Some(owned) = recv(slice) {
            let _ = driver.ingest(owned);
        }
    }
}

/// Open LiveTdJson + receive bridge when gate + restore succeed.
///
/// Drop / [`LiveConnect::shutdown`] send `close`, wait for
/// `authorizationStateClosed`, then join the receive thread **before**
/// `libtdjson` is unloaded. Unloading while TDLib worker threads are still
/// running is what produced SIGSEGV (exit 139) after `--connect-smoke`.
///
/// Field order: `bridge` is dropped before `_live` (declaration order) so
/// `td_receive` is not in-flight during `dlclose`.
pub struct LiveConnect {
    pub driver: ConnectDriver<LiveSender>,
    pub bridge: ReceiveBridge,
    _live: LiveTdJson,
}

impl LiveConnect {
    /// Close the TDLib client and join the receive thread. Safe to call twice.
    /// Does not panic; a timeout still joins the thread so Drop can unload.
    pub fn shutdown(&mut self, wait: Duration) {
        if self.bridge.is_joined() {
            return;
        }
        let _ = wait_closed(
            &mut self.driver,
            |timeout| self.bridge.next_timeout(timeout),
            wait,
        );
        self.bridge.shutdown();
    }
}

impl Drop for LiveConnect {
    fn drop(&mut self) {
        self.shutdown(CLIENT_CLOSE_TIMEOUT);
    }
}

pub fn start_live_connect(
    credentials: TelegramCredentials,
    store: &(impl SecretStore + ?Sized),
    diagnostics: Arc<dyn DiagnosticSink>,
) -> Result<LiveConnect, ConnectBlocker> {
    match evaluate_gate(true) {
        ConnectGate::Blocked(b) => return Err(b),
        ConnectGate::Ready { .. } => {}
    }
    let app_root = default_app_root();
    let prepared = prepare_connect(&app_root, AccountKey::primary(), store, &credentials)?;
    let live = LiveTdJson::connect().map_err(|e| match e {
        TdJsonError::NotFound => ConnectBlocker::MissingTdjson,
        _ => ConnectBlocker::TdjsonLoad,
    })?;
    let sender = LiveSender::from_live(&live);
    let bridge = ReceiveBridge::spawn_live(live.api.clone(), diagnostics.clone());
    let session = Session::new(prepared.account.clone(), diagnostics.clone());
    let mut driver = ConnectDriver::new(session, sender, credentials, prepared);
    match crate::calls::engine::NtgcallsEngine::load() {
        Ok(engine) => {
            driver.set_call_engine(Box::new(engine));
            diagnostics.record(Diagnostic {
                category: "call",
                type_name: None,
                extra: None,
                seq: None,
                note: "call-engine-ready",
            });
        }
        Err(_) => diagnostics.record(Diagnostic {
            category: "call",
            type_name: None,
            extra: None,
            seq: None,
            note: "call-engine-unavailable-signaling-only",
        }),
    }
    driver.kickoff().map_err(|_| ConnectBlocker::TdjsonLoad)?;
    Ok(LiveConnect {
        driver,
        bridge,
        _live: live,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calls::engine::{
        CallEngine, MediaDevice, MediaDeviceKind, MockEngine, RemoteVideoState, VideoFrame,
    };
    use crate::diagnostics::MemorySink;
    use crate::platform::MemorySecretStore;
    use crate::state::{ActiveCall, ActiveGroupCall};
    use crate::telegram::client::copy_and_parse;
    use crate::telegram::envelope::ChannelMemberStatus;
    use serde_json::Value;
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::AtomicU64;

    static TDJSON_ENV_LOCK: StdMutex<()> = StdMutex::new(());

    fn test_credentials() -> TelegramCredentials {
        TelegramCredentials {
            api_id: 99,
            api_hash: "unit-test-hash-not-for-network".into(),
        }
    }

    fn prepared_tmp(store: &MemorySecretStore) -> (std::path::PathBuf, PreparedConnect) {
        let dir = std::env::temp_dir().join(format!(
            "quill-connect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let prepared = prepare_connect(&dir, AccountKey::primary(), store, &test_credentials())
            .expect("prepare");
        (dir, prepared)
    }

    #[test]
    fn gate_blocks_without_credentials() {
        assert_eq!(
            evaluate_gate(false),
            ConnectGate::Blocked(ConnectBlocker::MissingCredentials)
        );
    }

    #[test]
    fn gate_blocks_without_tdjson_when_credentials_present() {
        let _lock = TDJSON_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var("QUILL_TDJSON_PATH").ok();
        // SAFETY: exclusive lock; restored below.
        unsafe { std::env::remove_var("QUILL_TDJSON_PATH") };
        let gate = evaluate_gate(true);
        if let Some(v) = previous {
            unsafe { std::env::set_var("QUILL_TDJSON_PATH", v) };
        }
        assert_eq!(gate, ConnectGate::Blocked(ConnectBlocker::MissingTdjson));
        assert!(
            ConnectBlocker::MissingTdjson
                .user_message()
                .contains("QUILL_TDJSON_PATH")
        );
        assert!(
            ConnectBlocker::MissingTdjson
                .user_message()
                .contains("native-bundle")
        );
    }

    #[test]
    fn set_tdlib_parameters_shape_includes_hash_but_debug_redacts_credentials() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let creds = test_credentials();
        let params = build_set_tdlib_parameters(&creds, &prepared.paths, &prepared.database_key);
        let json = params.to_json(RequestId(7));
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setTdlibParameters");
        assert_eq!(v["@extra"], "7");
        assert_eq!(v["api_id"], 99);
        assert_eq!(v["api_hash"], "unit-test-hash-not-for-network");
        assert_eq!(v["use_secret_chats"], true);
        assert_eq!(v["use_file_database"], true);
        assert!(v["database_directory"].as_str().unwrap().contains("tdlib"));
        assert!(!v["database_encryption_key"].as_str().unwrap().is_empty());
        let debug = format!("{creds:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("unit-test-hash"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase B1: the three secret-chat request shapes
    /// (`createNewSecretChat`, `getSecretChat`, `closeSecretChat`;
    /// schema 1.8.67 lines 13340, 11516, 15242).
    #[test]
    fn secret_chat_request_shapes() {
        let create = create_new_secret_chat(RequestId(11), 41);
        let v: Value = serde_json::from_str(&create).unwrap();
        assert_eq!(v["@type"], "createNewSecretChat");
        assert_eq!(v["@extra"], "11");
        assert_eq!(v["user_id"], 41);

        let get = get_secret_chat(RequestId(12), 7);
        let v: Value = serde_json::from_str(&get).unwrap();
        assert_eq!(v["@type"], "getSecretChat");
        assert_eq!(v["@extra"], "12");
        assert_eq!(v["secret_chat_id"], 7);

        let close = close_secret_chat_request(RequestId(13), 7);
        let v: Value = serde_json::from_str(&close).unwrap();
        assert_eq!(v["@type"], "closeSecretChat");
        assert_eq!(v["@extra"], "13");
        assert_eq!(v["secret_chat_id"], 7);
    }

    /// Phase B4: `setChatMessageAutoDeleteTime` value rule (schema 1.8.67,
    /// line 13454) is enforced driver-side: secret chats accept arbitrary
    /// non-negative seconds; other chats need 0 or day-multiples up to a
    /// year. Unknown chats are rejected too.
    #[test]
    fn driver_validates_auto_delete_time_values() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        for json in [
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":31,"user_id":7,"state":{"@type":"secretChatStateReady"},"is_outbound":true,"key_hash":"","layer":144}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Cloud","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        }
        // Secret chat: arbitrary seconds are fine; negatives are not.
        for secs in [0, 5, 90, 3600, 604800] {
            let extra = driver
                .set_chat_message_auto_delete_time(ChatId(31), secs)
                .expect("secret chat accepts arbitrary seconds");
            let sent = recorder.snapshot();
            let last = sent.last().unwrap();
            assert!(last.contains("setChatMessageAutoDeleteTime"));
            assert!(last.contains(&format!("\"message_auto_delete_time\":{secs}")));
            assert!(last.contains(&format!("\"@extra\":\"{}\"", extra.0)));
        }
        assert_eq!(
            driver.set_chat_message_auto_delete_time(ChatId(31), -1),
            Err(ConnectSendError::InvalidRequest)
        );
        // Regular chat: 0 or day-multiples up to 365 days.
        for secs in [0, 86_400, 604_800, 2_592_000, 365 * 86_400] {
            driver
                .set_chat_message_auto_delete_time(ChatId(11), secs)
                .expect("regular chat accepts 0 / day multiples");
        }
        for secs in [-1, 5, 3600, 90_000, 365 * 86_400 + 86_400] {
            assert_eq!(
                driver.set_chat_message_auto_delete_time(ChatId(11), secs),
                Err(ConnectSendError::InvalidRequest),
                "regular chat rejects {secs}"
            );
        }
        // Unknown chat: rejected.
        assert_eq!(
            driver.set_chat_message_auto_delete_time(ChatId(999), 3600),
            Err(ConnectSendError::InvalidRequest)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase D3c: `fetch_chat_event_log` sends one `getChatEventLog`
    /// (null filters, 100-event page, cursor 0), dedupes while a request
    /// is in flight, pages older via `fetch_chat_event_log_more`, and
    /// no-ops for non-admins / unknown chats / an inactive chats path
    /// (schema 1.8.67, line 15252).
    #[test]
    fn driver_fetch_chat_event_log_sends_dedupes_and_gates() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);

        // Chats path inactive before authorization is Ready: rejected.
        assert_eq!(
            driver.fetch_chat_event_log(ChatId(13)),
            Err(ConnectSendError::InvalidRequest)
        );

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        // Unknown chat and non-admin channel: quiet no-ops (other Ready
        // bookkeeping like loadChats may still send).
        assert_eq!(driver.fetch_chat_event_log(ChatId(999)), Ok(None));
        assert_eq!(driver.fetch_chat_event_log(ChatId(13)), Ok(None));
        let event_log_sends = || {
            recorder
                .snapshot()
                .into_iter()
                .filter(|sent| sent.contains("getChatEventLog"))
                .collect::<Vec<_>>()
        };
        assert!(event_log_sends().is_empty());

        // Become an administrator: the first page goes out.
        driver
            .session
            .chats
            .get_mut(&13)
            .unwrap()
            .set_member_status(ChannelMemberStatus::Administrator, None);
        let extra = driver
            .fetch_chat_event_log(ChatId(13))
            .expect("event log fetch")
            .expect("request id");
        let sent = event_log_sends();
        assert_eq!(sent.len(), 1);
        let first: Value = serde_json::from_str(&sent[0]).unwrap();
        assert_eq!(first["@type"], "getChatEventLog");
        assert_eq!(first["@extra"], extra.0.to_string());
        assert_eq!(first["chat_id"], 13);
        assert_eq!(first["query"], "");
        assert_eq!(first["from_event_id"], 0);
        assert_eq!(first["limit"], 100);
        assert!(first["filters"].is_null());
        assert!(first["user_ids"].as_array().unwrap().is_empty());

        // In-flight dedupe: further fetches no-op while the first is out.
        assert_eq!(driver.fetch_chat_event_log(ChatId(13)), Ok(None));
        assert_eq!(driver.fetch_chat_event_log_more(ChatId(13)), Ok(None));
        assert_eq!(event_log_sends().len(), 1);

        // Feed a full page (100 events) back through the reducer; the
        // next older page goes out with the oldest event id as cursor.
        let events: Vec<String> = (401..=500)
            .rev()
            .map(|id| {
                format!(
                    r#"{{"@type":"chatEvent","id":{id},"date":1700000000,"member_id":{{"@type":"messageSenderUser","user_id":777}},"action":{{"@type":"chatEventMemberJoined"}}}}"#
                )
            })
            .collect();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chatEvents","@extra":"{}","events":[{}]}}"#,
                        extra.0,
                        events.join(",")
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let extra_more = driver
            .fetch_chat_event_log_more(ChatId(13))
            .expect("event log more")
            .expect("request id");
        let sent = event_log_sends();
        assert_eq!(sent.len(), 2);
        let more: Value = serde_json::from_str(&sent[1]).unwrap();
        assert_eq!(more["@type"], "getChatEventLog");
        assert_eq!(more["@extra"], extra_more.0.to_string());
        assert_eq!(more["from_event_id"], 401);

        // A plain fetch after a load is cached: no-op.
        assert_eq!(driver.fetch_chat_event_log(ChatId(13)), Ok(None));
        assert_eq!(event_log_sends().len(), 2);

        // Complete the "more" request with a short page (log exhausted),
        // then refresh clears the cache and re-sends from the top.
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chatEvents","@extra":"{}","events":[]}}"#,
                        extra_more.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.fetch_chat_event_log_more(ChatId(13)), Ok(None));
        let extra_refresh = driver
            .refresh_chat_event_log(ChatId(13))
            .expect("event log refresh")
            .expect("request id");
        let sent = event_log_sends();
        assert_eq!(sent.len(), 3);
        let refresh: Value = serde_json::from_str(&sent[2]).unwrap();
        assert_eq!(refresh["@extra"], extra_refresh.0.to_string());
        assert_eq!(refresh["from_event_id"], 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase B1: the ordinary `sendMessage` path works for a Ready
    /// secret chat (secret chats are plain chat ids at the send layer);
    /// Pending chats are rejected by the same `can_post` gate as
    /// everything else.
    #[test]
    fn driver_sends_into_ready_secret_chat_only() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        for json in [
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":31,"user_id":7,"state":{"@type":"secretChatStateReady"},"is_outbound":true,"key_hash":"","layer":144}}"#,
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":33,"user_id":7,"state":{"@type":"secretChatStatePending"},"is_outbound":true,"key_hash":"","layer":144}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":33,"title":"Pending secret","type":{"@type":"chatTypeSecret","secret_chat_id":33,"user_id":7},"unread_count":0}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        }
        driver.select_chat(ChatId(31)).unwrap();
        let snap = crate::composer::ComposerSnapshot::capture(
            ChatId(31),
            driver.session.view_generation,
            "CANARY_SECRET_SEND",
        );
        let extra = driver.send_text_snapshot(&snap).unwrap();
        let sent = recorder.snapshot();
        let send_json = sent.last().unwrap();
        assert!(send_json.contains("sendMessage"));
        assert!(send_json.contains("\"chat_id\":31"));
        assert!(send_json.contains("CANARY_SECRET_SEND"));
        assert!(send_json.contains(&format!("\"@extra\":\"{}\"", extra.0)));

        driver.select_chat(ChatId(33)).unwrap();
        let pending_snap = crate::composer::ComposerSnapshot::capture(
            ChatId(33),
            driver.session.view_generation,
            "nope",
        );
        assert_eq!(
            driver.send_text_snapshot(&pending_snap),
            Err(ConnectSendError::InvalidRequest)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_sends_parameters_then_reaches_wait_phone_via_injection() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let kick = driver.kickoff().unwrap();
        assert_eq!(kick.0, 1);

        let seq = AtomicU64::new(0);
        let wait_params = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitTdlibParameters"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_params).unwrap();
        assert!(driver.parameters_sent());

        let sent = recorder.snapshot();
        assert_eq!(sent.len(), 2); // getAuthorizationState + setTdlibParameters
        assert!(sent[0].contains("getAuthorizationState"));
        assert!(sent[1].contains("setTdlibParameters"));
        assert!(sent[1].contains("\"api_id\":99"));
        assert!(sent[1].contains("unit-test-hash-not-for-network"));
        assert!(!sink.rendered().contains("unit-test-hash"));

        let ok = copy_and_parse(r#"{"@type":"ok","@extra":"2"}"#, &seq, &dyn_sink).unwrap();
        driver.ingest(ok).unwrap();
        let wait_phone = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_phone).unwrap();
        assert!(matches!(
            driver.session.auth,
            AuthorizationState::WaitPhoneNumber
        ));
        assert_eq!(
            driver.session.auth_view.action,
            crate::auth::AuthAction::EnterPhone
        );

        let phone_extra = driver.submit_phone("+15551212").unwrap();
        let sent = recorder.snapshot();
        let phone_json = sent.last().unwrap();
        assert!(phone_json.contains("setAuthenticationPhoneNumber"));
        assert!(phone_json.contains(&format!("\"@extra\":\"{}\"", phone_extra.0)));
        assert!(phone_json.contains("+15551212"));
        assert!(!sink.rendered().contains("+15551212"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_submits_code_and_password_only_in_matching_states() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

        assert_eq!(
            driver.submit_code("12345"),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(
            driver.submit_password("secret"),
            Err(ConnectSendError::InvalidRequest)
        );

        let seq = AtomicU64::new(0);
        let wait_code = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitCode","code_info":{"@type":"authenticationCodeInfo","type":{"@type":"authenticationCodeTypeSms","length":5}}}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_code).unwrap();
        assert!(matches!(
            driver.session.auth,
            AuthorizationState::WaitCode {
                code_length: Some(5)
            }
        ));
        assert_eq!(
            driver.submit_password("secret"),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(
            driver.submit_code("  "),
            Err(ConnectSendError::InvalidRequest)
        );
        let code_extra = driver.submit_code("  12345 ").unwrap();
        let sent = recorder.snapshot();
        let code_json = sent.last().unwrap();
        assert!(code_json.contains("checkAuthenticationCode"));
        assert!(code_json.contains(&format!("\"@extra\":\"{}\"", code_extra.0)));
        assert!(code_json.contains("\"code\":\"12345\""));
        assert!(!sink.rendered().contains("12345"));

        let wait_password = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPassword","password_hint":"CANARY_HINT","has_recovery_email_address":true}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_password).unwrap();
        assert_eq!(
            driver.submit_code("12345"),
            Err(ConnectSendError::InvalidRequest)
        );
        let pw_extra = driver.submit_password(" unit-pw ").unwrap();
        let sent = recorder.snapshot();
        let pw_json = sent.last().unwrap();
        assert!(pw_json.contains("checkAuthenticationPassword"));
        assert!(pw_json.contains(&format!("\"@extra\":\"{}\"", pw_extra.0)));
        // Password is not trimmed.
        assert!(pw_json.contains("\"password\":\" unit-pw \""));
        assert!(!sink.rendered().contains("unit-pw"));
        assert!(!sink.rendered().contains("CANARY_HINT"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wait_closed_sends_close_and_reaches_closed_via_injection() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

        let seq = AtomicU64::new(0);
        let wait_phone = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_phone).unwrap();

        let envelopes = vec![
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosing"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosed"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        ];
        let mut iter = envelopes.into_iter();
        assert!(wait_closed(
            &mut driver,
            |_| iter.next(),
            Duration::from_secs(2)
        ));
        assert!(matches!(driver.session.auth, AuthorizationState::Closed));
        let sent = recorder.snapshot();
        let close_json = sent.last().expect("close request");
        assert!(close_json.contains("\"@type\":\"close\""));
        assert!(!sink.rendered().contains("unit-test-hash"));
        // Second call is a no-op once Closed (no extra send).
        let before = sent.len();
        assert!(wait_closed(&mut driver, |_| None, Duration::ZERO));
        assert_eq!(recorder.snapshot().len(), before);
        drop(driver);
        drop(recorder);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_tdjson_message_is_actionable() {
        let msg = ConnectBlocker::MissingTdjson.user_message();
        assert!(msg.contains("QUILL_TDJSON_PATH"));
        assert!(msg.contains("never searched"));
    }

    #[test]
    fn driver_loads_chats_after_ready_then_send_text() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

        assert_eq!(
            driver.select_chat(ChatId(1)),
            Err(ConnectSendError::InvalidRequest)
        );

        let seq = AtomicU64::new(0);
        let ready = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(ready).unwrap();
        assert!(matches!(driver.session.auth, AuthorizationState::Ready));

        let sent = recorder.snapshot();
        // Phase 9.1: `loadActiveStories(storyListMain)` follows `loadChats`
        // after Ready (feeds the story tray). Parity slice: the notification
        // slice then fetches the saved-sound list and the three scope
        // defaults (`getSavedNotificationSounds`,
        // `getScopeNotificationSettings` × 3).
        let types: Vec<&str> = sent
            .iter()
            .map(|json| {
                if json.contains(r#""@type":"loadChats""#) {
                    "loadChats"
                } else if json.contains(r#""@type":"loadActiveStories""#) {
                    "loadActiveStories"
                } else if json.contains(r#""@type":"getSavedNotificationSounds""#) {
                    "getSavedNotificationSounds"
                } else if json.contains(r#""@type":"getScopeNotificationSettings""#) {
                    "getScopeNotificationSettings"
                } else {
                    "other"
                }
            })
            .collect();
        assert_eq!(
            types,
            vec![
                "loadChats",
                "loadActiveStories",
                "getSavedNotificationSounds",
                "getScopeNotificationSettings",
                "getScopeNotificationSettings",
                "getScopeNotificationSettings",
            ]
        );
        let load = &sent[0];
        assert!(load.contains("chatListMain"));
        assert!(load.contains(&format!("\"limit\":{MAIN_CHAT_LOAD_LIMIT}")));
        let load_extra = driver
            .session
            .requests
            .has_purpose(RequestPurpose::LoadChats);
        assert!(load_extra);

        let new_chat = copy_and_parse(
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(new_chat).unwrap();
        let position = copy_and_parse(
            r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"12","is_pinned":false}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(position).unwrap();
        assert_eq!(driver.session.ordered_chats()[0].id.0, 7);

        // In-flight loadChats: ingest of unrelated updates must not send another page.
        let loads_before = recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .count();
        assert_eq!(loads_before, 1);

        assert!(
            driver
                .session
                .requests
                .has_purpose(RequestPurpose::LoadChats)
        );
        let load_ok = copy_and_parse(r#"{"@type":"ok","@extra":"1"}"#, &seq, &dyn_sink).unwrap();
        driver.ingest(load_ok).unwrap();
        // ok on loadChats means more may exist — one continuation page, not a per-tick loop.
        let loads_after_ok = recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .count();
        assert_eq!(loads_after_ok, 2);

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateUpdating"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            recorder
                .snapshot()
                .iter()
                .filter(|j| j.contains("loadChats"))
                .count(),
            2,
            "unrelated ingest must not re-page loadChats"
        );

        let last_load = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|j| j.contains("loadChats"))
            .unwrap();
        let v: Value = serde_json::from_str(&last_load).unwrap();
        let extra = v["@extra"].as_str().unwrap();
        let err404 = copy_and_parse(
            &format!(r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{extra}"}}"#),
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(err404).unwrap();
        assert!(driver.session.chats_exhausted);
        let loads_done = recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .count();
        assert_eq!(loads_done, 2);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            recorder
                .snapshot()
                .iter()
                .filter(|j| j.contains("loadChats"))
                .count(),
            loads_done
        );
        assert!(!sink.rendered().contains("Not Found"));

        let history_extra = driver.select_chat(ChatId(7)).unwrap().expect("history");
        let sent = recorder.snapshot();
        assert!(
            sent.iter()
                .any(|j| j.contains("\"@type\":\"openChat\"") && j.contains("\"chat_id\":7")),
            "select_chat must send openChat"
        );
        let history_json = sent.last().unwrap();
        assert!(history_json.contains("getChatHistory"));
        assert!(history_json.contains("\"chat_id\":7"));
        assert!(history_json.contains(&format!("\"@extra\":\"{}\"", history_extra.0)));

        let messages = copy_and_parse(
            &format!(
                r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":11,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}]}}"#,
                history_extra.0
            ),
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(messages).unwrap();
        assert!(
            driver
                .session
                .histories
                .get(&7)
                .unwrap()
                .messages
                .contains_key(&11)
        );
        let view_json = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|j| j.contains("viewMessages"))
            .expect("viewMessages after history");
        let view: Value = serde_json::from_str(&view_json).unwrap();
        assert_eq!(view["@type"], "viewMessages");
        assert_eq!(view["chat_id"], 7);
        assert_eq!(view["message_ids"], serde_json::json!([11]));
        assert_eq!(view["source"]["@type"], "messageSourceChatHistory");
        assert_eq!(view["force_read"], true);
        assert!(!view_json.contains("hi"));

        let snap = crate::composer::ComposerSnapshot::capture(
            ChatId(7),
            driver.session.view_generation,
            "CANARY_SEND_ping",
        );
        let send_extra = driver.send_text_snapshot(&snap).unwrap();
        let sent = recorder.snapshot();
        let send_json = sent.last().unwrap();
        assert!(send_json.contains("sendMessage"));
        assert!(send_json.contains("\"topic_id\":null"));
        assert!(send_json.contains("CANARY_SEND_ping"));
        assert!(send_json.contains(&format!("\"@extra\":\"{}\"", send_extra.0)));
        assert!(!sink.rendered().contains("CANARY_SEND"));

        let channel_no_post = copy_and_parse(
            r#"{"@type":"updateNewChat","chat":{"id":8,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":8,"is_channel":true},"unread_count":0}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(channel_no_post).unwrap();
        let channel_snap = crate::composer::ComposerSnapshot::capture(
            ChatId(8),
            driver.session.view_generation,
            "nope",
        );
        assert_eq!(
            driver.send_text_snapshot(&channel_snap),
            Err(ConnectSendError::InvalidRequest)
        );
        assert!(!sink.rendered().contains("CANARY_SEND"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_send_text_snapshot_in_topic_addresses_message_topic_forum() {
        // Parity slice 4: a send from a topic view carries
        // `topic_id = messageTopicForum{forum_topic_id}` (schema 1.8.67,
        // lines 12200 / 3004); a closed topic rejects the send.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        for json in [
            r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Demo forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
            r#"{"@type":"updateChatPosition","chat_id":16,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"25","is_pinned":false}}"#,
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        }
        // Inject the topic list through the reducer; topic 3 is closed.
        let extra = driver
            .session
            .request(RequestPurpose::GetForumTopics, Some(ChatId(16)));
        let topics = [(2, "General", false), (3, "Random", true)]
            .iter()
            .map(|(id, name, closed)| {
                format!(
                    "{{\"info\":{{\"@type\":\"forumTopicInfo\",\"chat_id\":16,\"forum_topic_id\":{id},\"name\":\"{name}\",\"is_general\":false,\"is_closed\":{closed}}},\"order\":\"{id}\",\"is_pinned\":false,\"unread_count\":0}}"
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        "{{\"@type\":\"forumTopics\",\"@extra\":\"{}\",\"total_count\":2,\"topics\":[{topics}],\"next_offset_date\":0,\"next_offset_message_id\":0,\"next_offset_forum_topic_id\":0}}",
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        driver.session.open_chat(ChatId(16));
        driver.session.select_topic(ChatId(16), 2);
        let snap = crate::composer::ComposerSnapshot::capture(
            ChatId(16),
            driver.session.view_generation,
            "CANARY_TOPIC_ping",
        );
        let send_extra = driver.send_text_snapshot(&snap).unwrap();
        let sent = recorder.snapshot();
        let send_json = sent.last().unwrap();
        let v: Value = serde_json::from_str(send_json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["chat_id"], 16);
        assert_eq!(v["topic_id"]["@type"], "messageTopicForum");
        assert_eq!(v["topic_id"]["forum_topic_id"], 2);
        assert!(send_json.contains("CANARY_TOPIC_ping"));
        assert!(send_json.contains(&format!("\"@extra\":\"{}\"", send_extra.0)));

        // A closed topic rejects the send (the composer is hidden there;
        // this guards a stale-snapshot race).
        driver.session.select_topic(ChatId(16), 3);
        let closed_snap = crate::composer::ComposerSnapshot::capture(
            ChatId(16),
            driver.session.view_generation,
            "nope",
        );
        assert_eq!(
            driver.send_text_snapshot(&closed_snap),
            Err(ConnectSendError::InvalidRequest)
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_load_folder_chats_pages_chat_list_folder() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        assert_eq!(
            driver.load_folder_chats(2),
            Err(ConnectSendError::InvalidRequest)
        );

        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        let extra = driver.load_folder_chats(2).expect("folder load");
        let sent = recorder.snapshot();
        let folder_load = sent
            .iter()
            .rev()
            .find(|j| j.contains("chatListFolder"))
            .expect("loadChats(chatListFolder)");
        let v: Value = serde_json::from_str(folder_load).unwrap();
        assert_eq!(v["@type"], "loadChats");
        assert_eq!(v["chat_list"]["@type"], "chatListFolder");
        assert_eq!(v["chat_list"]["chat_folder_id"], 2);
        assert_eq!(v["limit"], MAIN_CHAT_LOAD_LIMIT);
        assert_eq!(v["@extra"].as_str().unwrap(), extra.0.to_string());
        assert!(
            driver
                .session
                .requests
                .has_purpose_for_folder(RequestPurpose::LoadFolderChats, 2)
        );

        // Parity slice: the ok response pages on (folder "load more") — a
        // second `loadChats(chatListFolder)` goes out, not a main-list page.
        let loads_before = recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .count();
        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let loads: Vec<String> = recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .cloned()
            .collect();
        assert_eq!(loads.len(), loads_before + 1);
        assert!(loads.last().unwrap().contains("chatListFolder"));
        assert!(!loads.last().unwrap().contains("chatListMain"));
        let second_extra = driver
            .session
            .requests
            .pending_extra_for_folder(RequestPurpose::LoadFolderChats, 2)
            .expect("second page in flight");

        // A 404 marks the folder exhausted: the next ok pages no further.
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","@extra":"{}","code":404,"message":"not found"}}"#,
                        second_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(driver.session.folder_chats_exhausted.contains(&2));
        assert_eq!(
            driver.maybe_load_folder_chats(2).unwrap(),
            None,
            "exhausted folder pages no more"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_create_chat_folder_sends_create_chat_folder() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        let mut editor = crate::folders::FolderEditor::new();
        editor.name = "Work".to_string();
        editor.include_groups = true;
        editor.toggle_included(7);
        let spec = editor.to_spec();
        let extra = driver.create_chat_folder(&spec).expect("create");
        let json = recorder.snapshot().last().cloned().expect("sent");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "createChatFolder");
        assert_eq!(v["@extra"].as_str().unwrap(), extra.0.to_string());
        assert_eq!(v["folder"]["@type"], "chatFolder");
        assert_eq!(v["folder"]["name"]["text"]["text"], "Work");
        assert_eq!(v["folder"]["included_chat_ids"], serde_json::json!([7]));
        assert_eq!(v["folder"]["include_groups"], true);
        assert!(
            driver
                .session
                .requests
                .has_purpose(RequestPurpose::CreateChatFolder)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_edit_and_delete_folder_flow() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":5,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]}},"icon":null,"color_id":-1,"is_shareable":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        // `getChatFolder` response caches the full spec (keyed by folder id).
        let fetch = driver.fetch_chat_folder(5).unwrap().expect("fetch");
        assert_eq!(
            driver.fetch_chat_folder(5).unwrap(),
            None,
            "deduped in flight"
        );
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chatFolder","@extra":"{}","name":{{"@type":"chatFolderName","text":{{"@type":"formattedText","text":"Work","entities":[]}}}},"icon":null,"color_id":-1,"is_shareable":false,"pinned_chat_ids":[],"included_chat_ids":[7],"excluded_chat_ids":[],"exclude_muted":false,"exclude_read":false,"exclude_archived":false,"include_contacts":false,"include_non_contacts":true,"include_bots":false,"include_groups":false,"include_channels":false}}"#,
                        fetch.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let cached = driver.session.folder_specs.get(&5).expect("cached spec");
        assert_eq!(cached.name, "Work");
        assert_eq!(cached.included_chat_ids, vec![7]);

        // Edit sends `editChatFolder` with the full spec.
        let mut edited = cached.clone();
        edited.name = "Work stuff".to_string();
        let edit_extra = driver.edit_chat_folder(5, &edited).expect("edit");
        let json = recorder.snapshot().last().cloned().expect("sent");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "editChatFolder");
        assert_eq!(v["chat_folder_id"], 5);
        assert_eq!(v["folder"]["name"]["text"]["text"], "Work stuff");
        assert_eq!(v["@extra"].as_str().unwrap(), edit_extra.0.to_string());
        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, edit_extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        // Delete sends `deleteChatFolder` with leave ids; ok drops the tab.
        let delete_extra = driver.delete_chat_folder(5, &[7]).expect("delete");
        let json = recorder.snapshot().last().cloned().expect("sent");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "deleteChatFolder");
        assert_eq!(v["chat_folder_id"], 5);
        assert_eq!(v["leave_chat_ids"], serde_json::json!([7]));
        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, delete_extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(driver.session.chat_folders.is_empty());
        assert!(!driver.session.folder_specs.contains_key(&5));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_reorder_chat_folders_optimistic() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":1,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"A","entities":[]}},"icon":null,"color_id":-1,"is_shareable":false},{"@type":"chatFolderInfo","id":2,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"B","entities":[]}},"icon":null,"color_id":-1,"is_shareable":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        driver.reorder_chat_folders(&[2, 1]).expect("reorder");
        let json = recorder.snapshot().last().cloned().expect("sent");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "reorderChatFolders");
        assert_eq!(v["chat_folder_ids"], serde_json::json!([2, 1]));
        assert_eq!(v["main_chat_list_position"], 0);
        let ids: Vec<i32> = driver.session.chat_folders.iter().map(|f| f.id).collect();
        assert_eq!(ids, vec![2, 1], "optimistic reorder");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_toggle_chat_folder_tags() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        driver.toggle_chat_folder_tags(true).expect("toggle");
        let json = recorder.snapshot().last().cloned().expect("sent");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "toggleChatFolderTags");
        assert_eq!(v["are_tags_enabled"], true);
        assert!(driver.session.are_folder_tags_enabled);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_get_chat_lists_to_add_chat_caches() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        let first = driver
            .fetch_chat_lists_to_add_chat(ChatId(7))
            .unwrap()
            .expect("first");
        assert_eq!(
            driver.fetch_chat_lists_to_add_chat(ChatId(7)).unwrap(),
            None,
            "deduped in flight"
        );
        let json = recorder.snapshot().last().cloned().expect("sent");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getChatListsToAddChat");
        assert_eq!(v["chat_id"], 7);

        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chatLists","@extra":"{}","chat_lists":[{{"@type":"chatListMain"}},{{"@type":"chatListFolder","chat_folder_id":5}}]}}"#,
                        first.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let cached = driver
            .session
            .chat_lists_for_add
            .get(&7)
            .expect("cached lists");
        assert!(cached.contains(&crate::telegram::envelope::ChatList::Main));
        assert!(cached.contains(&crate::telegram::envelope::ChatList::Folder(5)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_add_chat_to_folder_sends_add_chat_to_list() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        let extra = driver.add_chat_to_folder(ChatId(7), 5).expect("add");
        let json = recorder.snapshot().last().cloned().expect("sent");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "addChatToList");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["chat_list"]["@type"], "chatListFolder");
        assert_eq!(v["chat_list"]["chat_folder_id"], 5);
        assert_eq!(v["@extra"].as_str().unwrap(), extra.0.to_string());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_remove_chat_from_folder_edits_spec() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":5,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]}},"icon":null,"color_id":-1,"is_shareable":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        // Chat 7 (Alice, private, user unknown → non-contact) is explicitly
        // included; the folder also matches non-contacts by filter, so
        // removal must both drop it from `included_chat_ids` and add it to
        // `excluded_chat_ids` (no `removeChatFromList` in 1.8.67).
        let fetch = driver.fetch_chat_folder(5).unwrap().expect("fetch");
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chatFolder","@extra":"{}","name":{{"@type":"chatFolderName","text":{{"@type":"formattedText","text":"Work","entities":[]}}}},"icon":null,"color_id":-1,"is_shareable":false,"pinned_chat_ids":[],"included_chat_ids":[7],"excluded_chat_ids":[],"exclude_muted":false,"exclude_read":false,"exclude_archived":false,"include_contacts":false,"include_non_contacts":true,"include_bots":false,"include_groups":false,"include_channels":false}}"#,
                        fetch.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        driver
            .remove_chat_from_folder(ChatId(7), 5)
            .expect("remove");
        assert!(
            driver.session.folder_remove_queue.is_empty(),
            "spec was cached — edit sent immediately"
        );
        let json = recorder
            .snapshot()
            .iter()
            .rev()
            .find(|j| j.contains("editChatFolder"))
            .cloned()
            .expect("editChatFolder sent");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "editChatFolder");
        assert_eq!(v["chat_folder_id"], 5);
        assert_eq!(v["folder"]["included_chat_ids"], serde_json::json!([]));
        assert_eq!(
            v["folder"]["excluded_chat_ids"],
            serde_json::json!([7]),
            "filter-matched chat must be excluded or it would reappear"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_remove_chat_from_folder_waits_for_spec() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        // No cached spec: the intent queues and a `getChatFolder` goes out;
        // the edit follows once the spec arrives (via `ingest`).
        driver
            .remove_chat_from_folder(ChatId(7), 5)
            .expect("remove");
        assert_eq!(driver.session.folder_remove_queue, vec![(ChatId(7), 5)]);
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("getChatFolder")),
            "fetch sent for uncached folder"
        );
        assert!(
            !recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("editChatFolder")),
            "no edit before the spec arrives"
        );
        let fetch_extra = driver
            .session
            .requests
            .pending_extra_for_folder(RequestPurpose::GetChatFolder, 5)
            .expect("fetch in flight");
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chatFolder","@extra":"{}","name":{{"@type":"chatFolderName","text":{{"@type":"formattedText","text":"Work","entities":[]}}}},"icon":null,"color_id":-1,"is_shareable":false,"pinned_chat_ids":[],"included_chat_ids":[7],"excluded_chat_ids":[],"exclude_muted":false,"exclude_read":false,"exclude_archived":false,"include_contacts":false,"include_non_contacts":false,"include_bots":false,"include_groups":false,"include_channels":false}}"#,
                        fetch_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(
            driver.session.folder_remove_queue.is_empty(),
            "edit completed on spec arrival"
        );
        let json = recorder
            .snapshot()
            .iter()
            .rev()
            .find(|j| j.contains("editChatFolder"))
            .cloned()
            .expect("editChatFolder sent after spec");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["folder"]["included_chat_ids"], serde_json::json!([]));
        // No filter flags set: the chat does not match, so no exclusion.
        assert_eq!(v["folder"]["excluded_chat_ids"], serde_json::json!([]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn channel_admin_send_succeeds_non_admin_send_rejected() {
        // Phase 2.3: the driver gate mirrors the composer gate — an admin
        // channel sends `sendMessage` with the channel chat_id; a channel
        // without posting rights rejects like a hidden composer.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        // Admin channel (id 9) and plain-member channel (id 10).
        for (id, title) in [(9, "Admin news"), (10, "Member news")] {
            driver
                .ingest(
                    copy_and_parse(
                        &format!(
                            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{id},"is_channel":true}},"unread_count":0}}}}"#
                        ),
                        &seq,
                        &dyn_sink,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        // getMe + getChatMember leave the viewer as an admin with the posting
        // right in channel 9.
        driver.session.my_user_id = Some(777);
        let me_extra = driver.session.request(RequestPurpose::GetMe, None);
        let admin_extra = driver
            .session
            .request(RequestPurpose::GetChatMember, Some(ChatId(9)));
        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"user","@extra":"{}","id":777}}"#, me_extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_post_messages":true}}}}}}"#,
                        admin_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(driver.session.chats.get(&9).unwrap().can_post());

        let snap = crate::composer::ComposerSnapshot::capture(
            ChatId(9),
            driver.session.view_generation,
            "CANARY_ADMIN_post",
        );
        let send_extra = driver.send_text_snapshot(&snap).unwrap();
        let sent = recorder.snapshot();
        let send_json = sent.last().unwrap();
        assert!(send_json.contains("sendMessage"));
        assert!(send_json.contains("\"chat_id\":9"));
        assert!(send_json.contains("CANARY_ADMIN_post"));
        assert!(send_json.contains(&format!("\"@extra\":\"{}\"", send_extra.0)));

        // Channel 10: membership unknown → composer hidden, send rejected.
        assert!(!driver.session.chats.get(&10).unwrap().can_post());
        let member_snap = crate::composer::ComposerSnapshot::capture(
            ChatId(10),
            driver.session.view_generation,
            "nope",
        );
        assert_eq!(
            driver.send_text_snapshot(&member_snap),
            Err(ConnectSendError::InvalidRequest)
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bot_info_fetched_once_on_chat_open() {
        // Phase 3.1: opening a bot chat lazily sends `getUserFullInfo` once;
        // the `userFullInfo` response populates the cache and suppresses
        // refetches. Non-bot chats never trigger the fetch.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        // Bot private chat (id 21) and a regular private chat (id 22).
        for json in [
            r#"{"@type":"updateUser","user":{"id":21,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":22,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":22},"unread_count":0}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        }
        // The bot chat rides the ordinary private-chat path: no gate, and
        // the composer is shown.
        let bot_chat = driver.session.chats.get(&21).unwrap();
        assert!(bot_chat.supported());
        assert!(bot_chat.kind.gate_reason().is_none());
        assert!(bot_chat.can_post());

        driver.select_chat(ChatId(21)).unwrap();
        let info_fetches = || {
            recorder
                .snapshot()
                .into_iter()
                .filter(|j| j.contains("\"@type\":\"getUserFullInfo\""))
                .collect::<Vec<_>>()
        };
        let first = info_fetches();
        assert_eq!(first.len(), 1);
        assert!(first[0].contains("\"user_id\":21"));
        let extra = driver
            .session
            .requests
            .pending_extra_for(RequestPurpose::GetUserFullInfo, Some(ChatId(21)));

        // Re-selecting while the fetch is in flight sends nothing new.
        driver.select_chat(ChatId(21)).unwrap();
        assert_eq!(info_fetches().len(), 1);

        // The response populates the cache; further opens stay quiet.
        let extra = extra.expect("getUserFullInfo in flight");
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"s","description":"CANARY_bot_desc","commands":[{{"@type":"botCommand","command":"start","description":"Start","is_ephemeral":false}}]}}}}"#,
                        extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let info = driver
            .session
            .bot_info_for_chat(ChatId(21))
            .expect("bot info cached");
        assert_eq!(info.description, "CANARY_bot_desc");
        assert_eq!(info.commands.len(), 1);
        assert_eq!(info.commands[0].command, "start");
        driver.select_chat(ChatId(21)).unwrap();
        assert_eq!(info_fetches().len(), 1);

        // A regular private chat never triggers the fetch.
        driver.select_chat(ChatId(22)).unwrap();
        assert_eq!(info_fetches().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase 6: ready driver with no chats (contacts tests don't need any).
    fn ready_driver(
        recorder: &Arc<RecordingSender>,
        prepared: PreparedConnect,
        dyn_sink: &Arc<dyn DiagnosticSink>,
        seq: &AtomicU64,
    ) -> ConnectDriver<Arc<RecordingSender>> {
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    seq,
                    dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
    }

    #[test]
    fn contacts_fetched_once_for_tab() {
        // Phase 6: `fetch_contacts` sends `getContacts` once; a second
        // call while the fetch is in flight or after the `users` response
        // lands sends nothing new.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        let extra = driver.fetch_contacts().unwrap().expect("getContacts sent");
        let sent = recorder.snapshot();
        let get = sent
            .iter()
            .find(|j| j.contains(r#""@type":"getContacts""#))
            .expect("getContacts in outbox");
        assert!(get.contains(&format!(r#""@extra":"{}""#, extra.0)));
        // In flight → no-op.
        assert_eq!(driver.fetch_contacts().unwrap(), None);
        assert_eq!(recorder.snapshot().len(), sent.len());
        // The `users` response settles the list; further calls stay quiet.
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"users","@extra":"{}","total_count":1,"user_ids":[31]}}"#,
                        extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.contacts.as_deref(), Some([31].as_slice()));
        assert_eq!(driver.fetch_contacts().unwrap(), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn user_full_info_fetched_once_per_user() {
        // Phase 6: `fetch_user_full_info` sends `getUserFullInfo` once per
        // user; the `userFullInfo` response (correlated by
        // `PendingRequest::user_id`, not by chat) caches the bio and
        // suppresses refetches.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        let extra = driver
            .fetch_user_full_info(31)
            .unwrap()
            .expect("getUserFullInfo sent");
        let sent = recorder.snapshot();
        let info_json = sent
            .iter()
            .find(|j| j.contains(r#""@type":"getUserFullInfo""#))
            .expect("getUserFullInfo in outbox");
        assert!(info_json.contains(r#""user_id":31"#));
        // In flight → no-op.
        assert_eq!(driver.fetch_user_full_info(31).unwrap(), None);
        assert_eq!(recorder.snapshot().len(), sent.len());
        // The response caches the bio; further fetches stay quiet.
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"userFullInfo","@extra":"{}","bio":{{"@type":"formattedText","text":"CANARY_bio","entities":[]}},"bot_info":null}}"#,
                        extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            driver.session.user_full_info(31).map(|i| i.bio.as_str()),
            Some("CANARY_bio")
        );
        assert_eq!(driver.fetch_user_full_info(31).unwrap(), None);
        // A different user still fetches.
        assert!(driver.fetch_user_full_info(32).unwrap().is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn supergroup_full_info_fetched_once() {
        // Phase 6: `fetch_supergroup_full_info` sends `getSupergroupFullInfo`
        // once per supergroup; the id-less `supergroupFullInfo` response
        // (correlated by `PendingRequest::supergroup_id`) caches the
        // description + member count.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        let extra = driver
            .fetch_supergroup_full_info(77)
            .unwrap()
            .expect("getSupergroupFullInfo sent");
        let sent = recorder.snapshot();
        let info_json = sent
            .iter()
            .find(|j| j.contains(r#""@type":"getSupergroupFullInfo""#))
            .expect("getSupergroupFullInfo in outbox");
        assert!(info_json.contains(r#""supergroup_id":77"#));
        assert_eq!(driver.fetch_supergroup_full_info(77).unwrap(), None);
        assert_eq!(recorder.snapshot().len(), sent.len());
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"supergroupFullInfo","@extra":"{}","description":"CANARY_desc","member_count":4321}}"#,
                        extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let info = driver.session.supergroup_full_info(77).expect("cached");
        assert_eq!(info.description, "CANARY_desc");
        assert_eq!(info.member_count, 4321);
        assert_eq!(driver.fetch_supergroup_full_info(77).unwrap(), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_contact_sends_imported_contact() {
        // Phase 6: `add_contact` sends `addContact` with the
        // `importedContact` shape; the `ok` answer invalidates the contacts
        // list so the tab refetches it.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        driver.session.contacts = Some(Vec::new());
        let extra = driver
            .add_contact(31, "+15550131", "Ada", "Lovelace")
            .unwrap()
            .expect("addContact sent");
        let sent = recorder.snapshot();
        let add = sent
            .iter()
            .find(|j| j.contains(r#""@type":"addContact""#))
            .expect("addContact in outbox");
        assert!(add.contains(r#""user_id":31"#));
        assert!(add.contains(r#""@type":"importedContact""#));
        assert!(add.contains(r#""phone_number":"+15550131""#));
        assert!(add.contains(r#""first_name":"Ada""#));
        assert!(add.contains(r#""last_name":"Lovelace""#));
        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(driver.session.contacts.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bot_commands_fetched_once_on_chat_open() {
        // Phase 3.3: opening a bot chat lazily sends `getCommands` once
        // (null scope selects the default scope, schema 1.8.67 line
        // 14953). The `botCommands` response populates the cache and
        // merges below the `botInfo` commands in `command_menu_items`;
        // an `error` answer is recorded as an empty set so the fetch is
        // never retried. Non-bot chats never trigger the fetch.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        // Bot private chat (id 21) and a regular private chat (id 22).
        for json in [
            r#"{"@type":"updateUser","user":{"id":21,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":22,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":22},"unread_count":0}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        }

        driver.select_chat(ChatId(21)).unwrap();
        let cmd_fetches = || {
            recorder
                .snapshot()
                .into_iter()
                .filter(|j| j.contains("\"@type\":\"getCommands\""))
                .collect::<Vec<_>>()
        };
        let first = cmd_fetches();
        assert_eq!(first.len(), 1);
        assert!(first[0].contains("\"scope\":null"));
        assert!(first[0].contains("\"language_code\":\"\""));
        let extra = driver
            .session
            .requests
            .pending_extra_for(RequestPurpose::GetCommands, Some(ChatId(21)))
            .expect("getCommands in flight");

        // Re-selecting while the fetch is in flight sends nothing new.
        driver.select_chat(ChatId(21)).unwrap();
        assert_eq!(cmd_fetches().len(), 1);

        // The `botCommands` response lands in the cache as global rows.
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"botCommands","@extra":"{}","bot_user_id":21,"commands":[{{"@type":"botCommand","command":"settings","description":"CANARY_global","is_ephemeral":false}}]}}"#,
                        extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let items = driver.session.command_menu_items(ChatId(21));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].command, "settings");
        assert_eq!(items[0].description, "CANARY_global");
        assert!(items[0].global);
        driver.select_chat(ChatId(21)).unwrap();
        assert_eq!(cmd_fetches().len(), 1);

        // `botInfo` commands merge first; duplicates keep the
        // bot-specific description and are not repeated.
        let full_extra = driver
            .session
            .request(RequestPurpose::GetUserFullInfo, Some(ChatId(21)));
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"","description":"","commands":[{{"@type":"botCommand","command":"start","description":"Start","is_ephemeral":false}},{{"@type":"botCommand","command":"settings","description":"Specific settings","is_ephemeral":false}}]}}}}"#,
                        full_extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let items = driver.session.command_menu_items(ChatId(21));
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].command, "start");
        assert!(!items[0].global);
        assert_eq!(items[1].command, "settings");
        assert_eq!(items[1].description, "Specific settings");
        assert!(!items[1].global);

        // A regular private chat never triggers the fetch.
        driver.select_chat(ChatId(22)).unwrap();
        assert_eq!(cmd_fetches().len(), 1);
        assert!(driver.session.command_menu_items(ChatId(22)).is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bot_commands_error_absorbed_without_retry() {
        // Phase 3.3: an `error` answer to `getCommands` (user sessions —
        // the schema annotates the method "for bots only") is recorded as
        // an empty command set, so opening the chat again does not
        // refetch; the menu falls back to the `botInfo` commands.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        for json in [
            r#"{"@type":"updateUser","user":{"id":21,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        }
        // Seed `botInfo` so the fallback menu has rows after the error.
        let full_extra = driver
            .session
            .request(RequestPurpose::GetUserFullInfo, Some(ChatId(21)));
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"","description":"","commands":[{{"@type":"botCommand","command":"start","description":"Start","is_ephemeral":false}}]}}}}"#,
                        full_extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        driver.select_chat(ChatId(21)).unwrap();
        let cmd_fetches = || {
            recorder
                .snapshot()
                .into_iter()
                .filter(|j| j.contains("\"@type\":\"getCommands\""))
                .count()
        };
        assert_eq!(cmd_fetches(), 1);
        let extra = driver
            .session
            .requests
            .pending_extra_for(RequestPurpose::GetCommands, Some(ChatId(21)))
            .expect("getCommands in flight");
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","@extra":"{}","code":400,"message":"CANARY_bots_only"}}"#,
                        extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        // The error records an empty set; re-opening the chat refetches
        // nothing, and the menu shows the `botInfo` commands only.
        driver.select_chat(ChatId(21)).unwrap();
        assert_eq!(cmd_fetches(), 1);
        let items = driver.session.command_menu_items(ChatId(21));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].command, "start");
        assert!(!items[0].global);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn callback_query_sent_for_button_press() {
        // Phase 3.2: pressing a callback button sends `getCallbackQueryAnswer`
        // (schema 1.8.67 line 13138) with the button's payload bytes
        // (base64 in JSON). Pending messages and unknown chats are refused.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        for json in [
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateUser","user":{"id":21,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Vote","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AQID"}}]],"force_reply":false},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Pick","entities":[]}}}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        }
        let extra = driver
            .send_callback_query(ChatId(21), MessageId(301), &[1, 2, 3])
            .expect("callback query sends");
        let sent = recorder.snapshot();
        let query = sent
            .iter()
            .find(|j| j.contains(r#""@type":"getCallbackQueryAnswer""#))
            .expect("getCallbackQueryAnswer recorded");
        let v: serde_json::Value = serde_json::from_str(query).unwrap();
        assert_eq!(v["chat_id"], 21);
        assert_eq!(v["message_id"], 301);
        assert_eq!(v["payload"]["@type"], "callbackQueryPayloadData");
        assert_eq!(v["payload"]["data"], "AQID");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert!(
            driver
                .session
                .requests
                .pending_extra_for(RequestPurpose::GetCallbackQueryAnswer, Some(ChatId(21)))
                .is_some()
        );
        // Pending (unsent, negative id) messages cannot be answered.
        assert!(
            driver
                .send_callback_query(ChatId(21), MessageId(-1), &[1])
                .is_err()
        );
        // Unknown chats are refused.
        assert!(
            driver
                .send_callback_query(ChatId(99), MessageId(301), &[1])
                .is_err()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn select_chat_closes_previous_and_does_not_mark_unread_locally() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":3,"last_read_inbox_message_id":1}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":1}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.select_chat(ChatId(7)).unwrap();
        assert_eq!(driver.session.chats.get(&7).unwrap().unread_count, 3);
        driver.select_chat(ChatId(8)).unwrap();
        let sent = recorder.snapshot();
        assert!(
            sent.iter()
                .any(|j| j.contains("\"@type\":\"closeChat\"") && j.contains("\"chat_id\":7"))
        );
        assert!(
            sent.iter()
                .any(|j| j.contains("\"@type\":\"openChat\"") && j.contains("\"chat_id\":8"))
        );
        // Unread is TDLib-authoritative; opening must not zero it locally.
        assert_eq!(driver.session.chats.get(&7).unwrap().unread_count, 3);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":9,"unread_count":0}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.chats.get(&7).unwrap().unread_count, 0);
        assert!(!sink.rendered().contains("Alice"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn forum_flow_topics_then_topic_history() {
        // Phase 5.1: selecting a forum supergroup resolves `is_forum` via
        // `getSupergroup`, loads `getForumTopics`, and selecting a topic
        // fetches its history with `searchChatMessages` + `messageTopicForum`.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        for json in [
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        }
        // Forum status unknown: selecting the chat fires `getSupergroup`.
        driver.select_chat(ChatId(16)).unwrap();
        let sent = recorder.snapshot();
        assert!(sent.iter().any(
            |j| j.contains("\"@type\":\"getSupergroup\"") && j.contains("\"supergroup_id\":16")
        ));
        assert!(
            !sent
                .iter()
                .any(|j| j.contains("\"@type\":\"getForumTopics\""))
        );
        // The response resolves `is_forum`; re-selecting loads the topics.
        let extra = driver
            .session
            .requests
            .pending_extra_for(RequestPurpose::GetSupergroup, Some(ChatId(16)))
            .expect("getSupergroup in flight");
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"supergroup","@extra":"{}","id":16,"is_forum":true}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(driver.session.chats.get(&16).unwrap().is_forum_chat());
        driver.select_chat(ChatId(16)).unwrap();
        let sent = recorder.snapshot();
        let topics_extra = driver
            .session
            .requests
            .pending_extra_for(RequestPurpose::GetForumTopics, Some(ChatId(16)))
            .expect("getForumTopics in flight");
        assert!(
            sent.iter().any(|j| j.contains("\"@type\":\"getForumTopics\"")
                && j.contains("\"chat_id\":16"))
        );
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"forumTopics","@extra":"{}","total_count":1,"topics":[{{"info":{{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":2,"name":"Random","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"creation_date":1,"creator_id":{{"@type":"messageSenderUser","user_id":6}},"is_general":false,"is_outgoing":false,"is_closed":false,"is_hidden":false,"is_name_implicit":false}},"last_message":null,"order":"100","is_pinned":false,"unread_count":0,"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{{"@type":"chatNotificationSettings"}},"draft_message":null}}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
                        topics_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.forum_topics.get(&16).unwrap().len(), 1);
        // Selecting the topic fetches per-topic history.
        driver.select_topic(2).unwrap();
        assert_eq!(driver.session.open_topic, Some(2));
        let sent = recorder.snapshot();
        let search_json = sent
            .iter()
            .find(|j| {
                j.contains("\"@type\":\"searchChatMessages\"")
                    && j.contains("\"query\":\"\"")
                    && j.contains("\"messageTopicForum\"")
            })
            .expect("topic searchChatMessages sent");
        assert!(search_json.contains("\"forum_topic_id\":2"));
        // The response populates the topic history.
        let extra = driver
            .session
            .requests
            .pending_extra_for(RequestPurpose::GetTopicHistory, Some(ChatId(16)))
            .expect("GetTopicHistory in flight");
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":50,"chat_id":16,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_DRV_topic","entities":[]}}}}}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let history = driver.session.topic_histories.get(&(16, 2)).unwrap();
        assert!(history.loaded_complete);
        assert!(history.messages.contains_key(&50));
        // Back to the topic list.
        driver.deselect_topic();
        assert_eq!(driver.session.open_topic, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn send_text_rejected_when_empty_or_no_open_chat() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let snap = crate::composer::ComposerSnapshot::capture(
            ChatId(1),
            driver.session.view_generation,
            "   ",
        );
        assert_eq!(
            driver.send_text_snapshot(&snap),
            Err(ConnectSendError::InvalidRequest)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    struct ViewCtlSender {
        sent: Mutex<Vec<String>>,
        fail_view: Mutex<bool>,
    }

    impl ViewCtlSender {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                sent: Mutex::new(Vec::new()),
                fail_view: Mutex::new(false),
            })
        }

        fn snapshot(&self) -> Vec<String> {
            self.sent.lock().expect("view ctl sender").clone()
        }

        fn set_fail_view(&self, fail: bool) {
            *self.fail_view.lock().expect("view ctl sender") = fail;
        }

        fn view_count(&self) -> usize {
            self.snapshot()
                .iter()
                .filter(|j| j.contains("viewMessages"))
                .count()
        }
    }

    impl JsonSender for Arc<ViewCtlSender> {
        fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
            if request.contains("viewMessages") && *self.fail_view.lock().expect("view ctl sender")
            {
                return Err(ConnectSendError::Native);
            }
            self.sent
                .lock()
                .expect("view ctl sender")
                .push(request.to_string());
            Ok(())
        }
    }

    fn ready_private_chat(
        driver: &mut ConnectDriver<Arc<ViewCtlSender>>,
        seq: &AtomicU64,
        sink: &Arc<dyn DiagnosticSink>,
    ) {
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    seq,
                    sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":1}}"#,
                    seq,
                    sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":11,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
                    seq,
                    sink,
                )
                .unwrap(),
            )
            .unwrap();
    }

    #[test]
    fn view_messages_send_failure_unsticks_gate_and_retries() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let sender = ViewCtlSender::new();
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver = ConnectDriver::new(session, sender.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        ready_private_chat(&mut driver, &seq, &dyn_sink);

        sender.set_fail_view(true);
        assert_eq!(driver.select_chat(ChatId(7)), Err(ConnectSendError::Native));
        assert!(
            !driver
                .session
                .requests
                .has_purpose(RequestPurpose::ViewMessages)
        );
        assert_eq!(
            driver.session.message_ids_to_view(ChatId(7)),
            vec![MessageId(11)]
        );
        assert_eq!(sender.view_count(), 0);

        sender.set_fail_view(false);
        let extra = driver
            .maybe_view_open_messages()
            .unwrap()
            .expect("retry viewMessages");
        assert!(
            driver
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::ViewMessages, ChatId(7))
        );
        assert!(driver.session.message_ids_to_view(ChatId(7)).is_empty());
        assert_eq!(sender.view_count(), 1);
        assert!(
            sender
                .snapshot()
                .last()
                .unwrap()
                .contains(&format!("\"@extra\":\"{}\"", extra.0))
        );
        assert!(!sink.rendered().contains("hi"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn view_messages_tdlib_error_unsticks_gate_and_retries() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let sender = ViewCtlSender::new();
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver = ConnectDriver::new(session, sender.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        ready_private_chat(&mut driver, &seq, &dyn_sink);

        let _history = driver
            .select_chat(ChatId(7))
            .unwrap()
            .expect("history after view");
        assert_eq!(sender.view_count(), 1);
        let view_extra = sender
            .snapshot()
            .iter()
            .find(|j| j.contains("viewMessages"))
            .and_then(|j| serde_json::from_str::<Value>(j).ok())
            .and_then(|v| v["@extra"].as_str().map(str::to_string))
            .expect("view extra");
        assert!(
            driver
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::ViewMessages, ChatId(7))
        );
        assert!(driver.session.message_ids_to_view(ChatId(7)).is_empty());

        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","code":400,"message":"CANARY_VIEW_TD","@extra":"{view_extra}"}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            sender.view_count(),
            2,
            "TDLib error must retry viewMessages"
        );
        let retry_extra = sender
            .snapshot()
            .into_iter()
            .rev()
            .find(|j| j.contains("viewMessages"))
            .and_then(|j| serde_json::from_str::<Value>(&j).ok())
            .and_then(|v| v["@extra"].as_str().map(str::to_string))
            .expect("retry extra");
        assert_ne!(
            retry_extra, view_extra,
            "retry must not reuse the failed extra"
        );
        assert!(driver.session.message_ids_to_view(ChatId(7)).is_empty());
        assert!(
            driver
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::ViewMessages, ChatId(7))
        );
        assert!(!sink.rendered().contains("CANARY_VIEW_TD"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sticker_panel_loads_installed_set_and_send_uses_input_file_id() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.select_chat(ChatId(7)).unwrap();
        driver.open_sticker_panel().unwrap();
        let sets_extra = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|json| json.contains("getInstalledStickerSets"))
            .expect("installed sets");
        let sets_extra: Value = serde_json::from_str(&sets_extra).unwrap();
        assert_eq!(sets_extra["sticker_type"]["@type"], "stickerTypeRegular");
        let extra = sets_extra["@extra"].as_str().unwrap();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"stickerSets","@extra":"{extra}","total_count":1,"sets":[{{"@type":"stickerSetInfo","id":"77","title":"Demo","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"size":1,"covers":[]}}]}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.stickers.selected_set_id, Some(77));
        let set_req = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|json| json.contains("getStickerSet"))
            .expect("getStickerSet");
        let set_req: Value = serde_json::from_str(&set_req).unwrap();
        assert_eq!(set_req["set_id"], "77");
        let set_extra = set_req["@extra"].as_str().unwrap();
        let file = r#"{"@type":"file","id":41,"size":8,"expected_size":8,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":8}}"#;
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"stickerSet","@extra":"{set_extra}","id":"77","title":"Demo","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"stickers":[{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatWebp"}},"width":128,"height":128,"file":{file}}},"sticker":{file}}}],"emojis":[]}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.stickers.stickers.len(), 1);
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|json| json.contains("downloadFile") && json.contains("\"file_id\":41"))
        );
        driver
            .send_sticker(
                ChatId(7),
                StickerSend {
                    file_id: FileId(41),
                    emoji: "😀",
                    width: 512,
                    height: 512,
                    thumb: Some((FileId(41), 128, 128)),
                    reply_to: None,
                    topic_id: None,
                },
            )
            .unwrap();
        let sent = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|json| json.contains("inputMessageSticker"))
            .expect("send sticker");
        let sent: Value = serde_json::from_str(&sent).unwrap();
        assert_eq!(
            sent["input_message_content"]["sticker"]["sticker"]["@type"],
            "inputFileId"
        );
        assert_eq!(
            sent["input_message_content"]["sticker"]["sticker"]["id"],
            41
        );
        assert_eq!(sent["input_message_content"]["emoji"], "😀");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gif_panel_loads_saved_animations_and_send_uses_input_animation() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.select_chat(ChatId(7)).unwrap();
        driver.open_gif_panel().unwrap();
        let saved = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|json| json.contains("getSavedAnimations"))
            .expect("saved animations");
        let saved: Value = serde_json::from_str(&saved).unwrap();
        let extra = saved["@extra"].as_str().unwrap();
        let thumb = r#"{"@type":"file","id":42,"size":4,"expected_size":4,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"t","unique_id":"tu","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":4}}"#;
        let file = r#"{"@type":"file","id":33,"size":9,"expected_size":9,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"a","unique_id":"au","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":9}}"#;
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"animations","@extra":"{extra}","animations":[{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"wave.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":120,"height":70,"file":{thumb}}},"animation":{file}}}]}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.gifs.animations.len(), 1);
        assert_eq!(driver.session.gifs.animations[0].file_id, FileId(33));
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|json| json.contains("downloadFile") && json.contains("\"file_id\":42"))
        );
        driver
            .send_animation(
                ChatId(7),
                AnimationSend {
                    file_id: FileId(33),
                    duration: 2,
                    width: 240,
                    height: 140,
                    reply_to: None,
                    topic_id: None,
                },
            )
            .unwrap();
        let sent = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|json| json.contains("inputMessageAnimation"))
            .expect("send animation");
        let sent: Value = serde_json::from_str(&sent).unwrap();
        assert_eq!(
            sent["input_message_content"]["animation"]["@type"],
            "inputAnimation"
        );
        assert_eq!(
            sent["input_message_content"]["animation"]["animation"]["id"],
            33
        );
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateSavedAnimations","animation_ids":[33]}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(
            recorder
                .snapshot()
                .iter()
                .filter(|json| json.contains("getSavedAnimations"))
                .count()
                >= 2
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn voice_note_send_uses_input_file_local_and_recording_action() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.select_chat(ChatId(7)).unwrap();
        driver.sync_voice_recording(true, 1_000).unwrap();
        driver.sync_voice_recording(true, 1_100).unwrap();
        let actions: Vec<String> = recorder
            .snapshot()
            .into_iter()
            .filter(|json| json.contains("sendChatAction"))
            .collect();
        assert_eq!(actions.len(), 1);
        assert!(actions[0].contains("chatActionRecordingVoiceNote"));
        let voice = dir.join("note.ogg");
        std::fs::write(&voice, b"OggS").unwrap();
        let draft = VoiceDraft {
            path: voice,
            duration_secs: 3,
            bars: vec![31, 0, 1],
        };
        driver
            .send_voice_note(&draft, "", Some(MessageId(4)))
            .unwrap();
        let sent = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|json| json.contains("inputMessageVoiceNote"))
            .expect("send voice");
        let sent: Value = serde_json::from_str(&sent).unwrap();
        assert_eq!(
            sent["input_message_content"]["voice_note"]["voice_note"]["@type"],
            "inputFileLocal"
        );
        assert_eq!(sent["input_message_content"]["voice_note"]["duration"], 3);
        assert!(
            !sent["input_message_content"]["voice_note"]["waveform"]
                .as_str()
                .unwrap()
                .is_empty()
        );
        assert_eq!(sent["input_message_content"]["caption"], Value::Null);
        assert_eq!(sent["reply_to"]["message_id"], 4);
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|json| json.contains("chatActionCancel"))
        );
        let file = r#"{"@type":"file","id":4,"size":4,"expected_size":4,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":4}}"#;
        let history = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":8,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":12,"waveform":"","mime_type":"audio/ogg","speech_recognition_result":null,"voice":{file}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"is_listened":false}}}}}}"#
        );
        driver
            .ingest(copy_and_parse(&history, &seq, &dyn_sink).unwrap())
            .unwrap();
        driver.open_voice_content(ChatId(7), MessageId(8)).unwrap();
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|json| json.contains("openMessageContent"))
        );
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageContentOpened","chat_id":7,"message_id":8}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let row = driver
            .session
            .histories
            .get(&7)
            .and_then(|h| h.messages.get(&8))
            .expect("voice row");
        match &row.content {
            crate::telegram::envelope::MessageContent::VoiceNote(note) => {
                assert!(note.is_listened);
                assert_eq!(note.duration, 12);
            }
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn photo_history_auto_downloads_thumb_and_user_open_sends_full() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let history_extra = driver.select_chat(ChatId(7)).unwrap().expect("history");
        let thumb = r#"{"@type":"file","id":1,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}"#;
        let full = r#"{"@type":"file","id":2,"size":80,"expected_size":80,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":80}}"#;
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"messages","@extra":"{extra}","messages":[{{"id":20,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":320,"height":240,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{full},"width":800,"height":600,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"CANARY_MEDIA","entities":[]}},"has_spoiler":false,"is_secret":false}}}}]}}"#,
                        extra = history_extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let sent = recorder.snapshot();
        let thumb_req = sent
            .iter()
            .rev()
            .find(|j| j.contains("downloadFile") && j.contains("\"file_id\":1"))
            .expect("auto thumb downloadFile");
        let thumb_json: Value = serde_json::from_str(thumb_req).unwrap();
        assert_eq!(thumb_json["priority"], THUMB_DOWNLOAD_PRIORITY);
        assert_eq!(thumb_json["synchronous"], false);
        assert!(
            !sent.iter().any(|j| j.contains("\"file_id\":2")),
            "must not auto-download the full size"
        );
        let user = driver
            .download_file(FileId(2), USER_DOWNLOAD_PRIORITY)
            .unwrap()
            .expect("user download");
        let last = recorder.snapshot();
        let user_req = last.last().unwrap();
        assert!(user_req.contains("downloadFile"));
        assert!(user_req.contains("\"file_id\":2"));
        assert!(user_req.contains(&format!("\"@extra\":\"{}\"", user.0)));
        let user_json: Value = serde_json::from_str(user_req).unwrap();
        assert_eq!(user_json["priority"], USER_DOWNLOAD_PRIORITY);
        assert_eq!(
            driver.download_file(FileId(2), USER_DOWNLOAD_PRIORITY),
            Ok(None),
            "in-flight download must not duplicate"
        );
        assert!(!sink.rendered().contains("CANARY_MEDIA"));
        assert!(!sink.rendered().contains("CANARY_REMOTE"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chat_list_photo_downloads_on_ingest_and_dedupes() {
        // Parity slice: `updateNewChat` with `chat.photo.small` triggers a
        // `downloadFile` (thumb priority) from the ingest hook; a second
        // ingest does not re-request while the download is in flight.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let small = r#"{"@type":"file","id":91,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}"#;
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"updateNewChat","chat":{{"id":7,"title":"Ada","type":{{"@type":"chatTypePrivate","user_id":7}},"unread_count":0,"photo":{{"@type":"chatPhotoInfo","small":{small},"big":null,"minithumbnail":null,"has_animation":false,"is_personal":false}}}}}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let sent = recorder.snapshot();
        let avatar_req = sent
            .iter()
            .find(|j| j.contains("downloadFile") && j.contains("\"file_id\":91"))
            .expect("chat photo downloadFile");
        let avatar_json: Value = serde_json::from_str(avatar_req).unwrap();
        assert_eq!(avatar_json["priority"], THUMB_DOWNLOAD_PRIORITY);
        assert_eq!(avatar_json["synchronous"], false);
        // A second ingest (any envelope) must not duplicate the in-flight
        // download.
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatTitle","chat_id":7,"title":"Ada"}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let after = recorder.snapshot();
        let avatar_downloads = after
            .iter()
            .filter(|j| j.contains("downloadFile") && j.contains("\"file_id\":91"))
            .count();
        assert_eq!(avatar_downloads, 1, "in-flight avatar download deduped");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn select_channel_fetches_supergroup_profile_and_full_info() {
        // Parity slice: opening a channel sends `getSupergroup` (for the
        // header @username) and `getSupergroupFullInfo` (description,
        // subscriber count, linked discussion group); private chats send
        // neither.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.select_chat(ChatId(13)).unwrap();
        let sent = recorder.snapshot();
        assert!(
            sent.iter()
                .any(|j| j.contains("getSupergroup") && j.contains("\"supergroup_id\":13")),
            "getSupergroup for the header username"
        );
        assert!(
            sent.iter()
                .any(|j| j.contains("getSupergroupFullInfo") && j.contains("\"supergroup_id\":13")),
            "getSupergroupFullInfo for the header extras"
        );
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let before = recorder.snapshot().len();
        driver.select_chat(ChatId(7)).unwrap();
        let after = recorder.snapshot();
        assert!(
            !after[before..].iter().any(|j| j.contains("getSupergroup")),
            "private chats must not fetch supergroup info"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_sends_photo_and_document_from_picked_paths() {
        use crate::composer::{AttachmentKind, ComposerAttachment, ComposerSnapshot};
        use std::time::{SystemTime, UNIX_EPOCH};

        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.select_chat(ChatId(7)).unwrap();

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pick_dir =
            std::env::temp_dir().join(format!("quill-send-pick-{}-{}", std::process::id(), nanos));
        std::fs::create_dir_all(&pick_dir).unwrap();
        let photo = pick_dir.join("out.png");
        let doc = pick_dir.join("notes.txt");
        std::fs::write(&photo, [1, 2, 3]).unwrap();
        std::fs::write(&doc, b"hello").unwrap();

        let photo_att = ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap();
        let photo_path = photo_att.send_path_str().unwrap();
        let snap = ComposerSnapshot::capture_with_attachment(
            ChatId(7),
            driver.session.view_generation,
            "CANARY_PHOTO_CAP",
            Some(photo_att),
        );
        let extra = driver.send_snapshot(&snap).unwrap();
        let last = recorder.snapshot();
        let send_json = last.last().unwrap();
        let v: Value = serde_json::from_str(send_json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["input_message_content"]["@type"], "inputMessagePhoto");
        assert_eq!(
            v["input_message_content"]["photo"]["photo"]["path"],
            photo_path
        );
        assert_eq!(
            v["input_message_content"]["caption"]["text"],
            "CANARY_PHOTO_CAP"
        );

        // Pending message response upserts outgoing media.
        let pending = copy_and_parse(
            &format!(
                r#"{{"@type":"message","@extra":"{}","id":-5,"chat_id":7,"is_outgoing":true,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{{"@type":"file","id":50,"size":3,"expected_size":3,"local":{{"@type":"localFile","path":"","can_be_downloaded":false,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":true,"is_uploading_completed":false,"uploaded_size":0}}}},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"CANARY_PHOTO_CAP","entities":[]}},"has_spoiler":false,"is_secret":false}}}}"#,
                extra.0
            ),
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(pending).unwrap();
        let msg = driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&-5)
            .unwrap();
        assert!(msg.pending);
        assert!(msg.is_outgoing);
        assert!(matches!(
            msg.content,
            crate::telegram::envelope::MessageContent::Photo(_)
        ));

        let doc_att = ComposerAttachment::pick(&doc, AttachmentKind::Document).unwrap();
        let doc_path = doc_att.send_path_str().unwrap();
        let doc_snap = ComposerSnapshot::capture_with_attachment(
            ChatId(7),
            driver.session.view_generation,
            "",
            Some(doc_att),
        );
        let doc_extra = driver.send_snapshot(&doc_snap).unwrap();
        let doc_json: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        assert_eq!(doc_json["@extra"], doc_extra.0.to_string());
        assert_eq!(
            doc_json["input_message_content"]["@type"],
            "inputMessageDocument"
        );
        assert_eq!(
            doc_json["input_message_content"]["document"]["document"]["path"],
            doc_path
        );
        assert_eq!(doc_json["input_message_content"]["caption"]["text"], "");

        let clip = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("docs/screenshots/fixtures/demo-clip.mp4");
        let video_att = ComposerAttachment::pick(&clip, AttachmentKind::Video).unwrap();
        let video_path = video_att.send_path_str().unwrap();
        let video_snap = ComposerSnapshot::capture_with_attachment(
            ChatId(7),
            driver.session.view_generation,
            "CANARY_VIDEO_CAP",
            Some(video_att),
        );
        let video_extra = driver.send_snapshot(&video_snap).unwrap();
        let video_json: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        assert_eq!(video_json["@extra"], video_extra.0.to_string());
        assert_eq!(
            video_json["input_message_content"]["@type"],
            "inputMessageVideo"
        );
        assert_eq!(
            video_json["input_message_content"]["video"]["video"]["path"],
            video_path
        );
        assert_eq!(
            video_json["input_message_content"]["video"]["thumbnail"],
            Value::Null
        );
        assert_eq!(video_json["input_message_content"]["video"]["duration"], 1);
        assert_eq!(video_json["input_message_content"]["video"]["width"], 320);
        assert_eq!(video_json["input_message_content"]["video"]["height"], 180);
        assert_eq!(
            video_json["input_message_content"]["video"]["supports_streaming"],
            true
        );
        assert_eq!(
            video_json["input_message_content"]["caption"]["text"],
            "CANARY_VIDEO_CAP"
        );

        let note = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("docs/screenshots/fixtures/demo-video-note.mp4");
        let note_att = ComposerAttachment::pick(&note, AttachmentKind::VideoNote).unwrap();
        let note_path = note_att.send_path_str().unwrap();
        let note_snap = ComposerSnapshot::capture_with_attachment(
            ChatId(7),
            driver.session.view_generation,
            "ignored caption",
            Some(note_att),
        );
        let note_extra = driver.send_snapshot(&note_snap).unwrap();
        let note_json: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        assert_eq!(note_json["@extra"], note_extra.0.to_string());
        assert_eq!(
            note_json["input_message_content"]["@type"],
            "inputMessageVideoNote"
        );
        let sent_note = &note_json["input_message_content"]["video_note"];
        assert_eq!(sent_note["video_note"]["path"], note_path);
        assert_eq!(sent_note["duration"], 1);
        assert_eq!(sent_note["length"], 240);
        assert_eq!(
            note_json["input_message_content"]["self_destruct_type"],
            Value::Null
        );
        assert!(note_json["input_message_content"].get("caption").is_none());
        let thumb = &sent_note["thumbnail"];
        if !thumb.is_null() {
            assert_eq!(thumb["@type"], "inputThumbnail");
            assert_eq!(thumb["width"], 240);
            assert_eq!(thumb["height"], 240);
            assert!(
                thumb["thumbnail"]["path"]
                    .as_str()
                    .unwrap_or("")
                    .ends_with(".jpg")
            );
        }
        let landscape = ComposerAttachment::pick(&clip, AttachmentKind::VideoNote).unwrap();
        let bad = ComposerSnapshot::capture_with_attachment(
            ChatId(7),
            driver.session.view_generation,
            "",
            Some(landscape),
        );
        assert_eq!(
            driver.send_snapshot(&bad),
            Err(ConnectSendError::InvalidRequest)
        );

        let album_photo = ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap();
        let album_video = ComposerAttachment::pick(&clip, AttachmentKind::Video).unwrap();
        let album_snap = ComposerSnapshot::capture_album(
            ChatId(7),
            driver.session.view_generation,
            "CANARY_ALBUM_CAP",
            vec![album_photo, album_video],
        )
        .with_reply(None);
        let album_extra = driver.send_snapshot(&album_snap).unwrap();
        let album_json: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        assert_eq!(album_json["@type"], "sendMessageAlbum");
        assert_eq!(album_json["@extra"], album_extra.0.to_string());
        assert!(album_json.get("reply_markup").is_none());
        let contents = album_json["input_message_contents"].as_array().unwrap();
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0]["@type"], "inputMessagePhoto");
        assert_eq!(contents[0]["caption"]["text"], "");
        assert_eq!(contents[0]["show_caption_above_media"], false);
        assert_eq!(contents[1]["@type"], "inputMessageVideo");
        assert_eq!(contents[1]["caption"]["text"], "CANARY_ALBUM_CAP");
        assert_eq!(contents[1]["show_caption_above_media"], false);
        let album_reply = copy_and_parse(
            &format!(
                r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":-8,"chat_id":7,"is_outgoing":true,"media_album_id":"9001","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{{"@type":"file","id":51,"size":3,"expected_size":3,"local":{{"@type":"localFile","path":"","can_be_downloaded":false,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":true,"is_uploading_completed":false,"uploaded_size":0}}}},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}},{{"id":-7,"chat_id":7,"is_outgoing":true,"media_album_id":"9001","content":{{"@type":"messageVideo","video":{{"@type":"video","duration":1,"width":320,"height":180,"file_name":"clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":null,"video":{{"@type":"file","id":52,"size":4,"expected_size":4,"local":{{"@type":"localFile","path":"","can_be_downloaded":false,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"y","unique_id":"v","is_uploading_active":true,"is_uploading_completed":false,"uploaded_size":0}}}}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"CANARY_ALBUM_CAP","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}]}}"#,
                album_extra.0
            ),
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(album_reply).unwrap();
        let history = driver.session.histories.get(&7).unwrap();
        let first = history.messages.get(&-8).unwrap();
        let second = history.messages.get(&-7).unwrap();
        assert!(first.pending && second.pending);
        assert_eq!(first.media_album_id, 9001);
        assert_eq!(second.media_album_id, 9001);
        assert_eq!(history.messages.get(&-5).unwrap().media_album_id, 0);

        // Reject paths that were not picked through ComposerAttachment.
        let forged = ComposerSnapshot::capture_with_attachment(
            ChatId(7),
            driver.session.view_generation,
            "",
            Some(crate::composer::ComposerAttachment {
                path: pick_dir.join("missing-forged.bin"),
                kind: AttachmentKind::Document,
                file_name: "missing-forged.bin".into(),
            }),
        );
        assert_eq!(
            driver.send_snapshot(&forged),
            Err(ConnectSendError::InvalidRequest)
        );
        assert!(!sink.rendered().contains("CANARY_PHOTO"));
        let _ = std::fs::remove_dir_all(&pick_dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_search_happy_empty_error_and_select() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        assert_eq!(
            driver.set_search_query("alice"),
            Err(ConnectSendError::InvalidRequest)
        );
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        let extras = commit_typed_search(&mut driver, "alice");
        let SearchFlight::Query(chats_extra, messages_extra, public_extra) = extras else {
            panic!("expected typed search");
        };
        let sent = recorder.snapshot();
        assert!(
            sent.iter()
                .any(|j| j.contains("\"@type\":\"searchChats\"") && j.contains("alice"))
        );
        assert!(sent.iter().any(|j| {
            j.contains("\"@type\":\"searchMessages\"") && j.contains("\"chat_list\":null")
        }));
        assert!(
            sent.iter()
                .any(|j| { j.contains("\"@type\":\"searchPublicChats\"") && j.contains("alice") })
        );
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[7]}}"#,
                        chats_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                        public_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":50,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_DRV_search","entities":[]}}}}}}]}}"#,
                        messages_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.search.status, SearchStatus::Ready);
        assert_eq!(driver.session.search.chat_ids, vec![ChatId(7)]);
        driver
            .select_search_message(ChatId(7), MessageId(50), "", None, 0)
            .unwrap();
        assert_eq!(driver.session.search.status, SearchStatus::Closed);
        assert_eq!(driver.session.open_chat, Some(ChatId(7)));
        assert!(
            driver
                .session
                .histories
                .get(&7)
                .unwrap()
                .messages
                .contains_key(&50)
        );
        assert!(recorder.snapshot().iter().any(
            |j| j.contains("\"@type\":\"addRecentlyFoundChat\"") && j.contains("\"chat_id\":7")
        ));
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("\"@type\":\"openChat\"") && j.contains("\"chat_id\":7"))
        );

        let recents = driver.open_search().unwrap().expect("recents");
        let SearchFlight::Recents(recents_extra) = recents else {
            panic!("expected recents");
        };
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("\"@type\":\"searchRecentlyFoundChats\""))
        );
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[7]}}"#,
                        recents_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.search.status, SearchStatus::Ready);
        assert!(driver.session.search.recents);

        let empty = commit_typed_search(&mut driver, "zzz");
        let SearchFlight::Query(empty_chats, empty_messages, empty_public) = empty else {
            panic!("expected typed empty search");
        };
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                        empty_chats.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"foundMessages","@extra":"{}","total_count":0,"next_offset":"","messages":[]}}"#,
                        empty_messages.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        // Phase 7.2: `Empty` only once the public search settles too.
        assert_eq!(driver.session.search.status, SearchStatus::Searching);
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                        empty_public.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.search.status, SearchStatus::Empty);

        let fail = commit_typed_search(&mut driver, "nope");
        let SearchFlight::Query(fail_chats, fail_messages, fail_public) = fail else {
            panic!("expected typed fail search");
        };
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","code":400,"message":"CANARY_DRV_ERR","@extra":"{}"}}"#,
                        fail_chats.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","code":400,"message":"CANARY_DRV_ERR2","@extra":"{}"}}"#,
                        fail_messages.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        // Phase 7.2: the public request still in flight keeps `Searching`.
        assert_eq!(driver.session.search.status, SearchStatus::Searching);
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","code":400,"message":"CANARY_DRV_ERR3","@extra":"{}"}}"#,
                        fail_public.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.search.status, SearchStatus::Failed);
        driver.close_search();
        assert_eq!(driver.session.search.status, SearchStatus::Closed);
        assert!(!sink.rendered().contains("CANARY_DRV"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn commit_typed_search<S: JsonSender>(
        driver: &mut ConnectDriver<S>,
        query: &str,
    ) -> SearchFlight {
        match driver.set_search_query(query).unwrap() {
            SearchQueryOutcome::Debounced { token } => driver
                .commit_debounced_search(token)
                .unwrap()
                .expect("debounced search"),
            other => panic!("expected Debounced, got {other:?}"),
        }
    }

    #[test]
    fn driver_typed_search_debounce_settles_once() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        let t1 = match driver.set_search_query("a").unwrap() {
            SearchQueryOutcome::Debounced { token } => token,
            other => panic!("{other:?}"),
        };
        let t2 = match driver.set_search_query("al").unwrap() {
            SearchQueryOutcome::Debounced { token } => token,
            other => panic!("{other:?}"),
        };
        let t3 = match driver.set_search_query("alice").unwrap() {
            SearchQueryOutcome::Debounced { token } => token,
            other => panic!("{other:?}"),
        };
        assert_ne!(t1, t3);
        assert_ne!(t2, t3);
        assert!(
            !recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("\"@type\":\"searchChats\""))
        );
        assert!(driver.commit_debounced_search(t1).unwrap().is_none());
        assert!(driver.commit_debounced_search(t2).unwrap().is_none());
        assert!(
            !recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("\"@type\":\"searchChats\""))
        );
        let settled = driver
            .commit_debounced_search(t3)
            .unwrap()
            .expect("settled");
        let SearchFlight::Query(_, _, _) = settled else {
            panic!("expected typed pair");
        };
        let sent: Vec<String> = recorder
            .snapshot()
            .into_iter()
            .filter(|j| {
                j.contains("\"@type\":\"searchChats\"")
                    || j.contains("\"@type\":\"searchMessages\"")
            })
            .collect();
        assert_eq!(sent.len(), 2);
        assert!(sent.iter().all(|j| j.contains("alice")));
        assert!(
            !sent
                .iter()
                .any(|j| j.contains("\"query\":\"a\"") || j.contains("\"query\":\"al\""))
        );
        assert_eq!(SEARCH_DEBOUNCE, Duration::from_millis(900));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn seed_ready_alice<S: JsonSender>(
        driver: &mut ConnectDriver<S>,
        seq: &AtomicU64,
        dyn_sink: &Arc<dyn DiagnosticSink>,
    ) {
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    seq,
                    dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    seq,
                    dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#,
                    seq,
                    dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello already here","entities":[]}}}}"#,
                    seq,
                    dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.select_chat(ChatId(7)).unwrap();
    }

    fn commit_typed_chat_search<S: JsonSender>(
        driver: &mut ConnectDriver<S>,
        query: &str,
    ) -> ChatSearchFlight {
        match driver.set_chat_search_query(query).unwrap() {
            ChatSearchQueryOutcome::Debounced { token } => driver
                .commit_debounced_chat_search(token)
                .unwrap()
                .expect("debounced chat search"),
            other => panic!("expected Debounced, got {other:?}"),
        }
    }

    #[test]
    fn driver_chat_search_debounce_jump_empty_and_close() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        assert_eq!(
            driver.set_chat_search_query("hello"),
            Err(ConnectSendError::InvalidRequest)
        );
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        assert!(driver.open_chat_search().unwrap());
        assert!(driver.session.chat_search.open);

        let t1 = match driver.set_chat_search_query("h").unwrap() {
            ChatSearchQueryOutcome::Debounced { token } => token,
            other => panic!("{other:?}"),
        };
        let t2 = match driver.set_chat_search_query("he").unwrap() {
            ChatSearchQueryOutcome::Debounced { token } => token,
            other => panic!("{other:?}"),
        };
        let t3 = match driver.set_chat_search_query("hello").unwrap() {
            ChatSearchQueryOutcome::Debounced { token } => token,
            other => panic!("{other:?}"),
        };
        assert!(
            !recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("\"@type\":\"searchChatMessages\""))
        );
        assert!(driver.commit_debounced_chat_search(t1).unwrap().is_none());
        assert!(driver.commit_debounced_chat_search(t2).unwrap().is_none());
        let ChatSearchFlight::Query(extra) = driver
            .commit_debounced_chat_search(t3)
            .unwrap()
            .expect("settled");
        let sent = recorder.snapshot();
        let search_json = sent
            .iter()
            .rev()
            .find(|j| j.contains("\"@type\":\"searchChatMessages\""))
            .expect("searchChatMessages");
        let v: Value = serde_json::from_str(search_json).unwrap();
        assert_eq!(v["query"], "hello");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["sender_id"], Value::Null);
        assert_eq!(v["from_message_id"], 0);
        assert_eq!(v["offset"], 0);
        assert_eq!(v["limit"], CHAT_SEARCH_LIMIT);
        assert_eq!(v["filter"], Value::Null);
        assert!(!search_json.contains("\"query\":\"h\""));

        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":40,"messages":[{{"id":50,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_DRV_chat","entities":[]}}}}}},{{"id":40,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.chat_search.status, SearchStatus::Ready);
        assert_eq!(
            driver.session.chat_search.jump,
            crate::state::ChatSearchJump::Ready {
                message_id: MessageId(50)
            }
        );
        let around = driver
            .jump_to_chat_search_message(MessageId(40))
            .unwrap()
            .expect("around load");
        let around_json = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|j| {
                j.contains("\"@type\":\"getChatHistory\"") && j.contains("\"from_message_id\":40")
            })
            .expect("getChatHistory around");
        let around_v: Value = serde_json::from_str(&around_json).unwrap();
        assert_eq!(around_v["offset"], HISTORY_AROUND_OFFSET);
        assert_eq!(around_v["limit"], HISTORY_AROUND_LIMIT);
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":40,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}},{{"id":39,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"neighbor","entities":[]}}}}}}]}}"#,
                        around.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            driver.session.chat_search.jump,
            crate::state::ChatSearchJump::Ready {
                message_id: MessageId(40)
            }
        );
        assert!(
            driver
                .session
                .histories
                .get(&7)
                .unwrap()
                .contains(MessageId(39))
        );

        let ChatSearchFlight::Query(empty_extra) = commit_typed_chat_search(&mut driver, "zzz");
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
                        empty_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.chat_search.status, SearchStatus::Empty);

        let before_close = driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .contains_key(&50);
        driver.close_chat_search();
        assert_eq!(driver.session.chat_search.status, SearchStatus::Closed);
        assert_eq!(driver.session.open_chat, Some(ChatId(7)));
        assert!(before_close);
        assert!(
            driver
                .session
                .histories
                .get(&7)
                .unwrap()
                .contains(MessageId(50))
        );
        assert_eq!(SEARCH_DEBOUNCE, Duration::from_millis(900));
        assert!(!sink.rendered().contains("CANARY_DRV_chat"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_send_reply_shape_and_jump_to_replied() {
        use crate::composer::ComposerReplyTo;

        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        let snap = crate::composer::ComposerSnapshot::capture(
            ChatId(7),
            driver.session.view_generation,
            "CANARY_REPLY_text",
        )
        .with_reply(Some(ComposerReplyTo::new(
            ChatId(7),
            MessageId(50),
            "hello already here",
        )));
        let extra = driver.send_snapshot(&snap).unwrap();
        let send_json = recorder.snapshot().last().cloned().expect("sendMessage");
        let v: Value = serde_json::from_str(&send_json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
        assert_eq!(v["reply_to"]["message_id"], 50);
        assert_eq!(v["reply_to"]["quote"], Value::Null);
        assert_eq!(v["reply_to"]["checklist_task_id"], 0);
        assert_eq!(v["reply_to"]["poll_option_id"], "");
        assert!(send_json.contains("CANARY_REPLY_text"));

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_REPLY_text","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":7,"message_id":50}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let reply = driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&60)
            .unwrap();
        assert_eq!(
            driver.session.reply_quote_preview(reply).as_deref(),
            Some("hello already here")
        );
        assert_eq!(
            driver
                .jump_to_replied_message(reply.reply_to.as_ref().unwrap().message_id)
                .unwrap(),
            None
        );
        assert_eq!(
            driver.session.chat_search.jump,
            crate::state::ChatSearchJump::Ready {
                message_id: MessageId(50)
            }
        );
        assert!(
            driver
                .jump_to_replied_message(MessageId(40))
                .unwrap()
                .is_some()
        );
        assert!(!sink.rendered().contains("CANARY_REPLY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_edit_text_shape_and_incoming_rejected() {
        use crate::composer::{ComposerEdit, ComposerEditKind};

        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"own outgoing","entities":[]}}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        let incoming = ComposerEdit {
            chat_id: ChatId(7),
            message_id: MessageId(50),
            original_text: "hello already here".into(),
            kind: ComposerEditKind::Text,
        };
        assert_eq!(
            driver.edit_snapshot(&incoming, "nope"),
            Err(ConnectSendError::InvalidRequest)
        );

        let edit = ComposerEdit {
            chat_id: ChatId(7),
            message_id: MessageId(60),
            original_text: "own outgoing".into(),
            kind: ComposerEditKind::Text,
        };
        assert_eq!(
            driver.edit_snapshot(&edit, "   "),
            Err(ConnectSendError::InvalidRequest)
        );
        let extra = driver.edit_snapshot(&edit, "CANARY_EDIT_text").unwrap();
        let json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("editMessageText");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "editMessageText");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_id"], 60);
        assert_eq!(v["reply_markup"], Value::Null);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageText");
        assert_eq!(
            v["input_message_content"]["text"]["text"],
            "CANARY_EDIT_text"
        );
        assert_eq!(v["input_message_content"]["clear_draft"], false);

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageContent","chat_id":7,"message_id":60,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_EDIT_text","entities":[]}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            driver
                .session
                .histories
                .get(&7)
                .unwrap()
                .messages
                .get(&60)
                .unwrap()
                .content
                .preview(),
            "CANARY_EDIT_text"
        );
        assert!(!sink.rendered().contains("CANARY_EDIT"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_delete_confirm_shape_and_tombstone() {
        use crate::composer::DeleteConfirm;

        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"own outgoing","entities":[]}}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        let incoming = DeleteConfirm {
            chat_id: ChatId(7),
            message_id: MessageId(50),
        };
        assert_eq!(
            driver.delete_confirmed(&incoming),
            Err(ConnectSendError::InvalidRequest)
        );

        let confirm = DeleteConfirm::own(ChatId(7), MessageId(60), true, false).unwrap();
        let extra = driver.delete_confirmed(&confirm).unwrap();
        let json = recorder.snapshot().last().cloned().expect("deleteMessages");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "deleteMessages");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_ids"], serde_json::json!([60]));
        assert_eq!(v["revoke"], true);

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateDeleteMessages","chat_id":7,"message_ids":[60],"is_permanent":true,"from_cache":false}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let history = driver.session.histories.get(&7).unwrap();
        assert!(!history.contains(MessageId(60)));
        assert!(history.is_tombstone(MessageId(60)));
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_forward_messages_shape_and_dest_result() {
        use crate::composer::ForwardDraft;

        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":8,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"8","is_pinned":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"own outgoing","entities":[]}}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        let mut draft = ForwardDraft::from_message(ChatId(7), MessageId(50), false).unwrap();
        draft.toggle(ChatId(7), MessageId(60), false);
        let extra = driver.forward_messages(ChatId(8), &draft).unwrap();
        let json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("forwardMessages");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "forwardMessages");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 8);
        assert_eq!(v["from_chat_id"], 7);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["message_ids"], serde_json::json!([50, 60]));
        assert_eq!(v["send_copy"], false);
        assert_eq!(v["remove_caption"], false);
        assert_eq!(v["options"], Value::Null);

        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":80,"chat_id":8,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hello already here","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":7}},"date":1}}}},{{"id":81,"chat_id":8,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"own outgoing","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":7}},"date":1}}}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let result = driver
            .session
            .last_forward
            .as_ref()
            .expect("forward result");
        assert_eq!(result.dest_title, "Bob");
        assert_eq!(result.forwarded_ids, vec![MessageId(80), MessageId(81)]);
        assert_eq!(result.success_label(), "Forwarded 2 messages to Bob");
        assert!(
            driver
                .session
                .histories
                .get(&8)
                .unwrap()
                .contains(MessageId(80))
        );
        assert_eq!(
            driver.session.forward_from_label(
                driver
                    .session
                    .histories
                    .get(&8)
                    .unwrap()
                    .messages
                    .get(&80)
                    .unwrap()
                    .forward_info
                    .as_ref()
                    .unwrap()
            ),
            "Forwarded from Alice"
        );
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_add_and_remove_message_reaction_then_interaction_info() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        let extra = driver
            .toggle_message_reaction(ChatId(7), MessageId(50), "❤")
            .unwrap();
        let json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("addMessageReaction");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "addMessageReaction");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_id"], 50);
        assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
        assert_eq!(v["reaction_type"]["emoji"], "❤");
        assert_eq!(v["is_big"], false);
        assert_eq!(v["update_recent_reactions"], true);
        assert!(!json.contains("setMessageReactions"));

        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageInteractionInfo","chat_id":7,"message_id":50,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":1,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let reacted = driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&50)
            .unwrap();
        assert!(reacted.chosen_emoji("❤"));
        assert_eq!(
            reacted.emoji_reaction_chips()[0].chip_label().as_deref(),
            Some("❤ 1")
        );

        let remove_extra = driver
            .toggle_message_reaction(ChatId(7), MessageId(50), "❤")
            .unwrap();
        let remove_json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("removeMessageReaction");
        let v: Value = serde_json::from_str(&remove_json).unwrap();
        assert_eq!(v["@type"], "removeMessageReaction");
        assert_eq!(v["@extra"], remove_extra.0.to_string());
        assert_eq!(v["reaction_type"]["emoji"], "❤");

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageInteractionInfo","chat_id":7,"message_id":50,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":null}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let cleared = driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&50)
            .unwrap();
        assert!(cleared.emoji_reaction_chips().is_empty());
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase 9.2: ingest a full `story` object into the driver's session,
    /// like the `getStory` response would.
    fn seed_story<S: JsonSender>(
        driver: &mut ConnectDriver<S>,
        seq: &AtomicU64,
        dyn_sink: &Arc<dyn DiagnosticSink>,
        chat_id: i64,
        story_id: i32,
        content_type: &str,
        flags: &str,
    ) {
        let json = format!(
            r#"{{"@type":"story","id":{story_id},"poster_chat_id":{chat_id},"date":1700000000,"content":{{"@type":"{content_type}"}},{flags}"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
        );
        driver
            .ingest(copy_and_parse(&json, seq, dyn_sink).unwrap())
            .unwrap();
    }

    #[test]
    fn driver_story_reaction_set_remove_and_gates() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        // Unknown story: rejected.
        assert_eq!(
            driver.set_story_reaction(ChatId(7), 99, Some("❤")),
            Err(ConnectSendError::InvalidRequest)
        );

        seed_story(
            &mut driver,
            &seq,
            &dyn_sink,
            7,
            5,
            "storyContentPhoto",
            r#""chosen_reaction_type":null,"#,
        );
        // Live stories: `setStoryReaction` is not supported for live
        // stories (schema 1.8.67 line 13809).
        seed_story(&mut driver, &seq, &dyn_sink, 7, 6, "storyContentLive", "");
        assert_eq!(
            driver.set_story_reaction(ChatId(7), 6, Some("❤")),
            Err(ConnectSendError::InvalidRequest)
        );
        // Empty emoji: rejected.
        assert_eq!(
            driver.set_story_reaction(ChatId(7), 5, Some("  ")),
            Err(ConnectSendError::InvalidRequest)
        );

        let extra = driver
            .set_story_reaction(ChatId(7), 5, Some("❤"))
            .unwrap()
            .expect("react sends");
        let json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("setStoryReaction");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setStoryReaction");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["story_poster_chat_id"], 7);
        assert_eq!(v["story_id"], 5);
        assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
        assert_eq!(v["reaction_type"]["emoji"], "❤");
        assert_eq!(v["update_recent_reactions"], true);

        // Removing sends `reaction_type: null`.
        let remove_extra = driver
            .set_story_reaction(ChatId(7), 5, None)
            .unwrap()
            .expect("remove sends");
        let json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("remove reaction");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setStoryReaction");
        assert_eq!(v["@extra"], remove_extra.0.to_string());
        assert_eq!(v["reaction_type"], Value::Null);

        // `updateStory` with a chosen reaction refreshes the cache (the
        // viewer reads `chosen_reaction_emoji` live from the cache).
        seed_story(
            &mut driver,
            &seq,
            &dyn_sink,
            7,
            5,
            "storyContentPhoto",
            r#""chosen_reaction_type":{"@type":"reactionTypeEmoji","emoji":"👍"},"#,
        );
        let story = driver.session.stories.get(&(7, 5)).unwrap();
        assert_eq!(story.chosen_reaction_emoji.as_deref(), Some("👍"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_delete_story_gated_on_can_be_deleted() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        assert_eq!(
            driver.delete_story(ChatId(7), 5),
            Err(ConnectSendError::InvalidRequest)
        );
        seed_story(&mut driver, &seq, &dyn_sink, 7, 5, "storyContentPhoto", "");
        assert_eq!(
            driver.delete_story(ChatId(7), 5),
            Err(ConnectSendError::InvalidRequest)
        );
        seed_story(
            &mut driver,
            &seq,
            &dyn_sink,
            7,
            6,
            "storyContentPhoto",
            r#""can_be_deleted":true,"#,
        );
        let extra = driver
            .delete_story(ChatId(7), 6)
            .unwrap()
            .expect("delete sends");
        let json = recorder.snapshot().last().cloned().expect("deleteStory");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "deleteStory");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["story_poster_chat_id"], 7);
        assert_eq!(v["story_id"], 6);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_send_story_reply_uses_input_message_reply_to_story() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        seed_story(&mut driver, &seq, &dyn_sink, 7, 5, "storyContentPhoto", "");
        // Not repliable and empty text are rejected.
        assert_eq!(
            driver.send_story_reply(ChatId(7), 5, "hello"),
            Err(ConnectSendError::InvalidRequest)
        );
        seed_story(
            &mut driver,
            &seq,
            &dyn_sink,
            7,
            6,
            "storyContentPhoto",
            r#""can_be_replied":true,"#,
        );
        assert_eq!(
            driver.send_story_reply(ChatId(7), 6, "   "),
            Err(ConnectSendError::InvalidRequest)
        );

        let extra = driver
            .send_story_reply(ChatId(7), 6, "Nice story!")
            .unwrap();
        let json = recorder.snapshot().last().cloned().expect("sendMessage");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToStory");
        assert_eq!(v["reply_to"]["story_poster_chat_id"], 7);
        assert_eq!(v["reply_to"]["story_id"], 6);
        assert_eq!(v["input_message_content"]["text"]["text"], "Nice story!");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_get_story_available_reactions_dedupes_and_caches() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        let extra = driver
            .get_story_available_reactions()
            .unwrap()
            .expect("first call sends");
        let sent = recorder.snapshot().len();
        // In-flight duplicate is deduped.
        assert!(driver.get_story_available_reactions().unwrap().is_none());
        assert_eq!(recorder.snapshot().len(), sent);

        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"availableReactions","@extra":"{}","top_reactions":[{{"@type":"availableReaction","type":{{"@type":"reactionTypeEmoji","emoji":"❤"}},"needs_premium":false}}],"recent_reactions":[],"popular_reactions":[],"allow_custom_emoji":false,"are_tags":false,"unavailability_reason":null}}"#, extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let cached = driver.session.story_available_reactions.clone().unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].emoji, "❤");
        // Cached: no new request.
        assert!(driver.get_story_available_reactions().unwrap().is_none());
        assert_eq!(recorder.snapshot().len(), sent);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_pin_and_unpin_chat_message_then_is_pinned_update() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        let extra = driver.pin_chat_message(ChatId(7), MessageId(50)).unwrap();
        let json = recorder.snapshot().last().cloned().expect("pinChatMessage");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "pinChatMessage");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_id"], 50);
        assert_eq!(v["disable_notification"], false);
        assert_eq!(v["only_for_self"], false);
        assert!(!json.contains("unpinAllChatMessages"));

        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageIsPinned","chat_id":7,"message_id":50,"is_pinned":true}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let pinned = driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&50)
            .unwrap();
        assert!(pinned.is_pinned);

        let unpin_extra = driver.unpin_chat_message(ChatId(7), MessageId(50)).unwrap();
        let unpin_json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("unpinChatMessage");
        let v: Value = serde_json::from_str(&unpin_json).unwrap();
        assert_eq!(v["@type"], "unpinChatMessage");
        assert_eq!(v["@extra"], unpin_extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_id"], 50);

        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, unpin_extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageIsPinned","chat_id":7,"message_id":50,"is_pinned":false}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let cleared = driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&50)
            .unwrap();
        assert!(!cleared.is_pinned);
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_mute_forever_then_unmute_and_archive() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        let extra = driver.mute_chat_forever(ChatId(7)).unwrap();
        let json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("setChatNotificationSettings");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setChatNotificationSettings");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(
            v["notification_settings"]["@type"],
            "chatNotificationSettings"
        );
        assert_eq!(v["notification_settings"]["use_default_mute_for"], false);
        assert_eq!(v["notification_settings"]["mute_for"], i32::MAX);
        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatNotificationSettings","chat_id":7,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":2147483647,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(driver.session.chats.get(&7).unwrap().is_muted());

        let unmute = driver.unmute_chat(ChatId(7)).unwrap();
        let unmute_json = recorder.snapshot().last().cloned().unwrap();
        let v: Value = serde_json::from_str(&unmute_json).unwrap();
        assert_eq!(v["@type"], "setChatNotificationSettings");
        assert_eq!(v["@extra"], unmute.0.to_string());
        assert_eq!(v["notification_settings"]["mute_for"], 0);

        let archive = driver.archive_chat(ChatId(7)).unwrap();
        let archive_json = recorder.snapshot().last().cloned().unwrap();
        let v: Value = serde_json::from_str(&archive_json).unwrap();
        assert_eq!(v["@type"], "addChatToList");
        assert_eq!(v["@extra"], archive.0.to_string());
        assert_eq!(v["chat_list"]["@type"], "chatListArchive");
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"0","is_pinned":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"4","is_pinned":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(driver.session.ordered_chats().is_empty());
        assert_eq!(driver.session.ordered_archived_chats()[0].id.0, 7);

        let unarchive = driver.unarchive_chat(ChatId(7)).unwrap();
        let unarchive_json = recorder.snapshot().last().cloned().unwrap();
        let v: Value = serde_json::from_str(&unarchive_json).unwrap();
        assert_eq!(v["@type"], "addChatToList");
        assert_eq!(v["@extra"], unarchive.0.to_string());
        assert_eq!(v["chat_list"]["@type"], "chatListMain");
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_sends_typing_then_cancel_on_empty_and_send() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        driver.sync_outgoing_typing("hello", false, 1_000).unwrap();
        let typing = recorder.snapshot().last().cloned().expect("sendChatAction");
        let v: Value = serde_json::from_str(&typing).unwrap();
        assert_eq!(v["@type"], "sendChatAction");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["business_connection_id"], "");
        assert_eq!(v["action"]["@type"], "chatActionTyping");

        let before = recorder.snapshot().len();
        driver.sync_outgoing_typing("hello!", false, 2_000).unwrap();
        assert_eq!(recorder.snapshot().len(), before, "4s throttle");

        driver
            .sync_outgoing_typing("hello!!", false, 1_000 + OUTGOING_TYPING_INTERVAL_MS)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(recorder.snapshot().last().unwrap()).unwrap()["action"]["@type"],
            "chatActionTyping"
        );

        driver.sync_outgoing_typing("   ", false, 9_000).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(recorder.snapshot().last().unwrap()).unwrap()["action"]["@type"],
            "chatActionCancel"
        );

        driver.sync_outgoing_typing("again", false, 10_000).unwrap();
        let snap = ComposerSnapshot::capture(ChatId(7), driver.session.view_generation, "again");
        driver.send_text_snapshot(&snap).unwrap();
        let sent = recorder.snapshot();
        assert_eq!(
            serde_json::from_str::<Value>(&sent[sent.len() - 2]).unwrap()["@type"],
            "sendMessage"
        );
        assert_eq!(
            serde_json::from_str::<Value>(sent.last().unwrap()).unwrap()["action"]["@type"],
            "chatActionCancel"
        );

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatAction","chat_id":7,"topic_id":null,"sender_id":{"@type":"messageSenderUser","user_id":7},"action":{"@type":"chatActionTyping"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(driver.session.chats.get(&7).unwrap().is_peer_typing());
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn private_draft_debounces_then_flushes_and_skips_channels() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0,"draft_message":{"@type":"draftMessage","reply_to":{"@type":"inputMessageReplyToMessage","message_id":3,"quote":null,"checklist_task_id":0,"poll_option_id":""},"date":1,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"meet at 6","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":8,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":8,"is_channel":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let restored = driver.session.chats.get(&7).unwrap().draft.clone().unwrap();
        assert_eq!(restored.text, "meet at 6");
        assert_eq!(restored.reply_to_message_id, Some(MessageId(3)));
        let outcome = driver
            .note_composer_draft(ChatId(7), "meet at 6!", None, 1_000, true)
            .unwrap();
        let DraftSaveOutcome::Debounced { token, delay } = outcome else {
            panic!("expected debounce");
        };
        assert_eq!(delay, DRAFT_SAVE_DEBOUNCE);
        assert!(
            driver
                .commit_debounced_draft(token.wrapping_add(9))
                .unwrap()
                .is_none()
        );
        driver.commit_debounced_draft(token).unwrap();
        let sent = recorder.snapshot();
        let draft_json = sent.last().unwrap();
        assert!(draft_json.contains("setChatDraftMessage"));
        assert!(draft_json.contains("meet at 6!"));
        assert!(draft_json.contains("\"topic_id\":null"));
        assert_eq!(
            driver
                .session
                .chats
                .get(&7)
                .unwrap()
                .draft
                .as_ref()
                .unwrap()
                .text,
            "meet at 6!"
        );
        let flushed = driver
            .note_composer_draft(ChatId(7), "leaving", Some(MessageId(3)), 2_000, false)
            .unwrap();
        assert_eq!(flushed, DraftSaveOutcome::Sent);
        assert!(
            driver
                .note_composer_draft(ChatId(8), "nope", None, 3_000, false)
                .unwrap()
                == DraftSaveOutcome::Skipped
        );
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageSendSucceeded","message":{"id":9,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"leaving","entities":[]}}},"old_message_id":-1}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.draft_clears, vec![ChatId(7)]);
        driver.clear_draft_after_send(ChatId(7), true).unwrap();
        assert!(driver.session.chats.get(&7).unwrap().draft.is_none());
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_open_flushes_leaving_draft_and_media_send_drops_reply() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        for (id, title) in [(7, "Ada"), (8, "Bob")] {
            driver
                .ingest(
                    copy_and_parse(
                        &format!(
                            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
                        ),
                        &seq,
                        &dyn_sink,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        driver.select_chat(ChatId(7)).unwrap();
        let outcome = driver
            .note_composer_draft(ChatId(7), "hello", Some(MessageId(3)), 1_000, true)
            .unwrap();
        assert!(matches!(outcome, DraftSaveOutcome::Debounced { .. }));
        driver
            .select_search_chat(ChatId(8), "hello", Some(MessageId(3)), 1_500)
            .unwrap();
        assert_eq!(driver.session.open_chat, Some(ChatId(8)));
        let saved = driver.session.chats.get(&7).unwrap().draft.clone().unwrap();
        assert_eq!(saved.text, "hello");
        assert_eq!(saved.reply_to_message_id, Some(MessageId(3)));
        let sent = recorder.snapshot();
        assert!(sent.iter().any(|json| {
            json.contains("setChatDraftMessage")
                && json.contains("\"chat_id\":7")
                && json.contains("hello")
                && json.contains("\"message_id\":3")
        }));
        driver.select_chat(ChatId(7)).unwrap();
        driver
            .note_composer_draft(ChatId(7), "caption", Some(MessageId(3)), 2_000, false)
            .unwrap();
        driver
            .note_composer_draft(ChatId(7), "caption", None, 2_100, false)
            .unwrap();
        let after = driver.session.chats.get(&7).unwrap().draft.clone().unwrap();
        assert_eq!(after.text, "caption");
        assert_eq!(after.reply_to_message_id, None);
        driver
            .note_composer_draft(ChatId(7), "  ", Some(MessageId(9)), 3_000, false)
            .unwrap();
        driver
            .select_search_message(ChatId(8), MessageId(1), "  ", None, 3_100)
            .unwrap();
        assert!(driver.session.chats.get(&7).unwrap().draft.is_none());
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Phase 4.2: driver guards around `send_poll_answer` / `send_poll_draft`.
    type PollDriverHarness = (
        std::path::PathBuf,
        ConnectDriver<Arc<RecordingSender>>,
        Arc<RecordingSender>,
        Arc<MemorySink>,
        Arc<dyn DiagnosticSink>,
        AtomicU64,
    );
    const POLL_OPEN_JSON: &str = r#"{"@type":"updateNewMessage","message":{"id":106,"chat_id":7,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9001,"question":{"@type":"formattedText","text":"Lunch?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Sushi","entities":[]},"voter_count":12,"vote_percentage":55,"is_chosen":false},{"@type":"pollOption","id":"b","text":{"@type":"formattedText","text":"Pizza","entities":[]},"voter_count":7,"vote_percentage":32,"is_chosen":false}],"total_voter_count":19,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{"@type":"pollTypeRegular"}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}}}"#;
    const POLL_CLOSED_JSON: &str = r#"{"@type":"updateNewMessage","message":{"id":107,"chat_id":7,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9002,"question":{"@type":"formattedText","text":"Red planet?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Mars","entities":[]},"voter_count":18,"vote_percentage":72,"is_chosen":false}],"total_voter_count":25,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":false,"is_closed":true,"type":{"@type":"pollTypeQuiz","correct_option_ids":[0],"explanation":{"@type":"formattedText","text":"","entities":[]}}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}}}"#;

    fn poll_driver() -> PollDriverHarness {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        driver
            .ingest(copy_and_parse(POLL_OPEN_JSON, &seq, &dyn_sink).unwrap())
            .unwrap();
        driver
            .ingest(copy_and_parse(POLL_CLOSED_JSON, &seq, &dyn_sink).unwrap())
            .unwrap();
        (dir, driver, recorder, sink, dyn_sink, seq)
    }

    fn assert_invalid<T: std::fmt::Debug>(result: Result<T, ConnectSendError>) {
        match result {
            Err(ConnectSendError::InvalidRequest) => {}
            other => panic!("expected InvalidRequest, got {other:?}"),
        }
    }

    #[test]
    fn driver_poll_answer_rejects_when_chats_path_inactive() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink);
        // No auth state seeded: `chats_path_active()` is false.
        let mut driver = ConnectDriver::new(session, recorder, test_credentials(), prepared);
        assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(106), 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_poll_answer_rejects_unsupported_chat() {
        let (dir, mut driver, _recorder, sink, dyn_sink, seq) = poll_driver();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":9},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_invalid(driver.send_poll_answer(ChatId(9), MessageId(106), 0));
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_poll_answer_guards_message_poll_and_index() {
        let (dir, mut driver, recorder, sink, _dyn_sink, _seq) = poll_driver();
        // Missing message.
        assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(404), 0));
        // Not a poll (message 50 is Alice's text message).
        assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(50), 0));
        // Closed poll (quiz, message 107).
        assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(107), 0));
        // Out-of-range option index.
        assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(106), 9));
        // Valid single-answer tap: `setPollAnswer` with the 0-based position,
        // plus an optimistic chosen mark before `updatePoll` arrives.
        let extra = driver
            .send_poll_answer(ChatId(7), MessageId(106), 0)
            .unwrap();
        let send_json = recorder.snapshot().last().cloned().expect("setPollAnswer");
        let v: Value = serde_json::from_str(&send_json).unwrap();
        assert_eq!(v["@type"], "setPollAnswer");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_id"], 106);
        assert_eq!(v["option_ids"], serde_json::json!([0]));
        let history = driver.session.histories.get(&7).unwrap();
        let message = history.messages.get(&106).unwrap();
        match &message.content {
            MessageContent::Poll(poll) => {
                assert!(poll.poll.options[0].is_chosen);
                assert!(!poll.poll.options[1].is_chosen);
            }
            other => panic!("{other:?}"),
        }
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_poll_draft_guards_and_request_shape() {
        use crate::poll::PollDraft;

        let (dir, mut driver, recorder, sink, dyn_sink, seq) = poll_driver();
        let valid = PollDraft {
            question: "Lunch?".into(),
            options: vec!["Sushi".into(), "Pizza".into()],
            is_anonymous: true,
            allows_multiple_answers: false,
        };
        let invalid = PollDraft {
            question: "  ".into(),
            options: vec!["Sushi".into(), "Pizza".into()],
            is_anonymous: true,
            allows_multiple_answers: false,
        };
        // Invalid draft rejected before any request is built.
        assert_invalid(driver.send_poll_draft(ChatId(7), &invalid, None));
        // Admin-gated broadcast channel: `can_post()` is false.
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":13,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_invalid(driver.send_poll_draft(ChatId(13), &valid, None));
        // Valid draft: `sendMessage` + `inputMessagePoll`.
        let extra = driver
            .send_poll_draft(ChatId(7), &valid, Some(MessageId(50)))
            .unwrap();
        let send_json = recorder.snapshot().last().cloned().expect("sendMessage");
        let v: Value = serde_json::from_str(&send_json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["reply_to"]["message_id"], 50);
        let content = &v["input_message_content"];
        assert_eq!(content["@type"], "inputMessagePoll");
        assert_eq!(content["question"]["text"], "Lunch?");
        assert_eq!(content["options"][0]["text"]["text"], "Sushi");
        assert_eq!(content["type"]["@type"], "inputPollTypeRegular");
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C3a: `join_video_chat` must accept the tracked, unjoined call
    /// (the normal flow: `getGroupCall` creates the tracker, then the
    /// overlay's Join button calls this) while rejecting a missing,
    /// mismatched, or already-joined call.
    #[test]
    fn join_video_chat_guard_allows_tracked_unjoined_call() {
        const UNJOINED_CALL: &str = r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Demo voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":false,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#;

        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        // No tracked call: rejected.
        assert_invalid(driver.join_video_chat(555));

        // Track call 555 unjoined, as `getGroupCall` would: join allowed.
        driver
            .ingest(copy_and_parse(UNJOINED_CALL, &seq, &dyn_sink).unwrap())
            .unwrap();
        assert!(
            driver
                .session
                .active_group_call
                .as_ref()
                .is_some_and(|c| c.id == 555 && !c.is_joined)
        );
        let extra = driver
            .join_video_chat(555)
            .expect("join tracked unjoined call");
        let sent = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("joinVideoChat sent");
        let v: Value = serde_json::from_str(&sent).unwrap();
        assert_eq!(v["@type"], "joinVideoChat");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["group_call_id"], 555);

        // A different tracked call id is rejected…
        assert_invalid(driver.join_video_chat(777));
        // …and so is the same call once joined.
        driver.session.active_group_call.as_mut().unwrap().is_joined = true;
        assert_invalid(driver.join_video_chat(555));

        let _ = std::fs::remove_dir_all(&dir);
    }

    type CallDriverHarness = (
        std::path::PathBuf,
        ConnectDriver<Arc<RecordingSender>>,
        Arc<RecordingSender>,
        Arc<dyn DiagnosticSink>,
        AtomicU64,
    );

    fn call_driver() -> CallDriverHarness {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        (dir, driver, recorder, sink, AtomicU64::new(0))
    }

    fn ingest_call_json(
        driver: &mut ConnectDriver<Arc<RecordingSender>>,
        seq: &AtomicU64,
        sink: &Arc<dyn DiagnosticSink>,
        json: &str,
    ) {
        driver
            .ingest(copy_and_parse(json, seq, sink).unwrap())
            .unwrap();
    }

    fn sent_request(recorder: &RecordingSender, type_name: &str) -> Value {
        recorder
            .snapshot()
            .into_iter()
            .rev()
            .map(|json| serde_json::from_str::<Value>(&json).unwrap())
            .find(|value| value["@type"] == type_name)
            .unwrap_or_else(|| panic!("missing {type_name} request"))
    }

    #[test]
    fn call_engine_lifecycle_bridge() {
        let (dir, mut driver, recorder, sink, seq) = call_driver();
        let mock = MockEngine::new();
        let handle = mock.clone();
        driver.set_call_engine(Box::new(mock));
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
        );
        assert_eq!(handle.started_calls(), vec![(77, 41, false)]);

        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateNewCallSignalingData","call_id":77,"data":"aW5ib3VuZA=="}"#,
        );
        assert_eq!(handle.signaling_received(), vec![(77, b"inbound".to_vec())]);

        driver.accept_call().unwrap();
        assert_eq!(handle.accepted_calls(), vec![77]);
        let accept = sent_request(&recorder, "acceptCall");
        assert_eq!(accept["protocol"]["udp_p2p"], true);
        assert_eq!(accept["protocol"]["udp_reflector"], true);
        assert_eq!(accept["protocol"]["min_layer"], 92);
        assert_eq!(accept["protocol"]["max_layer"], 92);
        assert_eq!(
            accept["protocol"]["library_versions"],
            serde_json::json!(["8.0.0", "9.0.0", "12.0.0", "13.0.0"])
        );

        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":false,"need_debug_information":false,"need_log":false}}}"#,
        );
        assert_eq!(handle.hung_up_calls(), vec![77]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_engine_emission_sends_signaling_data() {
        let (dir, mut driver, recorder, sink, seq) = call_driver();
        let mock = MockEngine::new();
        let handle = mock.clone();
        driver.set_call_engine(Box::new(mock));
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
        );

        handle.receive_signaling_data(77, b"emit-bytes");
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
        );
        let sent = sent_request(&recorder, "sendCallSignalingData");
        assert_eq!(sent["call_id"], 77);
        assert_eq!(sent["data"], "ZW1pdC1ieXRlcw==");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn seed_ready_call_user(
        driver: &mut ConnectDriver<Arc<RecordingSender>>,
        seq: &AtomicU64,
        sink: &Arc<dyn DiagnosticSink>,
    ) {
        ingest_call_json(
            driver,
            seq,
            sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        ingest_call_json(
            driver,
            seq,
            sink,
            r#"{"@type":"updateUser","user":{"id":41,"first_name":"Ada","type":{"@type":"userTypeRegular"}}}"#,
        );
    }

    #[test]
    fn create_call_uses_engine_protocol_when_present() {
        let (dir, mut driver, recorder, sink, seq) = call_driver();
        driver.set_call_engine(Box::new(MockEngine::new()));
        seed_ready_call_user(&mut driver, &seq, &sink);

        driver.start_call(41, false).unwrap();
        let create = sent_request(&recorder, "createCall");
        assert_eq!(create["protocol"]["udp_p2p"], true);
        assert_eq!(create["protocol"]["udp_reflector"], true);
        assert_eq!(create["protocol"]["min_layer"], 92);
        assert_eq!(create["protocol"]["max_layer"], 92);
        assert_eq!(
            create["protocol"]["library_versions"],
            serde_json::json!(["8.0.0", "9.0.0", "12.0.0", "13.0.0"])
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_call_keeps_signaling_only_protocol_without_engine() {
        let (dir, mut driver, recorder, sink, seq) = call_driver();
        seed_ready_call_user(&mut driver, &seq, &sink);

        driver.start_call(41, false).unwrap();
        let create = sent_request(&recorder, "createCall");
        assert_eq!(create["protocol"]["udp_p2p"], false);
        assert_eq!(create["protocol"]["udp_reflector"], false);
        assert_eq!(create["protocol"]["min_layer"], 65);
        assert_eq!(create["protocol"]["max_layer"], 92);
        assert_eq!(
            create["protocol"]["library_versions"],
            serde_json::json!([])
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_engine_absent_keeps_signaling_only() {
        let (dir, mut driver, _recorder, sink, seq) = call_driver();
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
        );
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateNewCallSignalingData","call_id":77,"data":"ZGlhZ25vc3RpYw=="}"#,
        );
        assert_eq!(
            driver.session.active_call.as_ref().unwrap().signaling_queue,
            vec![b"diagnostic".to_vec()]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_call_uses_signaling_only_protocol_when_engine_unavailable() {
        let (dir, mut driver, recorder, sink, seq) = call_driver();
        driver.set_call_engine(Box::new(MockEngine::unavailable()));
        seed_ready_call_user(&mut driver, &seq, &sink);

        driver.start_call(41, false).unwrap();
        let create = sent_request(&recorder, "createCall");
        assert_eq!(create["protocol"]["udp_p2p"], false);
        assert_eq!(create["protocol"]["udp_reflector"], false);
        assert_eq!(create["protocol"]["min_layer"], 65);
        assert_eq!(create["protocol"]["max_layer"], 92);
        assert_eq!(
            create["protocol"]["library_versions"],
            serde_json::json!([])
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_engine_ignores_signaling_for_untracked_call() {
        let (dir, mut driver, _recorder, sink, seq) = call_driver();
        let mock = MockEngine::new();
        let handle = mock.clone();
        driver.set_call_engine(Box::new(mock));
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        // No tracked call: signaling must stay diagnostic-only.
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateNewCallSignalingData","call_id":77,"data":"aW5ib3VuZA=="}"#,
        );
        assert!(driver.session.active_call.is_none());
        assert!(handle.signaling_received().is_empty());

        // Tracked call 77, then signaling for a different call id: gated.
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
        );
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateNewCallSignalingData","call_id":78,"data":"aW5ib3VuZA=="}"#,
        );
        assert!(handle.signaling_received().is_empty());

        // Signaling for the tracked call still bridges.
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateNewCallSignalingData","call_id":77,"data":"aW5ib3VuZA=="}"#,
        );
        assert_eq!(handle.signaling_received(), vec![(77, b"inbound".to_vec())]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2c: Ready `updateCall` carrying transport material
    /// (reflector + WebRTC servers, base64 key, `allow_p2p`).
    const READY_CALL_JSON: &str = r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateReady","protocol":{"@type":"callProtocol","udp_p2p":true,"udp_reflector":true,"min_layer":92,"max_layer":92,"library_versions":["13.0.0"]},"servers":[{"@type":"callServer","id":"7","ip_address":"149.154.167.40","ipv6_address":"2001:b28:f23d:f001::a","port":443,"type":{"@type":"callServerTypeTelegramReflector","peer_tag":"AAEC","is_tcp":true}},{"@type":"callServer","id":"8","ip_address":"203.0.113.1","ipv6_address":"","port":3478,"type":{"@type":"callServerTypeWebrtc","username":"alice","password":"secret","supports_turn":true,"supports_stun":false}}],"config":"{}","encryption_key":"AQIDBA==","emojis":[],"allow_p2p":true}}}"#;

    fn ready_call_driver() -> (
        std::path::PathBuf,
        ConnectDriver<Arc<RecordingSender>>,
        MockEngine,
        Arc<dyn DiagnosticSink>,
        AtomicU64,
    ) {
        let (dir, mut driver, _recorder, sink, seq) = call_driver();
        let mock = MockEngine::new();
        let handle = mock.clone();
        driver.set_call_engine(Box::new(mock));
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
        );
        (dir, driver, handle, sink, seq)
    }

    #[test]
    fn call_ready_connects_transport_once_with_mapped_params() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        // Pre-select devices before the transport exists.
        driver
            .select_call_devices(Some("mic-a".into()), Some("spk-a".into()))
            .unwrap();
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);

        let connects = handle.connects();
        assert_eq!(connects.len(), 1);
        let (call_id, params) = &connects[0];
        assert_eq!(*call_id, 77);
        assert_eq!(params.encryption_key, vec![1, 2, 3, 4]);
        assert!(params.is_outgoing);
        assert!(params.p2p_allowed);
        assert_eq!(
            params.library_versions,
            vec!["8.0.0", "9.0.0", "12.0.0", "13.0.0"]
        );
        assert_eq!(params.mic_input.as_deref(), Some("mic-a"));
        assert_eq!(params.speaker_input.as_deref(), Some("spk-a"));
        assert_eq!(params.servers.len(), 2);
        let reflector = &params.servers[0];
        assert_eq!(reflector.id, 7);
        assert_eq!(reflector.ipv4, "149.154.167.40");
        assert_eq!(reflector.ipv6, "2001:b28:f23d:f001::a");
        assert_eq!(reflector.port, 443);
        assert!(reflector.username.is_empty());
        assert!(reflector.turn);
        assert!(!reflector.stun);
        assert!(reflector.tcp);
        assert_eq!(reflector.peer_tag, vec![0, 1, 2]);
        let webrtc = &params.servers[1];
        assert_eq!(webrtc.id, 8);
        assert_eq!(webrtc.username, "alice");
        assert_eq!(webrtc.password, "secret");
        assert!(webrtc.turn);
        assert!(!webrtc.stun);
        assert!(!webrtc.tcp);
        assert!(webrtc.peer_tag.is_empty());

        let call = driver.session.active_call.as_ref().unwrap();
        assert_eq!(call.transport, Some(TransportState::Connecting));
        assert_eq!(call.transport_error, None);

        // A second Ready update must not reconnect.
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        assert_eq!(handle.connects().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_ready_without_key_fails_transport_honestly() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        let no_key = READY_CALL_JSON.replace(r#""encryption_key":"AQIDBA==","#, "");
        ingest_call_json(&mut driver, &seq, &sink, &no_key);

        assert!(handle.connects().is_empty());
        let call = driver.session.active_call.as_ref().unwrap();
        assert_eq!(call.transport, Some(TransportState::Failed));
        assert_eq!(
            call.transport_error.as_deref(),
            Some("call became ready without an encryption key")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_transport_callback_updates_session() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        handle.emit_transport_state(77, TransportState::Connected);
        // The next pump drains the transport outbox into the session.
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);

        let call = driver.session.active_call.as_ref().unwrap();
        assert_eq!(call.transport, Some(TransportState::Connected));
        assert_eq!(call.transport_error, None);
        assert_eq!(handle.connects().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_transport_retries_same_params_three_times_then_stops() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        let original = handle.connects()[0].1.clone();

        for expected_connects in 2..=4 {
            handle.emit_transport_state(77, TransportState::Failed);
            ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
            assert_eq!(handle.connects().len(), expected_connects);
            assert_eq!(handle.connects().last().unwrap().1, original);
            assert_eq!(
                driver.session.active_call.as_ref().unwrap().transport,
                Some(TransportState::Reconnecting)
            );
        }

        handle.emit_transport_state(77, TransportState::Failed);
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        assert_eq!(handle.connects().len(), 4);
        let call = driver.session.active_call.as_ref().unwrap();
        assert_eq!(call.transport, Some(TransportState::Failed));
        assert_eq!(
            call.transport_error.as_deref(),
            Some("reconnect attempts exhausted")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_transport_connected_resets_reconnect_attempts() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        for _ in 0..2 {
            handle.emit_transport_state(77, TransportState::Failed);
            ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        }
        handle.emit_transport_state(77, TransportState::Connected);
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);

        for _ in 0..3 {
            handle.emit_transport_state(77, TransportState::Failed);
            ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        }
        assert_eq!(handle.connects().len(), 6);
        assert_eq!(
            driver.session.active_call.as_ref().unwrap().transport,
            Some(TransportState::Reconnecting)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_transport_reconnect_error_is_reported() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        handle.fail_next_connect();
        handle.emit_transport_state(77, TransportState::Failed);
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);

        let call = driver.session.active_call.as_ref().unwrap();
        assert_eq!(call.transport, Some(TransportState::Failed));
        assert_eq!(
            call.transport_error.as_deref(),
            Some("call engine operation connect failed with code -1")
        );
        assert_eq!(handle.connects().len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_debug_information_contains_real_driver_fields() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        driver
            .select_call_devices(Some("mic-a".into()), Some("spk-a".into()))
            .unwrap();
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        handle.emit_transport_state(77, TransportState::Connected);
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":false,"need_debug_information":true,"need_log":false}}}"#,
        );

        let payload: Value =
            serde_json::from_str(&driver.call_debug_information().unwrap()).unwrap();
        assert_eq!(payload["app"], "quill");
        assert_eq!(payload["call_id"], 77);
        assert_eq!(payload["engine_available"], true);
        assert_eq!(payload["engine_protocol"]["min_layer"], 92);
        assert_eq!(
            payload["engine_protocol"]["library_versions"],
            serde_json::json!(["8.0.0", "9.0.0", "12.0.0", "13.0.0"])
        );
        assert_eq!(payload["final_transport_state"], "connected");
        assert_eq!(payload["had_audio"], true);
        assert_eq!(payload["microphone_device_id"], "mic-a");
        assert_eq!(payload["speaker_device_id"], "spk-a");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_mute_goes_through_engine_first() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        handle.emit_transport_state(77, TransportState::Connected);
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);

        assert!(driver.set_call_muted(true).is_ok());
        assert_eq!(handle.mute_changes(), vec![(77, true)]);
        assert!(driver.session.active_call.as_ref().unwrap().muted);
        assert!(driver.set_call_muted(false).is_ok());
        assert_eq!(handle.mute_changes(), vec![(77, true), (77, false)]);
        assert!(!driver.session.active_call.as_ref().unwrap().muted);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_mute_failure_leaves_state_unchanged() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        handle.emit_transport_state(77, TransportState::Connected);
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);

        handle.fail_mute();
        let err = driver.set_call_muted(true).unwrap_err();
        assert!(matches!(err, EngineError::Engine { .. }));
        assert!(!driver.session.active_call.as_ref().unwrap().muted);
        assert!(handle.mute_changes().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_mute_without_active_call_is_no_active_call() {
        let (dir, mut driver, _recorder, _sink, _seq) = call_driver();
        driver.set_call_engine(Box::new(MockEngine::new()));
        assert_eq!(driver.set_call_muted(true), Err(EngineError::NoActiveCall));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_device_selection_stored_before_connect_forwarded_after() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        handle.set_devices(vec![crate::calls::engine::MediaDevice {
            id: "mic-a".into(),
            name: "Mic A".into(),
            kind: crate::calls::engine::MediaDeviceKind::Microphone,
        }]);
        // Pending, not Ready: stored only, never forwarded; install does
        // not enumerate either.
        driver
            .select_call_devices(Some("mic-a".into()), Some("spk-a".into()))
            .unwrap();
        assert_eq!(
            driver.selected_call_devices(),
            (Some("mic-a"), Some("spk-a"))
        );
        assert!(handle.device_selections().is_empty());
        assert!(driver.call_devices().is_empty());

        // Ready connects with the stored selection as stream inputs and
        // refreshes the device cache.
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        let connects = handle.connects();
        assert_eq!(connects.len(), 1);
        assert_eq!(connects[0].1.mic_input.as_deref(), Some("mic-a"));
        assert_eq!(connects[0].1.speaker_input.as_deref(), Some("spk-a"));
        assert_eq!(driver.call_devices().len(), 1);

        // After connect the selection forwards to the engine.
        driver
            .select_call_devices(Some("mic-b".into()), None)
            .unwrap();
        assert_eq!(
            handle.device_selections(),
            vec![(77, Some("mic-b".to_string()), None)]
        );
        assert_eq!(driver.selected_call_devices(), (Some("mic-b"), None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_device_refresh_without_engine_is_best_effort() {
        let (dir, mut driver, _recorder, _sink, _seq) = call_driver();
        driver.refresh_call_devices();
        assert!(driver.call_devices().is_empty());
        assert_eq!(driver.selected_call_devices(), (None, None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2b: sender that fails the next sendCallSignalingData send once,
    /// then records. Mirrors the `ViewCtlSender` fail-one-message pattern.
    struct FailFirstCallSender {
        sent: Mutex<Vec<String>>,
        fail_next: Mutex<bool>,
    }

    impl FailFirstCallSender {
        fn new() -> Self {
            Self {
                sent: Mutex::new(Vec::new()),
                fail_next: Mutex::new(true),
            }
        }

        fn snapshot(&self) -> Vec<String> {
            self.sent.lock().expect("failing sender").clone()
        }
    }

    impl JsonSender for Arc<FailFirstCallSender> {
        fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
            if request.contains("sendCallSignalingData")
                && *self.fail_next.lock().expect("failing sender")
            {
                *self.fail_next.lock().expect("failing sender") = false;
                return Err(ConnectSendError::Native);
            }
            self.sent
                .lock()
                .expect("failing sender")
                .push(request.to_string());
            Ok(())
        }
    }

    /// Phase C2b: driver harness with a failing-first sender.
    type FailingCallDriverHarness = (
        std::path::PathBuf,
        ConnectDriver<Arc<FailFirstCallSender>>,
        Arc<FailFirstCallSender>,
        Arc<dyn DiagnosticSink>,
        AtomicU64,
    );

    fn failing_call_driver() -> FailingCallDriverHarness {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let sender = Arc::new(FailFirstCallSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, sender.clone(), test_credentials(), prepared);
        (dir, driver, sender, sink, AtomicU64::new(0))
    }

    fn ingest_failing(
        driver: &mut ConnectDriver<Arc<FailFirstCallSender>>,
        seq: &AtomicU64,
        sink: &Arc<dyn DiagnosticSink>,
        json: &str,
    ) -> Result<(), ConnectSendError> {
        driver.ingest(copy_and_parse(json, seq, sink).unwrap())
    }

    #[test]
    fn signaling_send_failure_requeues_and_cleans_request() {
        let (dir, mut driver, sender, sink, seq) = failing_call_driver();
        let mock = MockEngine::new();
        let handle = mock.clone();
        driver.set_call_engine(Box::new(mock));
        ingest_failing(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        )
        .unwrap();
        ingest_failing(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
        )
        .unwrap();

        handle.receive_signaling_data(77, b"emit-bytes");
        let signaling_sent = || {
            sender
                .snapshot()
                .into_iter()
                .filter(|json| json.contains("sendCallSignalingData"))
                .map(|json| serde_json::from_str::<Value>(&json).unwrap())
                .collect::<Vec<_>>()
        };
        assert!(signaling_sent().is_empty());

        let failed = ingest_failing(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
        );
        assert!(matches!(failed, Err(ConnectSendError::Native)));
        // The failed request was removed from bookkeeping and the unsent
        // bytes returned to the outbox: nothing sent, nothing lost.
        assert!(
            !driver
                .session
                .requests
                .has_purpose(RequestPurpose::SendCallSignalingData)
        );
        assert!(signaling_sent().is_empty());

        // The next ingest retries the same bytes exactly once.
        ingest_failing(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
        )
        .unwrap();
        let sent = signaling_sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["@type"], "sendCallSignalingData");
        assert_eq!(sent[0]["call_id"], 77);
        assert_eq!(sent[0]["data"], "ZW1pdC1ieXRlcw==");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2e: video-call variant of `READY_CALL_JSON`.
    fn ready_video_call_json() -> String {
        READY_CALL_JSON.replace("\"is_video\":false", "\"is_video\":true")
    }

    /// Phase C2e: the engine's peer camera state reaches the tracked
    /// call through the pump; emissions for other call ids are dropped.
    #[test]
    fn remote_video_state_drain() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        handle.emit_remote_video_state(999, RemoteVideoState::Active);
        handle.emit_remote_video_state(77, RemoteVideoState::Paused);
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        assert_eq!(
            driver.session.active_call.as_ref().unwrap().remote_video,
            RemoteVideoState::Paused
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2e: only the newest frame per (call, side) is kept — the
    /// UI always sees the latest, never a backlog.
    #[test]
    fn frame_slots_latest_wins() {
        let (dir, driver, handle, _sink, _seq) = ready_call_driver();
        let frame = |is_local: bool| VideoFrame {
            seq: 0,
            width: 2,
            height: 2,
            rgba: vec![0u8; 16],
            is_local,
            participant_user_id: None,
            is_screen: false,
        };
        handle.emit_video_frame(77, frame(false));
        handle.emit_video_frame(77, frame(false));
        let latest = driver.latest_video_frame(77, false).unwrap();
        assert_eq!(latest.seq, 1);
        assert!(driver.latest_video_frame(77, true).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2e: the UI camera toggle reaches the engine with the
    /// call id, the new state, and the selected camera — but only
    /// once a transport exists; before that the intent is stored
    /// cleanly (the real engine errors on an untracked call) so it
    /// can apply on connect.
    #[test]
    fn set_call_camera_gates_engine_on_transport() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        // No transport yet: intent stored, engine untouched.
        assert!(driver.set_call_camera(77, false).is_ok());
        assert!(handle.camera_changes().is_empty());
        assert!(!driver.session.active_call.as_ref().unwrap().camera_on);
        // Transport connected: the toggle drives the engine.
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        assert!(driver.set_call_camera(77, true).is_ok());
        assert_eq!(handle.camera_changes(), vec![(77, true, None)]);
        assert!(driver.session.active_call.as_ref().unwrap().camera_on);
        assert_eq!(
            driver.set_call_camera(999, true),
            Err(crate::calls::engine::EngineError::NoSuchCall(999))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2e: picking a camera before the transport exists stores
    /// the selection cleanly without error and without an engine
    /// forward.
    #[test]
    fn select_call_camera_without_transport_stores_selection() {
        let (dir, mut driver, handle, _sink, _seq) = ready_call_driver();
        assert!(driver.select_call_camera(Some("cam-1".into())).is_ok());
        assert!(handle.camera_changes().is_empty());
        assert_eq!(driver.selected_call_camera(), Some("cam-1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2e: no camera enumerated → a video call must not
    /// negotiate video.
    #[test]
    fn connect_params_video_honors_no_camera() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        driver.refresh_call_devices();
        ingest_call_json(&mut driver, &seq, &sink, &ready_video_call_json());
        let connects = handle.connects();
        assert_eq!(connects.len(), 1);
        assert!(!connects[0].1.video_enabled);
        assert_eq!(connects[0].1.camera_input, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2e: a camera-off toggle set before the transport exists
    /// survives into the connect params — the call is negotiated
    /// without video even though a camera is available.
    #[test]
    fn connect_params_camera_off_intent_survives_pre_connect() {
        let (dir, mut driver, _recorder, sink, seq) = call_driver();
        let mock = MockEngine::new();
        mock.set_devices(vec![MediaDevice {
            id: "cam-1".into(),
            name: "Test Cam".into(),
            kind: MediaDeviceKind::Camera,
        }]);
        let handle = mock.clone();
        driver.set_call_engine(Box::new(mock));
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":true,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
        );
        // Camera toggled off before the transport exists: stores the
        // intent without an engine forward.
        assert!(driver.set_call_camera(77, false).is_ok());
        assert!(handle.camera_changes().is_empty());
        ingest_call_json(&mut driver, &seq, &sink, &ready_video_call_json());
        let connects = handle.connects();
        assert_eq!(connects.len(), 1);
        assert!(!connects[0].1.video_enabled);
        assert_eq!(connects[0].1.camera_input, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2e: a camera enumerated → video negotiated with the
    /// user's camera pick (or the engine default when unset).
    #[test]
    fn connect_params_video_selects_camera() {
        let (dir, mut driver, _recorder, sink, seq) = call_driver();
        let mock = MockEngine::new();
        mock.set_devices(vec![MediaDevice {
            id: "cam-1".into(),
            name: "Test Cam".into(),
            kind: MediaDeviceKind::Camera,
        }]);
        let handle = mock.clone();
        driver.set_call_engine(Box::new(mock));
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        driver.select_call_camera(Some("cam-1".into())).unwrap();
        ingest_call_json(&mut driver, &seq, &sink, &ready_video_call_json());
        let connects = handle.connects();
        assert_eq!(connects.len(), 1);
        assert!(connects[0].1.video_enabled);
        assert_eq!(connects[0].1.camera_input, Some("cam-1".to_string()));
        assert_eq!(driver.selected_call_camera(), Some("cam-1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2e: the reducer initializes the camera intent from
    /// `is_video` — video calls start with the camera on.
    #[test]
    fn active_call_camera_on_from_is_video() {
        for (is_video, expected) in [(false, false), (true, true)] {
            let (dir, mut driver, _recorder, sink, seq) = call_driver();
            ingest_call_json(
                &mut driver,
                &seq,
                &sink,
                &format!(
                    r#"{{"@type":"updateCall","call":{{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":{is_video},"state":{{"@type":"callStatePending","is_created":true,"is_received":false}}}}}}"#
                ),
            );
            let call = driver.session.active_call.as_ref().unwrap();
            assert_eq!(call.camera_on, expected);
            assert_eq!(call.remote_video, RemoteVideoState::Inactive);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// Phase C2e: frame slots for an ended call are dropped — the UI
    /// can never render a stale picture from a previous call.
    #[test]
    fn frame_slots_cleared_on_call_end() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        handle.emit_video_frame(
            77,
            VideoFrame {
                seq: 0,
                width: 2,
                height: 2,
                rgba: vec![0u8; 16],
                is_local: false,
                participant_user_id: None,
                is_screen: false,
            },
        );
        assert!(driver.latest_video_frame(77, false).is_some());
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":false,"need_debug_information":false,"need_log":false}}}"#,
        );
        assert!(driver.latest_video_frame(77, false).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
    fn group_call_test_driver() -> (
        std::path::PathBuf,
        Arc<RecordingSender>,
        ConnectDriver<Arc<RecordingSender>>,
        AtomicU64,
    ) {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        (dir, recorder, driver, seq)
    }

    fn tracked_group_call(
        need_rejoin: bool,
        can_be_managed: bool,
        is_owned: bool,
    ) -> ActiveGroupCall {
        ActiveGroupCall {
            id: 77,
            title: "Team voice".into(),
            is_video_chat: false,
            is_joined: true,
            need_rejoin,
            can_be_managed,
            is_owned,
            is_muted_self: true,
            participant_count: 1,
            ..ActiveGroupCall::fresh(77)
        }
    }

    /// Phase C2f: participant-management request shapes and gates
    /// (schema 1.8.67 — inviteGroupCallParticipant :14375,
    /// banGroupCallParticipants :14385,
    /// setGroupCallParticipantVolumeLevel :14438,
    /// declineGroupCallInvitation :14380, joinGroupCall :14285).
    #[test]
    fn driver_group_call_participant_management_shapes_and_gates() {
        let (dir, recorder, mut driver, _seq) = group_call_test_driver();
        let last_sent =
            || serde_json::from_str::<Value>(recorder.snapshot().last().unwrap()).unwrap();

        // Gates with no active group call.
        assert_eq!(
            driver.invite_group_call_participant(7),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(
            driver.ban_group_call_participant(7),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(
            driver.set_group_call_participant_volume(MessageSender::User { user_id: 7 }, 10000),
            Err(ConnectSendError::InvalidRequest)
        );
        // Decline needs no active call.
        driver
            .decline_group_call_invitation(3, 42)
            .expect("decline sends without an active call");
        let sent = last_sent();
        assert_eq!(sent["@type"], "declineGroupCallInvitation");
        assert_eq!(sent["chat_id"], 3);
        assert_eq!(sent["message_id"], 42);

        driver.session.active_group_call = Some(tracked_group_call(false, true, false));

        // Invite: shape follows schema 1.8.67 :14375; `is_video`
        // follows the tracked call.
        driver
            .invite_group_call_participant(7)
            .expect("invite sends");
        let sent = last_sent();
        assert_eq!(sent["@type"], "inviteGroupCallParticipant");
        assert_eq!(sent["group_call_id"], 77);
        assert_eq!(sent["user_id"], 7);
        assert_eq!(sent["is_video"], false);

        // Ban: schema :14385 takes `user_ids:vector<int64>` (the
        // plural constructor), owner-gated on `groupCall.is_owned` —
        // `can_be_managed` is "for video chats and live stories only"
        // and does NOT grant ban rights in a voice chat.
        driver.session.active_group_call = Some(tracked_group_call(false, true, false));
        assert_eq!(
            driver.ban_group_call_participant(9),
            Err(ConnectSendError::InvalidRequest),
            "can_be_managed=true but is_owned=false must refuse"
        );
        driver.session.active_group_call = Some(tracked_group_call(false, false, true));
        driver
            .ban_group_call_participant(9)
            .expect("ban sends for owner");
        let sent = last_sent();
        assert_eq!(sent["@type"], "banGroupCallParticipants");
        assert_eq!(sent["group_call_id"], 77);
        assert_eq!(sent["user_ids"], serde_json::json!([9]));
        driver.session.active_group_call = Some(tracked_group_call(false, true, true));

        // Volume: schema :14438, 1-20000 (hundreds of percents).
        driver
            .set_group_call_participant_volume(MessageSender::User { user_id: 7 }, 5000)
            .expect("volume sends");
        let sent = last_sent();
        assert_eq!(sent["@type"], "setGroupCallParticipantVolumeLevel");
        assert_eq!(sent["group_call_id"], 77);
        assert_eq!(sent["volume_level"], 5000);
        assert_eq!(sent["participant_id"]["@type"], "messageSenderUser");
        assert_eq!(sent["participant_id"]["user_id"], 7);
        // Out-of-range levels clamp to the schema's 1-20000.
        driver
            .set_group_call_participant_volume(MessageSender::User { user_id: 7 }, 0)
            .expect("volume 0 clamps");
        assert_eq!(last_sent()["volume_level"], 1);
        driver
            .set_group_call_participant_volume(MessageSender::User { user_id: 7 }, 30_000)
            .expect("volume 30000 clamps");
        assert_eq!(last_sent()["volume_level"], 20000);

        // Accept: `joinGroupCall` with `inputGroupCallMessage`
        // (schema line 5288: accept via joinGroupCall). Refuses while
        // a 1:1 call is active.
        driver
            .accept_group_call_invitation(3, 42)
            .expect("accept sends without an active call");
        let sent = last_sent();
        assert_eq!(sent["@type"], "joinGroupCall");
        assert_eq!(sent["input_group_call"]["@type"], "inputGroupCallMessage");
        assert_eq!(sent["input_group_call"]["chat_id"], 3);
        assert_eq!(sent["input_group_call"]["message_id"], 42);
        driver.session.active_call = Some(ActiveCall {
            id: 5,
            user_id: 11,
            is_outgoing: true,
            is_video: false,
            state: CallState::Pending {
                is_created: true,
                is_received: false,
            },
            started_at: std::time::Instant::now(),
            ready_at: None,
            ready: None,
            transport: None,
            transport_error: None,
            signaling_queue: Vec::new(),
            signaling_dropped: 0,
            muted: false,
            camera_on: false,
            remote_video: RemoteVideoState::Inactive,
        });
        assert_eq!(
            driver.accept_group_call_invitation(3, 42),
            Err(ConnectSendError::InvalidRequest)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2h: management drivers — recording, invite revocation,
    /// RTMP, in-call chat, and scheduled starts. Shapes follow the
    /// pinned TDLib 1.8.67 schema; gates follow the tracked `groupCall`
    /// flags.
    #[test]
    fn driver_group_call_management_shapes_and_gates() {
        let (dir, recorder, mut driver, seq) = group_call_test_driver();
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let last_sent =
            || serde_json::from_str::<Value>(recorder.snapshot().last().unwrap()).unwrap();

        driver.session.active_group_call = Some(tracked_group_call(false, true, false));
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .is_video_chat = true;

        // Recording: `startGroupCallRecording` / `endGroupCallRecording`
        // (schema :14400 / :14407), gated on `groupCall.can_be_managed`
        // for video chats.
        driver
            .start_group_call_recording("Team voice".to_string(), true)
            .expect("recording starts");
        let sent = last_sent();
        assert_eq!(sent["@type"], "startGroupCallRecording");
        assert_eq!(sent["group_call_id"], 77);
        assert_eq!(sent["title"], "Team voice");
        assert_eq!(sent["record_video"], true);
        driver.stop_group_call_recording().expect("recording stops");
        assert_eq!(last_sent()["@type"], "endGroupCallRecording");
        assert_eq!(last_sent()["group_call_id"], 77);
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .can_be_managed = false;
        assert_eq!(
            driver.start_group_call_recording("Team voice".to_string(), true),
            Err(ConnectSendError::InvalidRequest),
            "non-manageable call must refuse recording"
        );
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .can_be_managed = true;

        // Invite revocation: `revokeGroupCallInviteLink` (:14396),
        // gated on `can_be_managed` for video chats.
        driver
            .revoke_video_chat_invite_link()
            .expect("revoke sends");
        assert_eq!(last_sent()["@type"], "revokeGroupCallInviteLink");
        assert_eq!(last_sent()["group_call_id"], 77);

        // In-call chat: `sendGroupCallMessage` (:14335), gated on
        // `can_send_messages && are_messages_allowed`.
        {
            let call = driver.session.active_group_call.as_mut().unwrap();
            call.can_send_messages = true;
            call.are_messages_allowed = true;
        }
        driver
            .send_group_call_message("hello".to_string())
            .expect("message sends");
        let sent = last_sent();
        assert_eq!(sent["@type"], "sendGroupCallMessage");
        assert_eq!(sent["group_call_id"], 77);
        assert_eq!(sent["text"]["text"], "hello");
        assert_eq!(sent["paid_message_star_count"], 0);
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .are_messages_allowed = false;
        assert_eq!(
            driver.send_group_call_message("hello".to_string()),
            Err(ConnectSendError::InvalidRequest),
            "disabled chat must refuse send"
        );
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .are_messages_allowed = true;
        // Chat toggle: `toggleGroupCallAreMessagesAllowed` (:14319),
        // gated on `can_toggle_are_messages_allowed`; flips the flag.
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .can_toggle_are_messages_allowed = true;
        driver
            .toggle_group_call_are_messages_allowed()
            .expect("toggle sends");
        assert_eq!(last_sent()["@type"], "toggleGroupCallAreMessagesAllowed");
        assert_eq!(last_sent()["are_messages_allowed"], false);

        // RTMP: `getVideoChatRtmpUrl` / `replaceVideoChatRtmpUrl`
        // (:14261 / :14264) resolve the chat from the tracked call.
        ingest_call_json(
            &mut driver,
            &seq,
            &dyn_sink,
            r#"{"@type":"updateNewChat","chat":{"id":51,"title":"Design voice","type":{"@type":"chatTypeSupergroup","supergroup_id":51,"is_channel":false},"unread_count":0,"video_chat":{"@type":"videoChat","group_call_id":77,"has_participants":true}}}"#,
        );
        driver.fetch_video_chat_rtmp_url().expect("rtmp url sends");
        let sent = last_sent();
        assert_eq!(sent["@type"], "getVideoChatRtmpUrl");
        assert_eq!(sent["chat_id"], 51);
        // Regenerate is owner-gated (`replaceVideoChatRtmpUrl` mints a
        // new stream key).
        driver.session.active_group_call.as_mut().unwrap().is_owned = true;
        driver
            .replace_video_chat_rtmp_url()
            .expect("rtmp replace sends");
        let sent = last_sent();
        assert_eq!(sent["@type"], "replaceVideoChatRtmpUrl");
        assert_eq!(sent["chat_id"], 51);

        // Scheduling: `createVideoChat` start_date (schema :14256) —
        // 0 starts immediately; scheduled dates must be ≥10s and ≤8d
        // ahead. Requires no tracked call (a start/join target).
        driver.session.active_group_call = None;
        driver
            .start_video_chat(51, "Planning".to_string(), 0)
            .expect("immediate start sends");
        let sent = last_sent();
        assert_eq!(sent["@type"], "createVideoChat");
        assert_eq!(sent["start_date"], 0);
        assert_eq!(sent["is_rtmp_stream"], false);
        let future = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 3600;
        driver
            .start_video_chat(51, "Planning".to_string(), future)
            .expect("scheduled start sends");
        assert_eq!(last_sent()["start_date"], future);
        assert!(driver.start_video_chat(51, "x".to_string(), 1).is_err());
        // Empty title is valid per schema :14256 ("if empty, chat title
        // will be used") — it sends through.
        driver
            .start_video_chat(51, String::new(), 0)
            .expect("empty title falls back to chat title");
        assert_eq!(last_sent()["title"], "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2f: `joinGroupCall` success (`groupCallInfo`, schema
    /// 1.8.67 :7190) triggers a `getGroupCall` fetch so the accepted
    /// call gets tracked; the join payload is stored like the
    /// `joinVideoChat` Text arm.
    #[test]
    fn driver_group_call_invitation_accept_starts_tracking() {
        let (dir, recorder, mut driver, seq) = group_call_test_driver();
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        driver
            .accept_group_call_invitation(3, 42)
            .expect("accept sends");
        let sent: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        let extra = sent["@extra"].as_str().unwrap().to_string();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"groupCallInfo","group_call_id":555,"join_payload":"payload-1","@extra":"{extra}"}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        // The fetch queue is drained by `maybe_fetch_group_calls`
        // during ingest — assert the `getGroupCall` went out.
        let sent: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        assert_eq!(sent["@type"], "getGroupCall");
        assert_eq!(sent["group_call_id"], 555);
        assert_eq!(driver.session.group_call_error, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2f: auto-rejoin discipline — a `need_rejoin` group call
    /// rejoins automatically (max 3 attempts, retaining the join
    /// parameters and self-mute); a clean joined `updateGroupCall`
    /// resets the counter; manual retry resets it too.
    #[test]
    fn driver_auto_rejoin_group_call_discipline() {
        let (dir, recorder, mut driver, seq) = group_call_test_driver();
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let join_sends = || {
            recorder
                .snapshot()
                .into_iter()
                .filter(|json| json.contains("\"joinVideoChat\""))
                .count()
        };
        let fail_last_join = |driver: &mut ConnectDriver<Arc<RecordingSender>>| {
            let sent: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
            let extra = sent["@extra"].as_str().unwrap().to_string();
            driver
                .ingest(
                    copy_and_parse(
                        &format!(
                            r#"{{"@type":"error","code":500,"message":"BOOM","@extra":"{extra}"}}"#
                        ),
                        &seq,
                        &dyn_sink,
                    )
                    .unwrap(),
                )
                .unwrap();
        };

        // No tracked call: nothing to rejoin.
        assert!(driver.maybe_auto_rejoin_group_call().is_ok());
        assert_eq!(join_sends(), 0);

        driver.session.active_group_call = Some(tracked_group_call(true, false, false));
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .reconnecting = true;

        // Three attempts, each followed by a TDLib join failure that
        // re-arms the reconnecting banner (the C2d discipline).
        for attempt in 1..=3 {
            driver.maybe_auto_rejoin_group_call().expect("rejoin sends");
            assert_eq!(join_sends(), attempt);
            let sent: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
            assert_eq!(sent["@type"], "joinVideoChat");
            assert_eq!(sent["group_call_id"], 77);
            // Honest no-device params, retaining the self-mute.
            assert_eq!(sent["join_parameters"]["audio_source_id"], 0);
            assert_eq!(sent["join_parameters"]["is_muted"], true);
            assert_eq!(
                driver
                    .session
                    .active_group_call
                    .as_ref()
                    .unwrap()
                    .rejoin_attempts,
                attempt
            );
            fail_last_join(&mut driver);
        }
        // Exhausted: the error line is honest and no more attempts go
        // out — the banner + manual Rejoin remain the way out.
        assert_eq!(
            driver.session.group_call_error.as_deref(),
            Some("Reconnect attempts exhausted.")
        );
        assert!(driver.maybe_auto_rejoin_group_call().is_ok());
        assert_eq!(join_sends(), 3);

        // A clean joined `updateGroupCall` resets the counter, clears
        // the banner, and drops the stale error line.
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":77,"title":"Team voice","is_active":true,"is_video_chat":false,"is_joined":true,"need_rejoin":false,"can_be_managed":false,"is_owned":false,"participant_count":1,"loaded_all_participants":false,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":false,"mute_new_participants":false,"can_toggle_mute_new_participants":false,"can_change_title":false,"can_change_desc":false,"can_change_emoji":false,"scheduled_start_date":0,"title":"","description":"","emoji":""}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let call = driver.session.active_group_call.as_ref().unwrap();
        assert_eq!(call.rejoin_attempts, 0);
        assert!(!call.reconnecting);
        assert_eq!(driver.session.group_call_error, None);

        // Manual retry resets the counter: with `need_rejoin` back,
        // three fresh auto attempts are allowed after
        // `rejoin_group_call(true)`.
        call_state_for_rejoin(&mut driver);
        driver.rejoin_group_call(true).expect("manual retry sends");
        assert_eq!(
            driver
                .session
                .active_group_call
                .as_ref()
                .unwrap()
                .rejoin_attempts,
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn call_state_for_rejoin(driver: &mut ConnectDriver<Arc<RecordingSender>>) {
        let call = tracked_group_call(true, false, false);
        driver.session.active_group_call = Some(call);
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .reconnecting = true;
    }

    /// Phase C2g: driver with a chat-bound unjoined group call (chat 51
    /// -> call 555) and an available mock engine.
    type GroupCallDriverHarness = (
        std::path::PathBuf,
        ConnectDriver<Arc<RecordingSender>>,
        Arc<RecordingSender>,
        MockEngine,
        Arc<dyn DiagnosticSink>,
        AtomicU64,
    );
    fn ready_group_call_driver() -> GroupCallDriverHarness {
        let (dir, mut driver, recorder, sink, seq) = call_driver();
        let mock = MockEngine::new();
        let handle = mock.clone();
        driver.set_call_engine(Box::new(mock));
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":51,"title":"Design voice","type":{"@type":"chatTypeSupergroup","supergroup_id":51,"is_channel":false},"unread_count":0}}"#,
        );
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateChatVideoChat","chat_id":51,"video_chat":{"@type":"videoChat","group_call_id":555,"has_participants":true,"default_participant_id":null}}"#,
        );
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":false,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#,
        );
        (dir, driver, recorder, handle, sink, seq)
    }

    fn group_participant_json(
        user_id: i64,
        current: bool,
        video_info: &str,
        screen_info: &str,
    ) -> String {
        format!(
            r#"{{"@type":"updateGroupCallParticipant","group_call_id":555,"participant":{{"@type":"groupCallParticipant","participant_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"audio_source_id":0,"screen_sharing_audio_source_id":0,"video_info":{video_info},"screen_sharing_video_info":{screen_info},"bio":"","is_current_user":{current},"is_speaking":false,"is_hand_raised":false,"can_be_muted_for_all_users":false,"can_be_unmuted_for_all_users":false,"can_be_muted_for_current_user":false,"can_be_unmuted_for_current_user":false,"is_muted_for_all_users":false,"is_muted_for_current_user":false,"can_unmute_self":false,"volume_level":10000,"order":"a1"}}}}"#,
        )
    }

    /// Phase C2g: the join carries the native offer (not the no-device
    /// fallback) and the audio SSRC parsed from it.
    #[test]
    fn group_join_carries_engine_offer() {
        let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
        driver.join_video_chat(555).expect("join tracked call");
        assert_eq!(handle.group_offers(), vec![(555, 51)]);
        let join = sent_request(&recorder, "joinVideoChat");
        assert_eq!(join["join_parameters"]["payload"], "mock-group-offer-555");
        // The mock offer carries no `a=ssrc:` lines: honest 0.
        assert_eq!(join["join_parameters"]["audio_source_id"], 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2g: the `joinVideoChat` Text answer finishes the native
    /// handshake exactly once.
    #[test]
    fn group_join_answer_connects_native_transport() {
        let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
        let extra = driver.join_video_chat(555).expect("join tracked call");
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
                extra.0
            ),
        );
        assert_eq!(
            handle.group_connects(),
            vec![(555, "join-answer".to_string(), false)]
        );
        assert!(
            driver
                .session
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.transport_ready)
        );
        // A second pump (no new answer) must not reconnect.
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":true,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#,
        );
        assert_eq!(handle.group_connects().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2g (review fix): a rejoin resets the transport handshake —
    /// the new `joinVideoChat` answer reconnects the fresh native context
    /// instead of being dropped by the pump's `transport_ready` filter,
    /// and an orphaned presentation is stopped.
    #[test]
    fn group_rejoin_answer_reconnects_native_transport() {
        let (dir, mut driver, recorder, mut handle, sink, seq) = ready_group_call_driver();
        let need_rejoin = r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":true,"need_rejoin":true,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#;
        // Join → answer → the native transport is connected.
        let extra = driver.join_video_chat(555).expect("join tracked call");
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
                extra.0
            ),
        );
        assert!(
            driver
                .session
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.transport_ready)
        );
        assert_eq!(handle.group_connects().len(), 1);
        // A live presentation plus tracked screen-share state: the
        // rejoin must stop the orphaned native presentation and clear
        // the stale flags.
        handle.start_screen_share(555).expect("mock presentation");
        driver
            .session
            .active_group_call
            .as_mut()
            .expect("tracked call")
            .screen_sharing = true;
        driver
            .session
            .active_group_call
            .as_mut()
            .expect("tracked call")
            .screen_share_answer = "old-answer".into();
        // `need_rejoin` auto-rejoins with a fresh offer and a reset
        // handshake gate.
        ingest_call_json(&mut driver, &seq, &sink, need_rejoin);
        assert_eq!(handle.group_offers().len(), 2);
        assert_eq!(handle.screen_share_stops(), vec![555]);
        let call = driver
            .session
            .active_group_call
            .as_ref()
            .expect("tracked call");
        assert!(!call.transport_ready);
        assert!(call.join_payload.is_empty());
        assert!(!call.screen_sharing && !call.screen_share_pending);
        assert!(call.screen_share_answer.is_empty());
        // The new answer reconnects the fresh native context.
        let extra = sent_request(&recorder, "joinVideoChat")["@extra"]
            .as_str()
            .unwrap()
            .to_string();
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            &format!(r#"{{"@type":"text","text":"rejoin-answer","@extra":"{extra}"}}"#),
        );
        assert_eq!(handle.group_connects().len(), 2);
        assert!(
            driver
                .session
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.transport_ready)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2g: participant `video_info` / `screen_sharing_video_info`
    /// become engine subscriptions; paused endpoints and the local user
    /// are skipped.
    #[test]
    fn group_participant_video_syncs_subscriptions() {
        let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
        let extra = driver.join_video_chat(555).expect("join tracked call");
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
                extra.0
            ),
        );
        let camera = r#"{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[111,112]}],"endpoint_id":"ep-42","is_paused":false}"#;
        let screen = r#"{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[222]}],"endpoint_id":"ep-42-screen","is_paused":false}"#;
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            &group_participant_json(42, false, camera, screen),
        );
        // Paused endpoint: skipped.
        let paused = r#"{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[333]}],"endpoint_id":"ep-43","is_paused":true}"#;
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            &group_participant_json(43, false, paused, "null"),
        );
        // Local user: skipped even with video info.
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            &group_participant_json(777, true, camera, "null"),
        );
        let syncs = handle.group_video_syncs();
        let (call_id, sources) = syncs.last().expect("sync recorded");
        assert_eq!(*call_id, 555);
        assert_eq!(sources.len(), 2);
        let camera_source = sources
            .iter()
            .find(|source| source.endpoint == "ep-42")
            .expect("camera source");
        assert_eq!(camera_source.user_id, 42);
        assert_eq!(camera_source.ssrc_groups.len(), 1);
        assert_eq!(camera_source.ssrc_groups[0].semantics, "SIM");
        assert_eq!(camera_source.ssrc_groups[0].ssrcs, vec![111, 112]);
        let screen_source = sources
            .iter()
            .find(|source| source.endpoint == "ep-42-screen")
            .expect("screen source");
        assert_eq!(screen_source.user_id, 42);
        assert_eq!(screen_source.ssrc_groups[0].ssrcs, vec![222]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2g: frames with `participant_user_id` land in the group
    /// slots (camera vs screen), never in the 1:1 slots.
    #[test]
    fn group_video_frames_route_to_participant_slots() {
        let (dir, driver, _recorder, handle, _sink, _seq) = ready_group_call_driver();
        let frame = |screen: bool| VideoFrame {
            seq: 0,
            width: 2,
            height: 2,
            rgba: vec![0u8; 16],
            is_local: false,
            participant_user_id: Some(42),
            is_screen: screen,
        };
        handle.emit_video_frame(555, frame(false));
        handle.emit_video_frame(555, frame(true));
        assert!(driver.latest_group_video_frame(555, 42, false).is_some());
        assert!(driver.latest_group_video_frame(555, 42, true).is_some());
        assert!(driver.latest_group_video_frame(555, 43, false).is_none());
        assert!(driver.latest_video_frame(555, false).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2g: leaving tears the native transport down and drops the
    /// retained frames.
    #[test]
    fn group_call_leave_tears_down_transport_and_frames() {
        let (dir, mut driver, _recorder, mut handle, _sink, _seq) = ready_group_call_driver();
        handle.emit_video_frame(
            555,
            VideoFrame {
                seq: 0,
                width: 2,
                height: 2,
                rgba: vec![0u8; 16],
                is_local: false,
                participant_user_id: Some(42),
                is_screen: false,
            },
        );
        assert!(driver.latest_group_video_frame(555, 42, false).is_some());
        // Review fix: a live presentation is stopped before the call
        // (privacy — capture ends first).
        handle.start_screen_share(555).expect("mock presentation");
        driver.leave_group_call().expect("leave");
        assert_eq!(handle.group_leaves(), vec![555]);
        assert_eq!(handle.screen_share_stops(), vec![555]);
        assert!(driver.latest_group_video_frame(555, 42, false).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2g: screen-share toggle runs the presentation handshake —
    /// offer into `startGroupCallScreenSharing`, answer into the engine,
    /// stop pairs `endGroupCallScreenSharing` with the engine stop.
    #[test]
    fn group_screen_share_toggle_handshake() {
        let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
        // The engine enumerates a screen source; the driver picks it up
        // on connect.
        handle.set_devices(vec![MediaDevice {
            id: "screen-0".into(),
            name: "Test screen".into(),
            kind: MediaDeviceKind::Screen,
        }]);
        // Join and finish the native handshake first, as in reality.
        let join_extra = driver.join_video_chat(555).expect("join tracked call");
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
                join_extra.0
            ),
        );
        assert!(
            driver
                .session
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.transport_ready)
        );
        // TDLib confirms the join via `updateGroupCall is_joined`; the
        // toggle gates on it.
        driver
            .session
            .active_group_call
            .as_mut()
            .expect("tracked call")
            .is_joined = true;
        let start_extra = driver
            .toggle_group_call_screen_share()
            .expect("start sharing");
        assert_eq!(handle.screen_share_offers(), vec![555]);
        let start = sent_request(&recorder, "startGroupCallScreenSharing");
        assert_eq!(start["group_call_id"], 555);
        assert_eq!(start["payload"], "mock-presentation-offer-555");
        assert!(
            driver
                .session
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.screen_share_pending && !call.screen_sharing)
        );
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"text","text":"share-answer","@extra":{}}}"#,
                start_extra.0
            ),
        );
        assert_eq!(
            handle.screen_share_connects(),
            vec![(555, "share-answer".to_string())]
        );
        assert!(
            driver
                .session
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.screen_sharing && !call.screen_share_pending)
        );
        driver
            .toggle_group_call_screen_share()
            .expect("stop sharing");
        let end = sent_request(&recorder, "endGroupCallScreenSharing");
        assert_eq!(end["group_call_id"], 555);
        assert_eq!(handle.screen_share_stops(), vec![555]);
        assert!(
            driver
                .session
                .active_group_call
                .as_ref()
                .is_some_and(|call| !call.screen_sharing && !call.screen_share_pending)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2g: a failed screen-share handshake (the reducer clears
    /// the tracked flags on a `startGroupCallScreenSharing` error)
    /// leaves no stray native presentation: the pump stops it.
    #[test]
    fn group_screen_share_failure_stops_native_presentation() {
        let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
        handle.set_devices(vec![MediaDevice {
            id: "screen-0".into(),
            name: "Test screen".into(),
            kind: MediaDeviceKind::Screen,
        }]);
        let join_extra = driver.join_video_chat(555).expect("join tracked call");
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
                join_extra.0
            ),
        );
        driver
            .session
            .active_group_call
            .as_mut()
            .expect("tracked call")
            .is_joined = true;
        driver
            .toggle_group_call_screen_share()
            .expect("start sharing");
        assert!(handle.presentation_active(555));
        // Simulate the reducer's error arm: flags cleared, error
        // surfaced, native presentation untouched.
        if let Some(call) = driver.session.active_group_call.as_mut() {
            call.screen_share_pending = false;
            call.screen_sharing = false;
            call.screen_share_answer.clear();
        }
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":true,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#,
        );
        assert_eq!(handle.screen_share_stops(), vec![555]);
        assert!(!handle.presentation_active(555));
        assert!(recorder.snapshot().len() >= 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2g: without an enumerated screen source the toggle is
    /// rejected and no request goes out ("No screen source available").
    #[test]
    fn group_screen_share_rejected_without_screen_source() {
        let (dir, mut driver, recorder, _handle, _sink, _seq) = ready_group_call_driver();
        driver
            .session
            .active_group_call
            .as_mut()
            .expect("tracked call")
            .is_joined = true;
        assert!(driver.toggle_group_call_screen_share().is_err());
        assert!(
            !driver
                .session
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.screen_share_pending || call.screen_sharing)
        );
        assert!(
            recorder
                .snapshot()
                .iter()
                .all(|json| !json.contains("startGroupCallScreenSharing"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
