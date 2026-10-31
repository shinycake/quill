//! Connect gate types: blockers, parameters, driver state.
use super::*;
use crate::auth::StartupFailure;
use crate::calls::engine::{CallEngine, ConnectParams, MediaDevice};
use crate::composer::DraftSaveClock;
use crate::credentials::TelegramCredentials;
use crate::ids::{AccountKey, ChatId, RequestId};
use crate::lifecycle::RestoreBlocker;
use crate::platform::DatabaseKey;
use crate::settings::AccountPaths;
use crate::state::Session;
use crate::telegram::ffi::LibraryOrigin;
use crate::telegram::requests::SendReply;
use std::collections::{HashMap, VecDeque};
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
/// `downloadFile.priority` for chat-list avatars, fetched for every chat
/// whether or not the list shows it (schema: 1–32).
pub const CHAT_LIST_PHOTO_PRIORITY: i32 = 1;
/// `downloadFile.priority` for automatic photo thumbs and the avatars a
/// view is showing: above the chat-list avatars.
pub const THUMB_DOWNLOAD_PRIORITY: i32 = 2;
/// `downloadFile.priority` for automatic full-media downloads (MED3: above
/// thumbs so enabled media actually arrives, far below explicit user
/// downloads).
pub const AUTO_MEDIA_DOWNLOAD_PRIORITY: i32 = 4;
/// Images and clips of the inline results panel (`@gif cats`): above
/// the open chat's thumbnails and auto-downloads, below a download the
/// user asked for.
pub const INLINE_RESULTS_DOWNLOAD_PRIORITY: i32 = 16;
/// Story viewer preload (tdesktop `preloadNext`): the next story gets the
/// highest of these, then the one after, then the previous one. All sit
/// above thumbs and chat auto-downloads and below the current story.
pub const STORY_PRELOAD_PRIORITIES: [i32; 3] = [7, 6, 5];
/// `downloadFile.priority` when the user opens media.
pub const USER_DOWNLOAD_PRIORITY: i32 = 32;
/// `searchChats.limit` / `searchMessages.limit` (Unigram messages page is 20).
pub const SEARCH_LIMIT: i32 = 20;
/// `searchRecentlyFoundChats.limit` — schema/Unigram cap is 50.
pub const RECENT_SEARCH_LIMIT: i32 = 50;
/// `getTopChats.limit` for the "Frequent contacts" strip (tdesktop shows the
/// first row and expands on "Show all"; TDLib allows up to 30).
pub const TOP_CHATS_LIMIT: i32 = 30;
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
    /// The "Public posts" scope: one `searchPublicPosts` /
    /// `searchPublicMessagesByTag` request.
    PublicPosts(RequestId),
    /// A media tab on an empty field: one `searchMessages` with the tab's
    /// filter and no query.
    Media(RequestId),
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
    /// The loaded tdjson is not the TDLib version this build pins.
    TdjsonVersion,
    /// The OS refused to start the TDLib receive thread.
    ReceiveThread,
    DataFolder,
}

