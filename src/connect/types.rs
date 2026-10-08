//! Connect gate types: blockers, parameters, driver state.
use super::*;
use crate::calls::engine::{CallEngine, ConnectParams, MediaDevice, VideoFrame};
use crate::composer::DraftSaveClock;
use crate::credentials::TelegramCredentials;
use crate::ids::{AccountKey, ChatId, RequestId};
use crate::lifecycle::RestoreBlocker;
use crate::platform::DatabaseKey;
use crate::settings::AccountPaths;
use crate::state::Session;
use crate::telegram::ffi::LibraryOrigin;
use crate::telegram::requests::SendReply;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How a [`NotificationSoundKind`] resolves to something playable
/// (parity slice: notification sounds).
pub enum SoundResolution {
    /// Play the app default tone (synthesized in-process).
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
/// Members listed in the in-chat "From:" picker.
pub const FROM_MEMBERS_LIMIT: i32 = 50;
/// Fetch the next page of hits when this few remain ahead of the selection.
pub const CHAT_SEARCH_PREFETCH: usize = 5;
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

/// Local paths + database key ready for `setTdlibParameters`.
#[derive(Debug)]
pub struct PreparedConnect {
    pub account: AccountKey,
    pub paths: AccountPaths,
    pub database_key: DatabaseKey,
    pub database_exists: bool,
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
    pub(crate) sender: S,
    pub(crate) call_engine: Option<Box<dyn CallEngine>>,
    pub(crate) signaling_outbox: SignalingOutbox,
    pub(crate) transport_outbox: TransportOutbox,
    /// Phase C2e: engine-emitted peer camera states (worker thread ->
    /// driver pump).
    pub(crate) video_state_outbox: VideoStateOutbox,
    /// Phase C2j: engine-emitted peer 1:1 screen-share states (worker
    /// thread -> driver pump). When the peer's share goes inactive the
    /// pump drops the retained screen frames so no stale picture can
    /// render.
    pub(crate) screen_state_outbox: VideoStateOutbox,
    /// Phase C2e: latest decoded video frame per (call id, is_local,
    /// is_screen).
    pub(crate) video_frame_slots: VideoFrameSlots,
    /// Phase C2g: latest decoded group video frame per (group call id,
    /// participant user id, is_screen); only the newest frame is kept.
    pub(crate) group_video_frame_slots: GroupVideoFrameSlots,
    /// Phase C2g: last camera-enabled value issued to the engine per
    /// group call id; the engine is only re-issued on change.
    pub(crate) group_camera_state: HashMap<i32, bool>,
    /// Phase C2e: selected camera device id; `None` means the engine
    /// default. No devices are fabricated, so this can legitimately be
    /// unset.
    pub(crate) selected_camera: Option<String>,
    /// Phase C2c: last-enumerated audio devices and the selected
    /// (microphone, speaker) ids. `None` means the engine default; no
    /// devices are fabricated, so this can legitimately be empty.
    pub(crate) call_devices_cache: Vec<MediaDevice>,
    pub(crate) selected_devices: (Option<String>, Option<String>),
    pub(crate) call_connect_params: Option<ConnectParams>,
    pub(crate) reconnect_attempts: usize,
    pub(crate) credentials: TelegramCredentials,
    pub(crate) paths: AccountPaths,
    pub(crate) database_key: DatabaseKey,
    pub(crate) parameters_sent: bool,
    pub(crate) search_debounce_token: u64,
    pub(crate) pending_typed_search: Option<(u64, String)>,
    pub(crate) chat_search_debounce_token: u64,
    pub(crate) pending_typed_chat_search: Option<(u64, String)>,
    /// Last `chatActionTyping` we sent (Unigram `_lastTypingTime`).
    pub(crate) outgoing_typing: Option<OutgoingTyping>,
    /// Last `chatActionRecordingVoiceNote` (Unigram record button).
    pub(crate) outgoing_voice: Option<OutgoingTyping>,
    /// tdesktop `saveDraft` clock for the open composer.
    pub(crate) draft_clock: DraftSaveClock,
    pub(crate) draft_save_token: u64,
    pub(crate) pending_draft: Option<PendingDraft>,
}

pub(crate) struct OutgoingTyping {
    pub(crate) chat_id: ChatId,
    pub(crate) last_sent_ms: u64,
}

pub(crate) struct PendingDraft {
    pub(crate) token: u64,
    pub(crate) chat_id: ChatId,
    pub(crate) text: String,
    pub(crate) reply_to: Option<SendReply>,
}

/// Result of noting a composer edit. UI arms a timer only for `Debounced`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftSaveOutcome {
    Debounced { token: u64, delay: Duration },
    Sent,
    Skipped,
}

/// Slice A4: which flag a session-row toggle flips (module scope — Rust
/// forbids enums inside `impl`). `ponytail:` a two-case enum plus one
/// shared send path beats two near-duplicate driver methods; upgrade
/// only if more per-session flags land.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ToggleSessionKind {
    SecretChats,
    Calls,
}
