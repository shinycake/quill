//! Live TDLib connect gate: credentials + tdjson → setTdlibParameters → auth updates.
//! Never logs api_hash, phone numbers, or codes.

use crate::calls::engine::{
    CallEngine, ConnectParams, EngineError, GroupVideoSource, GroupVideoSourceGroup, MediaDevice,
    MediaDeviceKind, RemoteVideoState, RtcServer, TransportState, VideoFrame,
    group_offer_audio_source_id, video_wanted,
};
use crate::composer::{
    AttachmentKind, ComposerEdit, ComposerEditKind, ComposerSnapshot, DeleteConfirm,
    DraftSaveClock, DraftSaveStep, ForwardDraft, SendOptions, draft_text_to_store,
    schedule_draft_save,
};
use crate::credentials::TelegramCredentials;
use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::folders::spec_without_chat;
use crate::ids::{AccountKey, ChatId, FileId, MessageId, RequestId, TopicId};
use crate::lifecycle::{RestoreBlocker, plan_restore};
use crate::notify::NotificationSoundKind;
use crate::platform::{DatabaseKey, KeyDecision, SecretStore, load_or_create_key};
use crate::poll::{PollDraft, can_stop_poll, poll_answer_for_tap};
use crate::rich::RichBlock;
use crate::settings::{
    AccountPaths, InstantViewMode, default_app_root, load_call_prefs, load_contact_prefs,
    load_media_prefs, save_call_prefs, save_contact_prefs,
};
use crate::state::{
    AdminListFetch, AdminRightsFetch, CHAT_EVENT_LOG_PAGE_SIZE, ChatEventLogFetch,
    ChatSearchJumpNeed, ChatStatisticsFetch, ComposerLinkPreview, ForwardFlight, InfoPanelTarget,
    InlineQueryFetch, InlineQuerySlot, InstantViewPage, InviteLinkFetch, JoinRequestFetch,
    LoginUrlRequest, MemberListFilter, MemberStatusChange, PasswordOp, PaymentRequest,
    PollVotersFetch, RequestPurpose, RequestRollback, SearchStatus, Session, SharedMediaTab,
    ShutdownPhase, SupergroupMembersFetch, WelcomeMessagesFetch,
};
use crate::story_composer::{StoryMediaKind, StoryPrivacy};
use crate::telegram::client::{LiveTdJson, OwnedEnvelope, ReceiveBridge};
use crate::telegram::envelope::{
    AuthorizationState, CallState, ChatAdminRights, ChatDraft, ChatFolderSpec, ChatKind,
    ChatNotificationSettings, ChatPermissions, EnvelopePayload, GroupCallVideoInfo, MUTE_FOREVER,
    MessageContent, MessageSender, NotificationSettingsScope, OrderInfoData,
    ParsedGroupCallParticipant, ReadyParams, RichMessageContent, ScopeNotificationSettings,
    StoryContentView, UsernameCheckResult,
};
use crate::telegram::ffi::{LibraryOrigin, TdJsonError, resolve_tdjson_path};
use crate::telegram::requests::{
    AnimationSend, ArchiveChatListSettings, CallPrivacySetting, ChatEventLogFilterSet,
    GroupCallJoinParams, ImportedContact, InputGroupCallRef, MessageSenderRef, PollSend,
    PollTypeSend, PrivacyWho, SendReply, SetTdlibParameters, StickerSend, VideoNoteSend,
    VideoNoteThumbnailSend, VideoSend, VoiceNoteSend, accept_call_with_protocol,
    activate_story_stealth_mode as activate_story_stealth_mode_request, add_chat_member,
    add_chat_members, add_chat_to_list, add_chat_to_list_value, add_chat_welcome_message,
    add_contact, add_message_reaction, add_recently_found_chat, ban_group_call_participants,
    boost_chat, can_post_story as can_post_story_request,
    cancel_download_file as cancel_download_file_request,
    cancel_recovery_email_address_verification, chat_member_status_administrator_json,
    chat_member_status_banned_json, chat_member_status_member_json,
    chat_member_status_restricted_json, check_authentication_code, check_authentication_password,
    check_chat_username, clear_imported_contacts, clear_recently_found_chats,
    click_chat_sponsored_message, close_chat, close_request,
    close_secret_chat as close_secret_chat_request, close_story, create_call_with_protocol,
    create_chat_folder, create_chat_invite_link, create_forum_topic, create_new_basic_group_chat,
    create_new_secret_chat, create_new_supergroup_chat, create_private_chat, create_video_chat,
    decline_group_call_invitation, delete_chat, delete_chat_folder, delete_chat_history,
    delete_chat_reply_markup as delete_chat_reply_markup_request, delete_chat_welcome_message,
    delete_forum_topic, delete_messages, delete_profile_photo, delete_story,
    discard_call as discard_call_request, disconnect_all_websites, disconnect_website,
    download_file as download_file_request, edit_chat_folder, edit_chat_invite_link,
    edit_chat_welcome_message, edit_forum_topic, edit_message_caption, edit_message_text,
    edit_story as edit_story_request, edit_story_cover as edit_story_cover_request, end_group_call,
    end_group_call_recording, end_group_call_screen_sharing, forward_messages, get_active_sessions,
    get_archive_chat_list_settings, get_authorization_state, get_available_chat_boost_slots,
    get_basic_group_full_info, get_bot_similar_bots, get_callback_query_answer,
    get_callback_query_answer_game, get_callback_query_answer_with_password,
    get_chat_active_stories, get_chat_administrators, get_chat_boost_status, get_chat_event_log,
    get_chat_folder, get_chat_history, get_chat_invite_links, get_chat_join_requests,
    get_chat_lists_to_add_chat, get_chat_member, get_chat_scheduled_messages,
    get_chat_sponsored_messages, get_chat_statistics,
    get_chats_to_post_stories as get_chats_to_post_stories_request, get_commands,
    get_connected_websites, get_contacts, get_forum_topics, get_full_rich_message, get_group_call,
    get_inline_query_results, get_installed_sticker_sets, get_link_preview, get_login_url,
    get_login_url_info, get_me, get_message_link, get_message_properties,
    get_message_thread_history, get_password_state, get_payment_form, get_payment_receipt,
    get_poll_voters, get_saved_animations, get_saved_notification_sounds,
    get_scope_notification_settings, get_secret_chat, get_sticker_set, get_storage_statistics,
    get_story, get_story_available_reactions,
    get_story_interactions as get_story_interactions_request, get_supergroup,
    get_supergroup_full_info, get_supergroup_members, get_user_full_info,
    get_user_privacy_setting_rules, get_video_chat_invite_link, get_video_chat_rtmp_url,
    get_web_page_instant_view, import_contacts, input_message_photo, input_message_video,
    invite_group_call_participant, join_chat, join_group_call, join_video_chat, leave_chat,
    leave_group_call, load_active_stories, load_chat_welcome_messages, load_chats, load_chats_list,
    load_group_call_participants, open_chat, open_message_content, open_story, pin_chat_message,
    post_story as post_story_request, process_chat_join_request, read_chat_list, recognize_speech,
    remove_contacts, remove_message_reaction, reorder_active_usernames, reorder_chat_folders,
    replace_primary_chat_invite_link, replace_video_chat_rtmp_url, report_chat,
    report_chat_sponsored_message, report_story as report_story_request,
    request_qr_code_authentication, resend_authentication_code, resend_messages,
    resend_recovery_email_address_code, revoke_chat_invite_link, revoke_group_call_invite_link,
    search_call_messages, search_chat_messages, search_chats, search_messages,
    search_messages_filter_json, search_public_chats, search_recently_found_chats, send_animation,
    send_bot_start_message as send_bot_start_message_request, send_call_debug_information,
    send_call_log, send_call_rating_detail, send_call_signaling_data, send_chat_action,
    send_chat_action_kind, send_document, send_group_call_message, send_message_album,
    send_payment_form as send_payment_form_request, send_photo, send_poll, send_rich_message,
    send_sticker, send_text, send_text_story_reply, send_video, send_video_note, send_voice_note,
    set_archive_chat_list_settings, set_authentication_phone_number, set_bio,
    set_chat_draft_message, set_chat_member_status, set_chat_member_tag,
    set_chat_message_auto_delete_time, set_chat_notification_settings, set_chat_permissions,
    set_chat_slow_mode_delay, set_group_call_participant_volume_level,
    set_message_sender_block_list, set_name, set_password, set_pinned_chats, set_poll_answer,
    set_profile_photo, set_recovery_email_address, set_scope_notification_settings,
    set_story_privacy_settings as set_story_privacy_settings_request, set_story_reaction,
    set_supergroup_username, set_user_privacy_setting_rules, set_username, set_video_chat_title,
    start_group_call_recording, start_group_call_screen_sharing, start_scheduled_video_chat,
    stop_poll as stop_poll_request, supergroup_members_filter_administrators_json,
    supergroup_members_filter_banned_json, supergroup_members_filter_recent_json,
    supergroup_members_filter_restricted_json, supergroup_members_filter_search_json,
    terminate_all_other_sessions, terminate_session, toggle_chat_folder_tags,
    toggle_chat_is_marked_as_unread, toggle_chat_is_pinned, toggle_forum_topic_closed,
    toggle_forum_topic_pinned, toggle_general_forum_topic_hidden,
    toggle_group_call_are_messages_allowed, toggle_group_call_is_my_video_enabled,
    toggle_group_call_is_my_video_paused, toggle_group_call_participant_is_hand_raised,
    toggle_group_call_participant_is_muted, toggle_session_can_accept_calls,
    toggle_session_can_accept_secret_chats, toggle_supergroup_aggressive_anti_spam,
    toggle_supergroup_is_broadcast_group, toggle_supergroup_join_by_request,
    toggle_supergroup_sign_messages, toggle_username_is_active,
    toggle_video_chat_enabled_start_notification, toggle_video_chat_mute_new_participants,
    unpin_all_chat_messages, unpin_chat_message,
    validate_order_info as validate_order_info_request, view_messages, view_sponsored_chat,
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
/// `downloadFile.priority` for automatic full-media downloads (MED3: above
/// thumbs so enabled media actually arrives, far below explicit user
/// downloads).
pub const AUTO_MEDIA_DOWNLOAD_PRIORITY: i32 = 4;
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
/// Slice media-shared-gallery: `searchChatMessages.limit` for one gallery-tab
/// page (`<= 100`, schema/td_api.tl:11862).
pub const SHARED_MEDIA_PAGE_SIZE: i32 = 50;
/// Unigram `LoadMessageSliceImpl`: `GetChatHistory(chatId, maxId, -25, 50)`.
pub const HISTORY_AROUND_OFFSET: i32 = -25;
/// Phase 5.1: `getForumTopics.limit` — first page of the topic list.
pub const FORUM_TOPICS_LIMIT: i32 = 100;
/// Phase 5.1: `searchChatMessages.limit` for per-topic history pages.
pub const TOPIC_HISTORY_PAGE_SIZE: i32 = 50;
/// Unigram `LoadMessageSliceImpl` page size around the jump target.
pub const HISTORY_AROUND_LIMIT: i32 = 50;
/// Slice CL: `getChatHistory.limit` for the chat-list peek preview — a
/// peek shows the most recent messages, not a scrollable history.
pub const PREVIEW_HISTORY_LIMIT: i32 = 10;

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
    /// MED4: caption exceeded `getOption("message_caption_length_max")`
    /// (schema:6088). The UI shows the limit; the send never goes out.
    CaptionTooLong {
        limit: i32,
    },
}