impl ConnectBlocker {
    /// What the failure card says. `None` for a build with no API
    /// credentials, where trying again cannot help.
    pub fn startup_failure(&self) -> Option<StartupFailure> {
        match self {
            ConnectBlocker::MissingCredentials => None,
            ConnectBlocker::MissingTdjson
            | ConnectBlocker::TdjsonLoad
            | ConnectBlocker::TdjsonVersion => Some(StartupFailure::Library),
            ConnectBlocker::MissingKeyAgainstExistingDb => Some(StartupFailure::KeyMissing),
            ConnectBlocker::LockedStore => Some(StartupFailure::KeyLocked),
            ConnectBlocker::StoreError => Some(StartupFailure::KeyUnreadable),
            ConnectBlocker::ReceiveThread => Some(StartupFailure::NoThread),
            ConnectBlocker::DataFolder => Some(StartupFailure::DataFolder),
        }
    }

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
            ConnectBlocker::ReceiveThread => {
                "could not start the Telegram receive thread — close other apps and retry"
            }
            ConnectBlocker::TdjsonLoad => {
                "tdjson library found but failed to load (missing symbols or wrong arch)"
            }
            ConnectBlocker::TdjsonVersion => {
                "tdjson is a different TDLib version than this build of Quill needs — rebuild it with scripts/build-tdlib.sh"
            }
            ConnectBlocker::DataFolder => "the account's data folder could not be used",
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
            ConnectBlocker::TdjsonVersion => "tdjson-version",
            ConnectBlocker::ReceiveThread => "receive-thread",
            ConnectBlocker::DataFolder => "data-folder",
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
    /// R8: an edited text exceeded `getOption("message_text_length_max")`
    /// (tdesktop refuses the edit with `lng_edit_limit_reached`). New
    /// messages are split instead, so only edits can raise this.
    TextTooLong {
        limit: i32,
    },
    /// The agent UI driver was injecting input and the request is not one
    /// the window may send for an agent (`agent::ui_driver::guard`).
    AgentDriving,
    /// No call engine (libquill_calls did not load): a voice chat, live
    /// stream or live story is not joined, since nothing would play or
    /// send. The UI says calls are unavailable, as for a 1:1 call.
    CallsUnavailable,
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

/// Session + outbound sender that auto-replies to `WaitTdlibParameters`.
pub struct ConnectDriver<S: JsonSender> {
    pub session: Session,
    pub(crate) sender: S,
    /// Poll-batch state (`begin_batch` / `end_batch`).
    pub(crate) batch: super::core::batch::BatchState,
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
    /// Engine-emitted peer 1:1 microphone states `(call id, muted)`
    /// (worker thread -> driver pump).
    pub(crate) audio_state_outbox: Arc<Mutex<VecDeque<(i32, bool)>>>,
    /// Your own microphone level in a group call (`connect::call_audio`).
    pub(crate) call_audio: super::call_audio::CallAudio,
    /// The call microphone Quill captures (`connect::call_capture`).
    pub(crate) call_capture: super::call_capture::CallCapture,
    /// The shared screen Quill captures (`connect::call_screen`).
    pub(crate) call_screen: super::call_screen::CallScreen,
    /// Every picture (group-call tracks, streams, 1:1 calls): one live
    /// feed each, newest frame only (`group_calls/live_feeds.rs`).
    pub(crate) live_feeds: super::group_calls::live_feeds::LiveFeeds,
    /// Phase C2g: the camera last issued to the engine per group call id
    /// (`None`: off); the engine is only re-issued on change.
    pub(crate) group_camera_state: HashMap<i32, Option<String>>,
    /// Microphone mute state the group-call engine last applied, per call.
    pub(crate) group_mute_state: HashMap<i32, bool>,
    /// The conference call's frame encryption and the engine key it is
    /// under (`group_calls::conference`).
    pub(crate) conference_crypto: Option<super::group_calls::ConferenceCrypto>,
    /// Stream-mode group calls: broadcast requests, feeders, pictures.
    pub(crate) group_call_streams: super::group_calls::stream::GroupCallStreams,
    /// Everyone's speaking levels in the joined group call
    /// (`group_calls/levels.rs`).
    pub(crate) group_levels: super::group_calls::levels::GroupLevels,
    /// The "You are on mute" hint (`group_calls/muted_hint.rs`).
    pub(crate) muted_hint: super::group_calls::muted_hint::MutedHint,
    /// Phase C2e: selected camera device id; `None` means the engine
    /// default. No devices are fabricated, so this can legitimately be
    /// unset.
    pub(crate) selected_camera: Option<String>,
    /// Phase C2c: last-enumerated audio devices and the selected
    /// (microphone, speaker) ids. `None` means the engine default; no
    /// devices are fabricated, so this can legitimately be empty.
    pub(crate) call_devices_cache: Vec<MediaDevice>,
    pub(crate) selected_devices: (Option<String>, Option<String>),
    /// The group calls whose sound you muted for yourself (the live
    /// stream's settings box, the live story's sound toggle).
    pub(crate) group_playback_muted: std::collections::HashSet<i32>,
    pub(crate) call_connect_params: Option<ConnectParams>,
    pub(crate) reconnect_attempts: usize,
    /// The 1:1 call whose sound is watched (`connect::calls::sound_watch`).
    pub(crate) call_sound_watch: Option<(i32, crate::calls::stream_audio::watch::PlaybackWatch)>,
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
    /// Q1: idempotent reads waiting out a 429 before being re-sent.
    pub(crate) flood_retries: Vec<super::flood_retry::FloodRetry>,
    /// Q1: re-send count per request id that hit a rate limit.
    pub(crate) flood_attempts: HashMap<u64, u8>,
    /// R5: when the stale-request sweep last scanned the registry.
    pub(crate) last_request_sweep: Option<std::time::Instant>,
    /// The last "Use system proxy settings" step sent and when; a step
    /// TDLib refused is not sent again until the OS setting changes or
    /// `system_proxy::RETRY` passes.
    pub(crate) system_proxy_sent: Option<crate::system_proxy::SentStep>,
    /// Where this client's web proxy carriers run (`connect::proxy_carrier`);
    /// `None` refuses them.
    pub(crate) carrier_host: Option<Box<dyn crate::proxy_carrier::CarrierHost>>,
}

pub(crate) struct OutgoingTyping {
    pub(crate) chat_id: ChatId,
    pub(crate) last_sent_ms: u64,
}

pub(crate) struct PendingDraft {
    pub(crate) token: u64,
    pub(crate) chat_id: ChatId,
    /// The slot the edit belongs to, fixed when the timer was armed: a
    /// topic or thread switch before it fires still saves to the right one.
    pub(crate) topic: crate::composer::DraftTopic,
    pub(crate) content: crate::composer::DraftContent,
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