/// MED4: outcome of an Instant View open attempt (TGX
/// `TdlibUi.fetchInstantView` + browser fallback).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstantViewOutcome {
    /// Instant View is off (or the request send failed) — the caller
    /// opens the URL in the browser.
    Browser,
    /// `getWebPageInstantView` was sent. Success lands in
    /// `Session::instant_view`; TDLib's 404 (no Instant View for this
    /// page) lands in `Session::instant_view_fallback_url` so the UI
    /// opens the browser — a refusal is never shown as success.
    Requested,
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
/// Phase C2e: latest decoded video frame per (call id, is_local,
/// is_screen); only the newest frame is kept, so the UI never sees a
/// backlog. The peer's screen share lives in its own slot so it can
/// never clobber the peer camera frame (Phase C2j).
type VideoFrameSlots = Arc<Mutex<HashMap<(i32, bool, bool), VideoFrame>>>;

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
    /// Phase C2j: engine-emitted peer 1:1 screen-share states (worker
    /// thread -> driver pump). When the peer's share goes inactive the
    /// pump drops the retained screen frames so no stale picture can
    /// render.
    screen_state_outbox: VideoStateOutbox,
    /// Phase C2e: latest decoded video frame per (call id, is_local,
    /// is_screen).
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
    reply_to: Option<SendReply>,
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
            screen_state_outbox: Arc::new(Mutex::new(VecDeque::new())),
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
        // Phase C2j: the peer's 1:1 screen-share state rides the same
        // worker-thread -> driver-pump path as the camera state.
        let screen_state_outbox = self.screen_state_outbox.clone();
        engine.set_remote_screen_state_callback(Arc::new(move |call_id, state| {
            screen_state_outbox
                .lock()
                .expect("call screen state outbox")
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
                // Phase C2j: the screen share gets its own slot keyed on
                // `is_screen` so it never clobbers the peer camera frame.
                video_frame_slots
                    .lock()
                    .expect("call video frame slots")
                    .insert((call_id, frame.is_local, frame.is_screen), frame);
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
    /// Phase C2i: shared by the 1:1 screen-share toggle (same native
    /// `MediaDeviceKind::Screen` gate, no presentation handshake).
    pub fn call_screen_source_available(&self) -> bool {
        self.call_devices_cache
            .iter()
            .any(|device| device.kind == MediaDeviceKind::Screen)
    }

    /// Phase C2g: group-call alias of `call_screen_source_available`.
    pub fn group_call_screen_source_available(&self) -> bool {
        self.call_screen_source_available()
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
    /// no frame has arrived yet. The peer's screen share lives in its
    /// own slot — see `latest_screen_frame`.
    pub fn latest_video_frame(&self, call_id: i32, is_local: bool) -> Option<VideoFrame> {
        self.video_frame_slots
            .lock()
            .expect("call video frame slots")
            .get(&(call_id, is_local, false))
            .cloned()
    }

    /// Phase C2j: newest decoded frame of the peer's 1:1 screen share;
    /// `None` when the peer is not sharing (or no frame has arrived yet).
    /// Post-Phase-9 UI renders this as the screen-share tile; until
    /// then the backend keeps the slot warm and drops it when the
    /// peer's share goes inactive.
    pub fn latest_screen_frame(&self, call_id: i32) -> Option<VideoFrame> {
        self.video_frame_slots
            .lock()
            .expect("call video frame slots")
            .get(&(call_id, false, true))
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
    /// Phase C2i: enabling the camera clears the screen-share intent —
    /// ntgcalls forbids camera+screen in Capture mode.
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
        if enabled {
            call.screen_sharing = false;
        }
        Ok(())
    }

    /// Phase C2i: 1:1 screen-share send toggle through the native
    /// engine. Same contract as `set_call_camera`: the engine is
    /// called first and its error propagates *without* flipping the
    /// session flag; without a connected transport the intent is only
    /// stored (it applies on connect, mirroring the pre-transport
    /// mute in `pump_call_engine`). Enabling clears the camera intent
    /// — ntgcalls forbids camera+screen in Capture mode. Rejected
    /// without an enumerated screen source
    /// (`call_screen_source_available`) when *enabling*; stopping
    /// needs no source (a vanished display must not trap the user in
    /// "sharing").
    pub fn set_call_screen_share(
        &mut self,
        call_id: i32,
        enabled: bool,
    ) -> Result<(), EngineError> {
        // Gate first: it borrows `&self`, which can't overlap the
        // mutable call borrow below.
        let screen_available = self.call_screen_source_available();
        let Some(call) = self.session.active_call.as_mut() else {
            return Err(EngineError::NoActiveCall);
        };
        if call.id != call_id {
            return Err(EngineError::NoSuchCall(call_id));
        }
        if enabled && !screen_available {
            return Err(EngineError::NoScreenSource);
        }
        if call.transport.is_some()
            && let Some(engine) = self.call_engine.as_deref_mut()
            && engine.is_available()
        {
            engine.set_screen_share_enabled(call.id, enabled)?;
        }
        call.screen_sharing = enabled;
        if enabled {
            call.camera_on = false;
        }
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
        // M2: capture the `getFullRichMessage` answer before `apply`
        // takes the pending request; the full blocks replace the
        // partial message's blocks in history after apply.
        let full_rich_answer: Option<(ChatId, MessageId, RichMessageContent)> =
            match &owned.envelope.payload {
                EnvelopePayload::RichMessage { rich } => owned
                    .envelope
                    .extra
                    .and_then(|id| self.session.requests.purpose(id))
                    .and_then(|purpose| match purpose {
                        RequestPurpose::GetFullRichMessage {
                            chat_id,
                            message_id,
                        } => Some((chat_id, message_id, rich.clone())),
                        _ => None,
                    }),
                _ => None,
            };
        // M1: capture the `getMessageLink` answer before `apply` takes the
        // pending request; the UI drains `Session::message_link_result`
        // into the clipboard.
        let message_link_answer: Option<String> = match &owned.envelope.payload {
            EnvelopePayload::MessageLink { link, .. } => owned
                .envelope
                .extra
                .and_then(|id| self.session.requests.purpose(id))
                .is_some_and(|purpose| purpose == RequestPurpose::GetMessageLink)
                .then(|| link.clone()),
            _ => None,
        };
        // M1 fix-up: capture the `getMessageProperties` answer for the
        // "Share link" gate before `apply` takes the pending request.
        let link_gate: Option<(ChatId, MessageId, bool)> = match &owned.envelope.payload {
            EnvelopePayload::MessageProperties { can_get_link } => owned
                .envelope
                .extra
                .and_then(|id| self.session.requests.purpose(id))
                .and_then(|purpose| match purpose {
                    RequestPurpose::GetMessageLinkProperties {
                        chat_id,
                        message_id,
                    } => Some((chat_id, message_id, *can_get_link)),
                    _ => None,
                }),
            _ => None,
        };
        // A5: capture the `checkChatUsername` verdict before `apply`
        // takes the pending request. The verdict is stashed with the
        // in-flight username text so the edit-profile dialog can ignore
        // stale answers for superseded text.
        let username_check_answer: Option<(String, UsernameCheckResult)> =
            match &owned.envelope.payload {
                EnvelopePayload::CheckChatUsernameResult(result) => owned
                    .envelope
                    .extra
                    .and_then(|id| self.session.requests.purpose(id))
                    .and_then(|purpose| {
                        self.session
                            .username_check_pending
                            .clone()
                            .filter(|_| purpose == RequestPurpose::CheckUsername)
                            .map(|username| (username, *result))
                    }),
                _ => None,
            };
        // MED4: capture the `getWebPageInstantView` answer before `apply`
        // takes the pending request; the UI drains
        // `Session::instant_view` into the IV reader. The URL rides
        // `Session::instant_view_urls` (the purpose stays `Copy`).
        let instant_view_answer: Option<(String, RichMessageContent)> =
            match &owned.envelope.payload {
                EnvelopePayload::WebPageInstantView { rich } => owned
                    .envelope
                    .extra
                    .and_then(|id| {
                        (self.session.requests.purpose(id)
                            == Some(RequestPurpose::GetWebPageInstantView))
                        .then_some(id)
                    })
                    .and_then(|id| {
                        self.session
                            .instant_view_urls
                            .remove(&id)
                            .map(|url| (url, rich.clone()))
                    }),
                _ => None,
            };
        // MED4: a failed `getWebPageInstantView` (TDLib 404 = the page has
        // no Instant View) falls back to the browser like TGX — the URL
        // is stashed for the UI drain, never rendered as a reader.
        let instant_view_fallback: Option<String> = match &owned.envelope.payload {
            EnvelopePayload::Error(_) => owned.envelope.extra.and_then(|id| {
                (self.session.requests.purpose(id) == Some(RequestPurpose::GetWebPageInstantView))
                    .then_some(id)
                    .and_then(|id| self.session.instant_view_urls.remove(&id))
            }),
            _ => None,
        };
        // MED4b: capture the `getLinkPreview` answer before `apply` takes
        // the pending request; the composer chip reads
        // `Session::composer_preview`. A late answer for a superseded URL
        // is dropped (the chip only cares about the latest request).
        let link_preview_answer: Option<ComposerLinkPreview> = match &owned.envelope.payload {
            EnvelopePayload::LinkPreview { preview } => owned
                .envelope
                .extra
                .and_then(|id| {
                    (self.session.requests.purpose(id) == Some(RequestPurpose::GetLinkPreview))
                        .then_some(id)
                })
                .and_then(|id| {
                    self.session
                        .composer_preview_urls
                        .remove(&id)
                        .map(|url| ComposerLinkPreview {
                            url,
                            preview: Some(preview.clone()),
                        })
                }),
            _ => None,
        };
        // MED4b: a failed `getLinkPreview` (TDLib 404 = no preview for
        // this URL) is "no link info" (TGX `LinkPreview.isNotFound`) —
        // never a card, never a crash.
        let link_preview_failed: Option<String> = match &owned.envelope.payload {
            EnvelopePayload::Error(_) => owned.envelope.extra.and_then(|id| {
                (self.session.requests.purpose(id) == Some(RequestPurpose::GetLinkPreview))
                    .then_some(id)
                    .and_then(|id| self.session.composer_preview_urls.remove(&id))
            }),
            _ => None,
        };
        // Slice CL2: our `createPrivateChat` answer — the bare `chat`
        // parses as `UpdateNewChat`; the `@extra` tells it apart from a
        // genuine `updateNewChat`. Captured before `apply` takes the
        // pending request; opened through the normal `select_chat`
        // flow (openChat + history) after apply inserts the chat.
        let created_chat: Option<ChatId> = match &owned.envelope.payload {
            EnvelopePayload::UpdateNewChat { chat_id, .. } => owned
                .envelope
                .extra
                .and_then(|id| self.session.requests.purpose(id))
                .and_then(|purpose| {
                    (purpose == RequestPurpose::CreatePrivateChat).then_some(*chat_id)
                }),
            _ => None,
        };
        // Slice G2: capture forum/welcome/boost mutations before `apply`
        // takes the pending request. The state drops the stale cache on
        // confirmed success; the post-apply refetch reloads it now that
        // the server has applied the change (never pre-confirmation).
        let mutation_refetch: Option<(RequestPurpose, ChatId)> = owned
            .envelope
            .extra
            .and_then(|id| self.session.requests.get(id))
            .and_then(|pending| match pending.purpose {
                RequestPurpose::CreateForumTopic
                | RequestPurpose::EditForumTopic { .. }
                | RequestPurpose::ToggleForumTopicClosed { .. }
                | RequestPurpose::ToggleForumTopicPinned { .. }
                | RequestPurpose::DeleteForumTopic { .. }
                | RequestPurpose::ToggleGeneralForumTopicHidden
                | RequestPurpose::AddChatWelcomeMessage
                | RequestPurpose::EditChatWelcomeMessage { .. }
                | RequestPurpose::DeleteChatWelcomeMessage { .. }
                | RequestPurpose::BoostChat => {
                    pending.chat_id.map(|chat_id| (pending.purpose, chat_id))
                }
                _ => None,
            });
        self.session.apply(owned);
        self.pump_call_engine(active_call_before, bridge_signaling)?;
        self.pump_group_call_transport(active_group_call_before)?;
        self.maybe_send_parameters()?;
        self.maybe_probe_channel_membership()?;
        // Slice G2: chain `boostChat` once the slots answer arrives.
        self.maybe_continue_boost()?;
        // Slice G2: refetch caches the state dropped after a confirmed
        // mutation. A dropped cache is the success signal — on a TDLib
        // error the cache stays and nothing refetches.
        if let Some((purpose, chat_id)) = mutation_refetch {
            match purpose {
                RequestPurpose::CreateForumTopic
                | RequestPurpose::EditForumTopic { .. }
                | RequestPurpose::ToggleForumTopicClosed { .. }
                | RequestPurpose::ToggleForumTopicPinned { .. }
                | RequestPurpose::DeleteForumTopic { .. }
                | RequestPurpose::ToggleGeneralForumTopicHidden
                    if !self.session.forum_topics.contains_key(&chat_id.0) =>
                {
                    let _ = self.refresh_forum_topics(chat_id);
                }
                RequestPurpose::AddChatWelcomeMessage
                | RequestPurpose::EditChatWelcomeMessage { .. }
                | RequestPurpose::DeleteChatWelcomeMessage { .. }
                    if !self.session.welcome_messages.contains_key(&chat_id.0) =>
                {
                    let _ = self.load_chat_welcome_messages(chat_id);
                }
                RequestPurpose::BoostChat
                    if !self.session.chat_boost_status.contains_key(&chat_id.0) =>
                {
                    let _ = self.fetch_chat_boost_status(chat_id);
                }
                _ => {}
            }
        }
        // Slice CL2: open the `createPrivateChat` chat (Saved Messages
        // flow) through the normal chat-open path — `openChat` and
        // history load. `let _` on purpose: the chat is already in the
        // model; a failed open must not fail the ingest.
        if let Some(chat_id) = created_chat {
            let _ = self.select_chat(chat_id);
        }
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
        // Slice A3: a `terminateSession` / `terminateAllOtherSessions`
        // `ok` marks the sessions list stale in the reducer; refetch the
        // authoritative answer on the same ingest.
        let _ = self.refresh_active_sessions_if_stale();
        // Slice A4: a `disconnectWebsite` / `disconnectAllWebsites` `ok`
        // marks the websites list stale in the reducer; same pattern.
        let _ = self.refresh_connected_websites_if_stale();
        if view_after {
            self.maybe_view_open_messages()?;
        }
        // M2: full rich blocks replace the partial message's blocks in
        // history. Only `RichMessage` rows are touched — an unrelated
        // response can never clobber a different content kind.
        if let Some((chat_id, message_id, rich)) = full_rich_answer
            && let Some(history) = self.session.histories.get_mut(&chat_id.0)
            && let Some(message) = history.messages.get_mut(&message_id.0)
            && let MessageContent::RichMessage(existing) = &mut message.content
        {
            *existing = rich;
        }
        // M1: stash the `getMessageLink` answer for the UI clipboard drain.
        if let Some(link) = message_link_answer {
            self.session.message_link_result = Some(link);
        }
        // A5: stash the `checkChatUsername` verdict for the
        // edit-profile dialog.
        if let Some((username, result)) = username_check_answer {
            self.session.username_check = Some((username, result));
        }
        // MED4: stash the `getWebPageInstantView` answer (success →
        // IV reader; error → browser fallback) for the UI drains.
        if let Some((url, rich)) = instant_view_answer {
            self.session.instant_view = Some(InstantViewPage { url, rich });
        }
        if let Some(url) = instant_view_fallback {
            self.session.instant_view_fallback_url = Some(url);
        }
        // MED4b: stash the `getLinkPreview` answer for the composer
        // chip; a 404 becomes "no link info" (`Some(None)`); answers for
        // superseded URLs are dropped.
        if let Some(answer) = link_preview_answer {
            let current = self.session.composer_preview.as_ref();
            if current.is_none_or(|p| p.url == answer.url) {
                self.session.composer_preview = Some(answer);
            }
        }
        if let Some(url) = link_preview_failed
            && self
                .session
                .composer_preview
                .as_ref()
                .is_some_and(|p| p.url == url && p.preview.is_none())
        {
            self.session.composer_preview = Some(ComposerLinkPreview {
                url,
                preview: Some(None),
            });
        }
        // M1 fix-up: "Share link" gate — chain to `getMessageLink` only
        // when `messageProperties.can_get_link` passed; otherwise tell
        // the user instead of silently doing nothing.
        if let Some((chat_id, message_id, can_get_link)) = link_gate {
            if can_get_link {
                let _ = self.send_message_link_request(chat_id, message_id);
            } else {
                self.session.message_link_error =
                    Some("message link not available for this message".into());
            }
        }
        if thumbs_after || self.session.stickers.open || self.session.gifs.open {
            self.maybe_download_open_thumbs()?;
            self.maybe_download_open_chat_media()?;
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
                .retain(|(slot_call_id, _, _), _| *slot_call_id != call_id);
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
                // Phase C2i: a screen-share toggle made before the
                // transport existed applies once it does (mirrors the
                // pre-transport mute above; non-fatal so the UI toggle
                // can retry).
                let pre_sharing = self
                    .session
                    .active_call
                    .as_ref()
                    .is_some_and(|call| call.screen_sharing);
                if result.is_ok()
                    && pre_sharing
                    && let Some(engine) = self.call_engine.as_deref_mut()
                {
                    let _ = engine.set_screen_share_enabled(call_id, true);
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

        // Phase C2j: the peer's 1:1 screen-share state follows the same
        // gate as the camera state. When the share goes inactive the
        // retained screen frames are dropped so a stale picture can
        // never render; while active the frames themselves carry the
        // picture, so no persistent state is kept.
        loop {
            let update = self
                .screen_state_outbox
                .lock()
                .expect("call screen state outbox")
                .pop_front();
            let Some((call_id, state)) = update else {
                break;
            };
            if state != RemoteVideoState::Inactive {
                continue;
            }
            self.video_frame_slots
                .lock()
                .expect("call video frame slots")
                .retain(|(slot_call_id, _, slot_is_screen), _| {
                    *slot_call_id != call_id || !slot_is_screen
                });
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
            // Slice calls-group-self-tile: the self tile lives in the
            // shared (call id, is_local, is_screen) slots — clear it too so
            // a stale local preview can't render after the call ends.
            self.video_frame_slots
                .lock()
                .expect("call video frame slots")
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
            self.maybe_download_open_chat_media()?;
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
    /// Slice G2: force a `getForumTopics` refresh (the manage dialog
    /// calls this after a mutation so the list shows the new state;
    /// the state layer already drops the cache on confirmed
    /// create/delete).
    pub fn refresh_forum_topics(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.forum_topics.remove(&chat_id.0);
        self.maybe_fetch_forum_topics(chat_id)
    }

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
            None,
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
    /// Phase C2i: `sendCallRating` with problems + comment (schema
    /// 1.8.67 :14234). `problems` are `CallProblem` constructor names
    /// (`callProblemEcho`, …).
    pub fn send_call_rating(
        &mut self,
        rating: i32,
        comment: &str,
        problems: &[&str],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let call_id = match &self.session.call_summary {
            Some(summary) if summary.need_rating && !summary.rating_sent => summary.call_id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(RequestPurpose::SendCallRating, None);
        if let Err(err) = self.sender.send_json(&send_call_rating_detail(
            extra, call_id, rating, comment, problems,
        )) {
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
        Ok(self.call_log_payload(summary).to_string())
    }

    /// Phase C2i: the local call-log payload shared by
    /// `sendCallDebugInformation` (inline text) and `sendCallLog` (the
    /// same text as a file). The honest local record: app/engine
    /// identity, call outcome, transport states — never invented media
    /// stats.
    fn call_log_payload(&self, summary: &crate::state::CallSummary) -> serde_json::Value {
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
        payload
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
                    // MED4: caption-length errors can't arise from a
                    // diagnostics upload; categorized as invalid request.
                    ConnectSendError::CaptionTooLong { .. } => {
                        "Could not upload diagnostics: invalid request".into()
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

    /// Phase C2i: `sendCallLog` (schema 1.8.67 :14240) — uploads the
    /// ended call's log file. The file is the local diagnostics
    /// payload written under the account's exports dir (schema allows
    /// only `inputFileLocal` / `inputFileGenerated`).
    pub fn send_call_log(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let summary = self
            .session
            .call_summary
            .as_ref()
            .filter(|s| s.need_log && !s.log_sent)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let log_text = self.call_log_payload(summary).to_string();
        let call_id = summary.call_id;
        let path = self.paths.exports.join(format!("call-{call_id}.log"));
        std::fs::create_dir_all(&self.paths.exports).map_err(|_| ConnectSendError::Native)?;
        std::fs::write(&path, log_text).map_err(|_| ConnectSendError::Native)?;
        let extra = self.session.request(RequestPurpose::SendCallLog, None);
        let path_str = path.to_string_lossy().into_owned();
        if let Err(err) = self
            .sender
            .send_json(&send_call_log(extra, call_id, &path_str))
        {
            self.session.requests.take(extra);
            if let Some(summary) = self.session.call_summary.as_mut() {
                summary.log_error = Some("Could not upload the call log".into());
            }
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2i: `searchCallMessages` (schema 1.8.67 :11903) — first
    /// page of the server-side recent-calls list. Called when the
    /// Recent-calls tab opens.
    pub fn fetch_call_history(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.recent_calls.clear();
        self.session.recent_calls_offset.clear();
        self.session.recent_calls_error = false;
        self.fetch_call_history_page()
    }

    /// Phase C2i: next `searchCallMessages` page, continuing from the
    /// stored `next_offset`.
    pub fn fetch_more_call_history(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.fetch_call_history_page()
    }

    fn fetch_call_history_page(&mut self) -> Result<RequestId, ConnectSendError> {
        if self.session.recent_calls_loading {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SearchCallMessages, None);
        let offset = self.session.recent_calls_offset.clone();
        if let Err(err) = self
            .sender
            .send_json(&search_call_messages(extra, &offset, 40))
        {
            self.session.requests.take(extra);
            self.session.recent_calls_error = true;
            return Err(err);
        }
        self.session.recent_calls_loading = true;
        Ok(extra)
    }

    /// Phase C2i: fetch both call privacy settings
    /// (`userPrivacySettingAllowCalls` /
    /// `userPrivacySettingAllowPeerToPeerCalls`, schema 1.8.67
    /// :15620).
    pub fn fetch_call_privacy(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.call_privacy_loading = true;
        self.session.call_privacy_error = false;
        for setting in [
            CallPrivacySetting::AllowCalls,
            CallPrivacySetting::PeerToPeer,
        ] {
            let extra = self
                .session
                .request(RequestPurpose::GetCallPrivacyRules { setting }, None);
            if let Err(err) = self
                .sender
                .send_json(&get_user_privacy_setting_rules(extra, setting))
            {
                self.session.requests.take(extra);
                self.session.call_privacy_loading = false;
                self.session.call_privacy_error = true;
                return Err(err);
            }
            self.session.call_privacy_pending += 1;
        }
        Ok(())
    }

    /// Phase C2i: change a call privacy setting (schema 1.8.67
    /// :15617). Applied optimistically; the `ok` / error response
    /// confirms or clears it.
    pub fn set_call_privacy(
        &mut self,
        setting: CallPrivacySetting,
        who: PrivacyWho,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetCallPrivacyRules { setting }, None);
        if let Err(err) = self
            .sender
            .send_json(&set_user_privacy_setting_rules(extra, setting, who))
        {
            self.session.requests.take(extra);
            self.session.call_privacy_loading = false;
            self.session.call_privacy_error = true;
            return Err(err);
        }
        match setting {
            CallPrivacySetting::AllowCalls => self.session.call_privacy_allow_calls = Some(who),
            CallPrivacySetting::PeerToPeer => self.session.call_privacy_p2p = Some(who),
        }
        self.session.call_privacy_loading = true;
        self.session.call_privacy_pending += 1;
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
        // Slice calls-group-self-tile: the self tile lives in the
        // shared (call id, is_local) slots — clear it too.
        self.video_frame_slots
            .lock()
            .expect("call video frame slots")
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
    /// :14398). Gated on `groupCall.can_be_managed` (video chats).
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

    /// Phase C2h: `startGroupCallRecording` (schema 1.8.67, :14405).
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

    /// Phase C2h: `endGroupCallRecording` (schema 1.8.67, :14408).
    /// Gated on `groupCall.can_be_managed` and `is_video_chat`
    /// (schema: "for video chats only"), matching the start gate.
    pub fn stop_group_call_recording(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.can_be_managed && call.is_video_chat => call.id,
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

    /// Phase C2h: `startScheduledVideoChat` (schema 1.8.67, :14277).
    /// Starts the tracked scheduled (not-yet-active) video chat early.
    /// Gated on `groupCall.can_be_managed && scheduled_start_date > 0`
    /// — the schema names no explicit right for this constructor, so
    /// `can_be_managed` (the tracked proxy for the
    /// `can_manage_video_chats` admin right) matches the other
    /// video-chat admin actions.
    pub fn start_scheduled_video_chat(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.can_be_managed && call.scheduled_start_date > 0 => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::StartScheduledVideoChat { group_call_id },
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&start_scheduled_video_chat(extra, group_call_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// `toggleVideoChatEnabledStartNotification` (schema 1.8.67,
    /// :14282): "notify me when this scheduled video chat starts".
    /// Gated on the tracked call still being scheduled — the schema
    /// marks the constructor for video chats (any viewer can set it;
    /// no admin right needed). The new flag arrives back as
    /// `updateGroupCall` (`enabled_start_notification`, :7154), which
    /// the reducer already stores on the tracked call.
    pub fn toggle_video_chat_start_notification(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, enabled) = match &self.session.active_group_call {
            Some(call) if call.scheduled_start_date > 0 => {
                (call.id, !call.enabled_start_notification)
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::ToggleVideoChatEnabledStartNotification {
                group_call_id,
                enabled,
            },
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&toggle_video_chat_enabled_start_notification(
                extra,
                group_call_id,
                enabled,
            ))
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

    /// Phase C2h: `sendGroupCallMessage` (schema 1.8.67, :14341).
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
    /// :14322). Gated on `can_toggle_are_messages_allowed`; flips the
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
    /// Phase C2i: the declined peer is recorded in
    /// `Session::call_busy_declined` so the UI can say so honestly
    /// instead of declining silently (TDLib has no hold/swap API —
    /// hold-and-answer is not possible).
    fn maybe_decline_busy_calls(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let queued: Vec<(i32, i64, bool)> =
            std::mem::take(&mut self.session.call_busy_decline_queue);
        for (call_id, user_id, is_video) in queued {
            let extra = self.session.request(RequestPurpose::DiscardCall, None);
            if let Err(err) = self
                .sender
                .send_json(&discard_call_request(extra, call_id, false, 0, is_video))
            {
                self.session.requests.take(extra);
                return Err(err);
            }
            // ponytail: cap the banner list — it is drained by the UI.
            if self.session.call_busy_declined.len() < 4 {
                self.session.call_busy_declined.push((user_id, is_video));
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
        reply_to: Option<SendReply>,
        now_ms: u64,
        delayed: bool,
    ) -> Result<DraftSaveOutcome, ConnectSendError> {
        if !self.chats_path_active() || !self.session.accepts_composer_draft(chat_id) {
            return Ok(DraftSaveOutcome::Skipped);
        }
        let stored = draft_text_to_store(text, reply_to.is_some()).map(str::to_string);
        let reply_to = stored.as_ref().and(reply_to);
        if self.draft_matches(chat_id, stored.as_deref(), reply_to.as_ref()) {
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
                self.send_draft(chat_id, stored.as_deref(), reply_to.as_ref())?;
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
        if self.draft_matches(pending.chat_id, text, reply_to.as_ref()) {
            self.session.store_composer_draft(
                pending.chat_id,
                self.session
                    .chats
                    .get(&pending.chat_id.0)
                    .and_then(|chat| chat.draft.clone()),
            );
            return Ok(None);
        }
        self.send_draft(pending.chat_id, text, reply_to.as_ref())
            .map(Some)
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
        reply_to: Option<&SendReply>,
    ) -> bool {
        let current = self
            .session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| chat.draft.as_ref());
        match (current, text) {
            (None, None) => true,
            (Some(draft), Some(text)) => {
                draft.text == text
                    && draft.reply_to_message_id == reply_to.map(|reply| reply.message_id)
                    && draft.quote == reply_to.and_then(|reply| reply.quote.clone())
            }
            _ => false,
        }
    }

    fn send_draft(
        &mut self,
        chat_id: ChatId,
        text: Option<&str>,
        reply_to: Option<&SendReply>,
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
                        reply_to_message_id: reply_to.map(|reply| reply.message_id),
                        quote: reply_to.and_then(|reply| reply.quote.clone()),
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
        match self.sender.send_json(&view_messages(
            extra,
            chat_id,
            &ids,
            "messageSourceChatHistory",
            true,
        )) {
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

    /// Slice G2: store the event-log search query for the chat (sent
    /// by the next `getChatEventLog`; empty clears it). Editing the
    /// query does not refetch by itself — the UI follows with
    /// `refresh_chat_event_log`.
    pub fn set_chat_event_log_query(&mut self, chat_id: ChatId, query: &str) {
        let query = query.trim().to_string();
        if query.is_empty() {
            self.session.event_log_queries.remove(&chat_id.0);
        } else {
            self.session.event_log_queries.insert(chat_id.0, query);
        }
    }

    /// Slice G2: flip one event-log filter category for the chat
    /// (`toggle` flips one field of the set; clearing the last active
    /// category removes the set so the log shows all types again).
    pub fn toggle_chat_event_log_filter(
        &mut self,
        chat_id: ChatId,
        toggle: impl FnOnce(&mut ChatEventLogFilterSet),
    ) {
        let mut set = self
            .session
            .event_log_filters
            .get(&chat_id.0)
            .copied()
            .unwrap_or_default();
        toggle(&mut set);
        if set.any_enabled() {
            self.session.event_log_filters.insert(chat_id.0, set);
        } else {
            self.session.event_log_filters.remove(&chat_id.0);
        }
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
    /// its cursor. Slice G2: sends the cached filter set / search query
    /// for the chat (both edited from the event-log dialog; changing them
    /// goes through `refresh_chat_event_log`, which clears the cache).
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
        let filters = self
            .session
            .event_log_filters
            .get(&chat_id.0)
            .copied()
            .filter(|filters| filters.any_enabled());
        let query = self
            .session
            .event_log_queries
            .get(&chat_id.0)
            .cloned()
            .unwrap_or_default();
        let extra = self.session.request(
            RequestPurpose::GetChatEventLog { from_event_id },
            Some(chat_id),
        );
        if let Err(err) = self.sender.send_json(&get_chat_event_log(
            extra,
            chat_id.0,
            &query,
            from_event_id,
            CHAT_EVENT_LOG_PAGE_SIZE,
            filters,
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

    /// Phase D3b / slice G1: `getSupergroupMembers` (TDLib 1.8.67,
    /// `schema/td_api.tl:15238`) for one member-list page. The page is
    /// cached per (chat, filter); `query` narrows the search-style
    /// filters (empty = no narrowing). The restricted/banned filters
    /// require the `can_restrict_members` administrator right (schema
    /// lines 2570/2574); the other filters are available to every
    /// member. Only supergroup chats (incl. channels) have members.
    /// Deduped like the other D3b fetches.
    pub fn fetch_supergroup_members(
        &mut self,
        chat_id: ChatId,
        filter: MemberListFilter,
        query: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if filter.requires_restrict_right() && !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup { supergroup_id, .. } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        let key = (chat_id.0, filter);
        let purpose = RequestPurpose::GetSupergroupMembers { filter };
        if matches!(
            self.session.supergroup_members.get(&key),
            Some(SupergroupMembersFetch::Loading | SupergroupMembersFetch::Loaded { .. })
        ) || self.session.requests.has_purpose_for_chat(purpose, chat_id)
        {
            return Ok(None);
        }
        self.session
            .supergroup_members
            .insert(key, SupergroupMembersFetch::Loading);
        let extra = self.session.request(purpose, Some(chat_id));
        let filter_json = match filter {
            MemberListFilter::Recent => supergroup_members_filter_recent_json(),
            MemberListFilter::Search => supergroup_members_filter_search_json(query),
            MemberListFilter::Administrators => supergroup_members_filter_administrators_json(),
            MemberListFilter::Restricted => supergroup_members_filter_restricted_json(query),
            MemberListFilter::Banned => supergroup_members_filter_banned_json(query),
        };
        if let Err(err) = self.sender.send_json(&get_supergroup_members(
            extra,
            supergroup_id,
            &filter_json,
            0,
            200,
        )) {
            self.session.requests.take(extra);
            self.session.supergroup_members.remove(&key);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: explicit refresh of one member-list page — clears the
    /// cached (chat, filter) page and re-sends.
    pub fn refresh_supergroup_members(
        &mut self,
        chat_id: ChatId,
        filter: MemberListFilter,
        query: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.supergroup_members.remove(&(chat_id.0, filter));
        self.fetch_supergroup_members(chat_id, filter, query)
    }

    /// Slice G1: `getBasicGroupFullInfo` (schema 1.8.67, line 11507) —
    /// the member list for a basic group. Any member may view it.
    /// Deduped like the other fetches.
    pub fn fetch_basic_group_members(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let basic_group_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::BasicGroup { basic_group_id } => basic_group_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        let purpose = RequestPurpose::GetBasicGroupFullInfo;
        if matches!(
            self.session.basic_group_members.get(&chat_id.0),
            Some(SupergroupMembersFetch::Loading | SupergroupMembersFetch::Loaded { .. })
        ) || self.session.requests.has_purpose_for_chat(purpose, chat_id)
        {
            return Ok(None);
        }
        self.session
            .basic_group_members
            .insert(chat_id.0, SupergroupMembersFetch::Loading);
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_basic_group_full_info(extra, basic_group_id))
        {
            self.session.requests.take(extra);
            self.session.basic_group_members.remove(&chat_id.0);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: `createNewBasicGroupChat` (schema 1.8.67, line 13327).
    /// The response is `createdBasicGroupChat`; the new chat itself
    /// arrives as `updateNewChat`.
    pub fn create_basic_group(
        &mut self,
        title: &str,
        user_ids: &[i64],
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::CreateBasicGroup;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self
            .sender
            .send_json(&create_new_basic_group_chat(extra, user_ids, title))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: `createNewSupergroupChat` (schema 1.8.67, line 13337).
    /// `is_channel` selects channel vs. supergroup; the new chat arrives
    /// as `updateNewChat`.
    pub fn create_supergroup_channel(
        &mut self,
        title: &str,
        is_channel: bool,
        description: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::CreateSupergroupChannel { is_channel };
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&create_new_supergroup_chat(
            extra,
            title,
            is_channel,
            description,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: `toggleSupergroupIsBroadcastGroup` (schema 1.8.67, line
    /// 15221). One-way upgrade of a supergroup to a broadcast group —
    /// the constructor takes no boolean and the schema offers no reverse.
    /// Requires owner privileges (line 15220). Applied optimistically;
    /// `updateSupergroup` confirms.
    pub fn upgrade_to_broadcast_group(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel: false,
                } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        if !self.session.chat_is_owner(chat_id)
            || self
                .session
                .supergroup_is_broadcast
                .get(&supergroup_id)
                .is_some_and(|b| *b)
        {
            return Ok(None);
        }
        let purpose = RequestPurpose::ToggleBroadcastGroup;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&toggle_supergroup_is_broadcast_group(extra, supergroup_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Optimistic: the toggle is one-way, so a sent request means the
        // group becomes a broadcast group barring a TDLib error.
        self.session
            .supergroup_is_broadcast
            .insert(supergroup_id, true);
        Ok(Some(extra))
    }

    /// Slice G1: `addChatMembers` / `addChatMember` (schema 1.8.67, lines
    /// 13584/13578). Basic groups use the singular variant (the bulk one
    /// is supergroups and channels only, line 13580). Both require the
    /// `can_invite_users` member right. The response is
    /// `failedToAddMembers`; added members arrive as `updateChatMember`.
    pub fn add_chat_members(
        &mut self,
        chat_id: ChatId,
        user_ids: &[i64],
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if user_ids.is_empty() || !self.session.chat_can_add_members(chat_id) {
            return Ok(None);
        }
        let is_basic_group = matches!(
            self.session.chats.get(&chat_id.0),
            Some(chat) if matches!(chat.kind, ChatKind::BasicGroup { .. })
        );
        // Slice G1 fix-up: basic groups send one `addChatMember` per
        // user, each answering `failedToAddMembers` — a distinct purpose
        // so per-user responses accumulate instead of replacing the
        // single bulk count.
        let purpose = if is_basic_group {
            RequestPurpose::AddChatMember
        } else {
            RequestPurpose::AddChatMembers
        };
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::AddChatMembers, chat_id)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::AddChatMember, chat_id)
        {
            return Ok(None);
        }
        // Slice G1: a new add attempt resets the failure count — errors
        // from a previous attempt must not linger into this one.
        self.session.add_members_failed.remove(&chat_id.0);
        // Basic groups need one `addChatMember` per user; supergroups and
        // channels take a single bulk `addChatMembers`.
        let mut first_extra = None;
        if is_basic_group {
            for user_id in user_ids {
                let req_extra = self.session.request(purpose, Some(chat_id));
                if let Err(err) = self
                    .sender
                    .send_json(&add_chat_member(req_extra, chat_id.0, *user_id))
                {
                    self.session.requests.take(req_extra);
                    if first_extra.is_none() {
                        return Err(err);
                    }
                    break;
                }
                if first_extra.is_none() {
                    first_extra = Some(req_extra);
                }
            }
        } else {
            let extra = self.session.request(purpose, Some(chat_id));
            if let Err(err) = self
                .sender
                .send_json(&add_chat_members(extra, chat_id.0, user_ids))
            {
                self.session.requests.take(extra);
                return Err(err);
            }
            first_extra = Some(extra);
        }
        Ok(first_extra)
    }

    /// Slice G1: restrict a member (`chatMemberStatusRestricted`, schema
    /// 1.8.67, line 2510) via `setChatMemberStatus` (line 13592), which
    /// requires the `can_restrict_members` administrator right (lines
    /// 13586-13587). Not supported in basic groups and channels (line
    /// 2510) — non-channel supergroups only. The change itself arrives as
    /// `updateChatMember`.
    pub fn restrict_chat_member(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        until_date: i32,
        permissions: &ChatPermissions,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_group = matches!(
            self.session.chats.get(&chat_id.0),
            Some(chat) if matches!(
                chat.kind,
                ChatKind::Supergroup {
                    is_channel: false,
                    ..
                }
            )
        );
        if !is_group || !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let status = chat_member_status_restricted_json(true, until_date, &permissions.to_json());
        let purpose = RequestPurpose::SetChatMemberStatus {
            user_id,
            kind: MemberStatusChange::Restrict,
        };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let member_id = MessageSenderRef::User(user_id).to_value();
        if let Err(err) = self.sender.send_json(&set_chat_member_status(
            extra, chat_id.0, &member_id, &status,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: ban a member (`chatMemberStatusBanned`, schema 1.8.67,
    /// line 2517) via `setChatMemberStatus`. Works in supergroups and
    /// channels ("Chats can be only banned and unbanned in supergroups
    /// and channels", line 13587); requires `can_restrict_members`.
    pub fn ban_chat_member(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        until_date: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_bannable = matches!(
            self.session.chats.get(&chat_id.0),
            Some(chat) if matches!(chat.kind, ChatKind::Supergroup { .. })
        );
        if !is_bannable || !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let status = chat_member_status_banned_json(until_date);
        let purpose = RequestPurpose::SetChatMemberStatus {
            user_id,
            kind: MemberStatusChange::Ban,
        };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let member_id = MessageSenderRef::User(user_id).to_value();
        if let Err(err) = self.sender.send_json(&set_chat_member_status(
            extra, chat_id.0, &member_id, &status,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: unban / unrestrict a member — `setChatMemberStatus` back
    /// to plain `chatMemberStatusMember` (schema 1.8.67, line 2504).
    pub fn unban_chat_member(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_bannable = matches!(
            self.session.chats.get(&chat_id.0),
            Some(chat) if matches!(chat.kind, ChatKind::Supergroup { .. })
        );
        if !is_bannable || !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let status = chat_member_status_member_json();
        let purpose = RequestPurpose::SetChatMemberStatus {
            user_id,
            kind: MemberStatusChange::Unban,
        };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let member_id = MessageSenderRef::User(user_id).to_value();
        if let Err(err) = self.sender.send_json(&set_chat_member_status(
            extra, chat_id.0, &member_id, &status,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: `setChatPermissions` (schema 1.8.67, line 13464) —
    /// changes the default chat member permissions. Supported only for
    /// basic groups and supergroups; requires the `can_restrict_members`
    /// administrator right (line 13461). Applied optimistically;
    /// `updateChatPermissions` confirms.
    pub fn set_chat_permissions(
        &mut self,
        chat_id: ChatId,
        permissions: &ChatPermissions,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = matches!(
            self.session.chats.get(&chat_id.0),
            Some(chat) if matches!(
                chat.kind,
                ChatKind::BasicGroup { .. }
                    | ChatKind::Supergroup {
                        is_channel: false,
                        ..
                    }
            )
        );
        if !supported || !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetChatPermissions;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&set_chat_permissions(
            extra,
            chat_id.0,
            &permissions.to_json(),
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Optimistic: barring a TDLib error the block applies as sent.
        // The pre-request values ride on the pending entry so the error
        // arm can roll back.
        if let Some(chat) = self.session.chats.get_mut(&chat_id.0) {
            let rollback = RequestRollback::ChatPermissions {
                previous: chat.permissions,
                previous_can_send: chat.can_send_basic_messages,
            };
            chat.permissions = Some(*permissions);
            chat.can_send_basic_messages = permissions.can_send_basic_messages;
            if let Some(pending) = self.session.requests.pending_mut(extra) {
                pending.rollback = Some(rollback);
            }
        }
        Ok(Some(extra))
    }

    /// Slice G1: `replacePrimaryChatInviteLink` (schema 1.8.67, line
    /// 14089) — available for basic groups, supergroups, and channels;
    /// requires administrator privileges and the `can_invite_users`
    /// right. The new link arrives as `chatInviteLink`.
    pub fn replace_primary_chat_invite_link(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_invite_users(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::ReplacePrimaryChatInviteLink;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&replace_primary_chat_invite_link(extra, chat_id.0))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: `toggleSupergroupJoinByRequest` (schema 1.8.67, line
    /// 15188) — whether directly joining the supergroup needs admin
    /// approval. Requires the `can_restrict_members` administrator right
    /// (line 15182); not for broadcast groups or channels. Applied
    /// optimistically; `updateSupergroup` confirms.
    pub fn toggle_supergroup_join_by_request(
        &mut self,
        chat_id: ChatId,
        join_by_request: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel: false,
                } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        if !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::ToggleSupergroupJoinByRequest;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&toggle_supergroup_join_by_request(
            extra,
            supergroup_id,
            join_by_request,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Optimistic: the previous flag rides on the pending entry so the
        // error arm can roll back.
        let previous = self
            .session
            .supergroup_join_by_request
            .get(&supergroup_id)
            .copied();
        self.session
            .supergroup_join_by_request
            .insert(supergroup_id, join_by_request);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::JoinByRequest {
                supergroup_id,
                previous,
            });
        }
        Ok(Some(extra))
    }

    /// Slice G1: `setSupergroupUsername` (schema 1.8.67, line 15136) —
    /// changes the editable public username; requires owner privileges
    /// (line 15133). Empty string removes the username. Applied
    /// optimistically; `updateSupergroup` confirms.
    pub fn set_supergroup_username(
        &mut self,
        chat_id: ChatId,
        username: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup { supergroup_id, .. } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        if !self.session.chat_is_owner(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetSupergroupUsername;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) =
            self.sender
                .send_json(&set_supergroup_username(extra, supergroup_id, username))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Optimistic: the previous username rides on the pending entry so
        // the error arm can roll back.
        let previous = self
            .session
            .supergroup_usernames
            .get(&supergroup_id)
            .cloned();
        self.session
            .set_supergroup_username(supergroup_id, username.to_string());
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::SupergroupUsername {
                supergroup_id,
                previous,
            });
        }
        Ok(Some(extra))
    }

    /// Slice G2: `toggleSupergroupSignMessages` (schema 1.8.67, line
    /// 15175). Channels only; gated on `can_change_info` (creator or an
    /// admin with the right, like Telegram X's `ProfileController`).
    /// Optimistic — the error arm rolls back via `RequestRollback`.
    pub fn toggle_sign_messages(
        &mut self,
        chat_id: ChatId,
        sign_messages: bool,
        show_message_sender: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel: true,
                } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        if !self.session.chat_can_change_info(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::ToggleSupergroupSignMessages;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&toggle_supergroup_sign_messages(
            extra,
            supergroup_id,
            sign_messages,
            show_message_sender,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Optimistic: the previous flags ride on the pending entry so
        // the error arm can roll back.
        let previous_sign = self
            .session
            .supergroup_sign_messages
            .get(&supergroup_id)
            .copied();
        let previous_show = self
            .session
            .supergroup_show_message_sender
            .get(&supergroup_id)
            .copied();
        self.session
            .supergroup_sign_messages
            .insert(supergroup_id, sign_messages);
        self.session
            .supergroup_show_message_sender
            .insert(supergroup_id, sign_messages && show_message_sender);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::SignMessages {
                supergroup_id,
                previous_sign,
                previous_show,
            });
        }
        Ok(Some(extra))
    }

    /// Slice G2: `toggleSupergroupHasAggressiveAntiSpamEnabled` (schema
    /// 1.8.67, line 15212). Non-channel supergroups only; the schema
    /// requires `supergroupFullInfo.can_toggle_aggressive_anti_spam`.
    /// Optimistic — the error arm rolls back via `RequestRollback`.
    pub fn toggle_aggressive_anti_spam(
        &mut self,
        chat_id: ChatId,
        enabled: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel: false,
                } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        if !self.session.chat_can_toggle_anti_spam(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::ToggleSupergroupAggressiveAntiSpam;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&toggle_supergroup_aggressive_anti_spam(
                extra,
                supergroup_id,
                enabled,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Optimistic: the previous flag rides on the pending entry so
        // the error arm can roll back.
        let previous = self
            .session
            .supergroup_anti_spam_enabled
            .get(&supergroup_id)
            .copied();
        self.session
            .supergroup_anti_spam_enabled
            .insert(supergroup_id, enabled);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::AntiSpam {
                supergroup_id,
                previous,
            });
        }
        Ok(Some(extra))
    }

    /// Slice G2: supergroup id for a non-channel supergroup chat —
    /// `None` for everything else (channels, basic groups, unknowns).
    fn forum_supergroup(&self, chat_id: ChatId) -> Option<i64> {
        self.session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel: false,
                } => Some(supergroup_id),
                _ => None,
            })
    }

    /// Slice G2: gate shared by every forum-topic mutation — requires
    /// the viewer to hold `can_manage_topics` in a non-channel
    /// supergroup.
    fn forum_topic_gate(&self, chat_id: ChatId) -> bool {
        self.forum_supergroup(chat_id).is_some() && self.session.chat_can_manage_topics(chat_id)
    }

    /// Slice G2: `createForumTopic` (schema 1.8.67, line 12665).
    /// Answers `forumTopicInfo`; the cached topic list is refetched on
    /// success. Returns `Err` for an empty name.
    pub fn create_forum_topic(
        &mut self,
        chat_id: ChatId,
        name: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if name.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.forum_topic_gate(chat_id) {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::CreateForumTopic)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::CreateForumTopic, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&create_forum_topic(extra, chat_id, name.trim()))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `editForumTopic` (schema 1.8.67, line 12674) — renames
    /// the topic. Answers `ok`; the topic list is refetched on success.
    pub fn edit_forum_topic(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
        name: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if name.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.forum_topic_gate(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::EditForumTopic { forum_topic_id };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&edit_forum_topic(
            extra,
            chat_id,
            forum_topic_id,
            name.trim(),
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `toggleForumTopicIsClosed` (schema 1.8.67, line 12713).
    /// Answers `ok`; the topic list is refetched on success.
    pub fn toggle_forum_topic_closed(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
        closed: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.toggle_forum_topic_flag(
            chat_id,
            forum_topic_id,
            RequestPurpose::ToggleForumTopicClosed { forum_topic_id },
            closed,
        )
    }

    /// Slice G2: `toggleForumTopicIsPinned` (schema 1.8.67, line 12725).
    /// Answers `ok`; the topic list is refetched on success.
    pub fn toggle_forum_topic_pinned(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
        pinned: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.toggle_forum_topic_flag(
            chat_id,
            forum_topic_id,
            RequestPurpose::ToggleForumTopicPinned { forum_topic_id },
            pinned,
        )
    }

    /// Slice G2: shared sender for the two boolean forum-topic toggles.
    fn toggle_forum_topic_flag(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
        purpose: RequestPurpose,
        flag: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.forum_topic_gate(chat_id) {
            return Ok(None);
        }
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let sent = if matches!(purpose, RequestPurpose::ToggleForumTopicClosed { .. }) {
            self.sender.send_json(&toggle_forum_topic_closed(
                extra,
                chat_id,
                forum_topic_id,
                flag,
            ))
        } else {
            self.sender.send_json(&toggle_forum_topic_pinned(
                extra,
                chat_id,
                forum_topic_id,
                flag,
            ))
        };
        if let Err(err) = sent {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `deleteForumTopic` (schema 1.8.67, line 12736).
    /// Answers `ok`; the topic list is refetched on success.
    pub fn delete_forum_topic(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.forum_topic_gate(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::DeleteForumTopic { forum_topic_id };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&delete_forum_topic(extra, chat_id, forum_topic_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `toggleGeneralForumTopicIsHidden` (schema 1.8.67, line
    /// 12718). Answers `ok`; the topic list is refetched on success.
    pub fn toggle_general_forum_topic_hidden(
        &mut self,
        chat_id: ChatId,
        hidden: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.forum_topic_gate(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::ToggleGeneralForumTopicHidden;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&toggle_general_forum_topic_hidden(extra, chat_id, hidden))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `getMessageThreadHistory` (schema 1.8.67, line 11839)
    /// for the channel-comments viewer — the first page of the comment
    /// thread under a channel post. Deduped per channel post while one
    /// is in flight.
    pub fn fetch_message_thread_history(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::GetMessageThreadHistory {
            message_id: message_id.0,
        };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_message_thread_history(
            extra,
            chat_id,
            message_id,
            MessageId(0),
            50,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice CL: `getChatHistory` (schema 1.8.67, line 11829) for the
    /// chat-list peek preview — the most recent messages of a chat the
    /// user has not opened. Deduped per chat while one is in flight; the
    /// `messages` answer lands in `Session::chat_preview_fetch`. Read-only:
    /// no `openChat`, so nothing is marked read.
    pub fn fetch_chat_preview_history(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::GetChatPreview;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        self.session.chat_preview_fetch = None;
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_chat_history(
            extra,
            chat_id,
            MessageId(0),
            0,
            PREVIEW_HISTORY_LIMIT,
            false,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `getChatBoostStatus` (schema 1.8.67, line 13917) —
    /// cached per chat (level + boost count) for the boost dialog.
    pub fn fetch_chat_boost_status(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup { .. })
        ) {
            return Ok(None);
        }
        if self.session.chat_boost_status.contains_key(&chat_id.0)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::GetChatBoostStatus, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatBoostStatus, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_boost_status(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: boost the chat. Sends `getAvailableChatBoostSlots`
    /// (schema 1.8.67, line 13914); the driver chains `boostChat` with
    /// the first slot once the answer arrives (`maybe_continue_boost`).
    /// Deduped while an intent or either request is in flight.
    pub fn request_chat_boost(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup { .. })
        ) {
            return Ok(None);
        }
        if self.session.boost_intent == Some(chat_id.0)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::GetBoostSlotsForBoost, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetBoostSlotsForBoost, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_available_chat_boost_slots(extra))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.boost_intent = Some(chat_id.0);
        Ok(Some(extra))
    }

    /// Slice G2: `boostChat` chain — once the slots answer for a pending
    /// boost intent arrives, send `boostChat` with the first slot id.
    /// An empty slot list (or a failed slots request) just drops the
    /// intent; `boostChat` errors are reported by the reducer.
    fn maybe_continue_boost(&mut self) -> Result<(), ConnectSendError> {
        let Some(chat_id) = self.session.boost_intent else {
            return Ok(());
        };
        let Some(slots) = self.session.boost_slots_by_chat.remove(&chat_id) else {
            return Ok(());
        };
        self.session.boost_intent = None;
        let Some(slot_id) = slots.into_iter().next() else {
            return Ok(());
        };
        let chat = ChatId(chat_id);
        let extra = self.session.request(RequestPurpose::BoostChat, Some(chat));
        if let Err(err) = self.sender.send_json(&boost_chat(extra, chat, &[slot_id])) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Slice G2: `loadChatWelcomeMessages` (schema 1.8.67, line 12630).
    /// The pack also arrives spontaneously as `updateChatWelcomeMessages`;
    /// deduped on a cached pack or an in-flight fetch.
    pub fn load_chat_welcome_messages(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup { .. })
        ) {
            return Ok(None);
        }
        if self.session.welcome_messages.contains_key(&chat_id.0)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::LoadChatWelcomeMessages, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::LoadChatWelcomeMessages, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&load_chat_welcome_messages(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session
            .welcome_message_fetches
            .insert(chat_id.0, WelcomeMessagesFetch::Loading);
        Ok(Some(extra))
    }

    /// Slice G2: shared gate for welcome-message mutations — requires
    /// `can_send_welcome_messages` (creator or an admin with the right)
    /// in a supergroup or channel.
    fn welcome_mutation_gate(&self, chat_id: ChatId) -> bool {
        matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup { .. })
        ) && self.session.chat_can_send_welcome_messages(chat_id)
    }

    /// Slice G2: `addChatWelcomeMessage` (schema 1.8.67, line 12639).
    /// Answers `ok`; the pack is refetched on success.
    pub fn add_chat_welcome_message(
        &mut self,
        chat_id: ChatId,
        text: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.welcome_mutation_gate(chat_id) {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::AddChatWelcomeMessage)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::AddChatWelcomeMessage, Some(chat_id));
        if let Err(err) =
            self.sender
                .send_json(&add_chat_welcome_message(extra, chat_id, text.trim()))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `editChatWelcomeMessage` (schema 1.8.67, line 12646).
    /// Answers `ok`; the pack is refetched on success.
    pub fn edit_chat_welcome_message(
        &mut self,
        chat_id: ChatId,
        welcome_message_id: i32,
        text: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.welcome_mutation_gate(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::EditChatWelcomeMessage { welcome_message_id };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&edit_chat_welcome_message(
            extra,
            chat_id,
            welcome_message_id,
            text.trim(),
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `deleteChatWelcomeMessage` (schema 1.8.67, line 12651).
    /// Answers `ok`; the pack is refetched on success.
    pub fn delete_chat_welcome_message(
        &mut self,
        chat_id: ChatId,
        welcome_message_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.welcome_mutation_gate(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::DeleteChatWelcomeMessage { welcome_message_id };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&delete_chat_welcome_message(
            extra,
            chat_id,
            welcome_message_id,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: `setChatMemberTag` (schema 1.8.67, line 13598) — the
    /// admin custom-title setter (0-16 characters, no emoji; Telegram
    /// X `EditRightsController` enforces the same limits client-side).
    /// Basic groups and supergroups only, not channels. Gated on the
    /// caller being able to manage tags: owner/creator, or an admin
    /// with `can_manage_tags` (changing your own tag is allowed for
    /// any admin).
    pub fn set_chat_member_tag(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        tag: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if tag.chars().count() > 16 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let me = self.session.my_user_id;
        let allowed = self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            !matches!(
                chat.kind,
                ChatKind::Supergroup {
                    is_channel: true,
                    ..
                }
            )
        }) && (Some(user_id) == me
            || self.session.chat_is_owner(chat_id)
            || self.session.chat_can_manage_tags(chat_id));
        if !allowed {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetChatMemberTag { user_id };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&set_chat_member_tag(extra, chat_id, user_id, tag))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: `deleteChat` (schema 1.8.67, line 11850) — deletes the
    /// chat along with all messages for all members; releases group
    /// usernames. Gated by `chat.can_be_deleted_for_all_users` (line
    /// 11848). The chat is dropped locally on `ok`.
    pub fn delete_chat(&mut self, chat_id: ChatId) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let deletable = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_be_deleted_for_all_users);
        if !deletable {
            return Ok(None);
        }
        let purpose = RequestPurpose::DeleteChat;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&delete_chat(extra, chat_id.0)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice CL1: `toggleChatIsPinned` (schema 1.8.67, line 13678).
    /// The pin is optimistic (the target list comes from the chat's
    /// current membership: archive list when `in_archive`, else main);
    /// the authoritative state arrives via `updateChatPosition` and the
    /// pre-request value rides on the pending entry for rollback. The
    /// pin-limit pre-check lives in the UI (TGX behavior).
    pub fn toggle_chat_pin(
        &mut self,
        chat_id: ChatId,
        pin: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::ToggleChatIsPinned;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let (archived, supported) = self
            .session
            .chats
            .get(&chat_id.0)
            .map(|chat| (chat.in_archive, chat.supported()))
            .unwrap_or((false, false));
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&toggle_chat_is_pinned(extra, chat_id.0, archived, pin))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(chat) = self.session.chats.get_mut(&chat_id.0) {
            let rollback = RequestRollback::ChatPin {
                previous: if archived {
                    chat.archive_is_pinned
                } else {
                    chat.is_pinned
                },
                archived,
            };
            if archived {
                chat.archive_is_pinned = pin;
            } else {
                chat.is_pinned = pin;
            }
            if let Some(pending) = self.session.requests.pending_mut(extra) {
                pending.rollback = Some(rollback);
            }
        }
        Ok(Some(extra))
    }

    /// Slice CL1: `toggleChatIsMarkedAsUnread` (schema 1.8.67, line
    /// 13519). Optimistic with rollback; the authoritative flag arrives
    /// via `updateChatIsMarkedAsUnread`.
    pub fn toggle_chat_marked_as_unread(
        &mut self,
        chat_id: ChatId,
        marked: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::ToggleChatIsMarkedAsUnread;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&toggle_chat_is_marked_as_unread(extra, chat_id.0, marked))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(chat) = self.session.chats.get_mut(&chat_id.0) {
            let rollback = RequestRollback::ChatMarkedAsUnread {
                previous: chat.is_marked_as_unread,
            };
            chat.is_marked_as_unread = marked;
            if let Some(pending) = self.session.requests.pending_mut(extra) {
                pending.rollback = Some(rollback);
            }
        }
        Ok(Some(extra))
    }

    /// Slice CL1: mark a chat as read the way Telegram X does
    /// (`Tdlib.markChatAsRead` with `MessageSourceChatList`): `viewMessages`
    /// over the newest known message reads real unread history, and the
    /// manual marked-unread flag is cleared when set. Honest noop when the
    /// chat has nothing unread.
    pub fn mark_chat_as_read(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (supported, unread, marked) = self
            .session
            .chats
            .get(&chat_id.0)
            .map(|chat| {
                (
                    chat.supported(),
                    chat.unread_count > 0,
                    chat.is_marked_as_unread,
                )
            })
            .unwrap_or((false, false, false));
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !unread && !marked {
            return Ok(None);
        }
        let mut sent: Option<RequestId> = None;
        // TGX: viewing the newest known message reads the real unread
        // history. Skipped when the chat was never opened (same as TGX's
        // `chat.lastMessage == null` guard); the authoritative
        // `updateChatReadInbox` then confirms.
        if unread {
            let newest = self
                .session
                .histories
                .get(&chat_id.0)
                .and_then(|history| history.messages.keys().next_back().copied())
                .map(MessageId);
            if let Some(id) = newest
                && !self
                    .session
                    .requests
                    .has_purpose_for_chat(RequestPurpose::ViewMessages, chat_id)
            {
                let extra = self
                    .session
                    .request(RequestPurpose::ViewMessages, Some(chat_id));
                match self.sender.send_json(&view_messages(
                    extra,
                    chat_id,
                    &[id],
                    "messageSourceChatList",
                    true,
                )) {
                    Ok(()) => {
                        self.session.begin_viewing(chat_id, &[id]);
                        sent = Some(extra);
                    }
                    Err(err) => {
                        self.session.requests.take(extra);
                        return Err(err);
                    }
                }
            }
        }
        if marked {
            sent = self.toggle_chat_marked_as_unread(chat_id, false)?.or(sent);
        }
        Ok(sent)
    }

    /// Slice CL2: `setPinnedChats` (schema 1.8.67, line 13681) — drag
    /// reorder of the pinned chats. `new_ids` is the full new pinned
    /// order (highest first), built by the UI from the current model
    /// order (TGX `ChatsAdapter.movePinnedChat` sends the reordered
    /// array the same way). The model applies it optimistically via
    /// `Session::reorder_pinned_chats`; a refusal restores the
    /// pre-reorder order values. `Ok(None)` = no-op: id-set mismatch
    /// or a reorder already in flight.
    pub fn set_pinned_chat_order(
        &mut self,
        archived: bool,
        new_ids: Vec<i64>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::SetPinnedChats;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        if self.session.pinned_chat_ids(archived) == new_ids {
            return Ok(None);
        }
        let previous = self.session.reorder_pinned_chats(archived, &new_ids);
        if previous.is_empty() {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self
            .sender
            .send_json(&set_pinned_chats(extra, archived, &new_ids))
        {
            self.session.requests.take(extra);
            // Transport failure: undo the optimistic swap with the
            // inverse permutation (same value multiset, original ids).
            let restored: Vec<i64> = previous.iter().map(|(id, _)| *id).collect();
            self.session.reorder_pinned_chats(archived, &restored);
            return Err(err);
        }
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::ChatPinOrder { previous, archived });
        }
        Ok(Some(extra))
    }

    /// Slice CL2: `readChatList` (schema 1.8.67, line 13684) — mark all
    /// chats in the list as read. The badges clear via
    /// `updateChatReadInbox` / `updateChatUnreadMentionCount`; nothing
    /// is faked locally. `Ok(None)` = nothing unread or a read already
    /// in flight.
    pub fn mark_all_chats_as_read(
        &mut self,
        archived: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::ReadChatList;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let any_unread = self.session.chats.values().any(|c| {
            let in_list = if archived {
                c.in_archive
            } else {
                c.in_main_list
            };
            in_list && c.is_unread()
        });
        if !any_unread {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&read_chat_list(extra, archived)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice CL2: `clearRecentlyFoundChats` (schema 1.8.67, line 11671).
    /// The recents are cleared optimistically — the schema defines no
    /// update for this, so the client can't wait for confirmation (TGX
    /// `SearchManager.clearRecentlyFoundChats` clears locally too); a
    /// refusal surfaces via `chat_action_error` and the next recents
    /// fetch restores the truth. `Ok(None)` = recents already empty or
    /// a clear already in flight.
    pub fn clear_recently_found_chats(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::ClearRecentlyFoundChats;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        if !(self.session.search.recents && !self.session.search.chat_ids.is_empty()) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&clear_recently_found_chats(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.search.chat_ids.clear();
        self.session.search.status = SearchStatus::Idle;
        Ok(Some(extra))
    }

    /// Slice CL2: `getArchiveChatListSettings` (schema 1.8.67, line
    /// 13421) — one-shot fetch feeding
    /// `Session::archive_chat_list_settings` (TGX
    /// `SettingsArchiveChatListController` fetches on open the same
    /// way). `Ok(None)` = already fetched or a fetch in flight.
    pub fn fetch_archive_chat_list_settings(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::GetArchiveChatListSettings;
        if self.session.archive_chat_list_settings.is_some()
            || self.session.requests.has_purpose(purpose)
        {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self
            .sender
            .send_json(&get_archive_chat_list_settings(extra))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.archive_settings_loading = true;
        Ok(Some(extra))
    }

    /// Slice CL2: `setArchiveChatListSettings` (schema 1.8.67, line
    /// 13424). Optimistic flip of the cached settings; a refusal
    /// restores the previous values. The optimistic value stands
    /// until the next fetch.
    pub fn set_archive_chat_list_settings(
        &mut self,
        settings: ArchiveChatListSettings,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::SetArchiveChatListSettings;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let Some(current) = self.session.archive_chat_list_settings else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if current == settings {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self
            .sender
            .send_json(&set_archive_chat_list_settings(extra, settings))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.archive_chat_list_settings = Some(settings);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::ArchiveChatListSettings {
                previous: Some(current),
            });
        }
        Ok(Some(extra))
    }

    /// Slice CL2: Saved Messages — `createPrivateChat` with the own user
    /// id (schema 1.8.67, line 9590: "Call createPrivateChat with
    /// getOption(\"my_id\") and open the chat"). The `chat` answer opens
    /// the chat via the `CreatePrivateChat` pending purpose. `Ok(None)`
    /// = own id unknown (call `getMe` first) or a creation already in
    /// flight; the UI prefers the already-listed self chat when one
    /// exists.
    pub fn create_private_chat_with_self(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(my_id) = self.session.my_user_id else {
            return Ok(None);
        };
        self.create_private_chat_for(my_id)
    }

    /// B1: `createPrivateChat` with an arbitrary user id (schema 1.8.67,
    /// line 9588) — the user button on an inline keyboard. The `chat`
    /// answer opens the chat via the `CreatePrivateChat` pending purpose.
    /// `Ok(None)` = a creation already in flight; the UI prefers an
    /// already-listed private chat with the user when one exists.
    pub fn create_private_chat_for(
        &mut self,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::CreatePrivateChat;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self
            .sender
            .send_json(&create_private_chat(extra, user_id, false))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice CL1: `deleteChatHistory` (schema 1.8.67, line 11845). The
    /// chat stays in the chat list (`remove_from_chat_list: false`);
    /// `revoke` clears for everyone when
    /// `chat.can_be_deleted_for_all_users` (the UI gates the choice).
    pub fn clear_chat_history(
        &mut self,
        chat_id: ChatId,
        revoke: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::DeleteChatHistory;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let deletable = self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            chat.supported()
                && (chat.can_be_deleted_only_for_self || chat.can_be_deleted_for_all_users)
        });
        if !deletable {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&delete_chat_history(extra, chat_id.0, false, revoke))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice CL1: remove a chat from the chat list the way Telegram X
    /// does (`Tdlib.deleteChat` for private chats, closed secret chats,
    /// and chats the user already left): `deleteChatHistory` with
    /// `remove_from_chat_list: true` (schema 1.8.67, line 11845).
    /// This is deliberately NOT the destructive `deleteChat`
    /// constructor (line 11850) — that deletes the chat for all
    /// members and stays on the group panel's "Delete group" (G1).
    pub fn remove_chat_from_list(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::RemoveChatFromList;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let removable = self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            chat.supported()
                && (chat.can_be_deleted_only_for_self || chat.can_be_deleted_for_all_users)
        });
        if !removable {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&delete_chat_history(extra, chat_id.0, true, false))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice CL3: `reportChat` (TDLib 1.8.67, schema line 15693) — the
    /// simple spam-report flow (empty option_id/message_ids/text,
    /// schema:3667). Returns `Ok(None)` when the chat is missing or
    /// `can_be_reported` is false; the `ReportChatResult` outcome
    /// surfaces via `Session::report_chat_outcome`.
    pub fn report_chat(&mut self, chat_id: ChatId) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let reportable = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_be_reported);
        if !reportable {
            return Ok(None);
        }
        let purpose = RequestPurpose::ReportChat;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&report_chat(extra, chat_id.0)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice B2: `sendBotStartMessage` (schema 1.8.67, line 12216) — what
    /// the START button and "Restart bot" send. `parameter` is the
    /// `internalLinkTypeBotStart.start_parameter` (line 9399); empty for
    /// a plain restart. When the chat is fully blocked, unblocks first
    /// (Telegram X `MessagesController.ACTION_BOT_START` likewise
    /// unblocks before the send). Unlike TGX, the start is queued without
    /// waiting for the unblock result; a refused unblock surfaces via
    /// `chat_action_error` and the start fails closed.
    pub fn send_bot_start_message(
        &mut self,
        chat_id: ChatId,
        bot_user_id: i64,
        parameter: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::SendBotStartMessage;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        // Slice B2: on a fully-blocked bot chat the start message would
        // fail; unblock first so the send lands (no-op on unblocked or
        // non-private chats).
        if self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.blocked)
        {
            self.set_chat_user_blocked(chat_id, false)?;
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&send_bot_start_message_request(
            extra,
            bot_user_id,
            chat_id.0,
            parameter,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice B2: `getBotSimilarBots` (schema 1.8.67, line 11640) for the
    /// similar-bots section of the bot profile. Fires once per bot
    /// (deduped by `Session::similar_bots` and in-flight purpose);
    /// response `users` lands via the reducer by `user_id`.
    pub fn fetch_similar_bots(
        &mut self,
        bot_user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.similar_bots.contains_key(&bot_user_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::GetBotSimilarBots;
        if self
            .session
            .requests
            .has_purpose_for_user(purpose, bot_user_id)
        {
            return Ok(None);
        }
        let extra = self.session.request_for_user(purpose, bot_user_id);
        if let Err(err) = self
            .sender
            .send_json(&get_bot_similar_bots(extra, bot_user_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice B2: "Restart bot" — clear the chat's history (`deleteChatHistory`,
    /// schema line 11845, kept in the chat list, `revoke: false`), then
    /// send `sendBotStartMessage` with an empty parameter. The clear is
    /// confirm-gated in the UI (both calls fail closed: a send failure
    /// leaves the request bookkeeping clean).
    pub fn restart_bot(
        &mut self,
        chat_id: ChatId,
        bot_user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if self.clear_chat_history(chat_id, false)?.is_none() {
            return Ok(None);
        }
        self.send_bot_start_message(chat_id, bot_user_id, "")
    }

    /// Slice CL3: `setMessageSenderBlockList` (TDLib 1.8.67, schema line
    /// 14492) for a private/secret chat's peer (schema:3674); `block =
    /// false` passes null `block_list` to unblock (TGX
    /// `Tdlib.unblockSender`). Returns `Ok(None)` when the chat isn't a
    /// private/secret chat or is the user's own chat. The new state
    /// arrives via `updateChatBlockList`.
    pub fn set_chat_user_blocked(
        &mut self,
        chat_id: ChatId,
        block: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let user_id = match self.session.chats.get(&chat_id.0).map(|chat| &chat.kind) {
            Some(ChatKind::Private { user_id }) | Some(ChatKind::Secret { user_id, .. }) => {
                user_id.0
            }
            _ => return Ok(None),
        };
        if self.session.my_user_id.is_some_and(|me| me == user_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetMessageSenderBlockList { block };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&set_message_sender_block_list(extra, user_id, block))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
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

    /// Slice A6: `removeContacts` (schema 1.8.67, line 14528) for one
    /// contact — the user-panel "Delete contact" confirm (TGX
    /// `TdlibUi.deleteContact` → `RemoveContacts`). The reducer
    /// invalidates the contacts list on `ok`.
    pub fn remove_contact(&mut self, user_id: i64) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::RemoveContact, user_id);
        if let Err(err) = self.sender.send_json(&remove_contacts(extra, &[user_id])) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice A6: `importContacts` (schema 1.8.67, line 14517) from the
    /// import dialog's parsed vCard contacts. `Ok(None)` = nothing to
    /// import or an import already in flight. The reducer invalidates
    /// the contacts list on `ok`; the `importedContacts` response itself
    /// carries no per-contact user mapping worth keeping.
    pub fn import_contacts(
        &mut self,
        contacts: &[ImportedContact],
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if contacts.is_empty() {
            return Ok(None);
        }
        let purpose = RequestPurpose::ImportContacts;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&import_contacts(extra, contacts)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice A6: "Delete synced contacts" (TGX
    /// `TdlibContactManager.deleteContacts`, `SyncContactsDeleteInfo`) —
    /// `clearImportedContacts` (schema 1.8.67, line 14539) wipes the
    /// imported set server-side, then `removeContacts` drops the contact
    /// associations TDLib keeps (schema: `clearImportedContacts` leaves
    /// "contact list remains unchanged"). Both are sent in order without
    /// waiting for the first `ok` (TGX issues them the same way), with
    /// TGX's middle empty `changeImportedContacts` step skipped — no
    /// device address book to sync against, so clear+remove fully
    /// achieves the delete (see DECISIONS.md slice A6). The reducer
    /// invalidates the contacts list on either `ok`.
    pub fn delete_synced_contacts(&mut self) -> Result<usize, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let mut sent = 0;
        let extra = self
            .session
            .request(RequestPurpose::ClearImportedContacts, None);
        if let Err(err) = self.sender.send_json(&clear_imported_contacts(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        sent += 1;
        if let Some(ids) = self.session.contacts.clone()
            && !ids.is_empty()
        {
            let extra = self.session.request(RequestPurpose::RemoveContact, None);
            if let Err(err) = self.sender.send_json(&remove_contacts(extra, &ids)) {
                self.session.requests.take(extra);
                return Err(err);
            }
            sent += 1;
        }
        Ok(sent)
    }

    /// Slice A6: user-scoped `setMessageSenderBlockList` (schema 1.8.67,
    /// line 14492) for the user info panel, where there is no chat to
    /// resolve through (the chat-scoped twin is
    /// `set_chat_user_blocked`, CL3). The new state arrives via
    /// `updateChatBlockList` / `updateUserFullInfo`.
    pub fn set_user_blocked(
        &mut self,
        user_id: i64,
        block: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.my_user_id.is_some_and(|me| me == user_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetMessageSenderBlockList { block };
        if self.session.requests.has_purpose_for_user(purpose, user_id) {
            return Ok(None);
        }
        let extra = self.session.request_for_user(purpose, user_id);
        if let Err(err) = self
            .sender
            .send_json(&set_message_sender_block_list(extra, user_id, block))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice A6: persist the contacts preferences edited from the
    /// Contacts tab (same account-scoped dir as the other settings
    /// files).
    pub fn save_contact_prefs(&mut self) -> std::io::Result<()> {
        save_contact_prefs(&self.paths, &self.session.contact_prefs)
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
        self.download_file(file_id, THUMB_DOWNLOAD_PRIORITY, false)
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

    /// Phase 9.5: `getStoryInteractions` — one page of an own story's
    /// viewers. Gated on the cached story and its
    /// `can_get_interactions` flag (the schema comment at
    /// `td_api.tl:6732` names this function as what the flag allows).
    /// Deduped per story while a fetch is in flight; the UI passes the
    /// previous page's `next_offset` for pagination.
    pub fn get_story_interactions(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        offset: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_get = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_get_interactions);
        if !can_get {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.requests.has_purpose_for_story(
            RequestPurpose::GetStoryInteractions,
            chat_id,
            story_id,
        ) {
            return Ok(None);
        }
        let extra =
            self.session
                .request_for_story(RequestPurpose::GetStoryInteractions, chat_id, story_id);
        let json = get_story_interactions_request(extra, story_id, "", offset, 50);
        match self.sender.send_json(&json) {
            Ok(()) => {
                if let Some(state) = self.session.story_viewers.as_mut()
                    && state.chat_id == chat_id.0
                    && state.story_id == story_id
                {
                    state.loading = true;
                }
                Ok(Some(extra))
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.5: `reportStory` — one step of the report flow. The UI
    /// starts with empty `option_id`/`text`; the
    /// `reportStoryResultOptionRequired` / `reportStoryResultTextRequired`
    /// answers tell it what to send next. Gated on the cached story;
    /// own stories (deletable) are not reportable.
    pub fn report_story(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        option_id: &str,
        text: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let reportable = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| !story.can_be_deleted);
        if !reportable {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_story(RequestPurpose::ReportStory, chat_id, story_id);
        let json = report_story_request(extra, chat_id, story_id, option_id, text);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.5: `activateStoryStealthMode` — hides the current user's
    /// story views (Premium only; the server decides). The state arrives
    /// as `updateStoryStealthMode`; a refused call surfaces as
    /// `Session::story_stealth_error`. Deduped while in flight.
    pub fn activate_story_stealth_mode(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::ActivateStoryStealthMode)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::ActivateStoryStealthMode, None);
        match self
            .sender
            .send_json(&activate_story_stealth_mode_request(extra))
        {
            Ok(()) => {
                self.session.story_stealth_error = None;
                Ok(Some(extra))
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.3: `canPostStory` eligibility check (TDLib 1.8.67,
    /// `schema/td_api.tl:13702`) for the given chat — the user's own
    /// story chat or an eligible channel / supergroup from
    /// `getChatsToPostStories`. The composer calls this before every
    /// post; the answer lands in `Session::story_post.eligibility`.
    /// Deduped per chat while a check is in flight.
    pub fn check_can_post_story(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.my_user_id.is_none() {
            return Err(ConnectSendError::InvalidRequest);
        };
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::CheckCanPostStory, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::CheckCanPostStory, Some(chat_id));
        match self
            .sender
            .send_json(&can_post_story_request(extra, chat_id))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.3: `postStory` (TDLib 1.8.67, `schema/td_api.tl:13715`) —
    /// posts the composer's photo/video with caption + privacy on
    /// `chat_id` — the current user's own story chat, or a channel /
    /// supergroup from `getChatsToPostStories` (post-as-channel, 9.5).
    /// `kind` must be detected and the file must exist; `SelectedUsers`
    /// needs at least one user. Phase 9.4: `active_period` must be one
    /// of the schema-legal values (21600 / 43200 / 86400 / 172800 —
    /// `td_api.tl:13715` comment); anything else is rejected before
    /// sending. Phase 9.5: `from_story` carries a repost source
    /// (`storyFullId`, `td_api.tl:6766`). The `story` response and
    /// `updateStoryPostSucceeded` / `updateStoryPostFailed` drive
    /// `Session::story_post.outcome`.
    #[allow(clippy::too_many_arguments)] // mirrors requests::post_story, one arg per schema field
    pub fn post_story(
        &mut self,
        chat_id: ChatId,
        kind: StoryMediaKind,
        path: &str,
        caption: &str,
        privacy: StoryPrivacy,
        user_ids: &[i64],
        areas: serde_json::Value,
        active_period: i32,
        from_story: Option<(i64, i32)>,
        is_posted_to_chat_page: bool,
        protect_content: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.my_user_id.is_none() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if kind == StoryMediaKind::Unknown || !std::path::Path::new(path).is_file() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if privacy == StoryPrivacy::SelectedUsers && user_ids.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !matches!(active_period, 21600 | 43200 | 86400 | 172800) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::PostStory, Some(chat_id));
        let json = post_story_request(
            extra,
            chat_id,
            kind,
            path,
            caption,
            privacy.settings_json(user_ids),
            areas,
            active_period,
            from_story,
            is_posted_to_chat_page,
            protect_content,
        );
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.5: `getChatsToPostStories` (TDLib 1.8.67,
    /// `schema/td_api.tl:13698`) — channels/supergroups where the user
    /// may post stories; stored in `Session::story_post_as_chats`.
    /// Deduped while a fetch is in flight.
    pub fn get_chats_to_post_stories(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetChatsToPostStories)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatsToPostStories, None);
        match self
            .sender
            .send_json(&get_chats_to_post_stories_request(extra))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.5: `editStory` (TDLib 1.8.67, `schema/td_api.tl:13732`).
    /// Gated on the cached story's `can_be_edited`; `None` fields keep
    /// the current value. Sets `Session::story_manage` pending/error.
    pub fn edit_story(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        content: Option<serde_json::Value>,
        areas: Option<serde_json::Value>,
        caption: Option<&str>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let editable = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_be_edited);
        if !editable {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_story(RequestPurpose::EditStory, chat_id, story_id);
        self.session.story_manage.pending = true;
        self.session.story_manage.error = None;
        match self.sender.send_json(&edit_story_request(
            extra, chat_id, story_id, content, areas, caption,
        )) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.story_manage.pending = false;
                Err(err)
            }
        }
    }

    /// Phase 9.5: `editStoryCover` (TDLib 1.8.67, `schema/td_api.tl:13738`).
    /// Gated on the cached story's `can_be_edited`.
    pub fn edit_story_cover(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        cover_frame_timestamp: f64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let editable = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_be_edited);
        if !editable {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra =
            self.session
                .request_for_story(RequestPurpose::EditStoryCover, chat_id, story_id);
        self.session.story_manage.pending = true;
        self.session.story_manage.error = None;
        match self.sender.send_json(&edit_story_cover_request(
            extra,
            chat_id,
            story_id,
            cover_frame_timestamp,
        )) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.story_manage.pending = false;
                Err(err)
            }
        }
    }

    /// Phase 9.5: `setStoryPrivacySettings` (TDLib 1.8.67,
    /// `schema/td_api.tl:13743`). Gated on the cached story's
    /// `can_set_privacy_settings`.
    pub fn set_story_privacy_settings(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        privacy_settings: serde_json::Value,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let settable = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_set_privacy_settings);
        if !settable {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request_for_story(
            RequestPurpose::SetStoryPrivacySettings,
            chat_id,
            story_id,
        );
        self.session.story_manage.pending = true;
        self.session.story_manage.error = None;
        match self.sender.send_json(&set_story_privacy_settings_request(
            extra,
            story_id,
            privacy_settings,
        )) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.story_manage.pending = false;
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
    /// MED4: caption-length gate against the runtime
    /// `message_caption_length_max` option (TDLib 1.8.67, `schema/td_api.tl:6088`).
    /// Counts Unicode scalar values; TDLib's exact limit unit is not
    /// source-verified (assumption — TDLib remains the final gate, and a
    /// server refusal surfaces in the status note). Plain-text sends use the
    /// separate `message_text_length_max` option (untracked here — out of
    /// this slice).
    fn check_caption_length(&self, caption: &str) -> Result<(), ConnectSendError> {
        let limit = self.session.message_caption_length_max;
        if caption.chars().count() as i64 > i64::from(limit.max(0)) {
            Err(ConnectSendError::CaptionTooLong { limit })
        } else {
            Ok(())
        }
    }

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
        // MED4: caption-length gate (runtime option; the counter in the
        // composer shows the same limit). Only the caption-carrying
        // paths are gated — plain text has its own (untracked) limit.
        if snapshot.attachment.is_some() || snapshot.is_media_album() {
            self.check_caption_length(caption)?;
        }
        // Slice G1: quote-carrying reply (`inputTextQuote`).
        let reply_to = snapshot.send_reply();
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
        // M1 fix-up: `textEntityTypeBlockQuote` is not supported in secret
        // chats (schema) — strip it from captions here; the text path
        // does the same via `SendOptions::is_secret`.
        let is_secret = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        // Contains caption / path — do not log `json`.
        let json = match (snapshot.attachment.as_ref(), media_path.as_deref()) {
            (Some(att), Some(path)) => match att.kind {
                AttachmentKind::Photo => send_photo(
                    extra,
                    chat_id,
                    topic_id,
                    path,
                    caption,
                    snapshot.caption_above_media,
                    reply_to,
                    self_destruct,
                    is_secret,
                ),
                AttachmentKind::Document => {
                    send_document(extra, chat_id, topic_id, path, caption, reply_to, is_secret)
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
                        snapshot.caption_above_media,
                        reply_to,
                        is_secret,
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
            (None, None) => {
                // Phase S1: secret chats never get link previews (TGX
                // default-off; previews are generated on Telegram servers,
                // which can't see E2E content).
                let is_secret = self
                    .session
                    .chats
                    .get(&chat_id.0)
                    .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
                // M1: the composer send options ride the snapshot; secret
                // chats force the preview toggle off regardless, and mark
                // the send so `textEntityTypeBlockQuote` is stripped
                // (unsupported in secret chats).
                let mut send_options = snapshot.send_options;
                if is_secret {
                    send_options.link_preview_disabled = true;
                    send_options.is_secret = true;
                }
                send_text(extra, chat_id, topic_id, caption, reply_to, &send_options)
            }
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

    /// M2: `sendMessage` with `inputMessageRichMessage` (TDLib 1.8.67, line
    /// 6084) — sends the rich editor's blocks. The composer clears only
    /// after a `message` response or a surfaced error; a failed send never
    /// reports success (the error is shown, the draft stays). No optimistic
    /// local row — M1's optimistic send is text-only, so a failed rich send
    /// can't strand a fake row.
    pub fn send_rich_snapshot(
        &mut self,
        chat_id: ChatId,
        blocks: &[RichBlock],
        reply_to: Option<SendReply>,
        options: &SendOptions,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Phase 2.3: channel posting is admin-gated, same as `send_snapshot`.
        let can_post = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_post());
        if !can_post {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Parity slice 4: never send into a closed forum topic.
        if self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let rich =
            crate::rich::input_rich_message(blocks).ok_or(ConnectSendError::InvalidRequest)?;
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        let json = send_rich_message(extra, chat_id, topic_id, &rich, reply_to, options);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// M2: `getFullRichMessage` (TDLib 1.8.67, line 11554) for a
    /// partially-received rich message (`is_full == false`). The reducer
    /// replaces the history row's blocks with the full ones on success; a
    /// failed fetch leaves the partial blocks in place (honest).
    pub fn fetch_full_rich_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::GetFullRichMessage {
                chat_id,
                message_id,
            },
            Some(chat_id),
        );
        let json = get_full_rich_message(extra, chat_id, message_id);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
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
        // MED4: caption-length gate (same runtime option as single
        // sends — `send_snapshot` returns here before its own check).
        self.check_caption_length(caption)?;
        // Phase B3: same private-chat gate as `send_snapshot` — the timer
        // applies per album item (the schema allows it per
        // `inputMessagePhoto`/`inputMessageVideo`).
        let self_destruct = self
            .session
            .chats
            .get(&chat_id.0)
            .filter(|chat| matches!(chat.kind, ChatKind::Private { .. }))
            .and(snapshot.self_destruct);
        // M1 fix-up: same secret-chat blockquote strip as `send_snapshot`.
        let is_secret = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        let mut contents = Vec::with_capacity(snapshot.album.len());
        for (index, att) in snapshot.album.iter().enumerate() {
            let path = att
                .send_path_str()
                .ok_or(ConnectSendError::InvalidRequest)?;
            let item_caption = if index == last { caption } else { "" };
            let content = match att.kind {
                AttachmentKind::Photo => input_message_photo(
                    &path,
                    item_caption,
                    snapshot.caption_above_media,
                    self_destruct,
                    is_secret,
                ),
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
                        snapshot.caption_above_media,
                        is_secret,
                    )
                }
                AttachmentKind::Document | AttachmentKind::VideoNote => {
                    return Err(ConnectSendError::InvalidRequest);
                }
            };
            contents.push(content);
        }
        // Slice G1: quote-carrying reply (`inputTextQuote`).
        let reply_to = snapshot.send_reply();
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
        reply_to: Option<SendReply>,
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

    /// `sendMessage` + `inputMessageVideoNote` for a finished camera
    /// capture. Mirrors `send_voice_note`; the draft is already squared
    /// and probed by `VideoNoteCapture::finish`.
    pub fn send_recorded_video_note(
        &mut self,
        draft: &crate::video::VideoNoteDraft,
        reply_to: Option<SendReply>,
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
        let thumbnail = crate::video::write_video_note_thumbnail(&draft.path).map(|thumb| {
            VideoNoteThumbnailSend {
                path: thumb.path.to_string_lossy().into_owned(),
                width: thumb.width,
                height: thumb.height,
            }
        });
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        let json = send_video_note(
            extra,
            chat_id,
            topic_id,
            &path,
            &VideoNoteSend {
                duration: draft.duration_secs,
                length: draft.length,
                thumbnail,
            },
            reply_to,
        );
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                let _ = self.sync_video_note_recording(false, 0);
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// MED2: `recognizeSpeech` for a voice/video note message. Only real
    /// (non-pending) messages qualify; the transcript arrives later via
    /// `updateMessageContent`. A refused request is an error, never a
    /// faked transcript.
    pub fn recognize_speech(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
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
            .request(RequestPurpose::RecognizeSpeech, Some(chat_id));
        let json = recognize_speech(extra, chat_id, message_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
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
        let extra =
            self.callback_query_extra(chat_id, message_id, RequestPurpose::GetCallbackQueryAnswer)?;
        let json = get_callback_query_answer(extra, chat_id, message_id, data);
        self.send_json_request(extra, &json)
    }

    /// B1: password-protected callback button —
    /// `callbackQueryPayloadDataWithPassword` (schema 1.8.67, line 7740).
    /// The caller drops the password right after the call.
    pub fn send_callback_query_with_password(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        password: &str,
        data: &[u8],
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.callback_query_extra(
            chat_id,
            message_id,
            RequestPurpose::GetCallbackQueryAnswerWithPassword,
        )?;
        let json =
            get_callback_query_answer_with_password(extra, chat_id, message_id, password, data);
        self.send_json_request(extra, &json)
    }

    /// B1: game button — `callbackQueryPayloadGame` with the `game.short_name`
    /// from the message's `messageGame` content (schema 1.8.67, line 7743).
    pub fn send_game_callback_query(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        game_short_name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.callback_query_extra(
            chat_id,
            message_id,
            RequestPurpose::GetCallbackQueryAnswerGame,
        )?;
        let json = get_callback_query_answer_game(extra, chat_id, message_id, game_short_name);
        self.send_json_request(extra, &json)
    }

    /// B1: resolve a login-URL button (`getLoginUrlInfo`, schema 1.8.67,
    /// line 12985). `fallback_url` is the button's raw URL, kept in the
    /// session so a TDLib error degrades to a plain URL-button press.
    pub fn send_login_url_info(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        button_id: i64,
        fallback_url: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let extra =
            self.callback_query_extra(chat_id, message_id, RequestPurpose::GetLoginUrlInfo)?;
        self.session.login_url_request = Some(LoginUrlRequest {
            chat_id,
            message_id,
            button_id,
            raw_url: fallback_url.to_string(),
        });
        let json = get_login_url_info(extra, chat_id, message_id, button_id);
        self.send_json_request(extra, &json)
    }

    /// B1: the authorized URL after the user consented to a
    /// `loginUrlInfoRequestConfirmation` (`getLoginUrl`, schema 1.8.67,
    /// line 12993; TGX `TGInlineKeyboard` does exactly this). `fallback_url`
    /// is the button's raw URL, used when TDLib errors.
    pub fn send_login_url(
        &mut self,
        request: &LoginUrlRequest,
        allow_write_access: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.callback_query_extra(
            request.chat_id,
            request.message_id,
            RequestPurpose::GetLoginUrl,
        )?;
        self.session.login_url_request = Some(request.clone());
        let json = get_login_url(
            extra,
            request.chat_id,
            request.message_id,
            request.button_id,
            allow_write_access,
        );
        self.send_json_request(extra, &json)
    }

    /// B1: `deleteChatReplyMarkup` after a one-time custom keyboard was used
    /// (schema 1.8.67, line 13183). Best-effort: the local keyboard hides
    /// regardless of the answer.
    pub fn delete_chat_reply_markup(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteChatReplyMarkup, Some(chat_id));
        let json = delete_chat_reply_markup_request(extra, chat_id, message_id);
        self.send_json_request(extra, &json)
    }

    /// A5: `setName` (schema 1.8.67, line 14823). Best-effort: the name
    /// refreshes via `updateUser`; failures surface in
    /// `Session::profile_edit_error`.
    pub fn set_name(
        &mut self,
        first_name: &str,
        last_name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetName, None);
        let json = set_name(extra, first_name, last_name);
        self.send_json_request(extra, &json)
    }

    /// A5: `setBio` (schema 1.8.67, line 14826).
    pub fn set_bio(&mut self, bio: &str) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetBio, None);
        let json = set_bio(extra, bio);
        self.send_json_request(extra, &json)
    }

    /// A5: `setUsername` (schema 1.8.67, line 14830). Changes the
    /// editable username; empty string removes it.
    pub fn set_username(&mut self, username: &str) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetUsername, None);
        let json = set_username(extra, username);
        self.send_json_request(extra, &json)
    }

    /// A5: `checkChatUsername` for the current user's own username (schema
    /// 1.8.67, line 11677; the private chat with self is the documented
    /// chat id — TGX `EditUsernameController` sends it with
    /// `tdlib.selfChatId()`). The verdict lands in
    /// `Session::username_check`; the in-flight text in
    /// `Session::username_check_pending` so the dialog can ignore stale
    /// verdicts.
    pub fn check_username(&mut self, username: &str) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(me) = self.session.my_user_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let extra = self.session.request(RequestPurpose::CheckUsername, None);
        self.session.username_check_pending = Some(username.to_string());
        let json = check_chat_username(extra, ChatId(me), username);
        let sent = self.send_json_request(extra, &json);
        if sent.is_err() {
            // Don't leave the dialog showing "Checking…" for a request
            // that never went out.
            self.session.username_check_pending = None;
        }
        sent
    }

    /// A5: `reorderActiveUsernames` (schema 1.8.67, line 14838) — the
    /// full active list in the new order.
    pub fn reorder_active_usernames(
        &mut self,
        usernames: &[String],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ReorderActiveUsernames, None);
        let json = reorder_active_usernames(extra, usernames);
        self.send_json_request(extra, &json)
    }

    /// Slice P1: fetch the `paymentForm` for a Buy button press
    /// (`getPaymentForm`, schema 1.8.67, line 15262). The dialog opens when
    /// the `paymentForm` answer is applied.
    pub fn send_payment_form_request(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        let extra =
            self.callback_query_extra(chat_id, message_id, RequestPurpose::GetPaymentForm)?;
        self.session.payment_request = Some(PaymentRequest {
            chat_id,
            message_id,
        });
        let json = get_payment_form(extra, chat_id, message_id);
        self.send_json_request(extra, &json)
    }

    /// Slice P1: `validateOrderInfo` (schema 1.8.67, line 15268) — validate
    /// the order form and fetch the shipping options.
    pub fn validate_payment_order_info(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        order: &OrderInfoData,
        allow_save: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let extra =
            self.callback_query_extra(chat_id, message_id, RequestPurpose::ValidateOrderInfo)?;
        self.session.payment_request = Some(PaymentRequest {
            chat_id,
            message_id,
        });
        let json = validate_order_info_request(extra, chat_id, message_id, order, allow_save);
        self.send_json_request(extra, &json)
    }

    /// Slice P1: `sendPaymentForm` (schema 1.8.67, line 15277) — submit the
    /// validated order + credentials from the checkout dialog.
    #[allow(clippy::too_many_arguments)]
    pub fn submit_payment_form(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        payment_form_id: i64,
        order_info_id: &str,
        shipping_option_id: &str,
        credentials: serde_json::Value,
        tip_amount: i64,
    ) -> Result<RequestId, ConnectSendError> {
        let extra =
            self.callback_query_extra(chat_id, message_id, RequestPurpose::SendPaymentForm)?;
        self.session.payment_request = Some(PaymentRequest {
            chat_id,
            message_id,
        });
        let json = send_payment_form_request(
            extra,
            chat_id,
            message_id,
            payment_form_id,
            order_info_id,
            shipping_option_id,
            credentials,
            tip_amount,
        );
        self.send_json_request(extra, &json)
    }

    /// Slice P1: `getPaymentReceipt` (schema 1.8.67, line 15280) — fetch
    /// the receipt for a paid invoice (its `receipt_message_id`).
    pub fn fetch_payment_receipt(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetPaymentReceipt, Some(chat_id));
        self.session.payment_request = Some(PaymentRequest {
            chat_id,
            message_id,
        });
        let json = get_payment_receipt(extra, chat_id, message_id);
        self.send_json_request(extra, &json)
    }
    /// A5: `toggleUsernameIsActive` (schema 1.8.67, line 14835).
    pub fn toggle_username_is_active(
        &mut self,
        username: &str,
        is_active: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ToggleUsernameIsActive, None);
        let json = toggle_username_is_active(extra, username, is_active);
        self.send_json_request(extra, &json)
    }

    /// A5: `setProfilePhoto` with `inputChatPhotoStatic` / `inputFileLocal`
    /// (schema 1.8.67, lines 14803/1042/325). `is_public` is hard-coded
    /// false: this edits the main photo, not the public one (which stays
    /// visible even when the main photo is hidden by privacy settings).
    pub fn set_profile_photo(&mut self, photo_path: &str) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetProfilePhoto, None);
        let json = set_profile_photo(extra, photo_path, false);
        self.send_json_request(extra, &json)
    }

    /// A5: `deleteProfilePhoto` (schema 1.8.67, line 14806).
    pub fn delete_profile_photo(
        &mut self,
        profile_photo_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteProfilePhoto, None);
        let json = delete_profile_photo(extra, profile_photo_id);
        self.send_json_request(extra, &json)
    }

    /// Shared gate for callback-query sends: real (non-pending) messages in
    /// a supported chat. Returns the reserved `@extra`.
    fn callback_query_extra(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        purpose: RequestPurpose,
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
        Ok(self.session.request(purpose, Some(chat_id)))
    }

    /// Send a prebuilt request JSON; roll the reserved `@extra` back when
    /// the sender refuses it.
    fn send_json_request(
        &mut self,
        extra: RequestId,
        json: &str,
    ) -> Result<RequestId, ConnectSendError> {
        match self.sender.send_json(json) {
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
        self.sync_record_action(active, now_ms, "chatActionRecordingVoiceNote")
    }

    /// MED2: same as [`Self::sync_voice_recording`] for round video-note
    /// capture (`chatActionRecordingVideoNote`, schema 1.8.67 line 6392).
    pub fn sync_video_note_recording(
        &mut self,
        active: bool,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        self.sync_record_action(active, now_ms, "chatActionRecordingVideoNote")
    }

    fn sync_record_action(
        &mut self,
        active: bool,
        now_ms: u64,
        action: &str,
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
        self.send_voice_action(chat_id, true, now_ms, action)
    }

    fn cancel_outgoing_voice(&mut self) -> Result<(), ConnectSendError> {
        let Some(prev) = self.outgoing_voice.take() else {
            return Ok(());
        };
        if !self.typing_chat_allowed(prev.chat_id) {
            return Ok(());
        }
        self.send_voice_action(prev.chat_id, false, 0, "chatActionRecordingVoiceNote")
    }

    fn send_voice_action(
        &mut self,
        chat_id: ChatId,
        recording: bool,
        now_ms: u64,
        action: &str,
    ) -> Result<(), ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::SendChatAction, Some(chat_id));
        let json = send_chat_action_kind(
            extra,
            chat_id,
            if recording {
                action
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
        // M1: scheduled sends live in `session.scheduled_messages`
        // (`ParsedMessage`, never pending), not in history
        // (`HistoryMessage`) — same `editMessageText` request, different
        // validation source.
        let owned = if edit.scheduled {
            self.session
                .scheduled_messages
                .iter()
                .find(|m| m.chat_id == edit.chat_id && m.id == edit.message_id)
                .map(|m| (m.chat_id, m.id, m.is_outgoing, false, &m.content))
        } else {
            self.session
                .histories
                .get(&edit.chat_id.0)
                .and_then(|history| history.messages.get(&edit.message_id.0))
                .map(|m| (m.chat_id, m.id, m.is_outgoing, m.pending, &m.content))
        };
        let Some((chat_id, message_id, is_outgoing, pending, content)) = owned else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if ComposerEdit::from_own_content(chat_id, message_id, is_outgoing, pending, content)
            .is_none()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let caption = text.trim();
        if matches!(edit.kind, ComposerEditKind::Text) && caption.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // MED4: caption-length gate for caption edits (same runtime
        // option as media sends).
        if matches!(edit.kind, ComposerEditKind::Caption) {
            self.check_caption_length(caption)?;
        }
        let extra = self
            .session
            .request(RequestPurpose::EditMessage, Some(edit.chat_id));
        // M1 fix-up: secret chats strip `textEntityTypeBlockQuote` from
        // the edited caption too (unsupported in secret chats).
        let strip_blockquote = self
            .session
            .chats
            .get(&edit.chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        let json = match edit.kind {
            ComposerEditKind::Text => edit_message_text(
                extra,
                edit.chat_id,
                edit.message_id,
                caption,
                strip_blockquote,
            ),
            ComposerEditKind::Caption => edit_message_caption(
                extra,
                edit.chat_id,
                edit.message_id,
                caption,
                // MED4 review nit: secret chats force caption-below on send
                // too (TGX `allowShowCaptionAboveMedia`).
                edit.caption_above && !strip_blockquote,
                strip_blockquote,
            ),
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
    /// `revoke: true` deletes for everyone (own outgoing default), false only
    /// for the current user (schema 1.8.67 line 12282).
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
        // M1: any sent message may be deleted; the for-everyone toggle is
        // only honored for own outgoing (schema 1.8.67 lines 6228–6229).
        if DeleteConfirm::for_message(
            message.chat_id,
            message.id,
            message.is_outgoing,
            message.pending,
        )
        .is_none()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        // M1: extract the borrow before the mutable `request` call below.
        let revoke = confirm.revoke && message.is_outgoing;
        let extra = self
            .session
            .request(RequestPurpose::DeleteMessages, Some(confirm.chat_id));
        let json = delete_messages(extra, confirm.chat_id, &[confirm.message_id], revoke);
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
        let json = forward_messages(
            extra,
            dest,
            draft.from_chat_id,
            &draft.message_ids,
            draft.send_copy,
            draft.remove_caption,
        );
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

    /// Pin a history message (tdesktop / Unigram Pin). `silent` maps to
    /// `pinChatMessage.disable_notification` (TDLib 1.8.67 line 13559);
    /// `only_for_self` stays false.
    pub fn pin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        silent: bool,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_pin_chat_message(chat_id, message_id, false, silent)
    }

    /// Unpin one pinned message (tdesktop PinnedBar cancel / Unpin).
    pub fn unpin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_pin_chat_message(chat_id, message_id, true, false)
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
        self.send_pin_chat_message(chat_id, message_id, pinned, false)
    }

    fn send_pin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        unpin: bool,
        silent: bool,
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
            pin_chat_message(extra, chat_id, message_id, silent, false)
        };
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// M1: unpin every pinned message in a chat (tdesktop pinned-bar menu /
    /// context action; TDLib 1.8.67 `schema/td_api.tl:13565`).
    pub fn unpin_all_chat_messages(
        &mut self,
        chat_id: ChatId,
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
            .request(RequestPurpose::UnpinAllChatMessages, Some(chat_id));
        let json = unpin_all_chat_messages(extra, chat_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// M1: share a message link (tdesktop context menu "Copy Message Link").
    /// M1 fix-up: `getMessageLink` is only valid when
    /// `messageProperties.can_get_link` (schema 1.8.67 line 12056), so
    /// this sends `getMessageProperties` first; `ingest` chains to the
    /// actual `getMessageLink` only when the gate passes, and otherwise
    /// stashes `Session::message_link_error` for the UI status note.
    /// The parsed `messageLink.link` lands in
    /// `session.message_link_result` for the UI to copy to the clipboard.
    pub fn get_message_link(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let message = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0));
        let Some(message) = message else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if message.pending || message.id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::GetMessageLinkProperties {
                chat_id,
                message_id,
            },
            Some(chat_id),
        );
        let json = get_message_properties(extra, chat_id, message_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// M1 fix-up: the second half of the "Share link" gate — sent from
    /// `ingest` only after `messageProperties.can_get_link` passed.
    fn send_message_link_request(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetMessageLink, Some(chat_id));
        let json = get_message_link(extra, chat_id, message_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// MED4: outcome of an Instant View open attempt (TGX
    /// `TdlibUi.fetchInstantView` + browser fallback).
    pub fn open_instant_view(&mut self, url: &str) -> InstantViewOutcome {
        // Mode Off (or a send failure) means the caller opens the URL in
        // the browser directly — never a fake reader.
        if !matches!(
            self.session.media_prefs.instant_view_mode,
            InstantViewMode::Telegram | InstantViewMode::All
        ) {
            return InstantViewOutcome::Browser;
        }
        let extra = self
            .session
            .request(RequestPurpose::GetWebPageInstantView, None);
        let json = get_web_page_instant_view(extra, url);
        match self.sender.send_json(&json) {
            Ok(()) => {
                self.session
                    .instant_view_urls
                    .insert(extra, url.to_string());
                InstantViewOutcome::Requested
            }
            Err(_) => {
                self.session.requests.take(extra);
                InstantViewOutcome::Browser
            }
        }
    }

    /// MED4b: `getLinkPreview` prefetch for the composer chip (TGX
    /// `LinkPreview.loadLinkPreview`: `GetLinkPreview(FormattedText(url),
    /// null)`). The UI debounces (schema: "Do not call this function too
    /// often"); the answer (or 404) lands in
    /// `Session::composer_preview` for the chip.
    pub fn request_composer_link_preview(&mut self, url: &str) -> Result<(), ConnectSendError> {
        let extra = self.session.request(RequestPurpose::GetLinkPreview, None);
        let json = get_link_preview(extra, url);
        match self.sender.send_json(&json) {
            Ok(()) => {
                self.session
                    .composer_preview_urls
                    .insert(extra, url.to_string());
                // Loading state — the chip shows "Getting link info…"
                // (TGX `LinkPreview.isLoading`).
                self.session.composer_preview = Some(ComposerLinkPreview {
                    url: url.to_string(),
                    preview: None,
                });
                Ok(())
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
    pub fn delete_scheduled_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // M1: delete a scheduled send. Scheduled messages live in
        // `session.scheduled_messages`, not in history, so the
        // history-validated `delete_confirmed` can't take them. `revoke`
        // is always false (no for-everyone distinction before sending).
        let known = self
            .session
            .scheduled_messages
            .iter()
            .any(|m| m.chat_id == chat_id && m.id == message_id);
        if !known {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteMessages, Some(chat_id));
        let json = delete_messages(extra, chat_id, &[message_id], false);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// M1: retry a failed send (`resendMessages`, TDLib 1.8.67,
    /// `schema/td_api.tl:12251`; `message.sending_state.can_retry`, schema
    /// line 3038). The driver only retries rows the reducer marked
    /// `failed` **and** retryable — not every failed send may be
    /// retried, and the context menu offers "Retry send" on the same
    /// gate.
    pub fn resend_failed_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_retry = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| message.failed && message.can_retry);
        if !can_retry {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ResendMessages, Some(chat_id));
        let json = resend_messages(extra, chat_id, &[message_id]);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// M1: load a chat's scheduled (pending) sends into
    /// `session.scheduled_messages` (TDLib 1.8.67,
    /// `schema/td_api.tl:12000`).
    pub fn get_chat_scheduled_messages(
        &mut self,
        chat_id: ChatId,
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
            .request(RequestPurpose::GetChatScheduledMessages, Some(chat_id));
        let json = get_chat_scheduled_messages(extra, chat_id);
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

    /// B4: one `getPollVoters` page (`schema/td_api.tl:12941`; page size 50,
    /// the schema max). Guards mirror `send_poll_answer` plus the
    /// `poll.can_get_voters` gate (schema line 12941). `option_index` is
    /// the 0-based option index. The answer lands in
    /// `Session::poll_voters` (first page replaces, later pages append).
    pub fn fetch_poll_voters(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_index: usize,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.fetch_poll_voters_page(chat_id, message_id, option_index, 0)
    }

    /// B4: the next `getPollVoters` page — `offset` is the already-loaded
    /// count. No-op unless the cache holds a loaded page; exhaustion is
    /// handled in the reducer, which clamps `total_count` on a short
    /// page so the UI hides "Load more".
    pub fn load_more_poll_voters(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_index: usize,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let key = (chat_id.0, message_id.0, option_index as i32);
        let offset = match self.session.poll_voters.get(&key) {
            Some(PollVotersFetch::Loaded { voters, .. }) => voters.len() as i32,
            _ => return Ok(None),
        };
        self.fetch_poll_voters_page(chat_id, message_id, option_index, offset)
    }

    /// B4: page size for `getPollVoters` (schema line 12941: limit ≤ 50).
    pub const POLL_VOTERS_PAGE_SIZE: i32 = 50;

    fn fetch_poll_voters_page(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_index: usize,
        offset: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
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
        if !poll.can_get_voters || option_index >= poll.options.len() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let key = (chat_id.0, message_id.0, option_index as i32);
        if matches!(
            self.session.poll_voters.get(&key),
            Some(PollVotersFetch::Loading)
        ) || self.session.requests.has_purpose_for_chat(
            RequestPurpose::GetPollVoters {
                chat_id,
                message_id,
                option_id: option_index as i32,
                offset,
            },
            chat_id,
        ) {
            return Ok(None);
        }
        // A "load more" must not clobber the loaded page while it flies.
        if offset > 0
            && !matches!(
                self.session.poll_voters.get(&key),
                Some(PollVotersFetch::Loaded { .. })
            )
        {
            return Ok(None);
        }
        if offset == 0 {
            self.session
                .poll_voters
                .insert(key, PollVotersFetch::Loading);
        }
        let extra = self.session.request(
            RequestPurpose::GetPollVoters {
                chat_id,
                message_id,
                option_id: option_index as i32,
                offset,
            },
            Some(chat_id),
        );
        let json = get_poll_voters(
            extra,
            chat_id,
            message_id,
            option_index as i32,
            offset,
            Self::POLL_VOTERS_PAGE_SIZE,
        );
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                if offset == 0 {
                    self.session.poll_voters.remove(&key);
                }
                Err(err)
            }
        }
    }

    /// Bots slice: `getInlineQueryResults` (TDLib 1.8.67, line 13019).
    /// `offset` is "" for the first chunk, the loaded page's
    /// `next_offset` for the next. No-op while a request is in flight for
    /// the (chat, bot) pair. A first page marks the single slot `Loading`
    /// (a re-query replaces it); pagination leaves the loaded page.
    pub fn inline_query(
        &mut self,
        bot_user_id: i64,
        chat_id: ChatId,
        query: &str,
        offset: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_inline_query_in_flight(chat_id, bot_user_id)
        {
            return Ok(None);
        }
        let first_page = offset.is_empty();
        if first_page {
            self.session.inline_query = Some(InlineQuerySlot {
                chat_id,
                bot_user_id,
                query: query.to_string(),
                fetch: InlineQueryFetch::Loading,
            });
        }
        let extra = self.session.request(
            RequestPurpose::GetInlineQueryResults {
                chat_id,
                bot_user_id,
                first_page,
            },
            Some(chat_id),
        );
        let json = get_inline_query_results(extra, bot_user_id, chat_id, query, offset);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                if first_page {
                    self.session.inline_query = None;
                }
                Err(err)
            }
        }
    }

    /// B4: stop a poll / quiz via `stopPoll` (schema 1.8.67, line 12953).
    /// Guards: chats path active, supported chat, the message is a live
    /// open poll. The UI confirms before calling; `can_be_edited`
    /// (schema line 12951) is the server gate and the UI only offers it
    /// on own polls (`poll::can_stop_poll`). Response is `ok`; the poll
    /// closes via `updatePoll`.
    pub fn stop_poll(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
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
        let can_stop = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .filter(|message| !message.pending && message.id.0 > 0)
            .and_then(|message| match &message.content {
                // F4: defense in depth — the UI menu already gates on
                // `can_stop_poll`, but the driver checks ownership too.
                MessageContent::Poll(poll) => Some(can_stop_poll(message.is_outgoing, &poll.poll)),
                _ => None,
            })
            .unwrap_or(false);
        if !can_stop {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::StopPoll, Some(chat_id));
        let json = stop_poll_request(extra, chat_id, message_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 4.2: create a poll from the composer dialog via `sendMessage` +
    /// `inputMessagePoll` (TDLib 1.8.67). Guards: chats path active, chat can
    /// post (admin-gated channels, same as `send_snapshot`), valid draft
    /// (`PollDraft::validate`). Quiz drafts send `inputPollTypeQuiz`
    /// (schema line 488); quiz mode forces no revoting (Telegram X
    /// `CreatePollController` does the same on quiz toggle) and
    /// single-answer (Quill's own stricter choice; TGX allows
    /// multi-correct quizzes).
    pub fn send_poll_draft(
        &mut self,
        chat_id: ChatId,
        draft: &PollDraft,
        reply_to: Option<SendReply>,
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
        let description = draft.description.trim().to_string();
        let options: Vec<String> = draft
            .usable_options()
            .into_iter()
            .map(str::to_string)
            .collect();
        let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
        let country_refs: Vec<&str> = draft.country_codes.iter().map(String::as_str).collect();
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        // Parity slice 4: sends from a topic view address the open topic.
        let topic_id = self.send_topic(chat_id);
        // `validate()` above guarantees `quiz_correct` is `Some` in quiz mode.
        let correct_option_ids: Vec<i32> = vec![draft.quiz_correct.unwrap_or(0) as i32];
        let json = send_poll(
            extra,
            chat_id,
            PollSend {
                question: &question,
                options: &option_refs,
                description: &description,
                is_anonymous: draft.is_anonymous,
                allows_multiple_answers: draft.allows_multiple_answers && !draft.is_quiz,
                allows_revoting: draft.allows_revoting && !draft.is_quiz,
                shuffle_options: draft.shuffle_options,
                country_codes: &country_refs,
                poll_type: if draft.is_quiz {
                    PollTypeSend::Quiz {
                        correct_option_ids: &correct_option_ids,
                        explanation: draft.quiz_explanation.trim(),
                    }
                } else {
                    PollTypeSend::Regular
                },
                open_period: draft.open_period_secs(),
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

    /// Phase S2: `getStorageStatistics` for the storage-usage overlay —
    /// once per session unless forced (guarded by the cache and the
    /// in-flight purpose). `chat_limit` 0: the overlay aggregates by file
    /// type across chats, so per-chat splits are not needed (schema
    /// 1.8.67 line 15781).
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
        match self.sender.send_json(&get_storage_statistics(extra, 0)) {
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
        let _ = self.download_file(file_id, USER_DOWNLOAD_PRIORITY, false);
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

    /// Send `resendAuthenticationCode` (reason: user request) when auth is
    /// WaitCode. No local cooldown is invented: a too-early resend fails
    /// server-side (429) and surfaces through `last_auth_error`.
    pub fn resend_code(&mut self) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitCode { .. }) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self
            .session
            .request(RequestPurpose::ResendAuthenticationCode, None);
        self.sender.send_json(&resend_authentication_code(extra))?;
        Ok(extra)
    }

    /// Send `requestQrCodeAuthentication` when auth is WaitPhoneNumber.
    /// TDLib answers with `updateAuthorizationState` carrying
    /// `authorizationStateWaitOtherDeviceConfirmation` (with the QR link).
    pub fn request_qr_login(&mut self) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitPhoneNumber) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self
            .session
            .request(RequestPurpose::RequestQrCodeAuthentication, None);
        self.sender
            .send_json(&request_qr_code_authentication(extra))?;
        Ok(extra)
    }

    /// Slice A2: shared send path for every 2FA management request. All
    /// five answer `passwordState`; one request is in flight at a time
    /// so double-clicks can't double-send a password change. Passwords
    /// are never stored on the session or diagnostics — they ride the
    /// request JSON only.
    fn password_op_send(
        &mut self,
        op: PasswordOp,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.password_state_loading {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.password_op_error = None;
        let extra = self
            .session
            .request(RequestPurpose::PasswordStateOp { op }, None);
        self.session.password_state_loading = true;
        match self.sender.send_json(&build(extra)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.password_state_loading = false;
                Err(err)
            }
        }
    }

    /// Slice A2: send `getPasswordState` (schema 1.8.67, line 11426).
    /// Cached state is reused and an in-flight fetch is never
    /// duplicated; `password_op_send` enforces the connection gate.
    /// `Ok(None)` = no request needed.
    pub fn fetch_password_state(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if self.session.password_state.is_some() || self.session.password_state_loading {
            return Ok(None);
        }
        self.password_op_send(PasswordOp::Fetch, get_password_state)
            .map(Some)
    }

    /// Slice A2: send `setPassword` (schema 1.8.67, line 11434) — enable
    /// (empty `old_password`, the TGX MODE_NEW convention), change, or
    /// disable (empty `new_password`). `recovery_email` is sent in the
    /// same call on first-time enable, like TGX's password controller;
    /// otherwise `None`. Not trimmed: spaces can be significant.
    pub fn set_two_step_password(
        &mut self,
        old_password: &str,
        new_password: &str,
        new_hint: &str,
        recovery_email: Option<&str>,
    ) -> Result<RequestId, ConnectSendError> {
        let op = if new_password.is_empty() {
            PasswordOp::DisablePassword
        } else {
            PasswordOp::SetPassword
        };
        // Change/disable need the current password. A doomed request is
        // rejected before it leaves; TDLib validates the rest honestly.
        if new_password.is_empty() && old_password.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.password_op_send(op, |extra| {
            set_password(extra, old_password, new_password, new_hint, recovery_email)
        })
    }

    /// Slice A2: send `setRecoveryEmailAddress` (schema 1.8.67, line
    /// 11458). Requires the current two-step password; the change is
    /// not applied until the new address is confirmed.
    pub fn set_recovery_email(
        &mut self,
        password: &str,
        email: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if password.is_empty() || email.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.password_op_send(PasswordOp::SetRecoveryEmail, |extra| {
            set_recovery_email_address(extra, password, email)
        })
    }

    /// Slice A2: send `resendRecoveryEmailAddressCode` (schema 1.8.67,
    /// line 11464). Only meaningful while an email confirmation is
    /// pending; TDLib enforces its own server-side cooldown (429 on
    /// too-early resend), so no local countdown is invented.
    pub fn resend_recovery_email_code(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self
            .session
            .password_state
            .as_ref()
            .is_some_and(|s| s.pending_email_pattern.is_some())
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.password_op_send(PasswordOp::ResendCode, resend_recovery_email_address_code)
    }

    /// Slice A2: send `cancelRecoveryEmailAddressVerification` (schema
    /// 1.8.67, line 11467). Only meaningful while an email confirmation
    /// is pending.
    pub fn cancel_recovery_email_setup(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self
            .session
            .password_state
            .as_ref()
            .is_some_and(|s| s.pending_email_pattern.is_some())
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.password_op_send(
            PasswordOp::AbortEmailSetup,
            cancel_recovery_email_address_verification,
        )
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
        leaving_reply: Option<SendReply>,
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
        leaving_reply: Option<SendReply>,
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
        leaving_reply: Option<SendReply>,
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

    /// Slice media-shared-gallery: open the gallery for the open chat and
    /// fetch the active tab's first page. Returns `false` when there is no
    /// open chat to gallery-ize.
    pub fn open_shared_media(&mut self) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Ok(false);
        };
        // Reopening the gallery for the same chat keeps the already-fetched
        // tabs (`SharedMediaState::open_for`); only a fresh open fetches the
        // active tab.
        let reopening =
            self.session.shared_media.open && self.session.shared_media.chat_id == Some(chat_id);
        let tab = self.session.shared_media.open_for(chat_id);
        if !reopening {
            self.fetch_shared_media(tab)?;
        }
        Ok(true)
    }

    pub fn close_shared_media(&mut self) {
        self.session.shared_media.close();
    }

    /// Slice media-shared-gallery: switch tabs; fetch only tabs that were
    /// never fetched (each tab caches its first page).
    pub fn select_shared_media_tab(&mut self, tab: SharedMediaTab) -> Result<(), ConnectSendError> {
        if self.session.shared_media.select_tab(tab) {
            self.fetch_shared_media(tab)?;
        }
        Ok(())
    }

    /// Slice media-shared-gallery: one `searchChatMessages` page with the
    /// tab's `searchMessagesFilter*` filter (`schema/td_api.tl:11864`).
    pub fn fetch_shared_media(&mut self, tab: SharedMediaTab) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.shared_media.chat_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let generation = self.session.shared_media.begin_fetch(tab);
        let extra = self.session.request(
            RequestPurpose::GetSharedMedia { tab, generation },
            Some(chat_id),
        );
        let filter = search_messages_filter_json(tab.filter_constructor());
        match self.sender.send_json(&search_chat_messages(
            extra,
            chat_id,
            &TopicId::None,
            "",
            MessageId(0),
            0,
            SHARED_MEDIA_PAGE_SIZE,
            Some(filter),
        )) {
            Ok(()) => Ok(()),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.shared_media.fail(
                    chat_id,
                    tab,
                    generation,
                    "Could not send the shared-media request.".to_string(),
                );
                Err(err)
            }
        }
    }

    /// Slice media-shared-gallery: gallery row click — close the gallery and
    /// jump to the message with the same history-around pipeline in-chat
    /// search jumps use (`jump_to_replied_message` does the same).
    pub fn jump_to_shared_media_item(
        &mut self,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.shared_media.close();
        self.jump_to_chat_search_message(message_id)
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
            None,
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

/// Slice A4: which flag a session-row toggle flips (module scope — Rust
/// forbids enums inside `impl`). `ponytail:` a two-case enum plus one
/// shared send path beats two near-duplicate driver methods; upgrade
/// only if more per-session flags land.
#[derive(Debug, Clone, Copy)]
enum ToggleSessionKind {
    SecretChats,
    Calls,
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
    // Phase C2i: local call prefs (confirm-before-calling, less-data)
    // are loaded once here; the UI saves them back on toggle.
    let mut session = session;
    session.call_prefs = load_call_prefs(&prepared.paths);
    // MED1: local media prefs (remember-media-grouping) load the same way.
    session.media_prefs = load_media_prefs(&prepared.paths);
    // Slice A6: local contacts prefs (sync toggle) load the same way.
    session.contact_prefs = load_contact_prefs(&prepared.paths);
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
    use crate::telegram::envelope::ParsedSession;
    use crate::telegram::envelope::ParsedWebsite;
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

    /// Bots slice: `ConnectDriver::inline_query` — a first page sets the
    /// slot to `Loading` and sends one `getInlineQueryResults`, a second
    /// call while in flight is a no-op that leaves the slot alone, and a
    /// first-page send failure rolls the slot back to `None`.
    #[test]
    fn driver_inline_query_first_page_sends_dedupes_and_rolls_back() {
        type InlineQueryDriverHarness = (
            std::path::PathBuf,
            ConnectDriver<Arc<RecordingSender>>,
            Arc<RecordingSender>,
        );

        fn inline_query_driver() -> InlineQueryDriverHarness {
            let store = MemorySecretStore::new();
            let (dir, prepared) = prepared_tmp(&store);
            let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
            let sender = Arc::new(RecordingSender::new());
            let session = Session::new(AccountKey::primary(), sink.clone());
            let mut driver =
                ConnectDriver::new(session, sender.clone(), test_credentials(), prepared);
            let seq = AtomicU64::new(0);
            driver
                .ingest(
                    copy_and_parse(
                        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                        &seq,
                        &sink,
                    )
                    .unwrap(),
                )
                .unwrap();
            (dir, driver, sender)
        }

        fn failing_inline_query_driver() -> (
            std::path::PathBuf,
            ConnectDriver<Arc<FailFirstInlineQuerySender>>,
        ) {
            let store = MemorySecretStore::new();
            let (dir, prepared) = prepared_tmp(&store);
            let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
            let sender = Arc::new(FailFirstInlineQuerySender::new());
            let session = Session::new(AccountKey::primary(), sink.clone());
            let mut driver = ConnectDriver::new(session, sender, test_credentials(), prepared);
            let seq = AtomicU64::new(0);
            driver
                .ingest(
                    copy_and_parse(
                        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                        &seq,
                        &sink,
                    )
                    .unwrap(),
                )
                .unwrap();
            (dir, driver)
        }

        let (dir, mut driver, sender) = inline_query_driver();

        // (a) first page: slot → Loading, one getInlineQueryResults sent.
        let extra = driver
            .inline_query(77, ChatId(1), "@gif cats", "")
            .expect("first page")
            .expect("request id");
        match &driver.session.inline_query {
            Some(slot) => {
                assert_eq!(slot.chat_id, ChatId(1));
                assert_eq!(slot.bot_user_id, 77);
                assert_eq!(slot.query, "@gif cats");
                assert!(matches!(slot.fetch, InlineQueryFetch::Loading));
            }
            None => panic!("inline query slot missing"),
        }
        let inline_query_sends = || {
            sender
                .snapshot()
                .into_iter()
                .filter(|json| json.contains("getInlineQueryResults"))
                .map(|json| serde_json::from_str::<Value>(&json).unwrap())
                .collect::<Vec<_>>()
        };
        let first_page = inline_query_sends();
        assert_eq!(first_page.len(), 1);
        assert_eq!(first_page[0]["@type"], "getInlineQueryResults");
        assert_eq!(first_page[0]["@extra"], extra.0.to_string());
        assert_eq!(first_page[0]["bot_user_id"], 77);
        assert_eq!(first_page[0]["chat_id"], 1);
        assert_eq!(first_page[0]["query"], "@gif cats");
        assert_eq!(first_page[0]["offset"], "");

        // (b) in flight: the second call no-ops and the slot is untouched.
        assert_eq!(
            driver.inline_query(77, ChatId(1), "@gif cats", ""),
            Ok(None)
        );
        assert!(matches!(
            driver.session.inline_query.as_ref().map(|slot| &slot.fetch),
            Some(InlineQueryFetch::Loading)
        ));
        assert_eq!(inline_query_sends().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);

        // (c) a first-page send failure rolls the slot back to None.
        let (dir2, mut driver2) = failing_inline_query_driver();
        let failed = driver2.inline_query(77, ChatId(1), "@gif cats", "");
        assert!(matches!(failed, Err(ConnectSendError::Native)));
        assert!(driver2.session.inline_query.is_none());
        let _ = std::fs::remove_dir_all(&dir2);
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
            "CANARYSECRETSEND",
        );
        let extra = driver.send_text_snapshot(&snap).unwrap();
        let sent = recorder.snapshot();
        let send_json = sent.last().unwrap();
        assert!(send_json.contains("sendMessage"));
        assert!(send_json.contains("\"chat_id\":31"));
        assert!(send_json.contains("CANARYSECRETSEND"));
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
    fn driver_resend_code_and_qr_login_only_in_matching_states() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);

        // Both actions are gated: nothing valid to do in the initial state.
        assert_eq!(driver.resend_code(), Err(ConnectSendError::InvalidRequest));
        assert_eq!(
            driver.request_qr_login(),
            Err(ConnectSendError::InvalidRequest)
        );

        let wait_phone = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_phone).unwrap();
        // Resend needs WaitCode; QR login needs WaitPhoneNumber.
        assert_eq!(driver.resend_code(), Err(ConnectSendError::InvalidRequest));
        let qr_extra = driver.request_qr_login().unwrap();
        let sent = recorder.snapshot();
        let qr_json = sent.last().unwrap();
        assert!(qr_json.contains("requestQrCodeAuthentication"));
        assert!(qr_json.contains("\"other_user_ids\":[]"));
        assert!(qr_json.contains(&format!("\"@extra\":\"{}\"", qr_extra.0)));

        let wait_code = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitCode","code_info":{"@type":"authenticationCodeInfo","type":{"@type":"authenticationCodeTypeSms","length":5}}}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_code).unwrap();
        assert_eq!(
            driver.request_qr_login(),
            Err(ConnectSendError::InvalidRequest)
        );
        let resend_extra = driver.resend_code().unwrap();
        let sent = recorder.snapshot();
        let resend_json = sent.last().unwrap();
        assert!(resend_json.contains("resendAuthenticationCode"));
        assert!(resend_json.contains("resendCodeReasonUserRequest"));
        assert!(resend_json.contains(&format!("\"@extra\":\"{}\"", resend_extra.0)));

        // The QR link from the auth update lands on the session state and is
        // never written to diagnostics.
        let wait_qr = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitOtherDeviceConfirmation","link":"tg://login/?token=unit-test-token"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_qr).unwrap();
        assert!(matches!(
            &driver.session.auth,
            AuthorizationState::WaitOtherDeviceConfirmation { link }
            if link == "tg://login/?token=unit-test-token"
        ));
        assert!(!sink.rendered().contains("unit-test-token"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice A2: the 2FA driver gates sends, dedupes the fetch, shapes
    /// `setPassword` correctly, and never leaks passwords into
    /// diagnostics.
    #[test]
    fn driver_two_step_password_ops_gate_dedupe_and_shape() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);

        // Gated before the chats path is active.
        assert_eq!(
            driver.fetch_password_state(),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(
            driver.set_two_step_password("", "s3cret", "", None),
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

        // Fetch sends `getPasswordState` once; a second fetch dedupes.
        let fetch_extra = driver.fetch_password_state().unwrap().unwrap();
        assert_eq!(driver.fetch_password_state(), Ok(None));
        let sent = recorder.snapshot();
        let fetch_json = sent.last().unwrap();
        assert!(fetch_json.contains("\"getPasswordState\""));
        assert!(fetch_json.contains(&format!("\"@extra\":\"{}\"", fetch_extra.0)));

        // Doomed requests are rejected before leaving: disable needs the
        // current password; recovery email needs password + address.
        assert_eq!(
            driver.set_two_step_password("", "", "", None),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(
            driver.set_recovery_email("", "me@example.com"),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(
            driver.set_recovery_email("s3cret", ""),
            Err(ConnectSendError::InvalidRequest)
        );
        // One op in flight: a second send is rejected (no double-send).
        assert_eq!(
            driver.set_two_step_password("", "s3cret", "hint", Some("me@example.com")),
            Err(ConnectSendError::InvalidRequest)
        );

        // The `passwordState` answer lands on the session, clears loading,
        // and the password never reaches diagnostics.
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":false,"password_hint":"","has_recovery_email_address":false,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        fetch_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(driver.session.password_state.is_some());
        assert!(!driver.session.password_state_loading);

        // Enable: empty old password, email in the same call.
        let enable_extra = driver
            .set_two_step_password("", "s3cret", "hint", Some("me@example.com"))
            .unwrap();
        let sent = recorder.snapshot();
        let enable_json: serde_json::Value = serde_json::from_str(sent.last().unwrap()).unwrap();
        assert_eq!(enable_json["@type"], "setPassword");
        assert_eq!(enable_json["old_password"], "");
        assert_eq!(enable_json["new_password"], "s3cret");
        assert_eq!(enable_json["new_hint"], "hint");
        assert_eq!(enable_json["set_recovery_email_address"], true);
        assert_eq!(enable_json["new_recovery_email_address"], "me@example.com");
        assert_eq!(
            enable_json["@extra"],
            serde_json::Value::String(enable_extra.0.to_string())
        );

        // Answer the enable: password now set, recovery email confirmed.
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":true,"password_hint":"hint","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        enable_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(
            driver
                .session
                .password_state
                .as_ref()
                .is_some_and(|s| s.has_password)
        );

        // Resend/abort are meaningless without a pending confirmation.
        assert_eq!(
            driver.resend_recovery_email_code(),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(
            driver.cancel_recovery_email_setup(),
            Err(ConnectSendError::InvalidRequest)
        );

        // New recovery email → pending confirmation state.
        let email_extra = driver
            .set_recovery_email("s3cret", "new@example.com")
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":true,"password_hint":"hint","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":{{"@type":"emailAddressAuthenticationCodeInfo","email_address_pattern":"n***@example.com","length":6}},"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        email_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            driver
                .session
                .password_state
                .as_ref()
                .and_then(|s| s.pending_email_pattern.clone())
                .as_deref(),
            Some("n***@example.com")
        );

        // Now resend and abort send their requests.
        let resend_extra = driver.resend_recovery_email_code().unwrap();
        let sent = recorder.snapshot();
        assert!(
            sent.last()
                .unwrap()
                .contains("resendRecoveryEmailAddressCode")
        );
        // One in flight blocks the abort until the resend answers.
        assert_eq!(
            driver.cancel_recovery_email_setup(),
            Err(ConnectSendError::InvalidRequest)
        );
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":true,"password_hint":"hint","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        resend_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        // Pending again → abort sends `cancelRecoveryEmailAddressVerification`.
        let email_extra2 = driver
            .set_recovery_email("s3cret", "new@example.com")
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":true,"password_hint":"hint","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":{{"@type":"emailAddressAuthenticationCodeInfo","email_address_pattern":"n***@example.com","length":6}},"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        email_extra2.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.cancel_recovery_email_setup().unwrap();
        let sent = recorder.snapshot();
        assert!(
            sent.last()
                .unwrap()
                .contains("cancelRecoveryEmailAddressVerification")
        );

        // Passwords ride request JSON only — never diagnostics.
        for token in ["s3cret", "me@example.com", "new@example.com"] {
            assert!(
                !sink.rendered().contains(token),
                "secret leaked to diagnostics: {token}"
            );
        }

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
            &format!(
                r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{id}"}}"#,
                id = extra
            ),
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
            "CANARYSENDping",
        );
        let send_extra = driver.send_text_snapshot(&snap).unwrap();
        let sent = recorder.snapshot();
        let send_json = sent.last().unwrap();
        assert!(send_json.contains("sendMessage"));
        assert!(send_json.contains("\"topic_id\":null"));
        assert!(send_json.contains("CANARYSENDping"));
        assert!(send_json.contains(&format!("\"@extra\":\"{}\"", send_extra.0)));
        assert!(!sink.rendered().contains("CANARYSEND"));

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
        assert!(!sink.rendered().contains("CANARYSEND"));

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
            "CANARYTOPICping",
        );
        let send_extra = driver.send_text_snapshot(&snap).unwrap();
        let sent = recorder.snapshot();
        let send_json = sent.last().unwrap();
        let v: Value = serde_json::from_str(send_json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["chat_id"], 16);
        assert_eq!(v["topic_id"]["@type"], "messageTopicForum");
        assert_eq!(v["topic_id"]["forum_topic_id"], 2);
        assert!(send_json.contains("CANARYTOPICping"));
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
            "CANARYADMINpost",
        );
        let send_extra = driver.send_text_snapshot(&snap).unwrap();
        let sent = recorder.snapshot();
        let send_json = sent.last().unwrap();
        assert!(send_json.contains("sendMessage"));
        assert!(send_json.contains("\"chat_id\":9"));
        assert!(send_json.contains("CANARYADMINpost"));
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
    fn a6_remove_contact_sends_and_invalidates() {
        // Slice A6: `remove_contact` sends `removeContacts([user_id])`
        // (schema 1.8.67, line 14528); the `ok` answer invalidates the
        // contacts list and records the notice — never optimistic.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        driver.session.contacts = Some(vec![31]);
        let extra = driver
            .remove_contact(31)
            .unwrap()
            .expect("removeContacts sent");
        let sent = recorder.snapshot();
        let remove = sent
            .iter()
            .find(|j| j.contains(r#""@type":"removeContacts""#))
            .expect("removeContacts in outbox");
        assert!(remove.contains(r#""user_ids":[31]"#));
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
        assert_eq!(
            driver.session.contacts_notice.as_deref(),
            Some("Contact deleted.")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a6_delete_synced_contacts_clears_then_removes() {
        // Slice A6: `delete_synced_contacts` sends `clearImportedContacts`
        // first (the server-side wipe, schema 1.8.67 line 14539), then
        // `removeContacts` for the cached ids (TGX `deleteContacts`).
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        driver.session.contacts = Some(vec![31, 32]);
        let sent_count = driver.delete_synced_contacts().unwrap();
        assert_eq!(sent_count, 2);
        let sent = recorder.snapshot();
        let clear_pos = sent
            .iter()
            .position(|j| j.contains(r#""@type":"clearImportedContacts""#))
            .expect("clearImportedContacts in outbox");
        let remove_pos = sent
            .iter()
            .position(|j| j.contains(r#""@type":"removeContacts""#))
            .expect("removeContacts in outbox");
        assert!(clear_pos < remove_pos);
        assert!(sent[remove_pos].contains(r#""user_ids":[31,32]"#));

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
    fn cl1_mark_chat_as_read_sends_view_plus_untoggle() {
        // Slice CL1: "Mark as read" follows Telegram X
        // (`Tdlib.markChatAsRead` with `MessageSourceChatList`) —
        // `viewMessages` over the newest known message reads real
        // unread history, and the manual marked-unread flag is cleared.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = ViewCtlSender::new();
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        ready_private_chat(&mut driver, &seq, &dyn_sink);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatIsMarkedAsUnread","chat_id":7,"is_marked_as_unread":true}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        let sent = driver.mark_chat_as_read(ChatId(7)).expect("send");
        assert!(sent.is_some(), "something to read");
        let snapshot = recorder.snapshot();
        let view = snapshot
            .iter()
            .find(|j| j.contains("\"viewMessages\""))
            .expect("viewMessages sent");
        let view_value: Value = serde_json::from_str(view).unwrap();
        assert_eq!(view_value["source"]["@type"], "messageSourceChatList");
        assert_eq!(view_value["message_ids"], serde_json::json!([11]));
        assert_eq!(view_value["force_read"], true);
        let untoggle = snapshot
            .iter()
            .find(|j| j.contains("\"toggleChatIsMarkedAsUnread\""))
            .expect("untoggle sent");
        let untoggle_value: Value = serde_json::from_str(untoggle).unwrap();
        assert_eq!(untoggle_value["is_marked_as_unread"], false);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cl1_mark_chat_as_read_noop_when_nothing_unread() {
        // Slice CL1: "Mark as read" on a fully-read chat sends nothing
        // (honest noop, like TGX skipping when there is nothing to do).
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = ViewCtlSender::new();
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        ready_private_chat(&mut driver, &seq, &dyn_sink);
        // ready_private_chat seeds unread_count=1; mark it read first.
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":11,"unread_count":0}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        assert_eq!(driver.mark_chat_as_read(ChatId(7)).expect("send"), None);
        assert!(
            recorder
                .snapshot()
                .iter()
                .all(|j| !j.contains("viewMessages") && !j.contains("toggleChatIsMarkedAsUnread")),
            "noop must not send read requests"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cl2_set_pinned_chat_order_sends_full_list_and_rolls_back() {
        // Slice CL2: the pin drag reorder sends the full reordered id
        // list via `setPinnedChats` (TGX `ChatsAdapter.movePinnedChat`
        // semantics), applies optimistically, and restores the old
        // order on refusal.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = ViewCtlSender::new();
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        ready_private_chat(&mut driver, &seq, &dyn_sink);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":9},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        for (id, order) in [(7i64, 300i64), (9, 200)] {
            let chat = driver.session.chats.get_mut(&id).expect("chat");
            chat.in_main_list = true;
            chat.is_pinned = true;
            chat.order = order;
        }

        let sent = driver
            .set_pinned_chat_order(false, vec![9, 7])
            .expect("send");
        assert!(sent.is_some(), "order actually changed");
        let snapshot = recorder.snapshot();
        let set_pinned = snapshot
            .iter()
            .find(|j| j.contains("\"setPinnedChats\""))
            .expect("setPinnedChats sent");
        let set_pinned_value: Value = serde_json::from_str(set_pinned).unwrap();
        assert_eq!(set_pinned_value["chat_list"]["@type"], "chatListMain");
        assert_eq!(set_pinned_value["chat_ids"], serde_json::json!([9, 7]));
        // Optimistic: the model order flipped before the answer.
        assert_eq!(driver.session.chats.get(&9).expect("chat").order, 300);
        assert_eq!(driver.session.chats.get(&7).expect("chat").order, 200);

        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
                        sent.unwrap().0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.chats.get(&7).expect("chat").order, 300);
        assert_eq!(driver.session.chats.get(&9).expect("chat").order, 200);
        assert_eq!(
            driver.session.chat_action_error.as_deref(),
            Some("could not reorder pinned chats (error 400)")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cl2_set_pinned_chat_order_id_mismatch_is_noop() {
        // Slice CL2: a drag order whose id set doesn't match the
        // current pins sends nothing — `setPinnedChats` requires the
        // complete list.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = ViewCtlSender::new();
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        ready_private_chat(&mut driver, &seq, &dyn_sink);
        let chat = driver.session.chats.get_mut(&7).expect("chat");
        chat.in_main_list = true;
        chat.is_pinned = true;

        assert_eq!(
            driver
                .set_pinned_chat_order(false, vec![7, 999])
                .expect("send"),
            None
        );
        assert!(
            recorder
                .snapshot()
                .iter()
                .all(|j| !j.contains("setPinnedChats")),
            "mismatched order must not send"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cl2_mark_all_chats_as_read_sends_read_chat_list() {
        // Slice CL2: \"Mark all as read\" sends `readChatList`; badges
        // clear via the server updates, nothing is faked locally.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = ViewCtlSender::new();
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        ready_private_chat(&mut driver, &seq, &dyn_sink);
        driver.session.chats.get_mut(&7).expect("chat").in_main_list = true;

        let sent = driver.mark_all_chats_as_read(false).expect("send");
        assert!(sent.is_some(), "chat 7 is unread");
        let snapshot = recorder.snapshot();
        let read_list = snapshot
            .iter()
            .find(|j| j.contains("\"readChatList\""))
            .expect("readChatList sent");
        let read_list_value: Value = serde_json::from_str(read_list).unwrap();
        assert_eq!(read_list_value["chat_list"]["@type"], "chatListMain");
        // Second call while the first is in flight is a no-op.
        assert_eq!(driver.mark_all_chats_as_read(false).expect("send"), None);
        assert_eq!(
            snapshot
                .iter()
                .filter(|j| j.contains("\"readChatList\""))
                .count(),
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cl2_mark_all_chats_as_read_noop_when_all_read() {
        // Slice CL2: no unread chats means no `readChatList` — honest
        // no-op.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = ViewCtlSender::new();
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        ready_private_chat(&mut driver, &seq, &dyn_sink);
        let chat = driver.session.chats.get_mut(&7).expect("chat");
        chat.in_main_list = true;
        chat.unread_count = 0;

        assert_eq!(driver.mark_all_chats_as_read(false).expect("send"), None);
        assert!(
            recorder
                .snapshot()
                .iter()
                .all(|j| !j.contains("readChatList")),
            "all-read must not send"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cl2_clear_recently_found_chats_optimistic_clear() {
        // Slice CL2: \"Clear recents\" sends `clearRecentlyFoundChats`
        // and clears the local empty-search recents immediately (TGX
        // `SearchManager` clears locally too).
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = ViewCtlSender::new();
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        ready_private_chat(&mut driver, &seq, &dyn_sink);
        driver.session.search.recents = true;
        driver.session.search.chat_ids = vec![ChatId(7)];

        let sent = driver.clear_recently_found_chats().expect("send");
        assert!(sent.is_some(), "recents were non-empty");
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("\"clearRecentlyFoundChats\"")),
            "clearRecentlyFoundChats sent"
        );
        assert!(driver.session.search.chat_ids.is_empty());
        // A refusal surfaces — the next recents fetch restores truth.
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","@extra":"{}","code":500,"message":"CLEAR_FAILED"}}"#,
                        sent.unwrap().0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            driver.session.chat_action_error.as_deref(),
            Some("could not clear recent searches (error 500)")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cl2_archive_chat_list_settings_fetch_and_set() {
        // Slice CL2: `getArchiveChatListSettings` fills the session
        // cache; a toggle sends `setArchiveChatListSettings` with the
        // three schema fields and flips optimistically.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = ViewCtlSender::new();
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        ready_private_chat(&mut driver, &seq, &dyn_sink);

        let sent = driver.fetch_archive_chat_list_settings().expect("send");
        assert!(sent.is_some(), "settings not fetched yet");
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("\"getArchiveChatListSettings\"")),
            "getArchiveChatListSettings sent"
        );
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"archiveChatListSettings","@extra":"{}","archive_and_mute_new_chats_from_unknown_users":true,"keep_unmuted_chats_archived":false,"keep_chats_from_folders_archived":true}}"#,
                        sent.unwrap().0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(driver.session.archive_chat_list_settings.is_some());
        assert!(!driver.session.archive_settings_loading);

        let mut next = driver.session.archive_chat_list_settings.expect("fetched");
        next.keep_unmuted_chats_archived = true;
        let sent = driver.set_archive_chat_list_settings(next).expect("send");
        assert!(sent.is_some(), "a field actually changed");
        let snapshot = recorder.snapshot();
        let set = snapshot
            .iter()
            .find(|j| j.contains("\"setArchiveChatListSettings\""))
            .expect("setArchiveChatListSettings sent");
        let set_value: Value = serde_json::from_str(set).unwrap();
        assert_eq!(set_value["settings"]["keep_unmuted_chats_archived"], true);
        assert!(
            driver
                .session
                .archive_chat_list_settings
                .expect("cached")
                .keep_unmuted_chats_archived,
            "optimistic flip applied"
        );
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","@extra":"{}","code":400,"message":"ARCHIVE_SETTINGS_INVALID"}}"#,
                        sent.unwrap().0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(
            !driver
                .session
                .archive_chat_list_settings
                .expect("cached")
                .keep_unmuted_chats_archived,
            "refusal restored the old settings"
        );
        assert_eq!(
            driver.session.chat_action_error.as_deref(),
            Some("could not save archive settings (error 400)")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cl2_create_private_chat_answer_opens_through_select_chat() {
        // Slice CL2: the `createPrivateChat` answer is a bare `chat`
        // object (schema 1.8.67, line 13312), parsed as `UpdateNewChat`.
        // The driver opens it through the normal `select_chat` flow —
        // `openChat` + history — never by poking `session.open_chat`.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = ViewCtlSender::new();
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        ready_private_chat(&mut driver, &seq, &dyn_sink);
        driver.session.my_user_id = Some(777);

        let extra = driver
            .create_private_chat_with_self()
            .expect("send")
            .expect("request sent");
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("\"createPrivateChat\"")),
            "createPrivateChat sent"
        );
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chat","@extra":"{}","id":777001,"title":"Saved Messages","type":{{"@type":"chatTypePrivate","user_id":777}},"unread_count":0}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.open_chat, Some(ChatId(777001)));
        assert!(
            driver.session.chats.contains_key(&777001),
            "created chat inserted into the model"
        );
        let snapshot = recorder.snapshot();
        assert!(
            snapshot.iter().any(|j| {
                let v: Value = serde_json::from_str(j).unwrap();
                v["@type"] == "openChat" && v["chat_id"] == 777001
            }),
            "openChat sent for the created chat"
        );
        assert!(
            snapshot.iter().any(|j| {
                let v: Value = serde_json::from_str(j).unwrap();
                v["@type"] == "getChatHistory" && v["chat_id"] == 777001
            }),
            "history requested for the created chat"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cl2_create_private_chat_refusal_opens_nothing() {
        // Slice CL2: a refused `createPrivateChat` must not open
        // anything — the refusal surfaces on the chat-action error
        // line instead of silently doing nothing.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = ViewCtlSender::new();
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        ready_private_chat(&mut driver, &seq, &dyn_sink);
        driver.session.my_user_id = Some(777);

        let extra = driver
            .create_private_chat_with_self()
            .expect("send")
            .expect("request sent");
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_CREATE_FAILED"}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.open_chat, None, "nothing opened on refusal");
        assert_eq!(
            driver.session.chat_action_error.as_deref(),
            Some("could not open Saved Messages (error 400)")
        );
        assert!(
            !recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("\"openChat\"")),
            "no openChat sent on refusal"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cl1_remove_chat_from_list_sends_delete_history() {
        // Slice CL1: chat-list "Delete chat" is `deleteChatHistory`
        // with `remove_from_chat_list: true` (Telegram X
        // `Tdlib.deleteChat`), never the destructive `deleteChat`.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = ViewCtlSender::new();
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
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Old","type":{"@type":"chatTypePrivate","user_id":9},"unread_count":0,"can_be_deleted_only_for_self":true,"can_be_deleted_for_all_users":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        driver
            .remove_chat_from_list(ChatId(9))
            .expect("send")
            .expect("removable");
        let delete = recorder
            .snapshot()
            .into_iter()
            .find(|j| j.contains("deleteChatHistory"))
            .expect("deleteChatHistory sent");
        let value: Value = serde_json::from_str(&delete).unwrap();
        assert_eq!(value["@type"], "deleteChatHistory");
        assert_eq!(value["chat_id"], 9);
        assert_eq!(value["remove_from_chat_list"], true);
        assert_eq!(value["revoke"], false);
        assert!(
            !delete.contains("\"deleteChat\""),
            "must not use the destructive deleteChat constructor"
        );
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
            .send_voice_note(&draft, "", Some(SendReply::plain(MessageId(4))))
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
    fn photo_history_auto_downloads_thumb_and_full_photo() {
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
        // MED3: the full photo auto-downloads too when the photo flag is on
        // (TGX auto-download), at the auto-media priority below explicit
        // user downloads.
        let full_req = sent
            .iter()
            .rev()
            .find(|j| j.contains("downloadFile") && j.contains("\"file_id\":2"))
            .expect("auto full-photo downloadFile");
        let full_json: Value = serde_json::from_str(full_req).unwrap();
        assert_eq!(full_json["priority"], AUTO_MEDIA_DOWNLOAD_PRIORITY);
        assert_eq!(full_json["synchronous"], false);
        // A user open while the auto download is in flight dedupes instead
        // of re-requesting.
        assert_eq!(
            driver.download_file(FileId(2), USER_DOWNLOAD_PRIORITY, true),
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
            "CANARYPHOTOCAP",
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
            "CANARYPHOTOCAP"
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
            "CANARYVIDEOCAP",
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
            "CANARYVIDEOCAP"
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
            "CANARYALBUMCAP",
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
        assert_eq!(contents[1]["caption"]["text"], "CANARYALBUMCAP");
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
            "CANARYREPLYtext",
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
        assert!(send_json.contains("CANARYREPLYtext"));

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARYREPLYtext","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":7,"message_id":50}}}"#,
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
        assert!(!sink.rendered().contains("CANARYREPLY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_send_snapshot_rejects_overlong_caption() {
        // MED4: `message_caption_length_max` (runtime `updateOption`)
        // gates media captions before any file work; the limit rides
        // the error so the UI can show it.
        use crate::composer::{AttachmentKind, ComposerAttachment};

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
        driver.session.message_caption_length_max = 4;

        let snap = crate::composer::ComposerSnapshot::capture_with_attachment(
            ChatId(7),
            driver.session.view_generation,
            "toolong",
            Some(ComposerAttachment {
                path: std::path::PathBuf::from("/tmp/does-not-exist.png"),
                kind: AttachmentKind::Photo,
                file_name: "does-not-exist.png".to_string(),
            }),
        );
        let sent_before = recorder.snapshot().len();
        let err = driver.send_snapshot(&snap).unwrap_err();
        assert!(matches!(err, ConnectSendError::CaptionTooLong { limit: 4 }));
        // The gate runs before any file work or request — nothing new
        // went out.
        assert_eq!(recorder.snapshot().len(), sent_before);

        // At the limit the gate passes (the missing file then fails the
        // send — the gate is what this test pins).
        driver.session.message_caption_length_max = 7;
        let snap = crate::composer::ComposerSnapshot::capture_with_attachment(
            ChatId(7),
            driver.session.view_generation,
            "1234567",
            Some(ComposerAttachment {
                path: std::path::PathBuf::from("/tmp/does-not-exist.png"),
                kind: AttachmentKind::Photo,
                file_name: "does-not-exist.png".to_string(),
            }),
        );
        let err = driver.send_snapshot(&snap).unwrap_err();
        assert!(!matches!(err, ConnectSendError::CaptionTooLong { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_send_album_rejects_overlong_caption() {
        // MED4: the album path gates on `message_caption_length_max`
        // before any request — an overlong caption refuses with the
        // limit and emits nothing.
        use crate::composer::{AttachmentKind, ComposerAttachment};

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
        driver.session.message_caption_length_max = 4;

        let snap = crate::composer::ComposerSnapshot::capture_album(
            ChatId(7),
            driver.session.view_generation,
            "toolong",
            vec![
                ComposerAttachment {
                    path: std::path::PathBuf::from("/tmp/a.png"),
                    kind: AttachmentKind::Photo,
                    file_name: "a.png".to_string(),
                },
                ComposerAttachment {
                    path: std::path::PathBuf::from("/tmp/b.png"),
                    kind: AttachmentKind::Photo,
                    file_name: "b.png".to_string(),
                },
            ],
        );
        let sent_before = recorder.snapshot().len();
        let err = driver.send_album_snapshot(&snap).unwrap_err();
        assert!(matches!(err, ConnectSendError::CaptionTooLong { limit: 4 }));
        assert_eq!(recorder.snapshot().len(), sent_before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_edit_snapshot_rejects_overlong_caption() {
        // MED4: caption edits gate on `message_caption_length_max` too —
        // an overlong edit refuses with the limit and emits nothing.
        // The message must exist in history with a caption for the edit
        // path to reach the length gate.
        use crate::telegram::client::copy_and_parse;

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
        driver.session.message_caption_length_max = 4;

        // Inject an outgoing photo message with a caption into history.
        let msg_json = r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":7,"is_outgoing":true,"date":1700000000,"content":{"@type":"messagePhoto","photo":{"@type":"photo","has_stickers":false,"sizes":[]},"caption":{"@type":"formattedText","text":"hi","entities":[]},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}"#;
        let owned = copy_and_parse(msg_json, &seq, &dyn_sink).expect("parse msg");
        driver.ingest(owned).expect("ingest msg");

        let edit = crate::composer::ComposerEdit {
            chat_id: ChatId(7),
            message_id: crate::ids::MessageId(9),
            original_text: "hi".to_string(),
            kind: crate::composer::ComposerEditKind::Caption,
            scheduled: false,
            caption_above: false,
        };
        let sent_before = recorder.snapshot().len();
        let err = driver.edit_snapshot(&edit, "toolong").unwrap_err();
        assert!(matches!(err, ConnectSendError::CaptionTooLong { limit: 4 }));
        assert_eq!(recorder.snapshot().len(), sent_before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_instant_view_success_ingest() {
        // MED4: a `webPageInstantView` answer for a tracked
        // `GetWebPageInstantView` request populates `session.instant_view`
        // (the UI drains it into the reader). The URL rides
        // `instant_view_urls`, keyed by the request id.
        use crate::telegram::client::copy_and_parse;

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

        // Register a request like `open_instant_view` does.
        let extra = driver
            .session
            .request(RequestPurpose::GetWebPageInstantView, None);
        driver
            .session
            .instant_view_urls
            .insert(extra, "https://example.com/article".to_string());

        let json = format!(
            r#"{{"@type":"webPageInstantView","page_blocks":[],"@extra":"{id}"}}"#,
            id = extra.0,
        );
        let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse IV");
        driver.ingest(owned).expect("ingest IV");
        let iv = driver.session.instant_view.as_ref().expect("IV stored");
        assert_eq!(iv.url, "https://example.com/article");
        // A success is never stashed as a browser fallback.
        assert!(driver.session.instant_view_fallback_url.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_instant_view_error_falls_back() {
        // MED4: a TDLib error for `getWebPageInstantView` (e.g. 404 — no
        // Instant View for the page) stashes the URL for browser
        // fallback, never a fake reader.
        use crate::telegram::client::copy_and_parse;

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
            .session
            .request(RequestPurpose::GetWebPageInstantView, None);
        driver
            .session
            .instant_view_urls
            .insert(extra, "https://example.com/noiv".to_string());

        let json = format!(
            r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{id}"}}"#,
            id = extra.0,
        );
        let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse error");
        driver.ingest(owned).expect("ingest error");
        assert!(driver.session.instant_view.is_none());
        assert_eq!(
            driver.session.instant_view_fallback_url.as_deref(),
            Some("https://example.com/noiv")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_link_preview_prefetch_ingest() {
        // MED4b: a `linkPreview` answer for a tracked `GetLinkPreview`
        // request populates `session.composer_preview` (the chip reads
        // it); a 404 becomes "no link info" (`Some(None)`), never a
        // card. The URL rides `composer_preview_urls`, keyed by id.
        use crate::telegram::client::copy_and_parse;

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

        // Register a request like `request_composer_link_preview` does.
        let extra = driver.session.request(RequestPurpose::GetLinkPreview, None);
        driver
            .session
            .composer_preview_urls
            .insert(extra, "https://example.com/story".to_string());
        driver.session.composer_preview = Some(ComposerLinkPreview {
            url: "https://example.com/story".to_string(),
            preview: None,
        });

        let json = format!(
            r#"{{"@type":"linkPreview","url":"https://example.com/story","display_url":"example.com","site_name":"Example","title":"A short story","description":{{"@type":"formattedText","text":"Preview body","entities":[]}},"author":"","type":{{"@type":"linkPreviewTypeArticle"}},"has_large_media":true,"show_large_media":false,"show_media_above_description":false,"skip_confirmation":true,"show_above_text":false,"instant_view_version":0,"@extra":"{id}"}}"#,
            id = extra.0,
        );
        let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse preview");
        driver.ingest(owned).expect("ingest preview");
        let stored = driver
            .session
            .composer_preview
            .as_ref()
            .expect("preview stored");
        assert_eq!(stored.url, "https://example.com/story");
        let preview = stored
            .preview
            .as_ref()
            .expect("loaded")
            .as_ref()
            .expect("card");
        assert_eq!(preview.title, "A short story");
        assert_eq!(preview.description, "Preview body");
        assert!(preview.has_large_media);
        assert!(!preview.show_large_media);

        // A superseded URL's late answer must not clobber the chip.
        let stale = driver.session.request(RequestPurpose::GetLinkPreview, None);
        driver
            .session
            .composer_preview_urls
            .insert(stale, "https://example.com/old".to_string());
        let json = format!(
            r#"{{"@type":"linkPreview","url":"https://example.com/old","display_url":"example.com","site_name":"","title":"Old","description":{{"@type":"formattedText","text":"","entities":[]}},"author":"","type":{{"@type":"linkPreviewTypeArticle"}},"has_large_media":false,"show_large_media":false,"show_media_above_description":false,"skip_confirmation":false,"show_above_text":false,"instant_view_version":0,"@extra":"{id}"}}"#,
            id = stale.0,
        );
        let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse stale");
        driver.ingest(owned).expect("ingest stale");
        let stored = driver
            .session
            .composer_preview
            .as_ref()
            .expect("preview kept");
        assert_eq!(stored.url, "https://example.com/story");

        // 404 for the current URL → "no link info", never a card.
        let miss = driver.session.request(RequestPurpose::GetLinkPreview, None);
        driver
            .session
            .composer_preview_urls
            .insert(miss, "https://example.com/story".to_string());
        driver.session.composer_preview = Some(ComposerLinkPreview {
            url: "https://example.com/story".to_string(),
            preview: None,
        });
        let json = format!(
            r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{id}"}}"#,
            id = miss.0,
        );
        let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse 404");
        driver.ingest(owned).expect("ingest 404");
        let stored = driver
            .session
            .composer_preview
            .as_ref()
            .expect("state kept");
        assert_eq!(stored.url, "https://example.com/story");
        assert!(stored.preview.as_ref().expect("resolved").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_send_snapshot_carries_quote() {
        // Slice G1: a composer reply with a validated partial quote
        // reaches `sendMessage` as `inputTextQuote` (schema 1.8.67,
        // line 3056).
        use crate::composer::{ComposerReplyTo, QuoteSelection};

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
            "CANARYQUOTEtext",
        )
        .with_reply(Some(ComposerReplyTo::with_quote(
            ChatId(7),
            MessageId(50),
            "hello already here",
            QuoteSelection {
                text: "already".to_string(),
                position: 6,
            },
        )));
        let extra = driver.send_snapshot(&snap).unwrap();
        let send_json = recorder.snapshot().last().cloned().expect("sendMessage");
        let v: Value = serde_json::from_str(&send_json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
        assert_eq!(v["reply_to"]["message_id"], 50);
        assert_eq!(v["reply_to"]["quote"]["@type"], "inputTextQuote");
        assert_eq!(v["reply_to"]["quote"]["text"]["text"], "already");
        assert_eq!(v["reply_to"]["quote"]["position"], 6);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_fetch_basic_group_members_shape() {
        // Slice G1: `getBasicGroupFullInfo` (schema 1.8.67, line 11507)
        // for a basic group chat; the answer populates
        // `basic_group_members`.
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
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":3},"permissions":{"@type":"chatPermissions","can_send_basic_messages":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let extra = driver
            .fetch_basic_group_members(ChatId(9))
            .unwrap()
            .expect("request sent");
        let sent = recorder.snapshot().last().cloned().expect("request");
        let v: Value = serde_json::from_str(&sent).unwrap();
        assert_eq!(v["@type"], "getBasicGroupFullInfo");
        assert_eq!(v["basic_group_id"], 3);
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"basicGroupFullInfo","@extra":"{}","members":[{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":7}},"status":{{"@type":"chatMemberStatusMember"}}}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let fetch = driver.session.basic_group_members.get(&9).unwrap();
        match fetch {
            SupergroupMembersFetch::Loaded { members, .. } => assert_eq!(members.len(), 1),
            other => panic!("unexpected {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_set_chat_member_tag_gates_and_shape() {
        // Slice G1: `setChatMemberTag` (schema 1.8.67, line 13598) —
        // owner of a group may retitle; channels are rejected; tags
        // over 16 characters are rejected client-side.
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
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":3},"permissions":{"@type":"chatPermissions","can_send_basic_messages":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        // Not the owner → no request.
        assert!(
            driver
                .set_chat_member_tag(ChatId(9), 42, "boss")
                .unwrap()
                .is_none()
        );
        // Owner → sends `setChatMemberTag`.
        driver.session.my_user_id = Some(7);
        driver.session.chats.get_mut(&9).unwrap().my_member_status =
            Some(ChannelMemberStatus::Creator);
        let extra = driver
            .set_chat_member_tag(ChatId(9), 42, "boss")
            .unwrap()
            .expect("request sent");
        let sent = recorder.snapshot().last().cloned().expect("request");
        let v: Value = serde_json::from_str(&sent).unwrap();
        assert_eq!(v["@type"], "setChatMemberTag");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 9);
        assert_eq!(v["user_id"], 42);
        assert_eq!(v["tag"], "boss");
        // Over-long tag rejected without sending.
        assert!(
            driver
                .set_chat_member_tag(ChatId(9), 42, "this title is way too long")
                .is_err()
        );
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
            scheduled: false,
            caption_above: false,
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
            scheduled: false,
            caption_above: false,
        };
        assert_eq!(
            driver.edit_snapshot(&edit, "   "),
            Err(ConnectSendError::InvalidRequest)
        );
        let extra = driver.edit_snapshot(&edit, "CANARYEDITtext").unwrap();
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
        assert_eq!(v["input_message_content"]["text"]["text"], "CANARYEDITtext");
        assert_eq!(v["input_message_content"]["clear_draft"], false);

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageContent","chat_id":7,"message_id":60,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARYEDITtext","entities":[]}}}"#,
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
            "CANARYEDITtext"
        );
        assert!(!sink.rendered().contains("CANARYEDIT"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_edit_scheduled_message_uses_scheduled_list() {
        use crate::composer::{ComposerEdit, ComposerEditKind};
        use crate::ids::{ChatId, MessageId};
        use crate::telegram::envelope::{
            MessageContent, MessageSchedulingState, ParsedMessage, TextContent,
        };

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
        // A scheduled send lives in `session.scheduled_messages`, not history.
        driver.session.scheduled_messages.push(ParsedMessage {
            id: MessageId(70),
            chat_id: ChatId(7),
            date: 0,
            is_outgoing: true,
            is_pinned: false,
            topic_id: None,
            ephemeral: None,
            media_album_id: 0,
            author_signature: None,
            scheduling_state: Some(MessageSchedulingState::SendAtDate { send_date: 999 }),
            can_retry: false,
            content: MessageContent::Text(TextContent::plain("scheduled draft")),
            files: Vec::new(),
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
        });

        // A non-scheduled edit for the same id finds nothing in history.
        let plain = ComposerEdit {
            chat_id: ChatId(7),
            message_id: MessageId(70),
            original_text: "scheduled draft".into(),
            kind: ComposerEditKind::Text,
            scheduled: false,
            caption_above: false,
        };
        assert_eq!(
            driver.edit_snapshot(&plain, "nope"),
            Err(ConnectSendError::InvalidRequest)
        );

        // The scheduled edit validates against the scheduled list and sends
        // the same `editMessageText` request.
        let scheduled = ComposerEdit {
            scheduled: true,
            ..plain
        };
        let extra = driver.edit_snapshot(&scheduled, "CANARYSCHEDedit").unwrap();
        let json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("editMessageText");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "editMessageText");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_id"], 70);
        assert_eq!(
            v["input_message_content"]["text"]["text"],
            "CANARYSCHEDedit"
        );
        assert!(!sink.rendered().contains("CANARYSCHED"));
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
            revoke: false,
            can_revoke: false,
        };
        // M1: incoming messages are deletable for the current user
        // (`revoke: false`); the for-everyone toggle degrades to for-me.
        let extra = driver.delete_confirmed(&incoming).unwrap();
        let json = recorder.snapshot().last().cloned().expect("deleteMessages");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "deleteMessages");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_ids"], serde_json::json!([50]));
        assert_eq!(v["revoke"], false);

        let mut incoming_revoke = incoming.clone();
        incoming_revoke.revoke = true;
        let extra = driver.delete_confirmed(&incoming_revoke).unwrap();
        let json = recorder.snapshot().last().cloned().expect("deleteMessages");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["message_ids"], serde_json::json!([50]));
        assert_eq!(v["revoke"], false, "revoke is never sent for incoming");

        let missing = DeleteConfirm {
            chat_id: ChatId(7),
            message_id: MessageId(999),
            revoke: false,
            can_revoke: false,
        };
        assert_eq!(
            driver.delete_confirmed(&missing),
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
    fn driver_get_story_interactions_gates_and_dedupes() {
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

        // Uncached story and a story without `can_get_interactions` are
        // rejected.
        assert_eq!(
            driver.get_story_interactions(ChatId(7), 5, ""),
            Err(ConnectSendError::InvalidRequest)
        );
        seed_story(&mut driver, &seq, &dyn_sink, 7, 5, "storyContentPhoto", "");
        assert_eq!(
            driver.get_story_interactions(ChatId(7), 5, ""),
            Err(ConnectSendError::InvalidRequest)
        );
        seed_story(
            &mut driver,
            &seq,
            &dyn_sink,
            7,
            6,
            "storyContentPhoto",
            r#""can_get_interactions":true,"#,
        );
        let extra = driver
            .get_story_interactions(ChatId(7), 6, "")
            .unwrap()
            .expect("getStoryInteractions sends");
        let json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("getStoryInteractions");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getStoryInteractions");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["story_id"], 6);
        assert_eq!(v["offset"], "");
        assert_eq!(v["limit"], 50);
        // In-flight fetch dedupes to None (no second request).
        assert_eq!(driver.get_story_interactions(ChatId(7), 6, ""), Ok(None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_report_story_gates_own_stories() {
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

        // Uncached and own (deletable) stories are not reportable.
        assert_eq!(
            driver.report_story(ChatId(7), 5, "", ""),
            Err(ConnectSendError::InvalidRequest)
        );
        seed_story(
            &mut driver,
            &seq,
            &dyn_sink,
            7,
            5,
            "storyContentPhoto",
            r#""can_be_deleted":true,"#,
        );
        assert_eq!(
            driver.report_story(ChatId(7), 5, "", ""),
            Err(ConnectSendError::InvalidRequest)
        );
        seed_story(&mut driver, &seq, &dyn_sink, 7, 6, "storyContentPhoto", "");
        let extra = driver.report_story(ChatId(7), 6, "", "").unwrap();
        let json = recorder.snapshot().last().cloned().expect("reportStory");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "reportStory");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["story_poster_chat_id"], 7);
        assert_eq!(v["story_id"], 6);
        assert_eq!(v["option_id"], "");
        assert_eq!(v["text"], "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_activate_story_stealth_mode_sends_and_dedupes() {
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
            .activate_story_stealth_mode()
            .unwrap()
            .expect("activateStoryStealthMode sends");
        let json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("activateStoryStealthMode");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "activateStoryStealthMode");
        assert_eq!(v["@extra"], extra.0.to_string());
        // A second activation while the first is in flight is a no-op.
        assert_eq!(driver.activate_story_stealth_mode(), Ok(None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_manage_story_gated_on_cached_flags() {
        // Phase 9.5: `editStory` / `editStoryCover` need
        // `can_be_edited`; `setStoryPrivacySettings` needs
        // `can_set_privacy_settings` — both read from the cached
        // story, matching the delete_story gate pattern.
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
        assert_eq!(
            driver.edit_story(ChatId(7), 5, None, None, Some("x")),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(
            driver.edit_story_cover(ChatId(7), 5, 1.0),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(
            driver.set_story_privacy_settings(
                ChatId(7),
                5,
                StoryPrivacy::Everyone.settings_json(&[])
            ),
            Err(ConnectSendError::InvalidRequest)
        );

        seed_story(
            &mut driver,
            &seq,
            &dyn_sink,
            7,
            6,
            "storyContentPhoto",
            r#""can_be_edited":true,"can_set_privacy_settings":true,"#,
        );
        let extra = driver
            .edit_story(ChatId(7), 6, None, None, Some("new"))
            .unwrap();
        let v: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        assert_eq!(v["@type"], "editStory");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert!(v["content"].is_null());
        assert!(v["areas"].is_null());
        assert_eq!(v["caption"]["text"], "new");
        assert!(driver.session.story_manage.pending);

        let extra = driver.edit_story_cover(ChatId(7), 6, 2.5).unwrap();
        let v: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        assert_eq!(v["@type"], "editStoryCover");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["cover_frame_timestamp"], 2.5);
        assert!(driver.session.story_manage.pending);

        let extra = driver
            .set_story_privacy_settings(ChatId(7), 6, StoryPrivacy::Contacts.settings_json(&[]))
            .unwrap();
        let v: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        assert_eq!(v["@type"], "setStoryPrivacySettings");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(
            v["privacy_settings"]["@type"],
            "storyPrivacySettingsContacts"
        );
        assert!(driver.session.story_manage.pending);

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

        let extra = driver
            .pin_chat_message(ChatId(7), MessageId(50), false)
            .unwrap();
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
            .note_composer_draft(
                ChatId(7),
                "leaving",
                Some(SendReply::plain(MessageId(3))),
                2_000,
                false,
            )
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
    fn quoted_draft_round_trips_input_text_quote() {
        // Slice G1: a composer reply carrying a validated partial quote is
        // saved to the server draft as `inputTextQuote` (schema 1.8.67
        // line 3056) and kept in the local `ChatDraft` for restore.
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
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.select_chat(ChatId(7)).unwrap();
        let reply = SendReply {
            message_id: MessageId(3),
            quote: Some(("meet at".to_string(), 0)),
        };
        let outcome = driver
            .note_composer_draft(ChatId(7), "sounds good", Some(reply), 1_000, false)
            .unwrap();
        assert_eq!(outcome, DraftSaveOutcome::Sent);
        let sent = recorder.snapshot();
        let draft_json = sent.last().expect("draft request sent");
        assert!(draft_json.contains("setChatDraftMessage"));
        assert!(draft_json.contains("\"inputTextQuote\""));
        assert!(draft_json.contains("meet at"));
        let saved = driver.session.chats.get(&7).unwrap().draft.clone().unwrap();
        assert_eq!(saved.text, "sounds good");
        assert_eq!(saved.reply_to_message_id, Some(MessageId(3)));
        assert_eq!(saved.quote, Some(("meet at".to_string(), 0)));
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
            .note_composer_draft(
                ChatId(7),
                "hello",
                Some(SendReply::plain(MessageId(3))),
                1_000,
                true,
            )
            .unwrap();
        assert!(matches!(outcome, DraftSaveOutcome::Debounced { .. }));
        driver
            .select_search_chat(
                ChatId(8),
                "hello",
                Some(SendReply::plain(MessageId(3))),
                1_500,
            )
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
            .note_composer_draft(
                ChatId(7),
                "caption",
                Some(SendReply::plain(MessageId(3))),
                2_000,
                false,
            )
            .unwrap();
        driver
            .note_composer_draft(ChatId(7), "caption", None, 2_100, false)
            .unwrap();
        let after = driver.session.chats.get(&7).unwrap().draft.clone().unwrap();
        assert_eq!(after.text, "caption");
        assert_eq!(after.reply_to_message_id, None);
        driver
            .note_composer_draft(
                ChatId(7),
                "  ",
                Some(SendReply::plain(MessageId(9))),
                3_000,
                false,
            )
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
            description: "team vote".into(),
            options: vec!["Sushi".into(), "Pizza".into()],
            is_anonymous: true,
            allows_multiple_answers: false,
            allows_revoting: false,
            shuffle_options: true,
            duration_hours: "2".into(),
            country_codes: vec!["US".into()],
            ..Default::default()
        };
        let invalid = PollDraft {
            question: "  ".into(),
            options: vec!["Sushi".into(), "Pizza".into()],
            is_anonymous: true,
            allows_multiple_answers: false,
            ..Default::default()
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
            .send_poll_draft(ChatId(7), &valid, Some(SendReply::plain(MessageId(50))))
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
        assert_eq!(content["description"]["text"], "team vote");
        assert_eq!(content["options"][0]["text"]["text"], "Sushi");
        assert_eq!(content["allows_revoting"], false);
        assert_eq!(content["shuffle_options"], true);
        assert_eq!(content["country_codes"], serde_json::json!(["US"]));
        assert_eq!(content["open_period"], 2 * 3600);
        assert_eq!(content["type"]["@type"], "inputPollTypeRegular");
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_quiz_draft_sends_quiz_type() {
        use crate::poll::PollDraft;

        let (dir, mut driver, recorder, sink, _dyn_sink, _seq) = poll_driver();
        let quiz = PollDraft {
            question: "Capital of France?".into(),
            options: vec!["Paris".into(), "London".into()],
            is_quiz: true,
            quiz_correct: Some(0),
            quiz_explanation: "think Eiffel".into(),
            allows_multiple_answers: true, // normalized off for quizzes
            allows_revoting: true,         // normalized off for quizzes
            ..Default::default()
        };
        assert!(quiz.validate().is_none());
        driver
            .send_poll_draft(ChatId(7), &quiz, None)
            .expect("valid quiz draft sends");
        let send_json = recorder.snapshot().last().cloned().expect("sendMessage");
        let v: Value = serde_json::from_str(&send_json).unwrap();
        let content = &v["input_message_content"];
        assert_eq!(content["type"]["@type"], "inputPollTypeQuiz");
        assert_eq!(
            content["type"]["correct_option_ids"],
            serde_json::json!([0])
        );
        assert_eq!(content["type"]["explanation"]["text"], "think Eiffel");
        assert!(content["type"]["explanation_media"].is_null());
        assert_eq!(content["allows_multiple_answers"], false);
        assert_eq!(content["allows_revoting"], false);
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

    /// Bots slice: fails the first `getInlineQueryResults` send.
    struct FailFirstInlineQuerySender {
        sent: Mutex<Vec<String>>,
        fail_next: Mutex<bool>,
    }

    impl FailFirstInlineQuerySender {
        fn new() -> Self {
            Self {
                sent: Mutex::new(Vec::new()),
                fail_next: Mutex::new(true),
            }
        }
    }

    impl JsonSender for Arc<FailFirstInlineQuerySender> {
        fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
            if request.contains("getInlineQueryResults")
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

    /// Phase C2j: the peer's 1:1 screen share lands in its own slot and
    /// never clobbers the peer camera frame; latest-wins holds per slot.
    #[test]
    fn p2p_screen_frame_routes_to_own_slot() {
        let (dir, driver, handle, _sink, _seq) = ready_call_driver();
        let frame = |screen: bool| VideoFrame {
            seq: 0,
            width: 2,
            height: 2,
            rgba: vec![0u8; 16],
            is_local: false,
            participant_user_id: None,
            is_screen: screen,
        };
        handle.emit_video_frame(77, frame(false));
        handle.emit_video_frame(77, frame(true));
        handle.emit_video_frame(77, frame(true));
        let camera = driver.latest_video_frame(77, false).unwrap();
        assert!(!camera.is_screen);
        assert_eq!(camera.seq, 0);
        let screen = driver.latest_screen_frame(77).unwrap();
        assert!(screen.is_screen);
        assert_eq!(screen.seq, 2);
        assert!(driver.latest_video_frame(77, true).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2j: when the peer's 1:1 screen share goes inactive the
    /// pump drops the retained screen frames (no stale picture can
    /// render) while the peer camera frame is untouched; a non-inactive
    /// state leaves the slot alone.
    #[test]
    fn remote_screen_state_inactive_clears_screen_slot() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        let frame = |screen: bool| VideoFrame {
            seq: 0,
            width: 2,
            height: 2,
            rgba: vec![0u8; 16],
            is_local: false,
            participant_user_id: None,
            is_screen: screen,
        };
        let pump = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#;
        handle.emit_video_frame(77, frame(false));
        handle.emit_video_frame(77, frame(true));
        handle.emit_remote_screen_state(77, RemoteVideoState::Paused);
        ingest_call_json(&mut driver, &seq, &sink, pump);
        assert!(driver.latest_screen_frame(77).is_some());
        handle.emit_remote_screen_state(77, RemoteVideoState::Inactive);
        ingest_call_json(&mut driver, &seq, &sink, pump);
        assert!(driver.latest_screen_frame(77).is_none());
        assert!(driver.latest_video_frame(77, false).is_some());
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

    /// Phase C2i: the UI screen-share toggle reaches the engine with
    /// the call id and the new state — but only once a transport
    /// exists; before that the intent is stored cleanly so it can
    /// apply on connect. Without an enumerated screen source the
    /// toggle is rejected (`NoScreenSource`) and the flag stays put.
    #[test]
    fn set_call_screen_share_gates_engine_on_transport() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        handle.set_devices(vec![MediaDevice {
            id: "screen-0".into(),
            name: "Test Screen".into(),
            kind: MediaDeviceKind::Screen,
        }]);
        driver.refresh_call_devices();
        // No transport yet: intent stored, engine untouched.
        assert!(driver.set_call_screen_share(77, true).is_ok());
        assert!(handle.p2p_screen_share_changes().is_empty());
        assert!(driver.session.active_call.as_ref().unwrap().screen_sharing);
        // Phase C2i: screen share clears the camera intent (ntgcalls
        // forbids camera+screen in Capture mode).
        assert!(!driver.session.active_call.as_ref().unwrap().camera_on);
        // Transport connected: the pre-transport screen-share intent
        // applies on connect (pump_call_engine forwards it), then the
        // toggle drives the engine directly.
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        assert!(driver.set_call_screen_share(77, false).is_ok());
        assert_eq!(
            handle.p2p_screen_share_changes(),
            vec![(77, true), (77, false)]
        );
        assert!(!driver.session.active_call.as_ref().unwrap().screen_sharing);
        // Phase C2i: enabling the camera clears the screen-share
        // intent symmetrically.
        assert!(driver.set_call_camera(77, true).is_ok());
        assert!(driver.session.active_call.as_ref().unwrap().camera_on);
        assert!(!driver.session.active_call.as_ref().unwrap().screen_sharing);
        assert_eq!(
            driver.set_call_screen_share(999, true),
            Err(crate::calls::engine::EngineError::NoSuchCall(999))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase C2i: no enumerated screen source → *enabling* is
    /// rejected and the tracked flag is untouched; *stopping* still
    /// works (a vanished display must not trap the user in "sharing").
    #[test]
    fn set_call_screen_share_rejected_without_screen_source() {
        let (dir, mut driver, handle, sink, seq) = ready_call_driver();
        driver.refresh_call_devices();
        ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
        assert_eq!(
            driver.set_call_screen_share(77, true),
            Err(crate::calls::engine::EngineError::NoScreenSource)
        );
        assert!(!driver.session.active_call.as_ref().unwrap().screen_sharing);
        assert!(handle.p2p_screen_share_changes().is_empty());
        // Stopping needs no source: simulate a stranded sharing flag
        // and verify it can still be cleared.
        driver.session.active_call.as_mut().unwrap().screen_sharing = true;
        assert!(driver.set_call_screen_share(77, false).is_ok());
        assert!(!driver.session.active_call.as_ref().unwrap().screen_sharing);
        assert_eq!(handle.p2p_screen_share_changes(), vec![(77, false)]);
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
            screen_sharing: false,
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
        // (schema :14405 / :14408), gated on `groupCall.can_be_managed`
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

        // Start now: `startScheduledVideoChat` (:14277), gated on
        // `can_be_managed` for a still-scheduled call.
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .scheduled_start_date = 1_788_000_000;
        driver
            .start_scheduled_video_chat()
            .expect("start-now sends");
        assert_eq!(last_sent()["@type"], "startScheduledVideoChat");
        assert_eq!(last_sent()["group_call_id"], 77);
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .can_be_managed = false;
        assert_eq!(
            driver.start_scheduled_video_chat(),
            Err(ConnectSendError::InvalidRequest),
            "non-manageable call must refuse start-now"
        );
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .can_be_managed = true;
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .scheduled_start_date = 0;
        assert_eq!(
            driver.start_scheduled_video_chat(),
            Err(ConnectSendError::InvalidRequest),
            "already-active call must refuse start-now"
        );

        // Start-notification toggle:
        // `toggleVideoChatEnabledStartNotification` (:14282), gated on
        // a still-scheduled call; the driver flips the tracked
        // `enabled_start_notification` flag.
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .scheduled_start_date = 1_788_000_000;
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .enabled_start_notification = false;
        driver
            .toggle_video_chat_start_notification()
            .expect("notify-me sends");
        assert_eq!(
            last_sent()["@type"],
            "toggleVideoChatEnabledStartNotification"
        );
        assert_eq!(last_sent()["group_call_id"], 77);
        assert_eq!(last_sent()["enabled_start_notification"], true);
        // Same shape with the flag on: the driver turns it off.
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .enabled_start_notification = true;
        driver
            .toggle_video_chat_start_notification()
            .expect("un-notify-me sends");
        assert_eq!(last_sent()["enabled_start_notification"], false);
        driver
            .session
            .active_group_call
            .as_mut()
            .unwrap()
            .scheduled_start_date = 0;
        assert_eq!(
            driver.toggle_video_chat_start_notification(),
            Err(ConnectSendError::InvalidRequest),
            "already-active call must refuse the notify toggle"
        );

        // Invite revocation: `revokeGroupCallInviteLink` (:14398),
        // gated on `can_be_managed` for video chats.
        driver
            .revoke_video_chat_invite_link()
            .expect("revoke sends");
        assert_eq!(last_sent()["@type"], "revokeGroupCallInviteLink");
        assert_eq!(last_sent()["group_call_id"], 77);

        // In-call chat: `sendGroupCallMessage` (:14341), gated on
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
        // Chat toggle: `toggleGroupCallAreMessagesAllowed` (:14322),
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

    /// Slice calls-group-self-tile: a group-local frame (`is_local`,
    /// no participant — what the engine now delivers for group
    /// CAPTURE frames) lands in the shared (call id, is_local) slots,
    /// never in the participant slots.
    #[test]
    fn group_local_frame_routes_to_shared_local_slot() {
        let (dir, driver, _recorder, handle, _sink, _seq) = ready_group_call_driver();
        handle.emit_video_frame(
            555,
            VideoFrame {
                seq: 0,
                width: 2,
                height: 2,
                rgba: vec![0u8; 16],
                is_local: true,
                participant_user_id: None,
                is_screen: false,
            },
        );
        assert!(driver.latest_video_frame(555, true).is_some());
        assert!(driver.latest_video_frame(555, false).is_none());
        assert!(driver.latest_group_video_frame(555, 42, false).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice calls-group-self-tile: leaving the group call clears the
    /// self-tile slot along with the participant slots.
    #[test]
    fn group_call_leave_clears_local_frame_slot() {
        let (dir, mut driver, _recorder, handle, _sink, _seq) = ready_group_call_driver();
        handle.emit_video_frame(
            555,
            VideoFrame {
                seq: 0,
                width: 2,
                height: 2,
                rgba: vec![0u8; 16],
                is_local: true,
                participant_user_id: None,
                is_screen: false,
            },
        );
        assert!(driver.latest_video_frame(555, true).is_some());
        driver.leave_group_call().expect("leave");
        assert!(driver.latest_video_frame(555, true).is_none());
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

    /// Slice G2: `toggle_sign_messages` gates on `can_change_info` in a
    /// channel, sends the right JSON, applies optimistically, and rolls
    /// back on a TDLib error.
    #[test]
    fn driver_toggle_sign_messages_sends_and_rolls_back() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        let ingest_json = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        };
        ingest_json(
            &mut driver,
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"c","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        );
        // A plain member is gated out.
        ingest_json(
            &mut driver,
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"status":{"@type":"chatMemberStatusMember"}}}"#,
        );
        assert_eq!(
            driver.toggle_sign_messages(ChatId(13), true, true).unwrap(),
            None
        );
        // An admin with `can_change_info` passes the gate.
        ingest_json(
            &mut driver,
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_change_info":true}}}}"#,
        );
        let extra = driver
            .toggle_sign_messages(ChatId(13), true, true)
            .unwrap()
            .expect("toggle sent");
        let sent = recorder
            .snapshot()
            .into_iter()
            .find(|j| j.contains("toggleSupergroupSignMessages"))
            .expect("toggle JSON sent");
        let value: Value = serde_json::from_str(&sent).unwrap();
        assert_eq!(value["@type"], "toggleSupergroupSignMessages");
        assert_eq!(value["supergroup_id"], 13);
        assert_eq!(value["sign_messages"], true);
        assert_eq!(value["show_message_sender"], true);
        // Optimistic state.
        assert_eq!(
            driver.session.supergroup_sign_messages.get(&13),
            Some(&true)
        );
        // In flight → no-op.
        assert_eq!(
            driver.toggle_sign_messages(ChatId(13), true, true).unwrap(),
            None
        );
        // A TDLib error rolls the flags back.
        ingest_json(
            &mut driver,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            driver.session.supergroup_sign_messages.get(&13),
            Some(&false)
        );
        assert_eq!(
            driver.session.supergroup_show_message_sender.get(&13),
            Some(&false)
        );
        // A non-channel supergroup is not eligible at all.
        ingest_json(
            &mut driver,
            r#"{"@type":"updateNewChat","chat":{"id":14,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#,
        );
        assert_eq!(
            driver.toggle_sign_messages(ChatId(14), true, true).unwrap(),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice G2: `toggle_aggressive_anti_spam` is gated on
    /// `supergroupFullInfo.can_toggle_aggressive_anti_spam`.
    #[test]
    fn driver_toggle_anti_spam_gated_on_full_info_capability() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        let ingest_json = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        };
        ingest_json(
            &mut driver,
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":false},"unread_count":0}}"#,
        );
        // No full info → the toggle is not offered (quiet no-op).
        assert_eq!(
            driver
                .toggle_aggressive_anti_spam(ChatId(13), true)
                .unwrap(),
            None
        );
        ingest_json(
            &mut driver,
            r#"{"@type":"updateSupergroupFullInfo","supergroup_id":13,"supergroup_full_info":{"@type":"supergroupFullInfo","has_aggressive_anti_spam_enabled":false,"can_toggle_aggressive_anti_spam":true}}"#,
        );
        let extra = driver
            .toggle_aggressive_anti_spam(ChatId(13), true)
            .unwrap()
            .expect("toggle sent");
        let sent = recorder
            .snapshot()
            .into_iter()
            .find(|j| j.contains("toggleSupergroupHasAggressiveAntiSpamEnabled"))
            .expect("anti-spam JSON sent");
        let value: Value = serde_json::from_str(&sent).unwrap();
        assert_eq!(value["supergroup_id"], 13);
        assert_eq!(value["has_aggressive_anti_spam_enabled"], true);
        assert_eq!(
            driver.session.supergroup_anti_spam_enabled.get(&13),
            Some(&true)
        );
        ingest_json(
            &mut driver,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            driver.session.supergroup_anti_spam_enabled.get(&13),
            Some(&false)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice G2: forum-topic mutations gate on `can_manage_topics`,
    /// reject empty names, and send the right constructors.
    #[test]
    fn driver_forum_topic_mutations_send_and_gate() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        let ingest_json = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        };
        ingest_json(
            &mut driver,
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":false},"unread_count":0}}"#,
        );
        ingest_json(
            &mut driver,
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"status":{"@type":"chatMemberStatusMember"}}}"#,
        );
        // A plain member is gated out.
        assert_eq!(driver.create_forum_topic(ChatId(13), "news").unwrap(), None);
        // Empty names are rejected.
        assert!(driver.create_forum_topic(ChatId(13), "  ").is_err());
        // An admin with `can_manage_topics` passes.
        ingest_json(
            &mut driver,
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_manage_topics":true}}}}"#,
        );
        let extra = driver
            .create_forum_topic(ChatId(13), "news")
            .unwrap()
            .expect("create sent");
        let sent: Vec<Value> = recorder
            .snapshot()
            .into_iter()
            .filter(|j| j.contains("createForumTopic"))
            .map(|j| serde_json::from_str(&j).unwrap())
            .collect();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["chat_id"], 13);
        assert_eq!(sent[0]["name"], "news");
        assert_eq!(sent[0]["@extra"], extra.0.to_string());
        // The other mutations send their constructors.
        driver
            .edit_forum_topic(ChatId(13), 5, "announcements")
            .unwrap()
            .expect("edit sent");
        driver
            .toggle_forum_topic_closed(ChatId(13), 5, true)
            .unwrap()
            .expect("close sent");
        driver
            .toggle_forum_topic_pinned(ChatId(13), 5, true)
            .unwrap()
            .expect("pin sent");
        driver
            .delete_forum_topic(ChatId(13), 5)
            .unwrap()
            .expect("delete sent");
        driver
            .toggle_general_forum_topic_hidden(ChatId(13), true)
            .unwrap()
            .expect("hide sent");
        let types: Vec<String> = recorder
            .snapshot()
            .into_iter()
            .map(|j| {
                serde_json::from_str::<Value>(&j).unwrap()["@type"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        for expected in [
            "editForumTopic",
            "toggleForumTopicIsClosed",
            "toggleForumTopicIsPinned",
            "deleteForumTopic",
            "toggleGeneralForumTopicIsHidden",
        ] {
            assert!(types.iter().any(|t| t == expected), "{expected} sent");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice G2: confirmed mutations refetch the dropped cache — forum
    /// topic create (answers `forumTopicInfo`), welcome add (answers
    /// `ok`), and `boostChat` (answers `chatBoostSlots`). A failed
    /// mutation leaves the cache alone and refetches nothing.
    #[test]
    fn driver_mutation_confirmed_refetches_dropped_cache() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        let ingest_json = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        };
        ingest_json(
            &mut driver,
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":false},"unread_count":0}}"#,
        );
        ingest_json(
            &mut driver,
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"is_forum":true,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_manage_topics":true,"can_send_welcome_messages":true}}}}"#,
        );
        // Seed the caches the way the dialogs load them before mutating.
        driver.session.forum_topics.insert(13, vec![]);
        driver.session.welcome_messages.insert(13, vec![]);
        driver.session.chat_boost_status.insert(13, (0, 0));
        let sent_types = || {
            recorder
                .snapshot()
                .into_iter()
                .map(|j| {
                    serde_json::from_str::<Value>(&j).unwrap()["@type"]
                        .as_str()
                        .unwrap()
                        .to_string()
                })
                .collect::<Vec<_>>()
        };

        // Forum create confirmed: cache dropped, `getForumTopics` refetch.
        let extra = driver
            .create_forum_topic(ChatId(13), "news")
            .unwrap()
            .expect("create sent");
        ingest_json(
            &mut driver,
            &format!(
                r#"{{"@type":"forumTopicInfo","@extra":"{}","chat_id":13}}"#,
                extra.0
            ),
        );
        assert!(!driver.session.forum_topics.contains_key(&13));
        assert!(
            sent_types().iter().any(|t| t == "getForumTopics"),
            "forum list refetched after confirmed create"
        );

        // Forum create failed: cache kept, nothing refetched.
        driver.session.forum_topics.insert(13, vec![]);
        let refetches_before = sent_types()
            .iter()
            .filter(|t| *t == "getForumTopics")
            .count();
        let extra = driver
            .create_forum_topic(ChatId(13), "news")
            .unwrap()
            .expect("create sent");
        ingest_json(
            &mut driver,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"TOPIC_INVALID"}}"#,
                extra.0
            ),
        );
        assert!(driver.session.forum_topics.contains_key(&13));
        assert_eq!(
            sent_types()
                .iter()
                .filter(|t| *t == "getForumTopics")
                .count(),
            refetches_before,
            "no refetch on failure"
        );

        // Welcome add confirmed: pack dropped, reload sent.
        let extra = driver
            .add_chat_welcome_message(ChatId(13), "hi")
            .unwrap()
            .expect("add sent");
        ingest_json(
            &mut driver,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(!driver.session.welcome_messages.contains_key(&13));
        assert!(
            sent_types().iter().any(|t| t == "loadChatWelcomeMessages"),
            "welcome pack reloaded after confirmed add"
        );

        // Boost confirmed: status dropped, `getChatBoostStatus` refetch.
        let slots_extra = driver
            .request_chat_boost(ChatId(13))
            .unwrap()
            .expect("slots sent");
        ingest_json(
            &mut driver,
            &format!(
                r#"{{"@type":"chatBoostSlots","@extra":"{}","slots":[{{"slot_id":3}}]}}"#,
                slots_extra.0
            ),
        );
        let boost_extra = driver
            .session
            .requests
            .pending_extra_for(RequestPurpose::BoostChat, Some(ChatId(13)))
            .expect("boostChat chained");
        ingest_json(
            &mut driver,
            &format!(
                r#"{{"@type":"chatBoostSlots","@extra":"{}","slots":[]}}"#,
                boost_extra.0
            ),
        );
        assert!(!driver.session.chat_boost_status.contains_key(&13));
        assert!(
            sent_types().iter().any(|t| t == "getChatBoostStatus"),
            "boost status refetched after confirmed boost"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice G2: `request_chat_boost` sends `getAvailableChatBoostSlots`;
    /// the driver chains `boostChat` with the first slot id once the
    /// answer arrives.
    #[test]
    fn driver_boost_chain_sends_slots_then_boost() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":13,"title":"c","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let extra = driver
            .request_chat_boost(ChatId(13))
            .unwrap()
            .expect("slots request sent");
        // A second intent while the first is in flight → no-op.
        assert_eq!(driver.request_chat_boost(ChatId(13)).unwrap(), None);
        let sent = recorder.snapshot();
        assert!(
            sent.iter()
                .any(|j| j.contains("getAvailableChatBoostSlots"))
        );
        // The slots answer chains `boostChat` with the first slot id.
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chatBoostSlots","@extra":"{}","slots":[{{"slot_id":3}},{{"slot_id":7}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let boost = recorder
            .snapshot()
            .into_iter()
            .find(|j| j.contains(r#""@type":"boostChat""#))
            .expect("boostChat chained");
        let value: Value = serde_json::from_str(&boost).unwrap();
        assert_eq!(value["chat_id"], 13);
        assert_eq!(value["slot_ids"], serde_json::json!([3]));
        assert_eq!(driver.session.boost_intent, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice G2: `fetch_message_thread_history` sends
    /// `getMessageThreadHistory` and dedupes per channel post.
    #[test]
    fn driver_thread_history_sends_and_dedupes() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":13,"title":"c","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let extra = driver
            .fetch_message_thread_history(ChatId(13), MessageId(99))
            .unwrap()
            .expect("thread history sent");
        assert_eq!(
            driver
                .fetch_message_thread_history(ChatId(13), MessageId(99))
                .unwrap(),
            None
        );
        let sent = recorder
            .snapshot()
            .into_iter()
            .find(|j| j.contains("getMessageThreadHistory"))
            .expect("thread history JSON sent");
        let value: Value = serde_json::from_str(&sent).unwrap();
        assert_eq!(value["chat_id"], 13);
        assert_eq!(value["message_id"], 99);
        assert_eq!(value["@extra"], extra.0.to_string());
        // Success ingestion: the thread cache is populated for the post.
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":11,"chat_id":13,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let thread = driver
            .session
            .comment_thread
            .as_ref()
            .expect("thread cached");
        assert_eq!(thread.message_id, MessageId(99));
        assert_eq!(thread.messages.len(), 1);
        assert_eq!(thread.failed, None);
        // Failure ingestion: the viewer records the error for a retry.
        let extra = driver
            .fetch_message_thread_history(ChatId(13), MessageId(99))
            .unwrap()
            .expect("thread history retry sent");
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","@extra":"{}","code":400,"message":"MESSAGE_NOT_MODIFIED"}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let thread = driver.session.comment_thread.as_ref().expect("thread kept");
        assert!(thread.failed.is_some(), "failure recorded for the viewer");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice G2: welcome-message send methods gate on the right and
    /// send the right constructors.
    #[test]
    fn driver_welcome_message_mutations_send_and_gate() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        let ingest_json = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        };
        ingest_json(
            &mut driver,
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":false},"unread_count":0}}"#,
        );
        ingest_json(
            &mut driver,
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"status":{"@type":"chatMemberStatusMember"}}}"#,
        );
        // Plain member: gated out; empty text: rejected.
        assert_eq!(
            driver.add_chat_welcome_message(ChatId(13), "hi").unwrap(),
            None
        );
        assert!(driver.add_chat_welcome_message(ChatId(13), "  ").is_err());
        ingest_json(
            &mut driver,
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_send_welcome_messages":true}}}}"#,
        );
        driver
            .add_chat_welcome_message(ChatId(13), "welcome!")
            .unwrap()
            .expect("add sent");
        driver
            .edit_chat_welcome_message(ChatId(13), 7, "welcome back")
            .unwrap()
            .expect("edit sent");
        driver
            .delete_chat_welcome_message(ChatId(13), 7)
            .unwrap()
            .expect("delete sent");
        driver
            .load_chat_welcome_messages(ChatId(13))
            .unwrap()
            .expect("load sent");
        let types: Vec<String> = recorder
            .snapshot()
            .into_iter()
            .map(|j| {
                serde_json::from_str::<Value>(&j).unwrap()["@type"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        for expected in [
            "addChatWelcomeMessage",
            "editChatWelcomeMessage",
            "deleteChatWelcomeMessage",
            "loadChatWelcomeMessages",
        ] {
            assert!(types.iter().any(|t| t == expected), "{expected} sent");
        }
        let add = recorder
            .snapshot()
            .into_iter()
            .find(|j| j.contains("addChatWelcomeMessage"))
            .unwrap();
        let value: Value = serde_json::from_str(&add).unwrap();
        assert_eq!(value["chat_id"], 13);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MED2: `recognize_speech` sends one `recognizeSpeech` for a real,
    /// non-pending voice-note message; the chats path gate, unknown
    /// messages, and pending messages are rejected (a refused request is
    /// an error, never a faked transcript).
    #[test]
    fn driver_recognize_speech_sends_and_gates() {
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
            driver.recognize_speech(ChatId(11), MessageId(90)),
            Err(ConnectSendError::InvalidRequest)
        );
        for json in [
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Cloud","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        }
        // Unknown message: rejected.
        assert_eq!(
            driver.recognize_speech(ChatId(11), MessageId(90)),
            Err(ConnectSendError::InvalidRequest)
        );
        // A real voice-note message.
        let voice_json = r#"{"@type":"updateNewMessage","message":{"id":90,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageVoiceNote","voice_note":{"@type":"voiceNote","duration":3,"waveform":"","mime_type":"audio/ogg","voice":{"@type":"file","id":81,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"","is_downloading_completed":false,"is_downloading_active":false,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"","unique_id":"","is_uploading_completed":false,"is_uploading_active":false,"uploaded_size":0}}}},"is_listened":false}}"#;
        driver
            .ingest(copy_and_parse(voice_json, &seq, &dyn_sink).unwrap())
            .unwrap();
        let extra = driver
            .recognize_speech(ChatId(11), MessageId(90))
            .expect("real message sends");
        let last = recorder.snapshot().into_iter().last().unwrap();
        let value: Value = serde_json::from_str(&last).unwrap();
        assert_eq!(value["@type"], "recognizeSpeech");
        assert_eq!(value["chat_id"], 11);
        assert_eq!(value["message_id"], 90);
        assert_eq!(value["@extra"], extra.0.to_string());
        // Pending (unsent) message: rejected.
        let history = driver.session.histories.get_mut(&11).unwrap();
        let mut pending_msg = history.messages.get(&90).unwrap().clone();
        pending_msg.id = MessageId(0);
        history.messages.insert(91, pending_msg);
        assert_eq!(
            driver.recognize_speech(ChatId(11), MessageId(91)),
            Err(ConnectSendError::InvalidRequest)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MED2: `send_recorded_video_note` sends one `sendMessage` carrying
    /// `inputMessageVideoNote` (duration + square length from the draft);
    /// a closed chats path or an unsupported chat is rejected.
    #[test]
    fn driver_send_recorded_video_note_sends_input_message_video_note() {
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
            driver.send_recorded_video_note(
                &crate::video::VideoNoteDraft {
                    path: std::path::PathBuf::from("/nonexistent.mp4"),
                    duration_secs: 5,
                    length: 280,
                },
                None
            ),
            Err(ConnectSendError::InvalidRequest)
        );
        for json in [
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Cloud","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        }
        // No open chat: rejected.
        assert_eq!(
            driver.send_recorded_video_note(
                &crate::video::VideoNoteDraft {
                    path: std::path::PathBuf::from("/nonexistent.mp4"),
                    duration_secs: 5,
                    length: 280,
                },
                None
            ),
            Err(ConnectSendError::InvalidRequest)
        );
        driver.session.open_chat = Some(ChatId(11));
        // A real file so `pick_send_path` accepts it; the thumbnail probe
        // may fail on garbage bytes (thumbnail is optional).
        let clip = dir.join("clip.mp4");
        std::fs::write(&clip, b"not a real video").unwrap();
        let draft = crate::video::VideoNoteDraft {
            path: clip,
            duration_secs: 5,
            length: 280,
        };
        let extra = driver
            .send_recorded_video_note(&draft, None)
            .expect("sends");
        let last = recorder.snapshot().into_iter().last().unwrap();
        let value: Value = serde_json::from_str(&last).unwrap();
        assert_eq!(value["@type"], "sendMessage");
        assert_eq!(value["chat_id"], 11);
        assert_eq!(
            value["input_message_content"]["@type"],
            "inputMessageVideoNote"
        );
        assert_eq!(value["input_message_content"]["video_note"]["duration"], 5);
        assert_eq!(value["input_message_content"]["video_note"]["length"], 280);
        assert_eq!(value["@extra"], extra.0.to_string());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MED3: sender that fails every `downloadFile` send with a transport
    /// error.
    struct FailDownloadSender {
        sent: Mutex<Vec<String>>,
    }

    impl JsonSender for Arc<FailDownloadSender> {
        fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
            if request.contains("downloadFile") {
                return Err(ConnectSendError::Native);
            }
            self.sent
                .lock()
                .expect("download sender")
                .push(request.to_string());
            Ok(())
        }
    }

    #[test]
    fn download_file_send_failure_marks_failed_download() {
        // A `downloadFile` transport failure (the request never reached
        // TDLib) records the failure like an error response, so the row
        // offers Retry instead of silently returning to "not downloaded".
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let sender = Arc::new(FailDownloadSender {
            sent: Mutex::new(Vec::new()),
        });
        let mut driver = ConnectDriver::new(
            Session::new(AccountKey::primary(), sink.clone()),
            sender,
            test_credentials(),
            prepared,
        );
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &sink,
                )
                .unwrap(),
            )
            .unwrap();
        let err = driver.download_file(FileId(21), 1, true).unwrap_err();
        assert!(matches!(err, ConnectSendError::Native));
        assert!(driver.session.failed_downloads.contains(&21));
        assert!(!driver.session.downloading.contains(&21));
        assert!(!driver.session.user_downloads.contains(&21));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice B2: `send_bot_start_message` unblocks a fully-blocked bot
    /// chat first (Telegram X `MessagesController.ACTION_BOT_START`
    /// behavior), so "Restart bot" / the START press doesn't clear the
    /// chat and then fail the start.
    #[test]
    fn b2_bot_start_unblocks_first_on_fully_blocked_chat() {
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

        // Chat 7: private bot chat, fully blocked (`blockListMain`).
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Bot","type":{"@type":"chatTypePrivate","user_id":42},"unread_count":0,"block_list":{"@type":"blockListMain"}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(
            driver
                .session
                .chats
                .get(&7)
                .is_some_and(|chat| chat.blocked)
        );

        let extra = driver
            .send_bot_start_message(ChatId(7), 42, "startparam")
            .expect("start send")
            .expect("request id");

        // Unblock goes out BEFORE the start message (other Ready/chat
        // bookkeeping sends may also be in flight; filter like the
        // existing driver tests do).
        let of_type = |t: &str| {
            recorder
                .snapshot()
                .into_iter()
                .filter(|sent| sent.contains(&format!("\"@type\":\"{t}\"")))
                .collect::<Vec<_>>()
        };
        let unblock_sends = of_type("setMessageSenderBlockList");
        let start_sends = of_type("sendBotStartMessage");
        assert_eq!(unblock_sends.len(), 1);
        assert_eq!(start_sends.len(), 1);
        let unblock: Value = serde_json::from_str(&unblock_sends[0]).unwrap();
        assert!(unblock["block_list"].is_null());
        assert_eq!(unblock["sender_id"]["user_id"], 42);
        let start: Value = serde_json::from_str(&start_sends[0]).unwrap();
        assert_eq!(start["parameter"], "startparam");
        assert_eq!(start["@extra"], extra.0.to_string());
        // Order: the unblock was recorded before the start message.
        let snapshot = recorder.snapshot();
        let unblock_at = snapshot
            .iter()
            .position(|sent| sent.contains("\"@type\":\"setMessageSenderBlockList\""))
            .unwrap();
        let start_at = snapshot
            .iter()
            .position(|sent| sent.contains("\"@type\":\"sendBotStartMessage\""))
            .unwrap();
        assert!(unblock_at < start_at);

        // Chat 8: unblocked bot chat — only the start message, no unblock.
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bot2","type":{"@type":"chatTypePrivate","user_id":43},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .send_bot_start_message(ChatId(8), 43, "")
            .expect("start send")
            .expect("request id");
        assert_eq!(of_type("setMessageSenderBlockList").len(), 1);
        assert_eq!(of_type("sendBotStartMessage").len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Slice A3: driver harness for the sessions flow.
    type SessionsDriverHarness = (
        std::path::PathBuf,
        ConnectDriver<Arc<RecordingSender>>,
        Arc<RecordingSender>,
        Arc<MemorySink>,
        Arc<dyn DiagnosticSink>,
        AtomicU64,
    );

    fn sessions_driver() -> SessionsDriverHarness {
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
        (dir, driver, recorder, sink, dyn_sink, seq)
    }

    fn session_fixture(id: i64, current: bool, pending: bool) -> ParsedSession {
        ParsedSession {
            id,
            is_current: current,
            is_password_pending: pending,
            can_accept_secret_chats: false,
            can_accept_calls: true,
            device_model: format!("Device {id}"),
            application_name: "Quill".into(),
            application_version: "0.1".into(),
            platform: "Linux".into(),
            system_version: "6.8".into(),
            last_active_date: 1759000000,
            ip_address: "1.2.3.4".into(),
            location: "Austin".into(),
        }
    }

    /// Slice A3: the fetch is guarded by Ready, deduped while in flight
    /// and while the cache is fresh.
    #[test]
    fn sessions_fetch_guards() {
        // Not ready: refused.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink);
        let mut driver = ConnectDriver::new(session, recorder, test_credentials(), prepared);
        assert_invalid(driver.maybe_fetch_active_sessions());
        let _ = std::fs::remove_dir_all(&dir);

        // Ready: first fetch sends, the in-flight second is deduped.
        let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = sessions_driver();
        driver
            .maybe_fetch_active_sessions()
            .expect("send")
            .expect("request id");
        let snapshot = recorder.snapshot();
        assert!(
            snapshot
                .iter()
                .any(|s| s.contains("\"@type\":\"getActiveSessions\""))
        );
        let sent_before = snapshot.len();
        assert!(
            driver
                .maybe_fetch_active_sessions()
                .expect("dedupe")
                .is_none()
        );
        assert_eq!(recorder.snapshot().len(), sent_before);
        // Cached: no refetch.
        driver.session.sessions = Some(vec![]);
        assert!(
            driver
                .maybe_fetch_active_sessions()
                .expect("cached")
                .is_none()
        );
        assert_eq!(recorder.snapshot().len(), sent_before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice A3: terminate guards — unknown id and concurrent mutations
    /// are refused before anything is sent.
    #[test]
    fn sessions_terminate_guards() {
        let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = sessions_driver();
        driver.session.sessions = Some(vec![
            session_fixture(11, true, false),
            session_fixture(22, false, false),
        ]);
        // Unknown id: refused, nothing sent.
        let sent_before = recorder.snapshot().len();
        assert_invalid(driver.terminate_session(99));
        assert_eq!(recorder.snapshot().len(), sent_before);
        // Known id: sent; a second mutation while in flight is refused.
        driver.terminate_session(22).expect("terminate send");
        assert!(recorder.snapshot().iter().any(
            |s| s.contains("\"@type\":\"terminateSession\"") && s.contains("\"session_id\":22")
        ));
        assert_invalid(driver.terminate_session(11));
        assert_invalid(driver.terminate_all_other_sessions());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice A3: `terminateSession` → `ok` → the reducer marks the list
    /// stale → the same ingest refetches `getActiveSessions` → the
    /// authoritative answer replaces the cache (no optimistic deletion).
    #[test]
    fn sessions_terminate_ok_triggers_authoritative_refetch() {
        let (dir, mut driver, recorder, _sink, dyn_sink, seq) = sessions_driver();
        driver.session.sessions = Some(vec![
            session_fixture(11, true, false),
            session_fixture(22, false, false),
        ]);
        let extra = driver.terminate_session(22).expect("terminate send");
        let sent = recorder.snapshot().len();
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
        // The same ingest refetched the list (no optimistic deletion:
        // the old cache stays visible, marked stale, until the
        // authoritative answer replaces it).
        let snapshot = recorder.snapshot();
        assert_eq!(snapshot.len(), sent + 1);
        assert!(snapshot[sent].contains("\"@type\":\"getActiveSessions\""));
        assert_eq!(driver.session.sessions.as_ref().unwrap().len(), 2);
        assert!(driver.session.sessions_stale);
        assert!(!driver.session.sessions_mutating);
        // The authoritative answer lands — the @extra comes from the
        // recorded outbound JSON (what TDLib would echo back).
        let fetch_extra: i64 = {
            let snapshot = recorder.snapshot();
            let sent = snapshot
                .iter()
                .rev()
                .find(|s| s.contains("\"@type\":\"getActiveSessions\""))
                .expect("refetch sent");
            let v: serde_json::Value = serde_json::from_str(sent).unwrap();
            v["@extra"].as_str().unwrap().parse().unwrap()
        };
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"sessions","@extra":"{fetch_extra}","sessions":[{{"@type":"session","id":11,"is_current":true,"device_model":"Device 11","application_name":"Quill"}}]}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let sessions = driver.session.sessions.as_ref().expect("refetched");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, 11);
        assert!(!driver.session.sessions_stale);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice A4: toggle guards — unknown id and concurrent mutations are
    /// refused before anything is sent; the sent value negates the
    /// cached flag.
    #[test]
    fn session_toggle_guards() {
        let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = sessions_driver();
        driver.session.sessions = Some(vec![
            session_fixture(11, true, false),
            session_fixture(22, false, false),
        ]);
        // Unknown id: refused, nothing sent.
        let sent_before = recorder.snapshot().len();
        assert_invalid(driver.toggle_session_can_accept_secret_chats(99));
        assert_invalid(driver.toggle_session_can_accept_calls(99));
        assert_eq!(recorder.snapshot().len(), sent_before);
        // The fixture rejects secret chats: the toggle sends `true`.
        // (The builder sends `session_id` as a JSON number, like
        // `terminateSession`.)
        driver
            .toggle_session_can_accept_secret_chats(22)
            .expect("toggle send");
        assert!(recorder.snapshot().iter().any(|s| {
            s.contains("\"@type\":\"toggleSessionCanAcceptSecretChats\"")
                && s.contains("\"session_id\":22")
                && s.contains("\"can_accept_secret_chats\":true")
        }));
        // A second mutation while in flight is refused.
        assert_invalid(driver.toggle_session_can_accept_calls(22));
        assert_invalid(driver.terminate_session(22));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice A4: `toggleSessionCanAcceptCalls` → `ok` → the reducer marks
    /// the sessions list stale → the same ingest refetches
    /// `getActiveSessions` (the toggled value arrives in the
    /// authoritative answer, never optimistically).
    #[test]
    fn session_toggle_ok_triggers_authoritative_refetch() {
        let (dir, mut driver, recorder, _sink, dyn_sink, seq) = sessions_driver();
        driver.session.sessions = Some(vec![
            session_fixture(11, true, false),
            session_fixture(22, false, false),
        ]);
        let extra = driver
            .toggle_session_can_accept_calls(22)
            .expect("toggle send");
        let sent = recorder.snapshot().len();
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
        // The fixture accepted calls: the toggle sent `false`.
        assert!(recorder.snapshot()[sent - 1].contains("\"can_accept_calls\":false"));
        // The same ingest refetched the list; the cached flag is
        // untouched until the authoritative answer replaces it.
        let snapshot = recorder.snapshot();
        assert_eq!(snapshot.len(), sent + 1);
        assert!(snapshot[sent].contains("\"@type\":\"getActiveSessions\""));
        assert!(driver.session.sessions.as_ref().unwrap()[1].can_accept_calls);
        assert!(driver.session.sessions_stale);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice A4: websites fetch guards — in-flight fetch deduped, fresh
    /// cache reused.
    #[test]
    fn websites_fetch_guards() {
        let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = sessions_driver();
        let sent_before = recorder.snapshot().len();
        driver.maybe_fetch_connected_websites().expect("fetch");
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|s| s.contains("\"@type\":\"getConnectedWebsites\""))
        );
        // In flight: deduped.
        let sent_after_first = recorder.snapshot().len();
        assert!(
            driver
                .maybe_fetch_connected_websites()
                .expect("dedup")
                .is_none()
        );
        assert_eq!(recorder.snapshot().len(), sent_after_first);
        // Cached: no refetch.
        driver.session.connected_websites = Some(vec![]);
        assert!(
            driver
                .maybe_fetch_connected_websites()
                .expect("cached")
                .is_none()
        );
        assert_eq!(recorder.snapshot().len(), sent_after_first);
        assert!(sent_after_first > sent_before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice A4: disconnect guards — unknown website id and concurrent
    /// mutations are refused before anything is sent.
    #[test]
    fn websites_disconnect_guards() {
        let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = sessions_driver();
        // Disconnect-all with no cache (or an empty one): refused,
        // nothing sent.
        let sent_before = recorder.snapshot().len();
        assert_invalid(driver.disconnect_all_websites());
        driver.session.connected_websites = Some(vec![]);
        assert_invalid(driver.disconnect_all_websites());
        assert_eq!(recorder.snapshot().len(), sent_before);
        driver.session.connected_websites = Some(vec![ParsedWebsite {
            id: 55,
            domain_name: "example.com".into(),
            bot_user_id: 77,
            browser: "Chrome".into(),
            platform: "Web".into(),
            log_in_date: 1758900000,
            last_active_date: 1759000000,
            ip_address: "9.9.9.9".into(),
            location: "Boston, United States".into(),
        }]);
        // Unknown id: refused, nothing sent.
        let sent_before = recorder.snapshot().len();
        assert_invalid(driver.disconnect_website(66));
        assert_eq!(recorder.snapshot().len(), sent_before);
        // Known id: sent; a second mutation while in flight is refused.
        driver.disconnect_website(55).expect("disconnect send");
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|s| s.contains("\"@type\":\"disconnectWebsite\"")
                    && s.contains("\"website_id\":55"))
        );
        assert_invalid(driver.disconnect_website(55));
        assert_invalid(driver.disconnect_all_websites());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice A4: `disconnectAllWebsites` → `ok` → the reducer marks the
    /// websites list stale → the same ingest refetches
    /// `getConnectedWebsites` → the authoritative answer replaces the
    /// cache (no optimistic deletion).
    #[test]
    fn disconnect_all_websites_ok_triggers_authoritative_refetch() {
        let (dir, mut driver, recorder, _sink, dyn_sink, seq) = sessions_driver();
        driver.session.connected_websites = Some(vec![ParsedWebsite {
            id: 55,
            domain_name: "example.com".into(),
            bot_user_id: 77,
            browser: "Chrome".into(),
            platform: "Web".into(),
            log_in_date: 1758900000,
            last_active_date: 1759000000,
            ip_address: "9.9.9.9".into(),
            location: "Boston, United States".into(),
        }]);
        let extra = driver.disconnect_all_websites().expect("disconnect send");
        let sent = recorder.snapshot().len();
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
        // The same ingest refetched the list (no optimistic deletion:
        // the old cache stays visible, marked stale, until the
        // authoritative answer replaces it).
        let snapshot = recorder.snapshot();
        assert_eq!(snapshot.len(), sent + 1);
        assert!(snapshot[sent].contains("\"@type\":\"getConnectedWebsites\""));
        assert_eq!(driver.session.connected_websites.as_ref().unwrap().len(), 1);
        assert!(driver.session.websites_stale);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
