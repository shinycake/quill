mod synthetic;

use gpui_kit::component::button::*;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState, SliderValue};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::auth::{AuthAction, AuthView, view_for};
use quill::composer::{
    AttachmentKind, CommandMenuItem, ComposerAttachment, ComposerEdit, ComposerReplyTo,
    ComposerSnapshot, DeleteConfirm, ForwardDraft, begin_edit_keeping_reply, cancel_edit_draft,
    cancel_edit_keeping_reply, cancel_forward_draft, cancel_reply_draft, command_menu_trigger,
    draft_text_to_store, filter_command_menu_items, should_send_on_enter,
    strip_command_menu_trigger,
};
use quill::connect::{
    ChatSearchQueryOutcome, ConnectBlocker, ConnectGate, DraftSaveOutcome, LiveConnect,
    SEARCH_DEBOUNCE, SearchQueryOutcome, SoundResolution, USER_DOWNLOAD_PRIORITY, evaluate_gate,
    start_live_connect,
};
use quill::credentials::TelegramCredentials;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::folders::FolderEditor;
use quill::ids::{AccountKey, ChatId, FileId, MessageId};
use quill::key_fingerprint;
use quill::local_path::sandboxed_display_path;
use quill::media_viewer::{
    MediaViewer, MediaViewerItem, MediaViewerKind, ViewerVideoStart, ViewerZoom,
    collect_media_items, decide_viewer_video_start,
};
use quill::notify::{NotificationSoundKind, QueuedNotification};
use quill::platform::live_secret_store;
use quill::playback::PlaybackClock;
use quill::poll::{
    POLL_OPTIONS_MAX, POLL_OPTIONS_MIN, PollDraft, poll_bar_fraction, voter_count_label,
};
use quill::state::{
    ActiveCall, CallSummary, ChatSearchJump, ChatSummary, ContactRow, ForwardResult,
    HistoryMessage, InfoPanelTarget, OutboxReceipt, RequestPurpose, SearchStatus, Session,
    SponsoredReportFlight, outgoing_status_label, unix_ms_now, unread_badge_text,
};
use quill::story_viewer::{StoryViewer, StoryViewerItem, StoryViewerKind, collect_story_items};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    AuthorizationState, BotInfo, CallState, CallbackQueryAnswer, ChannelMemberStatus, ChatDraft,
    ChatFolderInfo, ChatFolderSpec, ChatKind, ChatList, ChatNotificationSettings,
    DEFAULT_EMOJI_REACTIONS, ForumTopic, InlineKeyboardButton, InlineKeyboardButtonStyle,
    InlineKeyboardButtonType, MUTE_FOR_1_HOUR, MUTE_FOR_2_DAYS, MUTE_FOR_8_HOURS, MUTE_FOREVER,
    MessageContent, MessageInteractionInfo, NotificationSettingsScope, NotificationSound,
    ParsedFile, ParsedSecretChat, ParsedStory, PollContent, PollOption, PollType,
    ScopeNotificationSettings, SecretChatState, SponsoredMessage, chat_ttl_service_label,
    format_ttl_setting, toggle_chosen_emoji_reaction,
};
use quill::telegram::requests::SelfDestructSend;
use quill::text::{TextEntity, styled_runs, utf8_to_utf16_offset};
use quill::voice::{self, VoiceCapture, format_voice_duration};
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;
use synthetic::{SyntheticChat, session_bubble_quoted, session_bubble_rich};
use zeroize::Zeroize;

actions!(
    quill_ui,
    [
        FocusSidebar,
        FocusComposer,
        LoadOlder,
        OpenSearch,
        OpenChatSearch,
        ChatSearchNewer,
        ChatSearchOlder,
        CancelSearch,
        QuitApp,
        SubmitPhone,
        SubmitCode,
        SubmitPassword,
        /// Parity slice 5: step the fullscreen media viewer to the
        /// previous / next item (left/right arrows, viewer-open only).
        ViewerPrev,
        ViewerNext,
        /// Parity slice 5: reset the viewer visual's zoom/pan to fit (`0`).
        ViewerZoomReset,
        /// Parity slice 5: zoom the viewer visual in/out (`=` / `-`,
        /// viewer-open only).
        ViewerZoomIn,
        ViewerZoomOut
    ]
);

/// Phase 8.1: cap on concurrent OS-notification worker threads (`notify-send
/// --wait` blocks until dismissal). Excess bursts are dropped, not stacked.
const MAX_OS_NOTIFICATION_THREADS: usize = 8;
/// Parity slice: cap for concurrent `quill-sound` player threads.
const MAX_OS_NOTIFICATION_SOUND_THREADS: usize = 2;

/// Parity slice: which settings object a sound-picker choice applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SoundPickerTarget {
    Chat(ChatId),
    Scope(NotificationSettingsScope),
}

/// Parity slice: a sound choice in the picker UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SoundChoice {
    /// App default tone (chat `use_default_sound`, scope `sound_id = -1`).
    Default,
    /// No sound (chat/scope `sound_id = 0`).
    Disabled,
    /// A saved notification sound id.
    Custom(i64),
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-q", QuitApp, None),
        KeyBinding::new("ctrl-q", QuitApp, None),
        KeyBinding::new("cmd-1", FocusSidebar, None),
        KeyBinding::new("ctrl-1", FocusSidebar, None),
        KeyBinding::new("cmd-l", FocusComposer, None),
        KeyBinding::new("ctrl-l", FocusComposer, None),
        KeyBinding::new("cmd-up", LoadOlder, None),
        KeyBinding::new("ctrl-up", LoadOlder, None),
        KeyBinding::new("cmd-k", OpenSearch, None),
        KeyBinding::new("ctrl-k", OpenSearch, None),
        KeyBinding::new("cmd-f", OpenChatSearch, None),
        KeyBinding::new("ctrl-f", OpenChatSearch, None),
        KeyBinding::new("cmd-g", ChatSearchNewer, None),
        KeyBinding::new("ctrl-g", ChatSearchNewer, None),
        KeyBinding::new("cmd-shift-g", ChatSearchOlder, None),
        KeyBinding::new("ctrl-shift-g", ChatSearchOlder, None),
        KeyBinding::new("escape", CancelSearch, None),
        // Parity slice 5: the handlers no-op (and let the keystroke reach
        // text inputs) unless the media viewer is open.
        KeyBinding::new("left", ViewerPrev, None),
        KeyBinding::new("right", ViewerNext, None),
        KeyBinding::new("0", ViewerZoomReset, None),
        KeyBinding::new("=", ViewerZoomIn, None),
        KeyBinding::new("-", ViewerZoomOut, None),
    ]);
}

/// Startup connect classification for the status bar (no secrets).
#[derive(Clone, PartialEq, Eq)]
pub enum ConnectUiStatus {
    NeedCredentials,
    NeedTdjson,
    RestoreBlocked(&'static str),
    /// Synthetic WaitPhoneNumber surface for screenshot proof (no live TDLib).
    DemoWaitPhone,
    /// Synthetic WaitCode surface for screenshot proof (no live TDLib).
    DemoWaitCode,
    /// Synthetic WaitPassword surface for screenshot proof (no live TDLib).
    DemoWaitPassword,
    /// Injected Ready + main chat list (no live Telegram).
    DemoReadyChats,
    Live,
}

/// Phase 4.2: poll creation dialog above the composer. Textarea entities are
/// created when the dialog opens (option rows are dynamic); the dialog
/// freezes into a validated `PollDraft` on "Create poll".
pub struct PollDialog {
    question_input: Entity<TextareaState>,
    option_inputs: Vec<Entity<TextareaState>>,
    is_anonymous: bool,
    allows_multiple_answers: bool,
}

impl PollDialog {
    fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let question_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Poll question")
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        let option_inputs = (0..POLL_OPTIONS_MIN)
            .map(|index| {
                cx.new(|cx| {
                    TextareaState::new(window, cx)
                        .placeholder(format!("Option {}", index + 1))
                        .auto_grow(1, 2)
                        .submit_on_enter(false)
                })
            })
            .collect();
        Self {
            question_input,
            option_inputs,
            is_anonymous: true,
            allows_multiple_answers: false,
        }
    }

    /// Freeze the dialog inputs into a `PollDraft` (validated by the caller).
    fn draft(&self, cx: &App) -> PollDraft {
        PollDraft {
            question: self.question_input.read(cx).value().to_string(),
            options: self
                .option_inputs
                .iter()
                .map(|input| input.read(cx).value().to_string())
                .collect(),
            is_anonymous: self.is_anonymous,
            allows_multiple_answers: self.allows_multiple_answers,
        }
    }
}

/// Phase 6: add-contact dialog opened from the user info panel. The phone
/// number is required — `addContact` needs an `importedContact` and Quill
/// does not offer adding by bare user id.
pub struct AddContactDialog {
    user_id: i64,
    phone_input: Entity<TextareaState>,
    first_name_input: Entity<TextareaState>,
    last_name_input: Entity<TextareaState>,
}

impl AddContactDialog {
    fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        user_id: i64,
        phone: &str,
        first_name: &str,
        last_name: &str,
    ) -> Self {
        let phone_input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder("Phone number")
                .auto_grow(1, 1)
                .submit_on_enter(false);
            state.set_value(phone, window, cx);
            state
        });
        let first_name_input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder("First name")
                .auto_grow(1, 1)
                .submit_on_enter(false);
            state.set_value(first_name, window, cx);
            state
        });
        let last_name_input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder("Last name")
                .auto_grow(1, 1)
                .submit_on_enter(false);
            state.set_value(last_name, window, cx);
            state
        });
        Self {
            user_id,
            phone_input,
            first_name_input,
            last_name_input,
        }
    }

    /// `None` when the phone field is empty (the Add button no-ops then).
    fn draft(&self, cx: &App) -> Option<(i64, String, String, String)> {
        let phone = self.phone_input.read(cx).value().to_string();
        if phone.trim().is_empty() {
            return None;
        }
        Some((
            self.user_id,
            phone,
            self.first_name_input.read(cx).value().to_string(),
            self.last_name_input.read(cx).value().to_string(),
        ))
    }
}

/// Parity slice: create/edit chat-folder dialog. The editable folder model
/// is [`FolderEditor`]; on save it freezes to a [`ChatFolderSpec`] sent via
/// `createChatFolder` / `editChatFolder`.
pub struct FolderEditorDialog {
    /// `None` = create; `Some(id)` = edit.
    folder_id: Option<i32>,
    editor: FolderEditor,
    name_input: Entity<TextareaState>,
    /// Edit flow: waiting on `getChatFolder` before the editor prefills.
    fetch_pending: bool,
    error: Option<String>,
}

impl FolderEditorDialog {
    fn new(window: &mut Window, cx: &mut Context<QuillApp>, folder_id: Option<i32>) -> Self {
        let name_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Folder name (1–12 characters)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            folder_id,
            editor: FolderEditor::new(),
            name_input,
            fetch_pending: folder_id.is_some(),
            error: None,
        }
    }

    /// Prefill from the `getChatFolder` response (edit flow).
    fn prefill_from_spec(
        &mut self,
        spec: &ChatFolderSpec,
        window: &mut Window,
        cx: &mut Context<QuillApp>,
    ) {
        self.editor = FolderEditor::from_spec(spec);
        self.fetch_pending = false;
        self.error = None;
        let name = spec.name.clone();
        self.name_input.update(cx, |input, cx| {
            input.set_value(name, window, cx);
        });
    }

    fn name(&self, cx: &App) -> String {
        self.name_input.read(cx).value().to_string()
    }
}

/// Parity slice: delete-folder confirmation. Optionally leaves suggested
/// chats with the folder (`getChatFolderChatsToLeave`).
pub struct FolderDeleteConfirm {
    folder_id: i32,
    name: String,
    leave_with_folder: bool,
}

pub struct QuillApp {
    chat: Entity<SyntheticChat>,
    composer: Entity<TextareaState>,
    /// Phase 3.3: `/` command menu state. Open while the composer text
    /// ends with a `/`-led token and the open bot chat has commands;
    /// `command_menu_selected` is the highlighted row (Up/Down/Enter).
    command_menu_open: bool,
    command_menu_selected: usize,
    phone_input: Entity<TextareaState>,
    code_input: Entity<TextareaState>,
    password_input: Entity<TextareaState>,
    search_input: Entity<TextareaState>,
    chat_search_input: Entity<TextareaState>,
    forward_search_input: Entity<TextareaState>,
    auth_demo: AuthorizationState,
    focus_sidebar: FocusHandle,
    connect_status: ConnectUiStatus,
    live: Option<LiveConnect>,
    status_note: String,
    /// Screenshot / synthetic demo: show the matching auth field without a live client.
    demo_auth_inputs: bool,
    /// Screenshot Ready list: same reducers as live, injected JSON only.
    demo_session: Option<Session>,
    demo_seq: AtomicU64,
    demo_sink: Arc<MemorySink>,
    /// Phase 8.1: chat ids whose OS notification was clicked (set by the
    /// notification worker threads); the next render focuses the chat.
    notify_clicks: Arc<Mutex<Vec<ChatId>>>,
    /// Phase 8.1: in-flight OS notification workers; capped so a message
    /// burst cannot stack threads.
    notify_inflight: Arc<AtomicUsize>,
    /// Local files the user explicitly attached (canonical paths via `pick`).
    /// One item sends with `sendMessage`. Two or more photos/videos send with
    /// `sendMessageAlbum`.
    pending_attachments: Vec<ComposerAttachment>,
    /// Phase B3: the composer's self-destruct choice for photo/video
    /// sends (`inputMessagePhoto`/`inputMessageVideo`
    /// `self_destruct_type`, schema 1.8.67 lines 6117/6128 — private
    /// chats only). Cycles Off → 5s → 30s → 1m → View once via the
    /// picker button; captured into `ComposerSnapshot` at submit time.
    composer_self_destruct: Option<SelfDestructSend>,
    /// Same-chat reply draft (tdesktop `FieldHeader::replyToMessage`).
    pending_reply: Option<ComposerReplyTo>,
    /// Chat whose draft should be cleared after `updateMessageSendSucceeded`
    /// if the composer is still empty.
    clear_draft_on_success: Option<ChatId>,
    /// Own-message edit (tdesktop `FieldHeader::editMessage`).
    pending_edit: Option<ComposerEdit>,
    /// Normal composer draft stashed while editing (`DraftType::Normal`).
    saved_edit_draft: String,
    /// Reply that belonged to that normal draft. Restored with the text on cancel.
    saved_edit_reply: Option<ComposerReplyTo>,
    /// Delete confirm (tdesktop `DeleteMessagesBox` / Unigram popup).
    pending_delete: Option<DeleteConfirm>,
    /// Phase B1: pending "Close secret chat" confirm for the open chat
    /// (`closeSecretChat`, schema 1.8.67 line 15242).
    pending_close_secret_chat: Option<ChatId>,
    /// tdesktop `Data::ForwardDraft` / history multi-select.
    pending_forward: Option<ForwardDraft>,
    /// ShareBox / `ShowForwardMessagesBox` dest picker overlay.
    forward_picker_open: bool,
    /// Last successful (or failed) `forwardMessages` result.
    forward_result: Option<ForwardResult>,
    /// tdesktop hover React / Unigram ReactionButton picker (emoji only).
    pending_react: Option<(ChatId, MessageId)>,
    /// tdesktop Mute submenu (1 hour / 8 hours / 2 days / Forever).
    mute_menu_open: bool,
    /// Phase B4: self-destruct / auto-delete timer picker below the
    /// conversation header (`setChatMessageAutoDeleteTime`).
    ttl_picker_open: bool,
    /// Parity slice: the notifications panel's sound picker sub-view is open.
    notif_sound_picker_open: bool,
    /// Parity slice: scope-default notification settings dialog is open.
    notification_defaults_open: bool,
    /// Parity slice: which scope section's sound picker is expanded in the
    /// defaults dialog (`None` = all collapsed).
    defaults_sound_picker: Option<NotificationSettingsScope>,
    /// Parity slice: in-flight notification-sound workers; capped so a
    /// message burst cannot stack players.
    notify_sound_inflight: Arc<AtomicUsize>,
    /// tdesktop `VoiceRecordBar` (click mic to record; Esc / Cancel discards).
    voice_capture: Option<VoiceCapture>,
    voice_tick: bool,
    /// Phase A1: the open chat whose slow-mode countdown is ticking
    /// (`Some` exactly while the 1s tick task runs). Mirrors `voice_tick`.
    slow_mode_tick_chat: Option<ChatId>,
    /// Phase B3: the open chat whose self-destruct countdown badges are
    /// ticking (`Some` exactly while the 1s tick task runs). Mirrors
    /// `slow_mode_tick_chat`.
    self_destruct_tick_chat: Option<ChatId>,
    /// Phase C1: whether the call-duration 1s tick task is running
    /// (keeps the overlay's ringing/connected clock fresh). Mirrors
    /// `voice_tick`.
    call_tick_active: bool,
    /// History row whose voice note is playing.
    playing_voice: Option<MessageId>,
    /// History row whose music file (`messageAudio`) is playing. Shares `voice_player`.
    playing_audio: Option<MessageId>,
    /// Play was tapped before the track was local. Resume when `downloadFile` finishes.
    pending_audio_play: Option<(MessageId, FileId, f64)>,
    voice_player: Option<Child>,
    /// Active audio/voice track's playback clock (playing or paused-with-offset).
    /// `Some` exactly when `playing_voice` or `playing_audio` is `Some` (Phase 4.6).
    playback_clock: Option<PlaybackClock>,
    /// Sandbox-checked local path of the active track, for ffplay restarts on seek.
    playback_path: Option<PathBuf>,
    /// Interactive seek slider bound to the active row (Phase 4.6).
    seek_slider: Option<Entity<SliderState>>,
    /// True while the user is dragging the seek slider (Change without Release yet).
    seek_scrubbing: bool,
    /// Drag preview position in seconds, shown in the time label while scrubbing.
    seek_preview_secs: Option<f64>,
    /// Guard for the playback progress tick task.
    playback_tick: bool,
    /// Last known position per message, so rows keep their seek bar fill (and
    /// resume from it) after pause/stop.
    playback_positions: HashMap<MessageId, f64>,
    /// History row whose GIF is looping (tdesktop clip / Unigram player).
    playing_animation: Option<MessageId>,
    animation_frames: Vec<PathBuf>,
    animation_frame: usize,
    animation_tick: bool,
    /// File whose extracted frames should be deleted when playback stops.
    animation_cache_file: Option<i32>,
    /// Play was tapped before the clip was local. Resume when `downloadFile` finishes.
    pending_gif_play: Option<(MessageId, FileId, String)>,
    /// History row whose video preview is looping.
    playing_video: Option<MessageId>,
    video_frames: Vec<PathBuf>,
    video_frame: usize,
    video_tick: bool,
    video_cache_file: Option<i32>,
    /// Play was tapped before the video was local. Resume when `downloadFile` finishes.
    /// The last field is the chat to mark opened (`openMessageContent`) once playback starts.
    pending_video_play: Option<(MessageId, FileId, String, i32, Option<ChatId>)>,
    /// `ReadySponsored` fixture surface: the demo channel renders sponsored rows
    /// instead of history. Normal live path unchanged.
    sponsored_demo: bool,
    /// Phase 4.1: revealed text-entity spoilers, keyed by
    /// (chat id, message id, run index, is-caption block). Message ids are
    /// only unique within a chat, so the chat id is part of the key.
    spoiler_revealed: HashSet<(i64, u64, u64, bool)>,
    /// Phase 4.2: poll creation dialog (open above the composer).
    poll_dialog: Option<PollDialog>,
    /// Phase 4.5: fullscreen media viewer (photo/video overlay).
    media_viewer: MediaViewer,
    /// Parity slice 5: zoom/pan of the viewer visual (reset on open/step).
    viewer_zoom: ViewerZoom,
    /// Parity slice 5: drag-pan anchor — last mouse position in px while the
    /// left button is held over the zoomed visual.
    viewer_drag: Option<(f32, f32)>,
    /// Parity slice 5: message whose video clip is playing in the viewer
    /// (ffplay child alive) or paused (clock frozen, no child).
    viewer_video: Option<MessageId>,
    /// Sandbox-checked local path of the viewer's clip, for pause/resume
    /// ffplay restarts.
    viewer_video_path: Option<PathBuf>,
    /// The viewer's ffplay child (audio-only `-nodisp`; the video frames
    /// render in-viewer). Killed when the viewer closes, steps, or pauses.
    viewer_player: Option<Child>,
    /// Playback clock for the viewer's clip (elapsed/total + pause freeze).
    viewer_clock: Option<PlaybackClock>,
    /// Decoded frames for the viewer's clip, rendered in-place (parity
    /// slice 5). Pre-decoded `RenderImage` handles: `img()` resolves
    /// `ImageSource::Render` synchronously, so the 125 ms tick can cycle
    /// frames without an async load round trip per frame. Empty until
    /// extraction + decode finish; the thumbnail shows meanwhile.
    viewer_video_frames: Vec<Arc<RenderImage>>,
    /// Frame rate of `viewer_video_frames`, for clock → frame-index mapping.
    viewer_video_fps: f64,
    /// File ID whose frames are in `viewer_video_frames` (cache invalidation).
    viewer_frame_cache_file: Option<i32>,
    /// Frame extraction in progress (async); the viewer shows the thumbnail
    /// with a "loading video" hint until frames land.
    viewer_extracting: bool,
    /// Running ffmpeg viewer-frame extraction, published by the background
    /// task. Taken and killed when the viewer closes or steps; stale
    /// completions are dropped by `viewer_extract_epoch`.
    viewer_extract_child: Option<Arc<Mutex<Option<Child>>>>,
    /// Shared cancellation flag for the in-flight extraction: set by
    /// `kill_viewer_extraction` so the worker can abort even if the UI
    /// kills before ffmpeg publishes its child into `viewer_extract_child`.
    viewer_extract_cancel: Option<Arc<AtomicBool>>,
    /// Generation counter for viewer frame extraction: bumped on every new
    /// extraction and on cancel, so a late completion from an abandoned run
    /// is dropped silently (no error note, no playback).
    viewer_extract_epoch: u64,
    /// Screenshot demo only: skip the async frame extraction in
    /// `maybe_autoplay_viewer_video` (the demo extracts + decodes
    /// synchronously itself for a deterministic capture).
    viewer_demo_sync_frames: bool,
    /// Guard for the viewer's 250 ms elapsed tick.
    viewer_tick: bool,
    /// Play was requested before the clip was local. Resumed from the poll
    /// loop when `downloadFile` finishes.
    viewer_pending_play: Option<(MessageId, FileId)>,
    /// Phase 9.1: fullscreen story viewer (active-story tray → overlay).
    story_viewer: StoryViewer,
    /// Phase 9.1: `(chat_id, story_id)` the user tapped while the story's
    /// full content was still being fetched; resolved on the next render
    /// once the `story` response lands in the cache.
    pending_story_open: Option<(i64, i32)>,
    /// Phase 9.2: story reaction picker open above the viewer overlay.
    story_reaction_picker_open: bool,
    /// Phase 9.2: story reply input open in the viewer overlay.
    story_reply_open: bool,
    /// Phase 9.2: reply-to-story draft (the viewer overlay's reply row).
    story_reply_input: Entity<TextareaState>,
    /// Phase 6: sidebar tab — `true` shows the contacts list instead of
    /// the chat list.
    contacts_tab_open: bool,
    /// Phase 7.1: selected folder tab (`None` = Main). Folder membership
    /// comes from chat positions (`chatListFolder`); the tab only filters.
    folder_tab: Option<i32>,
    /// Phase 6: add-contact dialog (phone + first/last name) opened from
    /// the user info panel.
    add_contact_dialog: Option<AddContactDialog>,
    /// Parity slice: folder management (manage dialog / editor / delete
    /// confirm / per-chat folder menu).
    folder_manage_open: bool,
    folder_editor: Option<FolderEditorDialog>,
    folder_delete_confirm: Option<FolderDeleteConfirm>,
    folder_menu_open: bool,
}

/// Forced UI surfaces for screenshot proof (no live Telegram / no real credentials).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenshotDemo {
    NeedTdjson,
    WaitPhone,
    WaitCode,
    WaitPassword,
    ReadyChats,
    ReadyChatsComposer,
    ReadyUnread,
    ReadyUnreadRead,
    ReadyMedia,
    /// Composer attachment chip + outgoing photo/document (injected, no live Telegram).
    ReadySendMedia,
    /// Sidebar search over injected recents / `searchChats` / `searchMessages`.
    ReadySearch,
    /// In-chat search (`searchChatMessages`) + jump-to-message.
    ReadySearchInChat,
    /// Reply-to-message: composer quote + history quote strip.
    ReadyReply,
    /// Own-message edit mode + delete confirm (injected, no live Telegram).
    ReadyEditDelete,
    /// Forward select + dest picker + success (injected, no live Telegram).
    ReadyForward,
    /// Emoji react / unreact + chips (injected, no live Telegram).
    ReadyReactions,
    /// Pin / unpin + pinned banner (injected, no live Telegram).
    ReadyPin,
    /// Mute presets + muted icon + archive section (injected, no live Telegram).
    ReadyMuteArchive,
    /// Peer `chatActionTyping` in the open-chat header and sidebar row.
    ReadyTyping,
    /// Sticker panel + sticker in history (injected, no live Telegram).
    ReadyStickers,
    /// Voice record bar + history playback (injected, no live Telegram).
    ReadyVoice,
    /// Link entities + web page (`linkPreview`) card (injected, no live Telegram).
    ReadyLinkPreview,
    /// Saved-GIF panel + a playing animation in history (injected, no live Telegram).
    ReadyGifs,
    /// Video bubble with Play/Pause in history (injected, no live Telegram).
    ReadyVideo,
    /// Round video note with Play/Pause in history (injected, no live Telegram).
    ReadyVideoNote,
    /// Music file bubble with title, performer, cover, and Play/Pause.
    ReadyAudio,
    /// Composer video attach chip plus an own-sent video playing in history.
    ReadyVideoSend,
    /// Composer video-note attach chip plus an own-sent round note in history.
    ReadyVideoNoteSend,
    /// Restored private-chat composer draft (`draftMessage`).
    ReadyDrafts,
    /// Received photo album plus an own-sent album and a multi-attach composer.
    ReadyAlbums,
    /// Channel sponsored / recommended rows + report flow (injected, no live Telegram).
    /// Fixture/proof surface only; the channel opens normally in live use.
    ReadySponsored,
    /// Broadcast channel demo (injected, no live Telegram): the ungated demo
    /// channel (id 13) renders broadcast posts with channel author + view
    /// counts, composer hidden for the non-admin viewer, and the join/leave
    /// footer.
    ReadyChannels,
    /// Broadcast channel demo (injected, no live Telegram): the demo channel
    /// (id 13) with the viewer as an administrator
    /// (`rights.can_post_messages: true`), so the composer is visible above
    /// the broadcast posts (Phase 2.3).
    ReadyChannelsAdmin,
    /// Bot chat demo (injected, no live Telegram): private chat with a
    /// `userTypeBot` user (id 21), opened with history plus a cached
    /// `botInfo` (description + commands), so the bot panel renders under
    /// the header and the composer is visible (Phase 3.1).
    ReadyBotChat,
    /// Inline keyboard demo (injected, no live Telegram): like
    /// `ReadyBotChat`, but the bot message carries a
    /// `replyMarkupInlineKeyboard` with URL / callback / switchInline /
    /// copy-text / unknown (disabled) buttons (Phase 3.2).
    ReadyBotKeyboard,
    /// Bot command menu demo (injected, no live Telegram): like
    /// `ReadyBotChat`, plus a cached `getCommands` response (global
    /// scope) so the `/` command menu renders open above the composer
    /// with the bot-specific and "Global" sections (Phase 3.3).
    ReadyBotCommandMenu,
    /// Text-entity demo (injected, no live Telegram): a message with mixed
    /// entities (bold/italic/underline/strikethrough/spoiler/code/pre, incl.
    /// nested runs) plus a photo whose caption carries entities (Phase 4.1).
    ReadyTextEntities,
    /// Poll demo (injected, no live Telegram): an open regular poll with a
    /// voted option (percentage bars + counts, tapping an option flips the
    /// chosen mark locally) and a closed poll (results, no voting
    /// affordance) (Phase 4.2).
    ReadyPoll,
    /// Location / venue / contact demo (injected, no live Telegram): a
    /// plain `messageLocation` (coordinates + accuracy), a
    /// `messageLiveLocation` (live-period / expires / heading /
    /// proximity-alert state), a `messageVenue` (title + address +
    /// provider), and a `messageContact` (name + phone + vCard +
    /// `user_id`), each with a tappable "Open map" link (Phase 4.3).
    ReadyLocation,
    /// Dice demo (injected, no live Telegram): a few `messageDice` rows —
    /// 🎲 with values 4 / 6 and a 🎯 — showing the large static emoji
    /// face plus the rolled value (Phase 4.4). No roll animation.
    ReadyDice,
    /// Media viewer demo (injected, no live Telegram): the ReadyMedia
    /// photo chat with the viewer overlay open on the downloaded photo
    /// (Phase 4.5).
    ReadyMediaViewer,
    /// Video-playback demo (injected, no live Telegram): the ReadyMedia
    /// seed plus a downloaded video (message 204); the viewer opens on it
    /// with playback faked mid-track (no ffplay subprocess — the tick
    /// advances the elapsed label, like the seek-bars demo). The clip's
    /// 12 s duration is fixture data for the screenshot.
    /// (Parity slice 5.)
    ReadyVideoPlayback,
    /// Story viewer demo (injected, no live Telegram): the story tray above
    /// the chat list for "Demo chat A"/"Demo chat B" plus the story viewer
    /// overlay open on Demo chat A's downloaded photo story (Phase 9.1).
    ReadyStories,
    /// Story posting slice demo (injected, no live Telegram): same seed as
    /// `ReadyStories`, but Demo chat A's photo story carries a chosen ❤
    /// reaction, interaction counts, and deletable/repliable flags; the
    /// viewer opens with the **reaction picker** and **reply row** visible,
    /// plus a seeded `availableReactions` response (Phase 9.2). The photo
    /// composer itself is absent: the pinned TDLib 1.8.67 schema has no
    /// `sendStory` constructor, so posting cannot be built honestly yet.
    ReadyStoryPost,
    /// Seek-bar demo (injected, no live Telegram): a voice note playing
    /// with its seek bar mid-track (elapsed advancing via the playback
    /// tick) plus a music track paused with a remembered position, both
    /// showing elapsed/total time labels (Phase 4.6). No subprocess is
    /// spawned — playback state is faked.
    ReadySeekBars,
    /// Forum-topics demo (injected, no live Telegram): a forum supergroup
    /// whose `updateSupergroup` marks it a forum and whose `getForumTopics`
    /// response seeds three topics (General pinned + unread, Announcements
    /// with a last-message preview, Random closed), shown as the topic
    /// list (Phase 5.1).
    ReadyForumTopics,
    /// Topic-posting demo (injected, no live Telegram): the forum's General
    /// topic is open with an injected two-message history and the composer
    /// enabled — posting routes `sendMessage` with
    /// `topic_id = messageTopicForum` (parity slice 4).
    ReadyTopicPost,
    /// Contacts demo (injected, no live Telegram): the sidebar shows the
    /// **Contacts** tab (three injected contacts: Ada online, Zed last
    /// seen within a week, Noor recently) and the user info panel is open
    /// for Zed (bio from an injected `userFullInfo`) with the **Add
    /// contact** affordance (Phase 6).
    ReadyContacts,
    /// Phase 7.1: folder tabs (injected `updateChatFolders` + folder
    /// positions) with the non-default "News" folder selected, so the chat
    /// list shows only that folder's chats.
    ReadyFolders,
    /// Parity slice: folder manage dialog over the ReadyFolders fixture
    /// (injected, no live Telegram).
    ReadyFoldersManage,
    /// Parity slice: chat-list avatars (injected, no live Telegram) — the
    /// chat list mixes photo avatars (private chat A, the demo channel)
    /// and colored-initial fallbacks (private chat B, a basic group, the
    /// discussion supergroup); the demo channel (id 13) is open with its
    /// header photo, @username, subscriber count, description snippet, and
    /// a "Discuss" link to the injected discussion group (id 16).
    ReadyChatAvatars,
    /// Notification settings demo (injected, no live Telegram): the open
    /// chat has a custom notification sound (`getSavedNotificationSounds`
    /// fixture) and the per-chat notifications panel is open with the sound
    /// picker expanded (parity slice: notification sounds).
    ReadyNotificationSound,
    /// Phase A1: slow-mode enforcement (injected, no live Telegram) — a
    /// dedicated supergroup (id 17) with `slow_mode_delay: 30` and
    /// `slow_mode_delay_expires_in: 25.0`, the viewer a plain member (no
    /// bypass), opened with two messages. The composer shows the
    /// "Slow mode · wait Ns" countdown and blocks sends until expiry.
    ReadySlowMode,
    /// Phase B1: secret chat lifecycle (injected, no live Telegram) — a
    /// Ready secret chat (id 41) with Zed (user 41): `updateSecretChat`
    /// (Ready) → `updateNewChat` (`chatTypeSecret`) → history, opened
    /// with three E2E messages. The chat-list row shows the 🔒 badge and
    /// the composer is live (a Pending chat would show "Waiting for Zed
    /// to come online…" and a Closed chat "Secret chat closed" instead).
    ReadySecretChat,
    /// Phase B2: key verification UI (injected, no live Telegram) — the
    /// same Ready secret chat as `ReadySecretChat` but with a real
    /// 36-byte `key_hash` (deterministic fixture), and the partner's
    /// info panel open showing the "Encryption key" 12×12 fingerprint
    /// grid plus the verification copy.
    ReadyKeyVerification,
    /// Phase B3: self-destructing media (injected, no live Telegram) —
    /// a Ready *private* (1:1 cloud) chat with Zed: an incoming photo
    /// with a live 60s `messageSelfDestructTypeTimer` countdown, an
    /// outgoing `messageSelfDestructTypeImmediately` ("view once")
    /// photo, and a pending photo attachment with the composer's timer
    /// picker on 30s. Private chat — not a secret chat — because TDLib
    /// only accepts per-media `self_destruct_type` in
    /// `chatTypePrivate` chats (schema 1.8.67 lines 6117/6128,
    /// "private chats only").
    ReadySelfDestruct,
    /// Phase C1: call signaling UI (injected, no live Telegram) — an
    /// incoming `callStatePending` voice call from Zed, so the overlay
    /// renders the ringing incoming-call card (Accept / Decline, clock
    /// ticking). Signaling only: the card carries the honest
    /// no-audio-transport note.
    ReadyCall,
    /// Phase B4: chat-level auto-delete / self-destruct timer (injected,
    /// no live Telegram) — the Ready secret chat with Zed (id 41) with
    /// `message_auto_delete_time` 3600, one message carrying a live
    /// `auto_delete_in` countdown, a
    /// `messageChatSetMessageAutoDeleteTime` service row, and the timer
    /// picker expanded under the header.
    ReadyChatTtl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneMode {
    Synthetic,
    Connecting,
    Ready,
}

/// Which kind of track the shared ffplay child is playing (Phase 4.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlaybackKind {
    Voice,
    Audio,
}

/// Seek-bar view model for one audio/voice history row (Phase 4.6).
#[derive(Clone)]
struct SeekBarView {
    /// Interactive slider entity — `Some` only on the active (playing or
    /// paused) row. Inactive rows render a static bar instead.
    slider: Option<Entity<SliderState>>,
    /// Seconds shown in the time label and as bar fill: live elapsed, scrub
    /// preview, paused offset, or the remembered position for inactive rows.
    display_secs: f64,
    /// Total track length in seconds (TDLib `duration`).
    duration_secs: f64,
    /// True while the active row's player is actually running (vs paused).
    is_playing: bool,
}

/// Parity slice: data for the channel/supergroup conversation header —
/// photo, description snippet, primary @username, subscriber/member count,
/// and the linked discussion chat id (`linked_chat_id`, 0 = none).
struct SupergroupHeaderExtras {
    is_channel: bool,
    photo: Option<PathBuf>,
    username: Option<String>,
    member_count: Option<i32>,
    description_snippet: Option<String>,
    discussion_chat_id: Option<i64>,
}

impl SeekBarView {
    fn fraction(&self) -> f64 {
        if self.duration_secs <= 0.0 {
            return 0.0;
        }
        (self.display_secs / self.duration_secs).clamp(0.0, 1.0)
    }
}

impl QuillApp {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        credentials: Option<TelegramCredentials>,
    ) -> Self {
        Self::new_with_demo(window, cx, credentials, None)
    }

    pub fn new_with_demo(
        window: &mut Window,
        cx: &mut Context<Self>,
        credentials: Option<TelegramCredentials>,
        demo: Option<ScreenshotDemo>,
    ) -> Self {
        let chat = cx.new(SyntheticChat::new);
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Message — Enter sends, Shift+Enter newline. IME Enter must not send.")
                .auto_grow(2, 6)
                .submit_on_enter(true)
        });
        let phone_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Phone (+country code)")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let code_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Verification code")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let password_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Two-step password")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let chat_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search in chat")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let forward_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search chats")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let story_reply_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Reply to story")
                .auto_grow(1, 3)
                .submit_on_enter(true)
        });
        cx.subscribe_in(
            &composer,
            window,
            |this, state, event: &InputEvent, window, cx| {
                let text = state.read(cx).value().to_string();
                this.sync_composer_typing(&text);
                this.note_open_draft(true, cx);
                // Phase 3.3: the `/` menu tracks the composer text (Blur
                // dismisses it); Enter picks the highlighted command
                // instead of sending while the menu is open.
                match event {
                    InputEvent::Blur => {
                        this.close_command_menu(cx);
                    }
                    _ => this.sync_command_menu(cx),
                }
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(quill::composer::enter_event_from_kit(
                        *shift, *secondary, marked,
                    )) {
                        if this.pick_command_menu_selection(window, cx) {
                            // Enter was consumed by the open menu.
                        } else if !text.trim().is_empty() {
                            this.submit_composer(text, window, cx);
                        }
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &phone_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(quill::composer::enter_event_from_kit(
                        *shift, *secondary, marked,
                    )) {
                        this.submit_phone(window, cx);
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &code_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(quill::composer::enter_event_from_kit(
                        *shift, *secondary, marked,
                    )) {
                        this.submit_code(window, cx);
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &password_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(quill::composer::enter_event_from_kit(
                        *shift, *secondary, marked,
                    )) {
                        this.submit_password(window, cx);
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &search_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                let text = state.read(cx).value().to_string();
                this.sync_search_query(&text, cx);
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(quill::composer::enter_event_from_kit(
                        *shift, *secondary, marked,
                    )) {
                        this.activate_first_search_result(window, cx);
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &chat_search_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                let text = state.read(cx).value().to_string();
                this.sync_chat_search_query(&text, cx);
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(quill::composer::enter_event_from_kit(
                        *shift, *secondary, marked,
                    )) {
                        this.jump_selected_chat_search_hit(cx);
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &forward_search_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(quill::composer::enter_event_from_kit(
                        *shift, *secondary, marked,
                    )) {
                        this.activate_first_forward_destination(cx);
                    }
                }
            },
        )
        .detach();

        let demo_sink = Arc::new(MemorySink::new());
        let mut demo_session = None;
        let (connect_status, live, status_note, auth_demo) = match demo {
            Some(ScreenshotDemo::NeedTdjson) => (
                ConnectUiStatus::NeedTdjson,
                None,
                ConnectBlocker::MissingTdjson.user_message().into(),
                AuthorizationState::WaitPhoneNumber,
            ),
            Some(ScreenshotDemo::WaitPhone) => (
                ConnectUiStatus::DemoWaitPhone,
                None,
                "screenshot demo — WaitPhoneNumber (injected auth, no live Telegram)".into(),
                AuthorizationState::WaitPhoneNumber,
            ),
            Some(ScreenshotDemo::WaitCode) => (
                ConnectUiStatus::DemoWaitCode,
                None,
                "screenshot demo — WaitCode (injected auth, no live Telegram)".into(),
                AuthorizationState::WaitCode {
                    code_length: Some(5),
                },
            ),
            Some(ScreenshotDemo::WaitPassword) => (
                ConnectUiStatus::DemoWaitPassword,
                None,
                "screenshot demo — WaitPassword (injected auth, no live Telegram)".into(),
                AuthorizationState::WaitPassword {
                    has_recovery_email: true,
                },
            ),
            Some(ScreenshotDemo::ReadyChats | ScreenshotDemo::ReadyChatsComposer) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — Ready chat list (injected updates, no live Telegram)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyUnread) => {
                demo_session = Some(seed_ready_unread_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — unread badge (injected updates, no live Telegram)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyUnreadRead) => {
                demo_session = Some(seed_ready_unread_read_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — after mark-read (injected updates, no live Telegram)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyMedia) => {
                demo_session = Some(seed_ready_media_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — photo/document (injected updates, no live Telegram)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadySendMedia) => {
                demo_session = Some(seed_ready_send_media_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — outgoing photo/document send (injected, no live Telegram)"
                        .into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadySearch) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — sidebar search (injected searchChats / searchMessages)"
                        .into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadySearchInChat) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — in-chat search (injected searchChatMessages)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyReply) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — reply to message (injected reply_to)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyEditDelete) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — edit + delete own messages (injected)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyForward) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — forward message(s) (injected forwardMessages)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyReactions) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — emoji reactions (injected interaction_info)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyPin) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — pin / unpin (injected updateMessageIsPinned)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyMuteArchive) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — mute / archive (injected notification + chat list updates)"
                        .into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyNotificationSound) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — notification sounds + settings (injected saved sounds + chat/scope settings)"
                        .into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyTyping) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — peer typing (injected updateChatAction)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyStickers) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — sticker panel + sticker in history".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyVoice) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — voice record bar + history playback".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyLinkPreview) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — link + web page preview".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyGifs) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — saved GIFs + history playback".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyVideo) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — video bubble playback".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyVideoNote) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — round video note playback".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyAudio) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — audio file playback".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyVideoSend) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — local video attach + own-sent playback".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyVideoNoteSend) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — local video note attach + own-sent round note".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyDrafts) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — restored private-chat draft".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyAlbums) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — received album and own-sent album".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadySponsored) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — sponsored / recommended channel rows".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyChannels) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — broadcast channel posts + join footer".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyChannelsAdmin) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — broadcast channel, admin composer".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyBotChat) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — bot chat with info panel".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyBotKeyboard) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — bot chat with inline keyboard".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyBotCommandMenu) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — bot chat with / command menu".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyTextEntities) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — text entities in text + caption".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyPoll) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — polls: voted + closed".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyLocation) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — location / venue / contact".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyDice) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — dice rolls".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyMediaViewer) => {
                demo_session = Some(seed_ready_media_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — fullscreen media viewer".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyVideoPlayback) => {
                demo_session = Some(seed_ready_media_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — in-viewer video playback".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyStories) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — story viewer".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyStoryPost) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — story reactions / reply / delete".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadySeekBars) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — audio/voice seek bars".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyForumTopics) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — forum topics".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyTopicPost) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — posting to a forum topic".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyContacts) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — contacts & profile".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyFolders) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — chat folder tabs (injected, no live Telegram)".into(),
                    AuthorizationState::Ready,
                )
            }
            // Parity slice: same fixture as ReadyFolders; the demo block
            // opens the manage dialog over it.
            Some(ScreenshotDemo::ReadyFoldersManage) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — folder management (injected, no live Telegram)".into(),
                    AuthorizationState::Ready,
                )
            }
            // Parity slice: chat-list avatars + channel header extras
            // (injected, no live Telegram).
            Some(ScreenshotDemo::ReadyChatAvatars) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — chat avatars & channel header (injected, no live Telegram)"
                        .into(),
                    AuthorizationState::Ready,
                )
            }
            // Phase A1: slow-mode enforcement fixture.
            Some(ScreenshotDemo::ReadySlowMode) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — slow-mode enforcement (injected, no live Telegram)".into(),
                    AuthorizationState::Ready,
                )
            }
            // Phase B1: secret chat lifecycle fixture (injected, no live
            // Telegram).
            Some(ScreenshotDemo::ReadySecretChat) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — secret chat lifecycle (injected, no live Telegram)".into(),
                    AuthorizationState::Ready,
                )
            }
            // Phase B2: key verification UI fixture (injected, no live
            // Telegram).
            Some(ScreenshotDemo::ReadyKeyVerification) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — secret chat key verification (injected, no live Telegram)"
                        .into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadySelfDestruct) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — self-destructing media (injected, no live Telegram)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyCall) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — incoming call (injected, no live Telegram)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyChatTtl) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — chat self-destruct timer (injected, no live Telegram)"
                        .into(),
                    AuthorizationState::Ready,
                )
            }
            None => bootstrap_connect(credentials),
        };

        let mut pending_attachments = Vec::new();
        if matches!(demo, Some(ScreenshotDemo::ReadySendMedia))
            && let Some(att) = ComposerAttachment::pick(
                &demo_media_allowlist().join("demo-notes.txt"),
                AttachmentKind::Document,
            )
        {
            ComposerAttachment::push_attachment(&mut pending_attachments, att);
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyVideoSend))
            && let Some(att) = ComposerAttachment::pick(
                &demo_media_allowlist().join("demo-clip.mp4"),
                AttachmentKind::Video,
            )
        {
            ComposerAttachment::push_attachment(&mut pending_attachments, att);
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyVideoNoteSend))
            && let Some(att) = ComposerAttachment::pick(
                &demo_media_allowlist().join("demo-video-note.mp4"),
                AttachmentKind::VideoNote,
            )
        {
            ComposerAttachment::push_attachment(&mut pending_attachments, att);
        }
        // Phase B3: pending photo for the self-destruct picker demo.
        if matches!(demo, Some(ScreenshotDemo::ReadySelfDestruct))
            && let Some(att) = ComposerAttachment::pick(
                &demo_media_allowlist().join("demo-thumb.png"),
                AttachmentKind::Photo,
            )
        {
            ComposerAttachment::push_attachment(&mut pending_attachments, att);
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyAlbums)) {
            for (path, kind) in [
                (
                    demo_media_allowlist().join("demo-thumb.png"),
                    AttachmentKind::Photo,
                ),
                (
                    demo_media_allowlist().join("demo-clip.mp4"),
                    AttachmentKind::Video,
                ),
            ] {
                if let Some(att) = ComposerAttachment::pick(&path, kind) {
                    ComposerAttachment::push_attachment(&mut pending_attachments, att);
                }
            }
        }

        let mut app = Self {
            chat,
            composer,
            command_menu_open: false,
            command_menu_selected: 0,
            phone_input,
            code_input,
            password_input,
            search_input,
            chat_search_input,
            forward_search_input,
            story_reply_input,
            auth_demo,
            focus_sidebar: cx.focus_handle(),
            connect_status,
            live,
            status_note,
            demo_auth_inputs: matches!(
                demo,
                Some(
                    ScreenshotDemo::WaitPhone
                        | ScreenshotDemo::WaitCode
                        | ScreenshotDemo::WaitPassword
                )
            ),
            demo_session,
            demo_seq: AtomicU64::new(0),
            demo_sink,
            notify_clicks: Arc::new(Mutex::new(Vec::new())),
            notify_inflight: Arc::new(AtomicUsize::new(0)),
            pending_attachments,
            composer_self_destruct: None,
            pending_reply: None,
            clear_draft_on_success: None,
            pending_edit: None,
            saved_edit_draft: String::new(),
            saved_edit_reply: None,
            pending_delete: None,
            pending_close_secret_chat: None,
            pending_forward: None,
            forward_picker_open: false,
            forward_result: None,
            pending_react: None,
            mute_menu_open: false,
            ttl_picker_open: false,
            notif_sound_picker_open: false,
            notification_defaults_open: false,
            defaults_sound_picker: None,
            notify_sound_inflight: Arc::new(AtomicUsize::new(0)),
            voice_capture: None,
            voice_tick: false,
            slow_mode_tick_chat: None,
            self_destruct_tick_chat: None,
            call_tick_active: false,
            playing_voice: None,
            playing_audio: None,
            pending_audio_play: None,
            voice_player: None,
            playback_clock: None,
            playback_path: None,
            seek_slider: None,
            seek_scrubbing: false,
            seek_preview_secs: None,
            playback_tick: false,
            playback_positions: HashMap::new(),
            playing_animation: None,
            animation_frames: Vec::new(),
            animation_frame: 0,
            animation_tick: false,
            animation_cache_file: None,
            pending_gif_play: None,
            playing_video: None,
            video_frames: Vec::new(),
            video_frame: 0,
            video_tick: false,
            video_cache_file: None,
            pending_video_play: None,
            sponsored_demo: false,
            spoiler_revealed: HashSet::new(),
            poll_dialog: None,
            media_viewer: MediaViewer::closed(),
            viewer_zoom: ViewerZoom::new(),
            viewer_drag: None,
            viewer_video: None,
            viewer_video_path: None,
            viewer_player: None,
            viewer_clock: None,
            viewer_tick: false,
            viewer_pending_play: None,
            viewer_video_frames: Vec::new(),
            viewer_video_fps: 0.0,
            viewer_frame_cache_file: None,
            viewer_extracting: false,
            viewer_extract_child: None,
            viewer_extract_cancel: None,
            viewer_extract_epoch: 0,
            viewer_demo_sync_frames: false,
            story_viewer: StoryViewer::closed(),
            pending_story_open: None,
            story_reaction_picker_open: false,
            story_reply_open: false,
            contacts_tab_open: false,
            folder_tab: None,
            folder_manage_open: false,
            folder_editor: None,
            folder_delete_confirm: None,
            folder_menu_open: false,
            add_contact_dialog: None,
        };
        if matches!(demo, Some(ScreenshotDemo::ReadyChatsComposer)) {
            app.composer.update(cx, |input, cx| {
                input.set_value("hello from composer", window, cx);
            });
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyChannelsAdmin)) {
            app.composer.update(cx, |input, cx| {
                input.set_value("admin post — hello from the channel", window, cx);
            });
        }
        if matches!(demo, Some(ScreenshotDemo::ReadySendMedia)) {
            app.composer.update(cx, |input, cx| {
                input.set_value("sending a photo too", window, cx);
            });
        }
        if matches!(demo, Some(ScreenshotDemo::ReadySearch)) {
            app.search_input.update(cx, |input, cx| {
                input.set_value("hello", window, cx);
                input.focus(window, cx);
            });
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_search(session, &app.demo_sink, &app.demo_seq);
            }
        }
        if matches!(demo, Some(ScreenshotDemo::ReadySearchInChat)) {
            app.chat_search_input.update(cx, |input, cx| {
                input.set_value("hello", window, cx);
                input.focus(window, cx);
            });
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_search_in_chat(session, &app.demo_sink, &app.demo_seq);
            }
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyReply)) {
            app.composer.update(cx, |input, cx| {
                input.set_value("sounds good", window, cx);
                input.focus(window, cx);
            });
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_reply(session, &app.demo_sink, &app.demo_seq);
                app.pending_reply = Some(ComposerReplyTo::new(
                    ChatId(11),
                    MessageId(101),
                    "Hello from injected JSON.",
                ));
            }
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyEditDelete)) {
            let edit = ComposerEdit::from_own_content(
                ChatId(11),
                MessageId(102),
                true,
                false,
                &quill::telegram::envelope::MessageContent::Text(
                    "Reply from the session reducer.".into(),
                ),
            );
            app.composer.update(cx, |input, cx| {
                input.set_value("Reply from the session reducer.", window, cx);
                input.focus(window, cx);
            });
            app.pending_edit = edit;
            app.saved_edit_draft = "unrelated draft stays".into();
            app.pending_delete = DeleteConfirm::own(ChatId(11), MessageId(102), true, false);
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyForward)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_forward(session, &app.demo_sink, &app.demo_seq);
                app.forward_result = session.last_forward.clone();
            }
            let mut draft =
                ForwardDraft::from_message(ChatId(11), MessageId(101), false).expect("forward 101");
            draft.toggle(ChatId(11), MessageId(102), false);
            app.pending_forward = Some(draft);
            app.forward_picker_open = true;
            app.forward_search_input.update(cx, |input, cx| {
                input.set_value("Demo chat B", window, cx);
                input.focus(window, cx);
            });
            app.status_note = "screenshot demo — select → pick dest → forwarded".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyReactions)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_reactions(session, &app.demo_sink, &app.demo_seq);
            }
            app.pending_react = Some((ChatId(11), MessageId(101)));
            app.status_note = "screenshot demo — react · unreact · chips".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyPin)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_pin(session, &app.demo_sink, &app.demo_seq);
                let _ = session.begin_chat_search_jump(MessageId(101));
            }
            app.status_note = "screenshot demo — pin · unpin · pinned bar".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyMuteArchive)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_mute_archive(session, &app.demo_sink, &app.demo_seq);
            }
            app.mute_menu_open = true;
            app.status_note = "screenshot demo — mute presets · muted icon · archive".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyNotificationSound)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_notification_sound(session, &app.demo_sink, &app.demo_seq);
            }
            app.mute_menu_open = true;
            app.notif_sound_picker_open = true;
            app.status_note =
                "screenshot demo — notification sounds · per-chat panel · scope defaults".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyTyping)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_typing(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note = "screenshot demo — typing…".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyStickers)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_stickers(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note = "screenshot demo — stickers · tap to send".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyVoice)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_voice(session, &app.demo_sink, &app.demo_seq);
            }
            let bars = vec![4, 16, 28, 12, 8, 20, 6, 18, 10, 24, 8, 14];
            app.voice_capture = Some(VoiceCapture::preview(
                demo_media_allowlist().join("demo-voice.ogg"),
                2,
                bars,
            ));
            app.playing_voice = Some(MessageId(91));
            app.status_note = "screenshot demo — recording voice · playing voice note".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyGifs)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_gifs(session, &app.demo_sink, &app.demo_seq);
            }
            app.playing_animation = Some(MessageId(501));
            app.animation_frames = vec![
                demo_media_allowlist().join("demo-gif-1.png"),
                demo_media_allowlist().join("demo-gif-2.png"),
            ];
            app.spawn_animation_tick(cx);
            app.status_note = "screenshot demo — GIFs · tap to send · playing".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyVideo)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_video(session, &app.demo_sink, &app.demo_seq);
            }
            app.playing_video = Some(MessageId(601));
            app.video_frames = vec![
                demo_media_allowlist().join("demo-gif-1.png"),
                demo_media_allowlist().join("demo-gif-2.png"),
            ];
            app.spawn_video_tick(cx);
            app.status_note = "screenshot demo — video · playing".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyVideoNote)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_video_note(session, &app.demo_sink, &app.demo_seq);
            }
            app.playing_video = Some(MessageId(611));
            app.video_frames = vec![
                demo_media_allowlist().join("demo-gif-1.png"),
                demo_media_allowlist().join("demo-gif-2.png"),
            ];
            app.spawn_video_tick(cx);
            app.status_note = "screenshot demo — video note · playing".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyAudio)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_audio(session, &app.demo_sink, &app.demo_seq);
            }
            app.playing_audio = Some(MessageId(801));
            app.status_note = "screenshot demo — audio · playing".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadySeekBars)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_voice(session, &app.demo_sink, &app.demo_seq);
                apply_ready_audio(session, &app.demo_sink, &app.demo_seq);
            }
            // Fake an in-progress playback without spawning ffplay: voice
            // note 90 (12 s) playing from 5.0 s — the tick advances it —
            // and the music track 801 (214 s) paused with a remembered
            // 1:27 position, so both rows show seek bars.
            app.begin_track_playback(PlaybackKind::Voice, MessageId(90), 12.0, 5.0, cx);
            app.playback_positions.insert(MessageId(801), 87.0);
            app.status_note = "screenshot demo — seek bars · voice playing · audio paused".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyForumTopics)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_forum_topics(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note = "screenshot demo — forum topics list".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyTopicPost)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_topic_post(session, &app.demo_sink, &app.demo_seq);
            }
            app.composer.update(cx, |input, cx| {
                input.set_value("Posting into the General topic…", window, cx);
            });
            app.status_note = "screenshot demo — posting to a forum topic".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyContacts)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_contacts(session, &app.demo_sink, &app.demo_seq);
            }
            app.contacts_tab_open = true;
            app.status_note = "screenshot demo — contacts tab + user info panel".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyFolders)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_folders(session, &app.demo_sink, &app.demo_seq);
            }
            // Select the non-default "News" folder so the screenshot shows
            // the filtered chat list.
            app.folder_tab = Some(2);
            app.status_note = "screenshot demo — folder tabs · News folder".into();
        }
        // Parity slice: manage dialog over the same folder fixture.
        if matches!(demo, Some(ScreenshotDemo::ReadyFoldersManage)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_folders(session, &app.demo_sink, &app.demo_seq);
            }
            // Open the manage dialog over the folder fixture.
            app.folder_manage_open = true;
            app.status_note = "screenshot demo — folder management dialog".into();
        }
        // Parity slice: chat-list avatars + channel header extras fixture.
        if matches!(demo, Some(ScreenshotDemo::ReadyChatAvatars)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_chat_avatars(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note = "chat avatars & channel header".into();
        }
        // Phase A1: slow-mode enforcement fixture — the composer shows the
        // countdown and blocks sends until it expires.
        if matches!(demo, Some(ScreenshotDemo::ReadySlowMode)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_slow_mode(session, &app.demo_sink, &app.demo_seq);
            }
            app.composer.update(cx, |input, cx| {
                input.set_value("this send will be blocked by slow mode…", window, cx);
            });
            app.status_note =
                "slow-mode enforcement — sends blocked until the timer expires".into();
        }
        // Phase B1: secret chat lifecycle fixture — a Ready secret chat
        // with Zed, opened with E2E history and the composer live.
        if matches!(demo, Some(ScreenshotDemo::ReadySecretChat)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_secret_chat(session, &app.demo_sink, &app.demo_seq);
            }
            app.composer.update(cx, |input, cx| {
                input.set_value("this goes through the E2E session…", window, cx);
            });
            app.status_note = "secret chat — Ready, 🔒 badge in the chat list".into();
        }
        // Phase B2: key verification fixture — the Ready secret chat with
        // a real 36-byte key_hash and Zed's info panel open on the
        // "Encryption key" fingerprint grid.
        if matches!(demo, Some(ScreenshotDemo::ReadyKeyVerification)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_key_verification(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note =
                "secret chat key verification — compare with your contact's device".into();
        }
        // Phase B3: self-destructing media fixture — a private chat with
        // Zed carrying a live-timer incoming photo and a view-once
        // outgoing photo; the composer's pending photo attachment has
        // the picker pre-set to 30s.
        if matches!(demo, Some(ScreenshotDemo::ReadySelfDestruct)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_self_destruct(session, &app.demo_sink, &app.demo_seq);
            }
            app.composer_self_destruct = Some(SelfDestructSend::Timer(30));
            app.status_note =
                "screenshot demo — self-destructing media · picker on 30s (injected, no live Telegram)"
                    .into();
        }
        // Phase C1: incoming-call fixture — Zed rings with a pending
        // voice call, so the overlay renders the incoming-call card
        // (Accept / Decline + ticking clock).
        if matches!(demo, Some(ScreenshotDemo::ReadyCall)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_call(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note =
                "screenshot demo — incoming call from Zed (injected, no live Telegram)".into();
        }
        // Phase B4: chat TTL fixture — the Ready secret chat with a 1h
        // self-destruct timer, a live `auto_delete_in` countdown on one
        // message, the timer-change service row, and the picker expanded.
        if matches!(demo, Some(ScreenshotDemo::ReadyChatTtl)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_chat_ttl(session, &app.demo_sink, &app.demo_seq);
            }
            app.ttl_picker_open = true;
            app.status_note =
                "screenshot demo — chat self-destruct timer 1h · picker open (injected, no live Telegram)"
                    .into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyVideoSend)) {
            app.composer.update(cx, |input, cx| {
                input.set_value("sending a clip", window, cx);
            });
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_video_send(session, &app.demo_sink, &app.demo_seq);
            }
            app.playing_video = Some(MessageId(701));
            app.video_frames = vec![
                demo_media_allowlist().join("demo-gif-1.png"),
                demo_media_allowlist().join("demo-gif-2.png"),
            ];
            app.spawn_video_tick(cx);
            app.status_note = "screenshot demo — attach video · own clip playing".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyVideoNoteSend)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_video_note_send(session, &app.demo_sink, &app.demo_seq);
            }
            app.playing_video = Some(MessageId(721));
            app.video_frames = vec![
                demo_media_allowlist().join("demo-gif-1.png"),
                demo_media_allowlist().join("demo-gif-2.png"),
            ];
            app.spawn_video_tick(cx);
            app.status_note = "screenshot demo — video note attach · own round note".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyAlbums)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_albums(session, &app.demo_sink, &app.demo_seq);
            }
            app.composer.update(cx, |input, cx| {
                input.set_value("album caption", window, cx);
            });
            app.status_note = "screenshot demo — received album · own album".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyDrafts)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_drafts(session, &app.demo_sink, &app.demo_seq);
            }
            app.restore_open_draft(window, cx);
            app.status_note = "screenshot demo — draft restored".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyLinkPreview)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_link_preview(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note = "screenshot demo — link preview".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyTextEntities)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_text_entities(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note = "screenshot demo — text entities".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyPoll)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_poll(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note = "screenshot demo — polls: voted + closed".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyLocation)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_location(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note = "screenshot demo — location / venue / contact".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyDice)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_dice(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note = "screenshot demo — dice rolls".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyMediaViewer)) {
            // The Media seed opens chat 11 with a downloaded photo
            // (message 201, "Loaded photo") and a pending one (202, loading
            // state); the document (203) is not viewer-openable.
            app.open_media_viewer(ChatId(11), MessageId(201), cx);
            app.status_note = "screenshot demo — fullscreen media viewer".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyVideoPlayback)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_video_viewer(session, &app.demo_sink, &app.demo_seq);
            }
            // The Media seed plus a downloaded 12 s video (204, "Demo clip",
            // file 96). The demo extracts + decodes frames synchronously
            // (blocking ~2 s) for a deterministic capture — real in-viewer
            // playback, not faked: the clock keeps ticking and the 125 ms
            // refresh shows the frame for the current clock position.
            // `viewer_demo_sync_frames` suppresses the async extraction that
            // `open_media_viewer` would otherwise start. The ffplay
            // subprocess is skipped (demo), like the audio slice.
            app.viewer_demo_sync_frames = true;
            app.open_media_viewer(ChatId(11), MessageId(204), cx);
            if let Some(item) = app.media_viewer.current().cloned()
                && let Some(path) = app.viewer_clip_path(&item)
            {
                let file_id = item.play_file_id.map(|id| id.0).unwrap_or(0);
                let cache = quill::video::viewer_frame_cache_dir(file_id);
                let mime = item.mime_type.clone().unwrap_or_default();
                let duration = item.duration_secs.unwrap_or(0);
                let start_timestamp = item.start_timestamp.unwrap_or(0);
                if let Ok(viewer_frames) = quill::video::viewer_playback_frames(
                    &path,
                    &mime,
                    &cache,
                    start_timestamp,
                    duration,
                ) && let Ok(decoded) = Self::decode_viewer_frames(&viewer_frames.frames)
                {
                    app.viewer_video_frames = decoded;
                    app.viewer_video_fps = viewer_frames.fps;
                    app.viewer_frame_cache_file = Some(file_id);
                    app.play_viewer_video(&item, &path, cx);
                    if let Some(clock) = app.viewer_clock.as_mut() {
                        clock.seek(5.0);
                    }
                }
            }
            // Keep `viewer_demo_sync_frames` true so no background extraction
            // races the synchronously decoded frames.
            app.status_note = "screenshot demo — in-viewer video playback".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyStories)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_stories(session, &app.demo_sink, &app.demo_seq);
            }
            // Phase 9.1: the tray above the chat list shows the seeded
            // active stories for chats 11/12; the viewer opens on chat 11's
            // downloaded photo story.
            app.open_story_viewer(ChatId(11), 5, cx);
            app.status_note = "screenshot demo — story viewer".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyStoryPost)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_story_post(session, &app.demo_sink, &app.demo_seq);
            }
            // Phase 9.2: viewer opens on the seeded own photo story with
            // the reaction picker and the reply row visible, seeded
            // `availableReactions`, and a chosen ❤ reaction. No composer:
            // the pinned schema has no `sendStory`.
            app.open_story_viewer(ChatId(11), 5, cx);
            app.story_reaction_picker_open = true;
            app.story_reply_open = true;
            app.story_reply_input.update(cx, |input, cx| {
                input.set_value("Great photo!", window, cx);
            });
            app.status_note = "screenshot demo — story reactions / reply / delete".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadySponsored)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_sponsored(session, &app.demo_sink, &app.demo_seq);
            }
            app.sponsored_demo = true;
            app.status_note = "screenshot demo — sponsored messages".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyChannels)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_channels(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note = "screenshot demo — broadcast channel".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyChannelsAdmin)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_channels_admin(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note = "screenshot demo — broadcast channel admin".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyBotChat)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_bot_chat(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note = "screenshot demo — bot chat with info panel".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyBotKeyboard)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_bot_keyboard(session, &app.demo_sink, &app.demo_seq);
            }
            app.status_note = "screenshot demo — bot chat with inline keyboard".into();
        }
        if matches!(demo, Some(ScreenshotDemo::ReadyBotCommandMenu)) {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_ready_bot_command_menu(session, &app.demo_sink, &app.demo_seq);
            }
            app.composer.update(cx, |input, cx| {
                input.set_value("/", window, cx);
            });
            app.sync_command_menu(cx);
            app.status_note = "screenshot demo — bot chat with / command menu".into();
        }
        // Phase 3.3: Esc / Up / Down for the `/` command menu. The
        // composer input consumes Escape and arrows in its own `Input`
        // key context, so a keystroke interceptor — which runs before
        // keymap dispatch — is the only reliable hook. It acts only while
        // the menu is open and stops propagation so the input never sees
        // the swallowed keystroke.
        let menu_app = cx.weak_entity();
        cx.intercept_keystrokes(move |event, _window, cx| {
            if event.keystroke.modifiers.modified() {
                return;
            }
            let handled = match event.keystroke.key.as_str() {
                "escape" => menu_app
                    .update(cx, |this, cx| this.close_command_menu(cx))
                    .unwrap_or(false),
                "up" => menu_app
                    .update(cx, |this, cx| this.step_command_menu(-1, cx))
                    .unwrap_or(false),
                "down" => menu_app
                    .update(cx, |this, cx| this.step_command_menu(1, cx))
                    .unwrap_or(false),
                _ => false,
            };
            if handled {
                cx.stop_propagation();
            }
        })
        .detach();
        if app.live.is_some() {
            app.spawn_poll_loop(cx);
        }
        app
    }

    fn spawn_poll_loop(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        this.poll_live(cx);
                        this.live.is_some()
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_live(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let prev_auth = live.driver.session.auth.clone();
        let mut progressed = false;
        let mut send_failed = false;
        while let Some(owned) = live.bridge.next_timeout(Duration::from_millis(0)) {
            if live.driver.ingest(owned).is_err() {
                send_failed = true;
            }
            progressed = true;
        }
        // Parity slice: the selected folder tab may have been deleted or
        // removed remotely (`updateChatFolders`); fall back to Main.
        if let Some(folder_id) = self.folder_tab
            && !live
                .driver
                .session
                .chat_folders
                .iter()
                .any(|f| f.id == folder_id)
        {
            self.folder_tab = None;
        }
        let err = live.driver.session.last_auth_error;
        let new_auth = live.driver.session.auth.clone();
        let chat_count = live.driver.session.ordered_chats().len();
        let chats_exhausted = live.driver.session.chats_exhausted;
        if send_failed {
            self.status_note = "failed to send TDLib request".into();
        } else if progressed {
            if let Some(err) = err {
                self.status_note = err.user_message().into();
            } else if new_auth != prev_auth {
                self.status_note = live_status_for(&new_auth);
            } else if matches!(new_auth, AuthorizationState::Ready) {
                self.status_note = if chats_exhausted {
                    format!("signed in — {chat_count} chats")
                } else {
                    format!("signed in — loading chats ({chat_count})")
                };
            }
        }
        if let Some(result) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.last_forward.take())
        {
            self.present_forward_result(result, cx);
            progressed = true;
        }
        // Phase 3.2: bot answers to inline keyboard callback presses.
        if let Some(answer) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.last_callback_answer.take())
        {
            self.present_callback_answer(answer, cx);
            progressed = true;
        }
        self.finish_successful_sends(cx);
        if progressed || send_failed {
            cx.notify();
        }
        self.resume_pending_gif(cx);
        self.resume_pending_video(cx);
        self.resume_pending_audio(cx);
        // Parity slice 5: a viewer video whose clip just finished downloading.
        self.resume_pending_viewer_video(cx);
    }

    /// Phase 8.1: drain notification click callbacks (focus the chat) and
    /// dispatch newly queued notifications on worker threads. Runs from
    /// `render`, which is the only UI path with a `&mut Window`.
    fn flush_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let clicks: Vec<ChatId> = self
            .notify_clicks
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default();
        for chat_id in clicks {
            self.select_listed_chat(chat_id, window, cx);
        }
        let queued: Vec<QueuedNotification> = self
            .live
            .as_mut()
            .map(|live| std::mem::take(&mut live.driver.session.pending_notifications))
            .unwrap_or_default();
        for queued in queued {
            if let Some(kind) = queued.sound {
                self.play_notification_sound(kind);
            }
            self.spawn_os_notification(queued);
        }
        // Parity slice: custom sounds whose downloads just completed.
        let plays: Vec<PathBuf> = self
            .live
            .as_mut()
            .map(|live| std::mem::take(&mut live.driver.session.pending_sound_plays))
            .unwrap_or_default();
        for path in plays {
            if let Some(command) = quill::notify::file_sound_command(&path.to_string_lossy()) {
                self.spawn_sound_command(command);
            }
        }
    }

    /// Parity slice: resolve a notification sound and play it. `Default`
    /// plays the synthesized tone; a saved sound plays its MP3 once
    /// downloaded (a pending download plays on completion via
    /// `pending_sound_plays`). Silent when the chat's sound is disabled —
    /// the reducer never queues a sound for those.
    fn play_notification_sound(&mut self, kind: NotificationSoundKind) {
        let command = match self.live.as_mut() {
            Some(live) => match live.driver.resolve_notification_sound(kind) {
                SoundResolution::DefaultTone => quill::notify::default_tone_command(),
                SoundResolution::FilePath(path) => {
                    quill::notify::file_sound_command(&path.to_string_lossy())
                }
                SoundResolution::Pending => None,
            },
            // No live driver (screenshot demo): the tone is the honest
            // stand-in — no TDLib file is reachable.
            None => quill::notify::default_tone_command(),
        };
        if let Some(command) = command {
            self.spawn_sound_command(command);
        }
    }

    /// Parity slice: play one notification sound on a worker thread. A
    /// player failure (missing ffplay, vanished file) is silent by design —
    /// a notification must never surface an error dialog.
    fn spawn_sound_command(&mut self, command: quill::notify::SoundCommand) {
        if self.notify_sound_inflight.fetch_add(1, Ordering::SeqCst)
            >= MAX_OS_NOTIFICATION_SOUND_THREADS
        {
            self.notify_sound_inflight.fetch_sub(1, Ordering::SeqCst);
            return;
        }
        let inflight = self.notify_sound_inflight.clone();
        let spawn = std::thread::Builder::new()
            .name("quill-sound".to_string())
            .spawn(move || {
                quill::notify::play_sound_command(&command);
                inflight.fetch_sub(1, Ordering::SeqCst);
            });
        if spawn.is_err() {
            self.notify_sound_inflight.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Phase 8.1: show one queued notification via the platform backend on a
    /// worker thread. A click (Linux `notify-send --wait --action`) records
    /// the chat id; the next render focuses it. Concurrent workers are
    /// capped; excess bursts are dropped rather than stacking threads.
    fn spawn_os_notification(&mut self, queued: QueuedNotification) {
        let notification = queued.for_display();
        let Some(command) = quill::notify::build_notification_command(&notification) else {
            return;
        };
        if self.notify_inflight.fetch_add(1, Ordering::SeqCst) >= MAX_OS_NOTIFICATION_THREADS {
            self.notify_inflight.fetch_sub(1, Ordering::SeqCst);
            return;
        }
        let clicks = self.notify_clicks.clone();
        let inflight = self.notify_inflight.clone();
        let spawn = std::thread::Builder::new()
            .name("quill-notify".to_string())
            .spawn(move || {
                let outcome = quill::notify::run_notification_command(&command);
                inflight.fetch_sub(1, Ordering::SeqCst);
                if outcome.clicked
                    && let Ok(mut guard) = clicks.lock()
                {
                    guard.push(notification.chat_id);
                }
            });
        if spawn.is_err() {
            self.notify_inflight.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn finish_successful_sends(&mut self, cx: &mut Context<Self>) {
        let clears = self
            .live
            .as_mut()
            .map(|live| std::mem::take(&mut live.driver.session.draft_clears))
            .unwrap_or_default();
        if clears.is_empty() {
            return;
        }
        let idle = self.pending_edit.is_none()
            && self.pending_reply.is_none()
            && self.composer.read(cx).value().trim().is_empty();
        for chat_id in clears {
            if self.clear_draft_on_success != Some(chat_id) {
                continue;
            }
            self.clear_draft_on_success = None;
            if !idle {
                continue;
            }
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.clear_draft_after_send(chat_id, true);
            }
        }
    }

    fn current_auth(&self) -> AuthorizationState {
        if let Some(live) = self.live.as_ref() {
            live.driver.session.auth.clone()
        } else if let Some(session) = self.demo_session.as_ref() {
            session.auth.clone()
        } else {
            self.auth_demo.clone()
        }
    }

    fn session(&self) -> Option<&Session> {
        self.live
            .as_ref()
            .map(|live| &live.driver.session)
            .or(self.demo_session.as_ref())
    }

    fn media_display_roots(&self) -> Vec<PathBuf> {
        let primary = if let Some(live) = self.live.as_ref() {
            vec![live.driver.tdlib_files().to_path_buf()]
        } else if self.demo_session.is_some() {
            vec![demo_media_allowlist()]
        } else {
            Vec::new()
        };
        if primary.is_empty() {
            primary
        } else {
            quill::video::with_viewer_frame_cache(quill::video::with_video_frame_cache(
                quill::animation::with_gif_frame_cache(primary),
            ))
        }
    }

    fn pane_mode(&self) -> PaneMode {
        if self
            .session()
            .is_some_and(|session| matches!(session.auth, AuthorizationState::Ready))
        {
            return PaneMode::Ready;
        }
        match self.connect_status {
            ConnectUiStatus::NeedCredentials if self.live.is_none() && !self.demo_auth_inputs => {
                PaneMode::Synthetic
            }
            _ => PaneMode::Connecting,
        }
    }

    fn submit_composer(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        match self.pane_mode() {
            PaneMode::Connecting => {
                self.status_note = "sign in before sending".into();
                cx.notify();
            }
            PaneMode::Ready => {
                if self.pending_edit.is_some() {
                    self.submit_edit(text, window, cx);
                    return;
                }
                if self.live.is_some() {
                    let plan = {
                        let session = &self.live.as_ref().expect("live").driver.session;
                        (
                            session.open_chat,
                            session.view_generation,
                            session.open_chat.and_then(|id| {
                                session.chats.get(&id.0).map(|chat| chat.supported())
                            }),
                        )
                    };
                    let (open_chat, view_generation, supported) = plan;
                    let Some(chat_id) = open_chat else {
                        self.status_note = "select a chat to send".into();
                        cx.notify();
                        return;
                    };
                    if supported != Some(true) {
                        self.status_note = "this chat type is not supported yet".into();
                        cx.notify();
                        return;
                    }
                    let attachments = self.pending_attachments.clone();
                    // Phase B3: self-destruct only leaves the composer on
                    // photo/video attachments; the driver additionally
                    // strips it for non-private chats (TDLib's 400 gate).
                    let self_destruct = attachments
                        .iter()
                        .all(|att| {
                            matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video)
                        })
                        .then_some(self.composer_self_destruct)
                        .flatten();
                    let snap = if attachments.len() >= 2
                        && attachments.iter().all(|att| {
                            matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video)
                        }) {
                        ComposerSnapshot::capture_album(chat_id, view_generation, text, attachments)
                    } else {
                        ComposerSnapshot::capture_with_attachment(
                            chat_id,
                            view_generation,
                            text,
                            attachments.first().cloned(),
                        )
                    }
                    .with_reply(self.pending_reply.clone())
                    .with_self_destruct(self_destruct);
                    if snap.is_empty() {
                        self.status_note = "type a message or attach a file".into();
                        cx.notify();
                        return;
                    }
                    // Phase A1: slow-mode gate (centralized in
                    // `slow_mode_blocked`).
                    if self.slow_mode_blocked(chat_id, cx) {
                        return;
                    }
                    let result = self
                        .live
                        .as_mut()
                        .expect("live")
                        .driver
                        .send_snapshot(&snap);
                    match result {
                        Ok(_) => {
                            self.pending_attachments.clear();
                            // Phase B3: the timer choice was consumed by the
                            // snapshot — reset the picker for the next send.
                            self.composer_self_destruct = None;
                            self.pending_reply = None;
                            self.clear_draft_on_success = Some(chat_id);
                            self.composer
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            self.forget_local_draft(chat_id);
                            self.status_note = "sending…".into();
                        }
                        Err(_) => {
                            let video_unreadable =
                                snap.attachment.iter().chain(snap.album.iter()).any(|att| {
                                    att.kind == AttachmentKind::Video
                                        && quill::video::probe_local_video(&att.path).is_err()
                                });
                            let note_unreadable = snap.attachment.iter().any(|att| {
                                att.kind == AttachmentKind::VideoNote
                                    && quill::video::probe_local_video_note(&att.path).is_err()
                            });
                            self.status_note = if note_unreadable {
                                "video note must be a square clip (max 60s, 640px)".into()
                            } else if video_unreadable {
                                "could not read video duration or size".into()
                            } else {
                                "could not send message".into()
                            };
                        }
                    }
                    cx.notify();
                    return;
                }
                if self.demo_session.is_some() {
                    // Phase A1: the slow-mode gate applies to the demo
                    // session too (fixture-driven countdown, no live
                    // Telegram).
                    let demo_chat = self.demo_session.as_ref().and_then(|s| s.open_chat);
                    if demo_chat.is_some_and(|chat_id| self.slow_mode_blocked(chat_id, cx)) {
                        return;
                    }
                    let attachments = self.pending_attachments.clone();
                    if attachments.iter().any(|att| {
                        att.kind == AttachmentKind::VideoNote
                            && quill::video::probe_local_video_note(&att.path).is_err()
                    }) {
                        self.status_note =
                            "video note must be a square clip (max 60s, 640px)".into();
                        cx.notify();
                        return;
                    }
                    let reply = self.pending_reply.clone();
                    if attachments.len() >= 2
                        && attachments.iter().all(|att| {
                            matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video)
                        })
                    {
                        self.apply_demo_album(&text, &attachments, reply.as_ref());
                    } else {
                        self.apply_demo_outgoing(&text, attachments.first(), reply.as_ref());
                    }
                    self.pending_attachments.clear();
                    self.pending_reply = None;
                    if let Some(chat_id) = self.demo_session.as_ref().and_then(|s| s.open_chat) {
                        self.forget_local_draft(chat_id);
                    }
                    self.composer
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    self.status_note = "demo send applied locally (no live Telegram)".into();
                    cx.notify();
                }
            }
            PaneMode::Synthetic => {
                self.chat.update(cx, |chat, cx| chat.send_text(text, cx));
                self.composer
                    .update(cx, |input, cx| input.set_value("", window, cx));
                cx.notify();
            }
        }
    }

    fn attach_local(&mut self, kind: AttachmentKind, cx: &mut Context<Self>) {
        // Explicit user action → pick. Prefer QUILL_ATTACH_PHOTO / QUILL_ATTACH_FILE /
        // QUILL_ATTACH_VIDEO when set (live testing); otherwise the demo fixtures
        // under docs/screenshots.
        // Never read paths from TDLib JSON for send.
        let env_key = match kind {
            AttachmentKind::Photo => "QUILL_ATTACH_PHOTO",
            AttachmentKind::Document => "QUILL_ATTACH_FILE",
            AttachmentKind::Video => "QUILL_ATTACH_VIDEO",
            AttachmentKind::VideoNote => "QUILL_ATTACH_VIDEO_NOTE",
        };
        let path = std::env::var_os(env_key)
            .map(PathBuf::from)
            .unwrap_or_else(|| match kind {
                AttachmentKind::Photo => demo_media_allowlist().join("demo-thumb.png"),
                AttachmentKind::Document => demo_media_allowlist().join("demo-notes.txt"),
                AttachmentKind::Video => demo_media_allowlist().join("demo-clip.mp4"),
                AttachmentKind::VideoNote => demo_media_allowlist().join("demo-video-note.mp4"),
            });
        match ComposerAttachment::pick(&path, kind) {
            Some(att) => {
                let name = att.file_name.clone();
                let before = self.pending_attachments.len();
                ComposerAttachment::push_attachment(&mut self.pending_attachments, att);
                self.status_note = if self.pending_attachments.len() == before {
                    format!("album is full ({before})")
                } else {
                    format!("attached {name}")
                };
            }
            None => {
                self.status_note = "could not attach file".into();
            }
        }
        cx.notify();
    }

    fn clear_attachment(&mut self, cx: &mut Context<Self>) {
        self.pending_attachments.clear();
        self.composer_self_destruct = None;
        self.status_note = "attachment cleared".into();
        cx.notify();
    }

    /// Phase B3: schema-valid self-destruct choices for
    /// `inputMessagePhoto`/`inputMessageVideo` (`messageSelfDestructType*`,
    /// schema 1.8.67 lines 5915–5918; the runtime validates the timer as
    /// 1..=60 seconds, so the picker offers only Off / 5s / 30s / 1m /
    /// View once — no `1h`/`1d`).
    const SELF_DESTRUCT_CHOICES: [Option<SelfDestructSend>; 5] = [
        None,
        Some(SelfDestructSend::Timer(5)),
        Some(SelfDestructSend::Timer(30)),
        Some(SelfDestructSend::Timer(60)),
        Some(SelfDestructSend::Immediately),
    ];

    /// Phase B3: whether the self-destruct picker may appear — a private
    /// (1:1 cloud) chat with a photo/video attachment pending, the only
    /// combination TDLib accepts `self_destruct_type` for (schema 1.8.67
    /// lines 6117/6128 "private chats only"; the driver also strips the
    /// choice for any other chat kind as defense in depth).
    fn self_destruct_picker_visible(&self) -> bool {
        let session = match self.session() {
            Some(session) => session,
            None => return false,
        };
        let Some(open) = session.open_chat else {
            return false;
        };
        let is_private = session
            .chats
            .get(&open.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Private { .. }));
        is_private
            && !self.pending_attachments.is_empty()
            && self
                .pending_attachments
                .iter()
                .all(|att| matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video))
    }

    /// Phase B3: cycle the composer's self-destruct choice (Off → 5s →
    /// 30s → 1m → View once → Off). Called from the picker button and the
    /// screenshot demo.
    fn cycle_composer_self_destruct(&mut self, cx: &mut Context<Self>) {
        let choices = Self::SELF_DESTRUCT_CHOICES;
        let next = choices
            .iter()
            .position(|choice| *choice == self.composer_self_destruct)
            .and_then(|index| choices.get(index + 1))
            .copied()
            .unwrap_or(choices[0]);
        self.composer_self_destruct = next;
        self.status_note = match next {
            None => "self-destruct off".into(),
            Some(SelfDestructSend::Timer(secs)) => format!("self-destruct: {secs}s"),
            Some(SelfDestructSend::Immediately) => "self-destruct: view once".into(),
        };
        cx.notify();
    }

    /// Phase B3: label for the picker button (`⏱` cycle affordance).
    fn self_destruct_button_label(&self) -> String {
        match self.composer_self_destruct {
            None => "⏱ Off".to_string(),
            Some(SelfDestructSend::Timer(secs)) => format!("⏱ {secs}s"),
            Some(SelfDestructSend::Immediately) => "⏱ Once".to_string(),
        }
    }

    fn apply_demo_album(
        &mut self,
        text: &str,
        attachments: &[ComposerAttachment],
        reply: Option<&ComposerReplyTo>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let Some(chat_id) = session.open_chat else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let caption = text.trim();
        let album_id = "88001";
        let reply_json = reply
            .filter(|r| r.chat_id == chat_id)
            .map(|r| {
                format!(
                    r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":{},"quote":null,"checklist_task_id":0,"poll_option_id":""}}"#,
                    r.chat_id.0, r.message_id.0
                )
            })
            .unwrap_or_default();
        for (index, att) in attachments.iter().enumerate() {
            let id = -((index as i64) + 20);
            let item_caption = if index + 1 == attachments.len() {
                caption
            } else {
                ""
            };
            let path = att.path.to_string_lossy();
            let file = demo_file_json(910 + index as i32, &path, true);
            let json = match att.kind {
                AttachmentKind::Photo => format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"media_album_id":"{album_id}","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":{},"entities":[]}},"has_spoiler":false,"is_secret":false}}}}{reply_json}}}}}"#,
                    chat_id.0,
                    serde_json::to_string(item_caption).unwrap_or_else(|_| "\"\"".into()),
                ),
                AttachmentKind::Video => {
                    let probe = quill::video::probe_local_video(&att.path).unwrap_or(
                        quill::video::VideoProbe {
                            duration: 0,
                            width: 320,
                            height: 180,
                            supports_streaming: false,
                        },
                    );
                    format!(
                        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"media_album_id":"{album_id}","content":{{"@type":"messageVideo","video":{{"@type":"video","duration":{},"width":{},"height":{},"file_name":{},"mime_type":"video/mp4","has_stickers":false,"supports_streaming":{},"minithumbnail":null,"thumbnail":null,"video":{file}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":{},"entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}{reply_json}}}}}"#,
                        chat_id.0,
                        probe.duration,
                        probe.width,
                        probe.height,
                        serde_json::to_string(&att.file_name)
                            .unwrap_or_else(|_| "\"clip.mp4\"".into()),
                        probe.supports_streaming,
                        serde_json::to_string(item_caption).unwrap_or_else(|_| "\"\"".into()),
                    )
                }
                AttachmentKind::Document | AttachmentKind::VideoNote => continue,
            };
            if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                session.apply(owned);
            }
        }
    }

    fn apply_demo_outgoing(
        &mut self,
        text: &str,
        attachment: Option<&ComposerAttachment>,
        reply: Option<&ComposerReplyTo>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let Some(chat_id) = session.open_chat else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let id = -(session.view_generation.0 as i64);
        let caption = text.trim();
        let reply_json = reply
            .filter(|r| r.chat_id == chat_id)
            .map(|r| {
                format!(
                    r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":{},"quote":null,"checklist_task_id":0,"poll_option_id":""}}"#,
                    r.chat_id.0, r.message_id.0
                )
            })
            .unwrap_or_default();
        let json = match attachment {
            Some(att) if att.kind == AttachmentKind::Photo => {
                let path = att.path.to_string_lossy();
                let file = demo_file_json(900, &path, true);
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":{},"entities":[]}},"has_spoiler":false,"is_secret":false}}{reply_json}}}}}"#,
                    chat_id.0,
                    serde_json::to_string(caption).unwrap_or_else(|_| "\"\"".into()),
                )
            }
            Some(att) if att.kind == AttachmentKind::VideoNote => {
                let path = att.path.to_string_lossy();
                let file = demo_file_json(903, &path, true);
                let probe = quill::video::probe_local_video_note(&att.path).unwrap_or(
                    quill::video::VideoNoteProbe {
                        duration: 1,
                        length: 240,
                    },
                );
                let thumb_path = quill::video::write_video_note_thumbnail(&att.path)
                    .map(|thumb| thumb.path.to_string_lossy().into_owned());
                let thumb = thumb_path
                    .as_deref()
                    .map(|path| demo_file_json(904, path, true))
                    .unwrap_or_else(|| "null".into());
                let thumb_obj = if thumb == "null" {
                    "null".to_string()
                } else {
                    format!(
                        r#"{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":240,"file":{thumb}}}"#
                    )
                };
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":{},"waveform":"","length":{},"minithumbnail":null,"thumbnail":{thumb_obj},"speech_recognition_result":null,"video":{file}}},"is_viewed":true,"is_secret":false}}}}{reply_json}}}}}"#,
                    chat_id.0, probe.duration, probe.length,
                )
            }
            Some(att) if att.kind == AttachmentKind::Video => {
                let path = att.path.to_string_lossy();
                let file = demo_file_json(902, &path, true);
                let probe = quill::video::probe_local_video(&att.path).unwrap_or(
                    quill::video::VideoProbe {
                        duration: 0,
                        width: 0,
                        height: 0,
                        supports_streaming: false,
                    },
                );
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":{},"width":{},"height":{},"file_name":{},"mime_type":"video/mp4","has_stickers":false,"supports_streaming":{},"minithumbnail":null,"thumbnail":null,"video":{file}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":{},"entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}{reply_json}}}}}"#,
                    chat_id.0,
                    probe.duration,
                    probe.width,
                    probe.height,
                    serde_json::to_string(&att.file_name).unwrap_or_else(|_| "\"clip.mp4\"".into()),
                    probe.supports_streaming,
                    serde_json::to_string(caption).unwrap_or_else(|_| "\"\"".into()),
                )
            }
            Some(att) => {
                let path = att.path.to_string_lossy();
                let file = demo_file_json(901, &path, true);
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":{},"mime_type":"text/plain","document":{file}}},"caption":{{"@type":"formattedText","text":{},"entities":[]}}}}{reply_json}}}}}"#,
                    chat_id.0,
                    serde_json::to_string(&att.file_name).unwrap_or_else(|_| "\"file\"".into()),
                    serde_json::to_string(caption).unwrap_or_else(|_| "\"\"".into()),
                )
            }
            None => format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{},"entities":[]}}}}{reply_json}}}}}"#,
                chat_id.0,
                serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into()),
            ),
        };
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    fn apply_demo_sticker(
        &mut self,
        chat_id: ChatId,
        emoji: &str,
        file_id: FileId,
        reply: Option<MessageId>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let id = -(session.view_generation.0 as i64);
        let path = session
            .files
            .get(&file_id.0)
            .and_then(|file| file.usable_path())
            .unwrap_or("");
        let file = demo_file_json(file_id.0, path, !path.is_empty());
        let reply_json = reply
            .map(|message_id| {
                format!(
                    r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":{},"quote":null,"checklist_task_id":0,"poll_option_id":""}}"#,
                    chat_id.0, message_id.0
                )
            })
            .unwrap_or_default();
        let emoji = serde_json::to_string(emoji).unwrap_or_else(|_| "\"\"".into());
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageSticker","is_premium":false,"sticker":{{"@type":"sticker","id":"0","set_id":"0","width":512,"height":512,"emoji":{emoji},"format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatPng"}},"width":128,"height":128,"file":{file}}},"sticker":{file}}}}}{reply_json}}}}}"#,
            chat_id.0
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    fn apply_demo_gif(
        &mut self,
        chat_id: ChatId,
        file_id: FileId,
        duration: i32,
        width: i32,
        height: i32,
        reply: Option<MessageId>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let id = -(session.view_generation.0 as i64);
        let path = session
            .files
            .get(&file_id.0)
            .and_then(|file| file.usable_path())
            .unwrap_or("");
        let file = demo_file_json(file_id.0, path, !path.is_empty());
        let reply_json = reply
            .map(|message_id| {
                format!(
                    r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":{},"quote":null,"checklist_task_id":0,"poll_option_id":""}}"#,
                    chat_id.0, message_id.0
                )
            })
            .unwrap_or_default();
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageAnimation","animation":{{"@type":"animation","duration":{duration},"width":{width},"height":{height},"file_name":"gif.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":{width},"height":{height},"file":{file}}},"animation":{file}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}{reply_json}}}}}"#,
            chat_id.0
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    fn apply_demo_edit(&mut self, edit: &ComposerEdit, text: &str) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        match edit.kind {
            quill::composer::ComposerEditKind::Text => {
                let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
                let body = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
                let json = format!(
                    r#"{{"@type":"updateMessageContent","chat_id":{},"message_id":{},"new_content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{body},"entities":[]}}}}}}"#,
                    edit.chat_id.0, edit.message_id.0
                );
                if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
            quill::composer::ComposerEditKind::Caption => {
                if let Some(message) = session
                    .histories
                    .get_mut(&edit.chat_id.0)
                    .and_then(|history| history.messages.get_mut(&edit.message_id.0))
                {
                    match &mut message.content {
                        MessageContent::Photo(photo) => photo.caption = text.to_string(),
                        MessageContent::Document(doc) => doc.caption = text.to_string(),
                        MessageContent::Text(body) => {
                            body.text = text.to_string();
                            body.entities.clear();
                            body.link_preview = None;
                        }
                        MessageContent::VoiceNote(note) => note.caption = text.to_string(),
                        MessageContent::Animation(animation) => {
                            animation.caption = text.to_string()
                        }
                        MessageContent::Video(video) => video.caption = text.to_string(),
                        MessageContent::Audio(audio) => audio.caption = text.to_string(),
                        MessageContent::VideoNote(_)
                        | MessageContent::Sticker(_)
                        | MessageContent::Poll(_)
                        | MessageContent::Location(_)
                        | MessageContent::Venue(_)
                        | MessageContent::Contact(_)
                        | MessageContent::Dice(_)
                        | MessageContent::ChatTtlChanged { .. }
                        | MessageContent::Unsupported { .. } => {}
                    }
                }
            }
        }
    }

    fn apply_demo_delete(&mut self, chat_id: ChatId, message_id: MessageId) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let json = format!(
            r#"{{"@type":"updateDeleteMessages","chat_id":{},"message_ids":[{}],"is_permanent":true,"from_cache":false}}"#,
            chat_id.0, message_id.0
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    fn select_listed_chat(&mut self, chat_id: ChatId, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_leaving_draft(cx);
        // Phase B4: the TTL picker belongs to the previous chat.
        self.ttl_picker_open = false;
        if self
            .pending_reply
            .as_ref()
            .is_some_and(|reply| reply.chat_id != chat_id)
        {
            self.pending_reply = None;
        }
        if self
            .pending_edit
            .as_ref()
            .is_some_and(|edit| edit.chat_id != chat_id)
        {
            self.pending_edit = None;
            self.saved_edit_draft.clear();
            self.saved_edit_reply = None;
        }
        if self
            .pending_delete
            .as_ref()
            .is_some_and(|confirm| confirm.chat_id != chat_id)
        {
            self.pending_delete = None;
        }
        if self
            .pending_forward
            .as_ref()
            .is_some_and(|draft| draft.from_chat_id != chat_id)
        {
            self.pending_forward = None;
            self.forward_picker_open = false;
        }
        if self
            .pending_react
            .is_some_and(|(react_chat, _)| react_chat != chat_id)
        {
            self.pending_react = None;
        }
        if self.voice_capture.is_some() {
            self.cancel_voice_recording(cx);
        }
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.stop_animation_playback();
        self.stop_video_playback();
        if self.gif_panel_open() {
            if let Some(live) = self.live.as_mut() {
                live.driver.close_gif_panel();
            } else if let Some(session) = self.demo_session.as_mut() {
                session.gifs.close();
            }
        }
        if self.sticker_panel_open() {
            if let Some(live) = self.live.as_mut() {
                live.driver.close_sticker_panel();
            } else if let Some(session) = self.demo_session.as_mut() {
                session.stickers.close();
            }
        }
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .select_chat(chat_id);
            self.status_note = match result {
                Ok(_) => "chat selected".into(),
                Err(_) => "could not open chat".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_chat(chat_id);
        }
        self.restore_open_draft(window, cx);
        let text = self.composer.read(cx).value().to_string();
        self.sync_composer_typing(&text);
        cx.notify();
    }

    fn open_message_url(&mut self, url: &str, cx: &mut Context<Self>) {
        self.status_note = if quill::platform::open_external_url(url) {
            "opened link".into()
        } else {
            "could not open link".into()
        };
        cx.notify();
    }

    /// Phase 3.2: press an inline keyboard callback button. Live sessions
    /// send `getCallbackQueryAnswer`; the bot's `callbackQueryAnswer`
    /// response is picked up by `poll_live` and shown in the status line.
    /// Demo sessions have no live TDLib, so the press is an honest no-op.
    fn press_inline_callback(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        data: Vec<u8>,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.send_callback_query(chat_id, message_id, &data);
            self.status_note = match result {
                Ok(_) => "sending…".into(),
                Err(_) => "could not send callback".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.status_note = "demo — callback sent (no live Telegram)".into();
            cx.notify();
        }
    }

    /// Phase 3.2: insert a `switchInline` query into the current chat's
    /// composer. `targetChatChosen` / `targetChatInternalLink` (no chat
    /// picker in this slice) use the current chat, same as `targetChatCurrent`.
    fn insert_switch_inline_query(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |input, cx| {
            let next = quill::composer::insert_switch_inline_text(&input.value(), query);
            input.set_value(next, window, cx);
        });
        self.sync_command_menu(cx);
    }

    /// Phase 3.3: recompute the `/` command menu from the composer text.
    /// Called on every composer event (`Change` path) and after
    /// programmatic `set_value` writes, which suppress `Change`. Opens
    /// when the text ends with a `/`-led token at a word boundary and the
    /// open chat's bot has commands; closes otherwise (non-bot chat, no
    /// commands, invalid trigger, empty composer).
    fn sync_command_menu(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        let triggered = command_menu_trigger(&text).is_some();
        let open_chat = self.session().and_then(|session| session.open_chat);
        let has_items = open_chat.is_some_and(|chat_id| {
            self.session()
                .map(|session| !session.command_menu_items(chat_id).is_empty())
                .unwrap_or(false)
        });
        let open = triggered && has_items;
        if open == self.command_menu_open {
            return;
        }
        self.command_menu_open = open;
        self.command_menu_selected = 0;
        cx.notify();
    }

    /// Phase 3.3: Esc / blur / selection / chat-switch dismissal. Returns
    /// true when the menu was open (the key interceptor swallows the
    /// keystroke only then).
    fn close_command_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.command_menu_open {
            return false;
        }
        self.command_menu_open = false;
        self.command_menu_selected = 0;
        cx.notify();
        true
    }

    /// Phase 3.3: Up/Down highlight. Returns true when the menu consumed
    /// the key (open with rows to move between); the selection wraps.
    fn step_command_menu(&mut self, delta: i32, cx: &mut Context<Self>) -> bool {
        let rows = self
            .command_menu_state(cx)
            .map(|(_, items)| items.len())
            .unwrap_or(0);
        if !self.command_menu_open || rows == 0 {
            return false;
        }
        self.command_menu_selected =
            (self.command_menu_selected as i32 + delta).rem_euclid(rows as i32) as usize;
        cx.notify();
        true
    }

    /// Phase 3.3: current menu rows — (typed prefix, prefix-filtered
    /// items). `None` when the menu is closed, the composer has no `/`
    /// trigger, the open chat's bot has no commands, or nothing matches.
    fn command_menu_state(&self, cx: &Context<Self>) -> Option<(String, Vec<CommandMenuItem>)> {
        if !self.command_menu_open {
            return None;
        }
        let text = self.composer.read(cx).value().to_string();
        let prefix = command_menu_trigger(&text)?;
        let chat_id = self.session()?.open_chat?;
        let items = self.session()?.command_menu_items(chat_id);
        if items.is_empty() {
            return None;
        }
        let filtered: Vec<CommandMenuItem> = filter_command_menu_items(&items, prefix)
            .into_iter()
            .cloned()
            .collect();
        if filtered.is_empty() {
            return None;
        }
        Some((prefix.to_string(), filtered))
    }

    /// Phase 3.3: Enter with the menu open picks the highlighted row.
    /// Returns true when Enter was consumed.
    fn pick_command_menu_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let items = self
            .command_menu_state(cx)
            .map(|(_, items)| items)
            .unwrap_or_default();
        if !self.command_menu_open || items.is_empty() {
            return false;
        }
        let index = self.command_menu_selected.min(items.len() - 1);
        self.pick_command_menu_index(index, window, cx);
        true
    }

    /// Phase 3.3: tap / Enter pick. The partial `/`-token is replaced via
    /// the 3.1 insert helper, then the menu closes (the next `Change`
    /// reopens it if a trigger token remains).
    fn pick_command_menu_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items = self
            .command_menu_state(cx)
            .map(|(_, items)| items)
            .unwrap_or_default();
        let Some(item) = items.get(index) else {
            return;
        };
        let command = item.command.clone();
        let current = self.composer.read(cx).value().to_string();
        let base = strip_command_menu_trigger(&current).unwrap_or(current.as_str());
        // Trailing space (tdesktop behavior): without it, the `/`-token
        // trigger still matches `/command`, the menu reopens on the next
        // Enter and consumes it in a no-op loop — Enter could never send.
        let next = format!(
            "{} ",
            quill::composer::insert_bot_command_text(base, &command).trim_end()
        );
        self.composer.update(cx, |input, cx| {
            input.set_value(next, window, cx);
        });
        self.close_command_menu(cx);
    }

    /// Phase 3.3: the `/` command menu popup above the composer.
    /// Bot-specific commands first, then the global (`getCommands`)
    /// section when both exist. Tap inserts; Up/Down/Enter via the key
    /// interceptor; Esc / blur / outside tap dismisses.
    fn command_menu_dropdown(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (_, items) = self.command_menu_state(cx)?;
        let selected = self.command_menu_selected.min(items.len() - 1);
        let has_specific = items.iter().any(|item| !item.global);
        let has_global = items.iter().any(|item| item.global);
        let show_headers = has_specific && has_global;
        let mut list = div()
            .id("command-menu")
            .flex()
            .flex_col()
            .mx_4()
            .mb_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar);
        let mut global_header_shown = false;
        for (index, item) in items.iter().enumerate() {
            if show_headers && item.global && !global_header_shown {
                global_header_shown = true;
                list = list.child(
                    div()
                        .px_3()
                        .pt_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Global"),
                );
            }
            let name = item.command.clone();
            let label = if item.description.is_empty() {
                format!("/{name}")
            } else {
                format!("/{name} — {}", item.description)
            };
            let highlighted = index == selected;
            list = list.child(
                div()
                    .id(("command-menu-item", index as u64))
                    .w_full()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .when(highlighted, |this| this.bg(cx.theme().selection))
                    .hover(|style| style.bg(cx.theme().accent.opacity(0.12)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.pick_command_menu_index(index, window, cx);
                    }))
                    .child(div().text_sm().child(label)),
            );
        }
        Some(list.into_any_element())
    }

    /// Phase 3.2: `inlineKeyboardButtonTypeCopyText` — copy to the clipboard.
    fn copy_inline_text(&mut self, text: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        self.status_note = "copied".into();
        cx.notify();
    }

    /// Phase 3.2: show a `callbackQueryAnswer` in the status line (the
    /// transient feedback surface this app has). A URL answer opens in the
    /// OS browser, like message-text links; `show_alert` has no modal yet,
    /// so its text lands in the status line too.
    fn present_callback_answer(&mut self, answer: CallbackQueryAnswer, cx: &mut Context<Self>) {
        if answer.url.is_empty() {
            self.status_note = if answer.text.is_empty() {
                "bot answered".into()
            } else {
                answer.text
            };
        } else {
            self.status_note = if quill::platform::open_external_url(&answer.url) {
                "opened link".into()
            } else {
                "could not open link".into()
            };
        }
        cx.notify();
    }

    /// `clickChatSponsoredMessage` for a sponsored row interaction. `is_media_click`
    /// is true when the user opened the row's media; false for the sponsor
    /// button/link. Demo sessions have no live driver, so the click is a no-op.
    fn click_sponsored_message(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
        is_media_click: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.click_chat_sponsored_message(
                chat_id,
                message_id,
                is_media_click,
                false,
            );
        }
        let _ = cx;
    }

    fn request_media_download(
        &mut self,
        file_id: FileId,
        sponsored: Option<(ChatId, i64)>,
        cx: &mut Context<Self>,
    ) {
        if file_id.0 == 0 {
            return;
        }
        if let Some((chat_id, message_id)) = sponsored {
            self.click_sponsored_message(chat_id, message_id, true, cx);
        }
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.download_file(file_id, USER_DOWNLOAD_PRIORITY);
            self.status_note = match result {
                Ok(Some(_)) => "downloading…".into(),
                Ok(None) => "already local or in progress".into(),
                Err(_) => "could not download".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo — downloadFile runs with live TDLib".into();
        }
        cx.notify();
    }

    fn load_older_action(&mut self, cx: &mut Context<Self>) {
        match self.pane_mode() {
            PaneMode::Ready => {
                if self.live.is_some() {
                    // Phase 5.1: a topic view pages its own history.
                    let topic_open = self
                        .live
                        .as_ref()
                        .and_then(|live| live.driver.session.open_topic)
                        .is_some();
                    let result = if topic_open {
                        self.live
                            .as_mut()
                            .expect("live")
                            .driver
                            .fetch_topic_history()
                    } else {
                        self.live.as_mut().expect("live").driver.fetch_history()
                    };
                    self.status_note = match result {
                        Ok(Some(_)) => "loading older messages".into(),
                        Ok(None) => "no older messages to load".into(),
                        Err(_) => "could not load history".into(),
                    };
                }
                cx.notify();
            }
            PaneMode::Synthetic => {
                self.chat.update(cx, |chat, cx| chat.prepend_older(cx));
            }
            PaneMode::Connecting => {}
        }
    }

    fn cycle_auth(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            return;
        }
        self.auth_demo = match &self.auth_demo {
            AuthorizationState::WaitPhoneNumber => AuthorizationState::WaitCode {
                code_length: Some(5),
            },
            AuthorizationState::WaitCode { .. } => AuthorizationState::WaitPassword {
                has_recovery_email: true,
            },
            AuthorizationState::WaitPassword { .. } => AuthorizationState::WaitPremiumPurchase,
            AuthorizationState::WaitPremiumPurchase => AuthorizationState::Ready,
            AuthorizationState::Ready => AuthorizationState::WaitPhoneNumber,
            other => other.clone(),
        };
        cx.notify();
    }

    fn submit_phone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(
            live.driver.session.auth,
            AuthorizationState::WaitPhoneNumber
        ) {
            return;
        }
        let phone = self.phone_input.read(cx).value().to_string();
        match live.driver.submit_phone(&phone) {
            Ok(_) => {
                self.phone_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.status_note = "phone submitted — waiting for Telegram".into();
            }
            Err(_) => {
                self.status_note = "could not submit phone".into();
            }
        }
        cx.notify();
    }

    fn submit_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(
            live.driver.session.auth,
            AuthorizationState::WaitCode { .. }
        ) {
            return;
        }
        let mut code = self.code_input.read(cx).value().to_string();
        let result = live.driver.submit_code(&code);
        code.zeroize();
        match result {
            Ok(_) => {
                self.code_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.status_note = "code submitted — waiting for Telegram".into();
            }
            Err(_) => {
                self.status_note = "could not submit code".into();
            }
        }
        cx.notify();
    }

    fn submit_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(
            live.driver.session.auth,
            AuthorizationState::WaitPassword { .. }
        ) {
            return;
        }
        let mut password = self.password_input.read(cx).value().to_string();
        let result = live.driver.submit_password(&password);
        password.zeroize();
        match result {
            Ok(_) => {
                self.password_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.status_note = "password submitted — waiting for Telegram".into();
            }
            Err(_) => {
                self.status_note = "could not submit password".into();
            }
        }
        cx.notify();
    }

    fn search_is_open(&self) -> bool {
        self.session().is_some_and(|session| session.search.open)
    }

    fn chat_search_is_open(&self) -> bool {
        self.session()
            .is_some_and(|session| session.chat_search.open)
    }

    fn open_search_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pane_mode() != PaneMode::Ready {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.open_search() {
                Ok(Some(_)) => self.status_note = "searching…".into(),
                Ok(None) => self.status_note = "search chats and messages".into(),
                Err(_) => self.status_note = "could not search".into(),
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_search();
            self.status_note = "search chats and messages".into();
        }
        self.search_input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn close_search_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_search();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.close_search();
        }
        self.search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.status_note = "search closed".into();
        cx.notify();
    }

    fn cancel_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Phase 9.1: the story viewer is the topmost overlay — Escape
        // closes it before the media viewer.
        if self.story_viewer.is_open() {
            self.close_story_viewer(cx);
            return;
        }
        if self.media_viewer.is_open() {
            self.close_media_viewer(cx);
            return;
        }
        if self.poll_dialog.is_some() {
            self.close_poll_dialog(cx);
            return;
        }
        if self.voice_capture.is_some() {
            self.cancel_voice_recording(cx);
            return;
        }
        if self.gif_panel_open() {
            self.close_gif_panel(cx);
            return;
        }
        if self.sticker_panel_open() {
            self.close_sticker_panel(cx);
            return;
        }
        if self.notification_defaults_open {
            self.notification_defaults_open = false;
            self.defaults_sound_picker = None;
            cx.notify();
            return;
        }
        if self.mute_menu_open {
            self.close_mute_menu(cx);
            return;
        }
        // Phase B4: Esc closes the TTL picker too.
        if self.ttl_picker_open {
            self.ttl_picker_open = false;
            cx.notify();
            return;
        }
        if self.pending_react.is_some() {
            self.close_reaction_picker(cx);
            return;
        }
        if self
            .session()
            .is_some_and(|session| session.sponsored_report.is_some())
        {
            self.dismiss_sponsored_report_ui(cx);
            return;
        }
        if self.forward_picker_open {
            self.close_forward_picker(window, cx);
            return;
        }
        if self.pending_delete.is_some() {
            self.cancel_delete(cx);
            return;
        }
        if self.chat_search_is_open() {
            self.close_chat_search_ui(window, cx);
            return;
        }
        if self.search_is_open() {
            let query = self.search_input.read(cx).value().to_string();
            if query.trim().is_empty() {
                self.close_search_ui(window, cx);
            } else {
                self.search_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.sync_search_query("", cx);
            }
            return;
        }
        if self.pending_reply.is_some() {
            self.clear_reply(cx);
            return;
        }
        if self.pending_edit.is_some() {
            self.clear_edit(window, cx);
            return;
        }
        if self.pending_forward.is_some() {
            self.clear_forward(window, cx);
            return;
        }
        if self.forward_result.is_some() {
            self.forward_result = None;
            self.status_note = "forward result dismissed".into();
            cx.notify();
        }
    }

    fn begin_reply_to(
        &mut self,
        reply: ComposerReplyTo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_edit.is_some() {
            self.clear_edit(window, cx);
        }
        self.pending_reply = Some(reply);
        self.note_open_draft(true, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        self.status_note = "replying".into();
        cx.notify();
    }

    fn begin_edit(&mut self, edit: ComposerEdit, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_attachments.clear();
        let current = self.composer.read(cx).value().to_string();
        // Flush while the reply is still set so a reply-only draft is not wiped.
        self.note_open_draft(false, cx);
        let reply = self.pending_reply.take();
        let (edit, field, saved, stashed) = begin_edit_keeping_reply(current, edit, reply);
        self.pending_edit = Some(edit);
        self.saved_edit_draft = saved;
        self.saved_edit_reply = stashed;
        self.composer.update(cx, |input, cx| {
            input.set_value(&field, window, cx);
            input.focus(window, cx);
        });
        self.sync_composer_typing(&field);
        self.status_note = "editing".into();
        cx.notify();
    }

    fn clear_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // tdesktop cancelEditMessage → applyDraft(): restore normal draft.
        let saved = std::mem::take(&mut self.saved_edit_draft);
        let stashed = self.saved_edit_reply.take();
        let (_, restored) = cancel_edit_draft(self.pending_edit.take(), saved);
        let (restored, reply) = cancel_edit_keeping_reply(restored, stashed);
        self.pending_reply = reply;
        self.composer
            .update(cx, |input, cx| input.set_value(&restored, window, cx));
        self.sync_composer_typing(&restored);
        self.status_note = "edit cancelled".into();
        cx.notify();
    }

    fn finish_edit_restore_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let saved = std::mem::take(&mut self.saved_edit_draft);
        let reply = self.saved_edit_reply.take();
        self.pending_edit = None;
        self.pending_reply = reply;
        self.composer
            .update(cx, |input, cx| input.set_value(&saved, window, cx));
    }

    fn submit_edit(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.pending_edit.clone() else {
            return;
        };
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .edit_snapshot(&edit, &text);
            match result {
                Ok(_) => {
                    self.finish_edit_restore_draft(window, cx);
                    self.status_note = "saving edit…".into();
                }
                Err(_) => {
                    self.status_note = "could not edit message".into();
                }
            }
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_edit(&edit, text.trim());
            self.finish_edit_restore_draft(window, cx);
            self.status_note = "demo edit applied locally (no live Telegram)".into();
            cx.notify();
        }
    }

    fn begin_delete(&mut self, confirm: DeleteConfirm, cx: &mut Context<Self>) {
        self.pending_delete = Some(confirm);
        self.status_note = "confirm delete".into();
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete = None;
        self.status_note = "delete cancelled".into();
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.pending_delete.take() else {
            return;
        };
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .delete_confirmed(&confirm);
            self.status_note = match result {
                Ok(_) => "deleting…".into(),
                Err(_) => "could not delete message".into(),
            };
        } else if self.demo_session.is_some() {
            self.apply_demo_delete(confirm.chat_id, confirm.message_id);
            self.status_note = "demo delete applied locally (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase B1: open the "Close secret chat" confirm banner for the
    /// given secret chat.
    fn open_close_secret_chat_confirm(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.pending_close_secret_chat = Some(chat_id);
        self.status_note = "confirm close secret chat".into();
        cx.notify();
    }

    /// Phase B1: cancel the "Close secret chat" confirm.
    fn cancel_close_secret_chat(&mut self, cx: &mut Context<Self>) {
        self.pending_close_secret_chat = None;
        self.status_note = "close cancelled".into();
        cx.notify();
    }

    /// Phase B1: confirm `closeSecretChat` (schema 1.8.67 line 15242).
    /// The state change to `secretChatStateClosed` arrives as
    /// `updateSecretChat`; the composer hides then.
    fn confirm_close_secret_chat(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self.pending_close_secret_chat.take() else {
            return;
        };
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .close_secret_chat(chat_id);
            self.status_note = match result {
                Ok(_) => "closing secret chat…".into(),
                Err(_) => "could not close secret chat".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo: secret chat close (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase B1: `createNewSecretChat` from a user profile. The new chat
    /// opens when its `updateNewChat` arrives; the state (Pending →
    /// Ready) arrives as `updateSecretChat`.
    fn start_secret_chat_for_user(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .start_secret_chat(user_id);
            self.status_note = match result {
                Ok(_) => "creating secret chat…".into(),
                Err(_) => "could not start secret chat".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo: secret chat create (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C1: `createCall` from a user profile. Audio-only
    /// (`is_video: false`) — video needs transport too (C3). The
    /// outgoing call is tracked once the `callId` answer arrives; its
    /// states arrive as `updateCall`.
    fn start_call_for_user(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self.live.as_mut().expect("live").driver.start_call(user_id);
            self.status_note = match result {
                Ok(_) => "calling…".into(),
                Err(_) => "could not start the call".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo: call start (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C1: `acceptCall` for the ringing incoming call.
    fn accept_incoming_call(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self.live.as_mut().expect("live").driver.accept_call();
            self.status_note = match result {
                Ok(_) => "answering…".into(),
                Err(_) => "could not answer the call".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo: call accept (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C1: `discardCall` for the tracked call (decline an
    /// incoming call, or hang up an active one).
    fn hang_up_call(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self.live.as_mut().expect("live").driver.discard_call();
            self.status_note = match result {
                Ok(_) => "hanging up…".into(),
                Err(_) => "could not hang up the call".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo: call hang up (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C1: `sendCallRating` from the call-end rating card.
    fn rate_last_call(&mut self, rating: i32, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .send_call_rating(rating);
            self.status_note = match result {
                Ok(_) => "thanks for your feedback".into(),
                Err(_) => "could not send the rating".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo: call rating (no live Telegram)".into();
            if let Some(session) = self.demo_session.as_mut()
                && let Some(summary) = session.call_summary.as_mut()
            {
                summary.rating_sent = true;
            }
        }
        cx.notify();
    }

    /// Phase C1: dismiss the call-end screen (rating skipped or
    /// acknowledged).
    fn dismiss_call_summary(&mut self, cx: &mut Context<Self>) {
        for session in self
            .live
            .as_mut()
            .map(|live| &mut live.driver.session)
            .into_iter()
            .chain(self.demo_session.as_mut())
        {
            session.call_summary = None;
        }
        cx.notify();
    }

    /// Phase C1: dismiss a shown call-request error.
    fn dismiss_call_error(&mut self, cx: &mut Context<Self>) {
        for session in self
            .live
            .as_mut()
            .map(|live| &mut live.driver.session)
            .into_iter()
            .chain(self.demo_session.as_mut())
        {
            session.call_error = None;
        }
        cx.notify();
    }

    fn clear_reply(&mut self, cx: &mut Context<Self>) {
        // tdesktop FieldHeader Escape / replyCancelled: header only — keep typed text.
        self.pending_reply = cancel_reply_draft(self.pending_reply.take(), String::new()).0;
        self.note_open_draft(true, cx);
        self.status_note = "reply cancelled".into();
        cx.notify();
    }

    fn begin_forward_one(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        pending: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(single) = ForwardDraft::from_message(chat_id, message_id, pending) else {
            self.status_note = "cannot forward this message".into();
            cx.notify();
            return;
        };
        if let Some(existing) = self.pending_forward.as_mut()
            && existing.from_chat_id == chat_id
            && self.forward_picker_open
        {
            existing.toggle(chat_id, message_id, pending);
            if existing.is_empty() {
                self.close_forward_picker(window, cx);
            }
            cx.notify();
            return;
        }
        self.pending_forward = Some(single);
        self.open_forward_picker(window, cx);
    }

    fn toggle_forward_select(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        pending: bool,
        cx: &mut Context<Self>,
    ) {
        match self.pending_forward.as_mut() {
            Some(draft) => {
                draft.toggle(chat_id, message_id, pending);
                if draft.is_empty() {
                    self.pending_forward = None;
                    self.forward_picker_open = false;
                }
            }
            None => {
                self.pending_forward = ForwardDraft::from_message(chat_id, message_id, pending);
            }
        }
        self.status_note = match self.pending_forward.as_ref().map(|d| d.count()) {
            Some(1) => "1 message selected".into(),
            Some(n) => format!("{n} messages selected"),
            None => "selection cleared".into(),
        };
        cx.notify();
    }

    fn open_forward_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_forward.as_ref().is_none_or(|d| d.is_empty()) {
            return;
        }
        self.forward_picker_open = true;
        self.forward_search_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
        self.status_note = "forward to…".into();
        cx.notify();
    }

    fn close_forward_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.forward_picker_open = false;
        self.forward_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.status_note = "forward picker closed".into();
        cx.notify();
    }

    fn clear_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = cancel_forward_draft(self.pending_forward.take());
        self.forward_picker_open = false;
        self.forward_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.status_note = "forward cancelled".into();
        cx.notify();
    }

    fn activate_first_forward_destination(&mut self, cx: &mut Context<Self>) {
        let query = self.forward_search_input.read(cx).value().to_string();
        let dest = self
            .session()
            .and_then(|session| session.forward_destinations(&query).into_iter().next())
            .map(|chat| chat.id);
        if let Some(dest) = dest {
            self.submit_forward_to(dest, cx);
        }
    }

    fn submit_forward_to(&mut self, dest: ChatId, cx: &mut Context<Self>) {
        let Some(draft) = self.pending_forward.clone() else {
            return;
        };
        // Phase A1: slow-mode gate applies to forwards — forwarding sends
        // messages to the destination chat.
        if self.slow_mode_blocked(dest, cx) {
            return;
        }
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .forward_messages(dest, &draft);
            self.status_note = match result {
                Ok(_) => "forwarding…".into(),
                Err(_) => "could not forward messages".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_forward(dest, &draft);
            if let Some(result) = self
                .demo_session
                .as_mut()
                .and_then(|session| session.last_forward.take())
            {
                self.present_forward_result(result, cx);
            }
        }
    }

    fn present_forward_result(&mut self, result: ForwardResult, cx: &mut Context<Self>) {
        self.status_note = result.success_label();
        self.forward_result = Some(result);
        self.pending_forward = None;
        self.forward_picker_open = false;
        cx.notify();
    }

    fn apply_demo_forward(&mut self, dest: ChatId, draft: &ForwardDraft) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let extra = session.request(RequestPurpose::ForwardMessages, Some(dest));
        session.in_flight_forward = Some(quill::state::ForwardFlight {
            extra,
            dest_chat_id: dest,
            from_chat_id: draft.from_chat_id,
            requested: draft.message_ids.len(),
        });
        let origin_user = session
            .chats
            .get(&draft.from_chat_id.0)
            .and_then(|chat| match chat.kind {
                quill::telegram::envelope::ChatKind::Private { user_id } => Some(user_id.0),
                _ => None,
            })
            .unwrap_or(draft.from_chat_id.0);
        let mut copies = Vec::new();
        for (offset, id) in draft.message_ids.iter().enumerate() {
            let preview = session
                .histories
                .get(&draft.from_chat_id.0)
                .and_then(|history| history.messages.get(&id.0))
                .map(|message| message.content.preview())
                .unwrap_or_else(|| "Message".into());
            let body = serde_json::to_string(&preview).unwrap_or_else(|_| "\"\"".into());
            copies.push(format!(
                r#"{{"id":{},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{body},"entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":{origin_user}}},"date":1}}}}"#,
                80 + offset as i64,
                dest.0
            ));
        }
        let json = format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":{},"messages":[{}]}}"#,
            extra.0,
            copies.len(),
            copies.join(",")
        );
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    fn open_reaction_picker(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let can_react = self.session().is_some_and(|session| {
            session
                .histories
                .get(&chat_id.0)
                .and_then(|history| history.messages.get(&message_id.0))
                .is_some_and(quill::state::HistoryMessage::can_react)
        });
        if !can_react {
            return;
        }
        self.pending_react = Some((chat_id, message_id));
        self.status_note = "react".into();
        cx.notify();
    }

    fn close_reaction_picker(&mut self, cx: &mut Context<Self>) {
        self.pending_react = None;
        self.status_note = "reaction picker closed".into();
        cx.notify();
    }

    /// Phase 4.5: open the fullscreen media viewer on the clicked message.
    /// Items are the chat's photo/video messages (oldest first); the clicked
    /// message becomes the current item. When nothing viewable is local yet
    /// the viewer shows a loading state and `downloadFile` is triggered —
    /// the 40ms poll loop re-renders when `updateFile` lands.
    fn open_media_viewer(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let items = self
            .session()
            .and_then(|session| session.histories.get(&chat_id.0))
            .map(|history| {
                collect_media_items(&history.ordered().into_iter().cloned().collect::<Vec<_>>())
            })
            .unwrap_or_default();
        if items.is_empty() {
            return;
        }
        let index = items.iter().position(|item| item.message_id == message_id);
        // The clicked message may not be viewer-openable (e.g. an album
        // tile for a non-photo/video part) — then stay closed instead of
        // opening on an unrelated item.
        let Some(index) = index else {
            return;
        };
        self.media_viewer = MediaViewer::open(items, index);
        self.viewer_zoom.reset();
        self.viewer_drag = None;
        self.stop_viewer_video();
        self.ensure_viewer_download(cx);
        self.maybe_autoplay_viewer_video(cx);
        cx.notify();
    }

    fn close_media_viewer(&mut self, cx: &mut Context<Self>) {
        self.stop_viewer_video();
        self.media_viewer.close();
        cx.notify();
    }

    fn step_media_viewer(&mut self, delta: i32, cx: &mut Context<Self>) {
        if delta < 0 {
            self.media_viewer.prev();
        } else {
            self.media_viewer.next();
        }
        self.viewer_zoom.reset();
        self.viewer_drag = None;
        self.stop_viewer_video();
        self.ensure_viewer_download(cx);
        self.maybe_autoplay_viewer_video(cx);
        cx.notify();
    }

    /// Trigger `downloadFile` for the current viewer item when no display
    /// candidate is local yet (photo: largest size; video: thumbnail, else
    /// the clip itself). Reuses `request_media_download`; no live request
    /// happens in demo mode (it only sets a status note).
    fn ensure_viewer_download(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.media_viewer.current().cloned() else {
            return;
        };
        let roots = self.media_display_roots();
        let local = self.session().is_some_and(|session| {
            item.display_file_ids.iter().any(|id| {
                session
                    .files
                    .get(&id.0)
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))
                    .is_some()
            })
        });
        if !local {
            self.request_media_download(item.download_file_id, None, cx);
        }
    }

    /// Parity slice 5: in-viewer video playback. The clip's frames are
    /// extracted with ffmpeg, pre-decoded into GPUI image handles, and
    /// rendered in-place in the viewer overlay (no GPUI video element in
    /// this stack — same frame-cycling approach as the row video preview,
    /// but full-clip); ffplay runs `-nodisp` for the audio track only. The
    /// overlay keeps Play/Pause and elapsed/total; the thumbnail shows
    /// until frames are ready. Closing or stepping the viewer stops
    /// playback and drops the frame cache.
    /// Fixed size of the viewer visual container (zoom/pan frame), in px.
    const VIEWER_FRAME: (f32, f32) = (720.0, 480.0);

    /// Sandbox-checked local path of the current item's full video clip.
    fn viewer_clip_path(&self, item: &MediaViewerItem) -> Option<PathBuf> {
        let play_id = item.play_file_id?;
        let roots = self.media_display_roots();
        self.session()?
            .files
            .get(&play_id.0)?
            .usable_path()
            .and_then(|path| sandboxed_display_path(path, &roots).map(|p| p.to_path_buf()))
    }

    /// Start viewer playback when the current item is a video whose clip is
    /// local; otherwise trigger `downloadFile` for the clip and park the
    /// request in `viewer_pending_play` (resumed from the poll loop).
    ///
    /// Parity slice 5: the clip's frames are extracted (async — ffmpeg takes
    /// ~2 s for a 12 s clip), decoded into pre-loaded image handles, and
    /// rendered in-viewer; ffplay runs `-nodisp` for audio only. If frames
    /// are already cached for this file (e.g. the screenshot demo decoded
    /// them synchronously), playback starts at once. Every start path
    /// routes through `decide_viewer_video_start` so a local clip without
    /// cached frames always goes through extraction — never straight to
    /// playback with an empty frame cache.
    fn maybe_autoplay_viewer_video(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.media_viewer.current().cloned() else {
            return;
        };
        let file_id = item.play_file_id.map(|id| id.0).unwrap_or(0);
        let path = self.viewer_clip_path(&item);
        let frames_ready =
            self.viewer_frame_cache_file == Some(file_id) && !self.viewer_video_frames.is_empty();
        match decide_viewer_video_start(&item, path.is_some(), frames_ready) {
            ViewerVideoStart::Nothing => {}
            ViewerVideoStart::ParkDownload => {
                if let Some(play_id) = item.play_file_id {
                    self.viewer_pending_play = Some((item.message_id, play_id));
                    self.request_media_download(play_id, None, cx);
                }
            }
            ViewerVideoStart::PlayNow => {
                self.viewer_pending_play = None;
                let path = path.expect("clip checked local by decide_viewer_video_start");
                self.play_viewer_video(&item, &path, cx);
            }
            ViewerVideoStart::ExtractFrames => {
                self.viewer_pending_play = None;
                // The screenshot demo extracts + decodes frames synchronously
                // itself; don't start a redundant background extraction.
                if self.viewer_demo_sync_frames {
                    return;
                }
                let path = path.expect("clip checked local by decide_viewer_video_start");
                self.extract_viewer_frames(&item, &path, cx);
            }
        }
    }

    /// Decode extracted viewer frame PNGs into pre-loaded GPUI image handles.
    ///
    /// `img()` resolves `ImageSource::Render` synchronously — the pinned
    /// `gpui-pre-0.3.5` `src/elements/img.rs` `use_data` returns
    /// `Some(Ok(data.to_owned()))` for `Render` immediately — so cycling
    /// frames on the 125 ms tick renders without the async fs-read +
    /// PNG-decode round trip that made path-based (`ImageSource::Resource`)
    /// sources flicker and lag (each new path re-entered
    /// `window.use_asset::<ImgResourceLoader>`, which returns `None` until
    /// the load completes, while the tick fired independently).
    fn decode_viewer_frames(paths: &[PathBuf]) -> Result<Vec<Arc<RenderImage>>, String> {
        paths
            .iter()
            .map(|path| {
                let rgba = image::open(path)
                    .map_err(|err| format!("{}: {err}", path.display()))?
                    .into_rgba8();
                Ok(Arc::new(RenderImage::new(SmallVec::from_buf([
                    image::Frame::new(rgba),
                ]))))
            })
            .collect()
    }

    /// Kill a running viewer frame extraction (ffmpeg child), if any, and
    /// invalidate its completion. Called on viewer close/step and before a
    /// fresh extraction starts, so an abandoned extraction can't run to
    /// completion on a discarded cache dir.
    fn kill_viewer_extraction(&mut self) {
        // Signal cancellation first: the worker checks this before spawning
        // and right after publishing the child, so a kill that lands before
        // ffmpeg publishes still aborts the run instead of orphaning it.
        if let Some(cancel) = self.viewer_extract_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        if let Some(slot) = self.viewer_extract_child.take() {
            // Take the child out of the lock before kill/wait: the worker
            // only holds the lock briefly around `try_wait`.
            let child = slot.lock().ok().and_then(|mut guard| guard.take());
            drop(slot);
            if let Some(mut child) = child {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        self.viewer_extract_epoch = self.viewer_extract_epoch.wrapping_add(1);
    }

    /// Extract the clip's frames on a background thread, decode them into
    /// pre-loaded image handles, then start playback if the viewer is still
    /// on the same item. The thumbnail stays visible with a loading hint
    /// meanwhile. The ffmpeg child is published so close/step can kill it;
    /// a completion from a killed or superseded run is dropped by epoch.
    fn extract_viewer_frames(
        &mut self,
        item: &MediaViewerItem,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        let file_id = item.play_file_id.map(|id| id.0).unwrap_or(0);
        if self
            .viewer_frame_cache_file
            .is_some_and(|cached| cached != file_id)
            && let Some(old) = self.viewer_frame_cache_file.take()
        {
            quill::video::discard_viewer_frame_cache(old);
        }
        self.viewer_frame_cache_file = Some(file_id);
        self.viewer_video_frames.clear();
        self.viewer_extracting = true;
        // A step between two videos goes through `stop_viewer_video` first,
        // but cancel explicitly anyway: a fresh run must not share the
        // previous run's slot or epoch.
        self.kill_viewer_extraction();
        let epoch = self.viewer_extract_epoch;
        let slot: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        self.viewer_extract_child = Some(slot.clone());
        let cancel: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        self.viewer_extract_cancel = Some(cancel.clone());
        cx.notify();

        let cache = quill::video::viewer_frame_cache_dir(file_id);
        let mime = item.mime_type.clone().unwrap_or_default();
        let duration = item.duration_secs.unwrap_or(0);
        let start_timestamp = item.start_timestamp.unwrap_or(0);
        let message_id = item.message_id;
        let path = path.to_path_buf();
        let item = item.clone();
        let extract_path = path.clone();
        let task_slot = slot.clone();
        let task_cancel = cancel.clone();
        cx.spawn(async move |this, cx| {
            let extracted = cx
                .background_executor()
                .spawn(async move {
                    let frames = quill::video::viewer_playback_frames_cancelable(
                        &extract_path,
                        &mime,
                        &cache,
                        start_timestamp,
                        duration,
                        &task_slot,
                        &task_cancel,
                    )?;
                    // Decode on the background thread: the render path needs
                    // pre-loaded handles, and decoding up to 600 PNGs must
                    // not block the UI thread.
                    let decoded = Self::decode_viewer_frames(&frames.frames)?;
                    Ok::<_, String>((decoded, frames.fps))
                })
                .await;
            this.update(cx, |this, cx| {
                // Stale completion (viewer closed/stepped, or a newer
                // extraction started): drop silently — no error note, no
                // playback, and crucially don't clear a newer run's
                // loading state.
                if this.viewer_extract_epoch != epoch {
                    return;
                }
                this.viewer_extracting = false;
                // Drop the slot only if it's still ours (`stop_viewer_video`
                // may have taken it to kill the child).
                if this
                    .viewer_extract_child
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &slot))
                {
                    this.viewer_extract_child = None;
                }
                let still_current = this
                    .media_viewer
                    .current()
                    .is_some_and(|current| current.message_id == message_id);
                if !still_current {
                    return;
                }
                match extracted {
                    Ok((decoded, fps)) => {
                        // The screenshot demo extracts + decodes synchronously
                        // and starts playback itself; don't restart it when the
                        // background extraction lands.
                        let already_playing = this.viewer_video == Some(message_id)
                            && !this.viewer_video_frames.is_empty();
                        if !already_playing {
                            this.viewer_video_frames = decoded;
                            this.viewer_video_fps = fps;
                            this.play_viewer_video(&item, &path, cx);
                        }
                    }
                    Err(_) => {
                        this.viewer_video_frames.clear();
                        this.status_note = "could not play video".into();
                    }
                }
                cx.notify();
            })
            .ok();
        });
    }

    /// Resume a parked viewer play once `downloadFile` lands the clip.
    /// Routes through `maybe_autoplay_viewer_video`: it re-derives the
    /// current item, clears the pending flag, and either reuses cached
    /// frames or starts async extraction — never straight to playback
    /// with an empty frame cache.
    fn resume_pending_viewer_video(&mut self, cx: &mut Context<Self>) {
        let Some((message_id, file_id)) = self.viewer_pending_play else {
            return;
        };
        let ready = self.session().is_some_and(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .is_some()
        });
        if !ready {
            return;
        }
        let matches = self.media_viewer.current().is_some_and(|item| {
            item.message_id == message_id && item.kind == MediaViewerKind::Video
        });
        if !matches {
            self.viewer_pending_play = None;
            return;
        }
        self.maybe_autoplay_viewer_video(cx);
    }

    /// Begin (or restart) viewer playback of `item`'s clip from offset 0.
    /// Stops every other player first — one thing plays at a time.
    /// Frames must already be in `viewer_video_frames` (extracted async by
    /// `extract_viewer_frames`, or synchronously by the screenshot demo).
    /// State only: the caller spawns ffplay (the screenshot demo skips the
    /// subprocess, like the audio slice's demo).
    fn begin_viewer_video(
        &mut self,
        item: &MediaViewerItem,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.stop_video_playback();
        self.stop_animation_playback();
        let duration = item.duration_secs.unwrap_or(0).max(0) as f64;
        let mut clock = PlaybackClock::new(duration);
        clock.seek(0.0);
        clock.resume();
        self.viewer_clock = Some(clock);
        self.viewer_video = Some(item.message_id);
        self.viewer_video_path = Some(path.to_path_buf());
        self.spawn_viewer_tick(cx);
        cx.notify();
    }

    /// Begin viewer playback *and* spawn ffplay for audio — unless this
    /// is a screenshot demo, which skips the subprocess (same posture as
    /// `request_media_download`'s demo branch).
    fn play_viewer_video(
        &mut self,
        item: &MediaViewerItem,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        self.begin_viewer_video(item, path, cx);
        if self.demo_session.is_none() {
            self.spawn_viewer_ffplay(path, 0.0);
        }
    }

    /// Spawn ffplay `-nodisp` (audio only) for the viewer clip. The video
    /// frames render in-viewer from `viewer_video_frames`; ffplay only
    /// supplies the soundtrack. `-autoexit` ends the child at the clip's
    /// end; our tick clears state to match. A missing ffplay (or a clip
    /// with no audio) just means silent playback — the frames still show.
    fn spawn_viewer_ffplay(&mut self, path: &std::path::Path, offset_secs: f64) -> bool {
        self.kill_viewer_player();
        let mut command = Command::new("ffplay");
        command.args(["-nodisp", "-autoexit", "-loglevel", "quiet"]);
        if offset_secs > 0.05 {
            command.arg("-ss").arg(format!("{offset_secs:.1}"));
        }
        match command
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => {
                self.viewer_player = Some(child);
                true
            }
            Err(_) => false,
        }
    }

    fn kill_viewer_player(&mut self) {
        if let Some(mut child) = self.viewer_player.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Pause: freeze the clock, kill ffplay, keep the item active so the
    /// controls stay and Play resumes from the frozen offset.
    fn pause_viewer_video(&mut self, cx: &mut Context<Self>) {
        if let Some(clock) = self.viewer_clock.as_mut() {
            clock.pause();
        }
        self.kill_viewer_player();
        cx.notify();
    }

    /// Resume from the frozen clock position.
    fn resume_viewer_video(&mut self, cx: &mut Context<Self>) {
        let offset = self.viewer_clock.as_ref().map(|c| c.elapsed_secs());
        let path = self.viewer_video_path.clone();
        match (offset, path) {
            (Some(offset), Some(path)) => {
                self.spawn_viewer_ffplay(&path, offset);
                if let Some(clock) = self.viewer_clock.as_mut() {
                    clock.resume();
                }
            }
            _ => self.stop_viewer_video(),
        }
        cx.notify();
    }

    /// The viewer Play/Pause button: playing → pause, paused → resume,
    /// never-started → begin from 0 (the clip is local here). The
    /// never-started path routes through `maybe_autoplay_viewer_video` so a
    /// local clip without extracted frames goes through async extraction
    /// instead of playing with an empty frame cache.
    fn toggle_viewer_video(&mut self, cx: &mut Context<Self>) {
        let playing = self
            .viewer_clock
            .as_ref()
            .is_some_and(|clock| clock.is_playing());
        if self.viewer_video.is_none() {
            self.maybe_autoplay_viewer_video(cx);
            return;
        }
        if playing {
            self.pause_viewer_video(cx);
        } else {
            self.resume_viewer_video(cx);
        }
    }

    /// Full stop: kill ffplay and any running frame extraction, and clear
    /// all viewer-video state. Called on viewer close/step and when any
    /// other player starts.
    fn stop_viewer_video(&mut self) {
        self.kill_viewer_player();
        self.kill_viewer_extraction();
        self.viewer_video = None;
        self.viewer_video_path = None;
        self.viewer_clock = None;
        self.viewer_pending_play = None;
        self.viewer_video_frames.clear();
        self.viewer_extracting = false;
        if let Some(cached) = self.viewer_frame_cache_file.take() {
            quill::video::discard_viewer_frame_cache(cached);
        }
    }

    /// 125 ms tick while a viewer clip is active: re-renders so the
    /// elapsed/total label advances and the in-viewer frame animates
    /// (8 fps frames need a sub-250 ms refresh); auto-stops when the clock
    /// reaches the duration (ffplay `-autoexit` exits on its own).
    fn spawn_viewer_tick(&mut self, cx: &mut Context<Self>) {
        if self.viewer_tick {
            return;
        }
        self.viewer_tick = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(125))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        let active = this.viewer_video.is_some();
                        if !active {
                            return false;
                        }
                        let finished = this
                            .viewer_clock
                            .as_ref()
                            .is_some_and(|clock| clock.is_playing() && clock.finished());
                        if finished {
                            this.stop_viewer_video();
                            cx.notify();
                            return false;
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.viewer_tick = false;
            });
        })
        .detach();
    }

    /// Parity slice 5: scroll-zoom the viewer visual (scroll up = zoom in,
    /// matching the platform's positive-y convention).
    fn viewer_zoom_scroll(&mut self, delta_y: f32, cx: &mut Context<Self>) {
        if delta_y == 0.0 {
            return;
        }
        self.viewer_zoom.step(delta_y > 0.0, Self::VIEWER_FRAME);
        cx.notify();
    }

    /// Parity slice 5: drag-pan the zoomed visual by a mouse delta in px.
    fn viewer_pan_drag(&mut self, dx: f32, dy: f32, cx: &mut Context<Self>) {
        if !self.viewer_zoom.is_zoomed() {
            return;
        }
        self.viewer_zoom.pan_by(dx, dy, Self::VIEWER_FRAME);
        cx.notify();
    }

    /// Parity slice 5: step the viewer zoom in or out one notch.
    fn viewer_zoom_step(&mut self, zoom_in: bool, cx: &mut Context<Self>) {
        self.viewer_zoom.step(zoom_in, Self::VIEWER_FRAME);
        cx.notify();
    }

    /// Parity slice 5: reset zoom/pan to fit (double-click / `0`).
    fn viewer_reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.viewer_zoom.reset();
        self.viewer_drag = None;
        cx.notify();
    }

    /// Phase 9.1: open the fullscreen story viewer on `(chat_id, story_id)`.
    /// Missing story details for the chat's active stories are fetched with
    /// `getStory` first; the clicked story opens once its `story` response
    /// lands in the cache (`pending_story_open`, resolved on the next
    /// render). `openStory` marks the current story as viewed.
    fn open_story_viewer(&mut self, chat_id: ChatId, story_id: i32, cx: &mut Context<Self>) {
        let missing: Vec<i32> = self
            .session()
            .and_then(|session| session.story_tray.get(&chat_id.0))
            .map(|tray| {
                tray.stories
                    .iter()
                    .map(|info| info.story_id)
                    .filter(|id| {
                        !self
                            .session()
                            .is_some_and(|s| s.stories.contains_key(&(chat_id.0, *id)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Some(live) = self.live.as_mut() {
            for id in missing {
                let _ = live.driver.get_story(chat_id, id);
            }
        } else if !missing.is_empty() {
            self.status_note = "demo — getStory runs with live TDLib".into();
        }
        if !self.rebuild_story_viewer(chat_id, story_id, cx) {
            self.pending_story_open = Some((chat_id.0, story_id));
        }
        cx.notify();
    }

    /// Build the viewer items for `chat_id`'s cached stories and open on
    /// `story_id`. Returns `false` when the clicked story is not cached yet.
    fn rebuild_story_viewer(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        cx: &mut Context<Self>,
    ) -> bool {
        let items: Vec<StoryViewerItem> = self
            .session()
            .and_then(|session| {
                session
                    .story_tray
                    .get(&chat_id.0)
                    .map(|tray| collect_story_items(chat_id, tray, &session.stories))
            })
            .unwrap_or_default();
        let Some(index) = items.iter().position(|item| item.story_id == story_id) else {
            return false;
        };
        self.story_viewer = StoryViewer::open(items, index);
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.open_story(chat_id, story_id);
        }
        self.ensure_story_download(cx);
        true
    }

    /// Phase 9.2: the viewer story's freshest `ParsedStory` (reaction
    /// state and interaction counts arrive via `updateStory` without the
    /// viewer items being rebuilt).
    fn current_story(&self) -> Option<ParsedStory> {
        let item = self.story_viewer.current()?;
        self.session()
            .and_then(|session| session.stories.get(&(item.chat_id.0, item.story_id)))
            .cloned()
    }

    /// Phase 9.2: quick-react — toggle the ❤ (`reactionTypeEmoji`,
    /// `schema/td_api.tl:2915`) reaction on the current story via
    /// `setStoryReaction` (`schema/td_api.tl:13809`). Removing sends
    /// `reaction_type: null`.
    fn quick_react_story(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.story_viewer.current().cloned() else {
            return;
        };
        let chosen = self
            .current_story()
            .and_then(|story| story.chosen_reaction_emoji);
        let emoji = if chosen.as_deref() == Some("❤") {
            None
        } else {
            Some("❤")
        };
        if let Some(live) = self.live.as_mut() {
            self.status_note =
                match live
                    .driver
                    .set_story_reaction(item.chat_id, item.story_id, emoji)
                {
                    Ok(_) => {
                        if emoji.is_some() {
                            "Reacted ❤".into()
                        } else {
                            "Reaction removed".into()
                        }
                    }
                    Err(_) => "could not set story reaction".into(),
                };
        } else if self.demo_session.is_some() {
            self.status_note = "demo — setStoryReaction runs with live TDLib".into();
        }
        self.story_reaction_picker_open = false;
        cx.notify();
    }

    /// Phase 9.2: toggle the reaction picker above the viewer. The first
    /// open on a live connection fetches `getStoryAvailableReactions`
    /// (`schema/td_api.tl:13802`).
    fn toggle_story_reaction_picker(&mut self, cx: &mut Context<Self>) {
        self.story_reaction_picker_open = !self.story_reaction_picker_open;
        if self.story_reaction_picker_open {
            if let Some(live) = self.live.as_mut() {
                if live.driver.session.story_available_reactions.is_none() {
                    match live.driver.get_story_available_reactions() {
                        Ok(_) => {}
                        Err(_) => self.status_note = "could not load story reactions".into(),
                    }
                }
            } else if self.demo_session.is_some() {
                self.status_note = "demo — story reactions run with live TDLib".into();
            }
        }
        cx.notify();
    }

    /// Phase 9.2: set the current story's reaction to a picker emoji.
    fn pick_story_reaction(&mut self, emoji: &str, cx: &mut Context<Self>) {
        let Some(item) = self.story_viewer.current().cloned() else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            self.status_note =
                match live
                    .driver
                    .set_story_reaction(item.chat_id, item.story_id, Some(emoji))
                {
                    Ok(_) => format!("Reacted {emoji}"),
                    Err(_) => "could not set story reaction".into(),
                };
        } else if self.demo_session.is_some() {
            self.status_note = "demo — story reactions run with live TDLib".into();
        }
        self.story_reaction_picker_open = false;
        cx.notify();
    }

    /// Phase 9.2: toggle the reply row in the viewer (`story.can_be_replied`
    /// gates the button).
    fn toggle_story_reply(&mut self, cx: &mut Context<Self>) {
        self.story_reply_open = !self.story_reply_open;
        cx.notify();
    }

    /// Phase 9.2: send the reply row's text as a message to the story
    /// poster with `inputMessageReplyToStory` (`schema/td_api.tl:3099`).
    fn send_story_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.story_viewer.current().cloned() else {
            return;
        };
        let text = self.story_reply_input.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            self.status_note =
                match live
                    .driver
                    .send_story_reply(item.chat_id, item.story_id, &text)
                {
                    Ok(_) => "Story reply sent".into(),
                    Err(_) => "could not send story reply".into(),
                };
            self.story_reply_input
                .update(cx, |input, cx| input.set_value("", window, cx));
        } else if self.demo_session.is_some() {
            self.status_note = "demo — story replies run with live TDLib".into();
        }
        self.story_reply_open = false;
        cx.notify();
    }

    /// Phase 9.2: delete the current story (`deleteStory`,
    /// `schema/td_api.tl:13754`; `story.can_be_deleted` gates the button).
    /// The deletion lands as `updateStoryDeleted`, which closes the viewer
    /// at render time.
    fn delete_story_viewer(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.story_viewer.current().cloned() else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.delete_story(item.chat_id, item.story_id) {
                Ok(_) => "Deleting story…".into(),
                Err(_) => "could not delete story".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo — deleteStory runs with live TDLib".into();
        }
        self.story_reaction_picker_open = false;
        self.story_reply_open = false;
        cx.notify();
    }

    /// Phase 9.1: close the story viewer; `closeStory` marks the current
    /// story as no longer being viewed.
    fn close_story_viewer(&mut self, cx: &mut Context<Self>) {
        if let Some(item) = self.story_viewer.current().cloned() {
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.close_story(item.chat_id, item.story_id);
            }
        }
        self.story_viewer.close();
        self.pending_story_open = None;
        self.story_reaction_picker_open = false;
        self.story_reply_open = false;
        cx.notify();
    }

    fn step_story_viewer(&mut self, delta: i32, cx: &mut Context<Self>) {
        let prev = self.story_viewer.current().cloned();
        if delta < 0 {
            self.story_viewer.prev();
        } else {
            self.story_viewer.next();
        }
        let next = self.story_viewer.current().cloned();
        if let (Some(prev), Some(next)) = (prev, next) {
            if (prev.chat_id, prev.story_id) != (next.chat_id, next.story_id) {
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.close_story(prev.chat_id, prev.story_id);
                    let _ = live.driver.open_story(next.chat_id, next.story_id);
                }
            }
        }
        self.story_reaction_picker_open = false;
        self.story_reply_open = false;
        self.ensure_story_download(cx);
        cx.notify();
    }

    /// Trigger `downloadFile` for the current story when no display
    /// candidate is local yet (photo: largest size; video: thumbnail, else
    /// the clip itself). Live-only, like `ensure_viewer_download`.
    fn ensure_story_download(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.story_viewer.current().cloned() else {
            return;
        };
        let roots = self.media_display_roots();
        let local = self.session().is_some_and(|session| {
            item.display_file_ids.iter().any(|id| {
                session
                    .files
                    .get(&id.0)
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))
                    .is_some()
            })
        });
        if !local && item.download_file_id.0 != 0 {
            self.request_media_download(item.download_file_id, None, cx);
        }
    }

    fn start_voice_recording(&mut self, cx: &mut Context<Self>) {
        if self.pending_edit.is_some() || self.voice_capture.is_some() {
            return;
        }
        if self.gif_panel_open() {
            self.close_gif_panel(cx);
        }
        if self.sticker_panel_open() {
            self.close_sticker_panel(cx);
        }
        match VoiceCapture::start() {
            Ok(capture) => {
                self.voice_capture = Some(capture);
                self.sync_voice_action();
                self.spawn_voice_tick(cx);
                self.status_note = "recording voice note".into();
            }
            Err(err) => {
                self.status_note = err;
            }
        }
        cx.notify();
    }

    fn cancel_voice_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(capture) = self.voice_capture.take() {
            capture.discard();
        }
        self.sync_voice_action();
        self.status_note = "voice recording cancelled".into();
        cx.notify();
    }

    fn send_voice_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Phase A1: slow-mode gate applies to voice notes too — checked
        // before consuming the capture so a blocked recording survives
        // until the timer expires.
        if self
            .open_chat_id()
            .is_some_and(|chat_id| self.slow_mode_blocked(chat_id, cx))
        {
            return;
        }
        let Some(capture) = self.voice_capture.take() else {
            return;
        };
        let caption = self.composer.read(cx).value().to_string();
        let draft = match capture.finish() {
            Ok(draft) => draft,
            Err(err) => {
                self.sync_voice_action();
                self.status_note = err;
                cx.notify();
                return;
            }
        };
        let reply = self.pending_reply.clone();
        if self.live.is_some() {
            let reply_to = reply
                .as_ref()
                .filter(|reply| {
                    self.live
                        .as_ref()
                        .is_some_and(|live| live.driver.session.open_chat == Some(reply.chat_id))
                })
                .map(|reply| reply.message_id);
            let result = self.live.as_mut().expect("live").driver.send_voice_note(
                &draft,
                caption.trim(),
                reply_to,
            );
            self.status_note = match result {
                Ok(_) => "sending voice note".into(),
                Err(_) => "could not send voice note".into(),
            };
            if self.status_note == "sending voice note" {
                self.pending_reply = None;
                self.clear_draft_on_success = Some(
                    self.live
                        .as_ref()
                        .and_then(|live| live.driver.session.open_chat)
                        .unwrap_or(ChatId(0)),
                );
                self.composer
                    .update(cx, |input, cx| input.set_value("", window, cx));
                if let Some(open) = self.open_chat_id() {
                    self.forget_local_draft(open);
                }
            }
        } else if self.demo_session.is_some() {
            self.apply_demo_voice(&draft, caption.trim(), reply.as_ref());
            self.pending_reply = None;
            self.composer
                .update(cx, |input, cx| input.set_value("", window, cx));
            if let Some(open) = self.demo_session.as_ref().and_then(|s| s.open_chat) {
                self.forget_local_draft(open);
            }
            self.status_note = "demo voice note applied locally (no live Telegram)".into();
        }
        self.sync_voice_action();
        cx.notify();
    }

    fn apply_demo_voice(
        &mut self,
        draft: &quill::voice::VoiceDraft,
        caption: &str,
        reply: Option<&ComposerReplyTo>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let Some(chat_id) = session.open_chat else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let id = -(session.view_generation.0 as i64);
        let path = draft.path.to_string_lossy();
        let file = demo_file_json(910, &path, true);
        let waveform = draft.waveform_b64();
        let reply_json = reply
            .filter(|r| r.chat_id == chat_id)
            .map(|r| {
                format!(
                    r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":{},"quote":null,"checklist_task_id":0,"poll_option_id":""}}"#,
                    r.chat_id.0, r.message_id.0
                )
            })
            .unwrap_or_default();
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":{},"waveform":{},"mime_type":"audio/ogg","speech_recognition_result":null,"voice":{file}}},"caption":{{"@type":"formattedText","text":{},"entities":[]}},"is_listened":true}}{reply_json}}}}}"#,
            chat_id.0,
            draft.duration_secs,
            serde_json::to_string(&waveform).unwrap_or_else(|_| "\"\"".into()),
            serde_json::to_string(caption).unwrap_or_else(|_| "\"\"".into()),
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    fn sync_voice_action(&mut self) {
        let active = self.voice_capture.is_some();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.sync_voice_recording(active, now_ms);
        }
    }

    fn stop_animation_playback(&mut self) {
        if let Some(file_id) = self.animation_cache_file.take() {
            quill::animation::discard_frame_cache(file_id);
        }
        self.playing_animation = None;
        self.animation_frames.clear();
        self.animation_frame = 0;
        self.pending_gif_play = None;
    }

    fn spawn_animation_tick(&mut self, cx: &mut Context<Self>) {
        if self.animation_tick {
            return;
        }
        self.animation_tick = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        let playing =
                            this.playing_animation.is_some() && this.animation_frames.len() > 1;
                        if playing {
                            this.animation_frame =
                                (this.animation_frame + 1) % this.animation_frames.len();
                            cx.notify();
                        }
                        this.playing_animation.is_some()
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.animation_tick = false;
            });
        })
        .detach();
    }

    fn toggle_animation_playback(
        &mut self,
        message_id: MessageId,
        file_id: FileId,
        mime: String,
        cx: &mut Context<Self>,
    ) {
        if self.playing_animation == Some(message_id) {
            self.stop_animation_playback();
            self.status_note = "GIF paused".into();
            cx.notify();
            return;
        }
        let path = self.session().and_then(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .map(str::to_string)
        });
        let Some(path) = path else {
            self.pending_gif_play = Some((message_id, file_id, mime));
            self.request_media_download(file_id, None, cx);
            self.status_note = "downloading GIF".into();
            return;
        };
        self.pending_gif_play = None;
        let roots = self.media_display_roots();
        let Some(safe) = sandboxed_display_path(&path, &roots) else {
            self.status_note = "GIF file is outside the account files".into();
            cx.notify();
            return;
        };
        let cache = quill::animation::gif_frame_cache_dir(file_id.0);
        match quill::animation::playback_frames(&safe, &mime, &cache) {
            Ok(frames) if !frames.is_empty() => {
                self.stop_voice_playback();
                self.stop_audio_playback();
                self.stop_video_playback();
                self.stop_viewer_video();
                self.pending_gif_play = None;
                if self.animation_cache_file.is_some_and(|id| id != file_id.0)
                    && let Some(old) = self.animation_cache_file.take()
                {
                    quill::animation::discard_frame_cache(old);
                }
                self.animation_cache_file = Some(file_id.0);
                self.playing_animation = Some(message_id);
                self.animation_frames = frames;
                self.animation_frame = 0;
                self.spawn_animation_tick(cx);
                self.status_note = "playing GIF".into();
            }
            _ => {
                self.status_note = "could not play GIF".into();
            }
        }
        cx.notify();
    }

    fn stop_video_playback(&mut self) {
        if let Some(file_id) = self.video_cache_file.take() {
            quill::video::discard_frame_cache(file_id);
        }
        self.playing_video = None;
        self.video_frames.clear();
        self.video_frame = 0;
        self.pending_video_play = None;
    }

    fn spawn_video_tick(&mut self, cx: &mut Context<Self>) {
        if self.video_tick {
            return;
        }
        self.video_tick = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        let playing = this.playing_video.is_some() && this.video_frames.len() > 1;
                        if playing {
                            this.video_frame = (this.video_frame + 1) % this.video_frames.len();
                            cx.notify();
                        }
                        this.playing_video.is_some()
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.video_tick = false;
            });
        })
        .detach();
    }

    fn toggle_video_playback(
        &mut self,
        message_id: MessageId,
        file_id: FileId,
        mime: String,
        start_timestamp: i32,
        mark_opened: Option<ChatId>,
        cx: &mut Context<Self>,
    ) {
        if self.playing_video == Some(message_id) {
            self.stop_video_playback();
            self.status_note = "video paused".into();
            cx.notify();
            return;
        }
        let path = self.session().and_then(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .map(str::to_string)
        });
        let Some(path) = path else {
            self.pending_video_play =
                Some((message_id, file_id, mime, start_timestamp, mark_opened));
            self.request_media_download(file_id, None, cx);
            self.status_note = "downloading video".into();
            return;
        };
        self.pending_video_play = None;
        let roots = self.media_display_roots();
        let Some(safe) = sandboxed_display_path(&path, &roots) else {
            self.status_note = "video file is outside the account files".into();
            cx.notify();
            return;
        };
        let cache = quill::video::video_frame_cache_dir(file_id.0);
        match quill::video::playback_frames(&safe, &mime, &cache, start_timestamp) {
            Ok(frames) if !frames.is_empty() => {
                self.stop_voice_playback();
                self.stop_audio_playback();
                self.stop_animation_playback();
                self.stop_viewer_video();
                self.pending_video_play = None;
                if self.video_cache_file.is_some_and(|id| id != file_id.0)
                    && let Some(old) = self.video_cache_file.take()
                {
                    quill::video::discard_frame_cache(old);
                }
                self.video_cache_file = Some(file_id.0);
                self.playing_video = Some(message_id);
                self.video_frames = frames;
                self.video_frame = 0;
                self.spawn_video_tick(cx);
                if let Some(chat_id) = mark_opened {
                    self.mark_voice_opened(chat_id, message_id);
                }
                self.status_note = "playing video".into();
            }
            _ => {
                self.status_note = "could not play video".into();
            }
        }
        cx.notify();
    }

    fn resume_pending_video(&mut self, cx: &mut Context<Self>) {
        let Some((message_id, file_id, mime, start_timestamp, mark_opened)) =
            self.pending_video_play.clone()
        else {
            return;
        };
        let ready = self.session().is_some_and(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .is_some()
        });
        if ready {
            self.toggle_video_playback(message_id, file_id, mime, start_timestamp, mark_opened, cx);
        }
    }

    fn resume_pending_gif(&mut self, cx: &mut Context<Self>) {
        let Some((message_id, file_id, mime)) = self.pending_gif_play.clone() else {
            return;
        };
        let ready = self.session().is_some_and(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .is_some()
        });
        if ready {
            self.toggle_animation_playback(message_id, file_id, mime, cx);
        }
    }

    fn spawn_voice_tick(&mut self, cx: &mut Context<Self>) {
        if self.voice_tick {
            return;
        }
        self.voice_tick = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        let recording = this.voice_capture.is_some();
                        if let Some(capture) = this.voice_capture.as_mut() {
                            capture.sample_bar();
                            this.sync_voice_action();
                            cx.notify();
                        }
                        recording
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.voice_tick = false;
            });
        })
        .detach();
    }

    /// Phase A1: slow-mode wait (whole seconds) for a chat, via the
    /// session gate (`Session::slow_mode_wait_secs`).
    fn slow_mode_wait_secs_for(&self, chat_id: ChatId) -> Option<u64> {
        let session = self.session()?;
        session.slow_mode_wait_secs(chat_id, unix_ms_now())
    }

    /// Phase A1: slow-mode wait for the currently open chat, if any.
    fn slow_mode_wait_secs(&self) -> Option<u64> {
        let chat_id = self.session()?.open_chat?;
        self.slow_mode_wait_secs_for(chat_id)
    }

    /// Phase A1: centralized slow-mode send gate. When the gate is active
    /// for `chat_id`, sets the status note and returns `true` — callers
    /// must not send. On live sessions a blocked send also re-reads the
    /// server value via `refresh_supergroup_full_info`, because the schema
    /// (1.8.67, line 2759) warns no `updateSupergroupFullInfo` fires when
    /// only the expiry changes while old and new are non-zero.
    fn slow_mode_blocked(&mut self, chat_id: ChatId, cx: &mut Context<Self>) -> bool {
        let Some(wait) = self.slow_mode_wait_secs_for(chat_id) else {
            return false;
        };
        self.status_note = format!("Slow mode: wait {wait}s before sending");
        if self.live.is_some() {
            let supergroup_id = self.live.as_ref().and_then(|live| {
                let session = &live.driver.session;
                match session.chats.get(&chat_id.0)?.kind {
                    ChatKind::Supergroup {
                        supergroup_id,
                        is_channel: false,
                    } => Some(supergroup_id),
                    _ => None,
                }
            });
            if let (Some(live), Some(supergroup_id)) = (self.live.as_mut(), supergroup_id) {
                let _ = live.driver.refresh_supergroup_full_info(supergroup_id);
            }
        }
        cx.notify();
        true
    }

    /// Phase A1: keep the slow-mode countdown re-rendering while the open
    /// chat is gated. At most one task per open chat (guarded by
    /// `slow_mode_tick_chat`, mirroring `spawn_voice_tick`); it exits
    /// when the gate lifts or the open chat changes. Called from
    /// `render`, which has the `&mut self` the tick needs.
    fn ensure_slow_mode_tick(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self
            .session()
            .and_then(|session| session.open_chat)
            .filter(|id| self.slow_mode_wait_secs_for(*id).is_some())
        else {
            return;
        };
        if self.slow_mode_tick_chat == Some(chat_id) {
            return;
        }
        self.slow_mode_tick_chat = Some(chat_id);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let cont = this
                    .update(cx, |this, cx| {
                        let still_open = this.session().and_then(|s| s.open_chat) == Some(chat_id);
                        let still_gated = this.slow_mode_wait_secs_for(chat_id).is_some();
                        if still_open && still_gated {
                            cx.notify();
                            true
                        } else {
                            if this.slow_mode_tick_chat == Some(chat_id) {
                                this.slow_mode_tick_chat = None;
                            }
                            false
                        }
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
        })
        .detach();
    }

    /// Phase B3: keep the self-destruct countdown badges re-rendering
    /// while the open chat has a live `self_destruct_in` timer. At most
    /// one task per open chat (guarded by `self_destruct_tick_chat`,
    /// mirroring the Phase A1 slow-mode tick); it exits when no timer is
    /// live or the open chat changes. Called from `render`, which has the
    /// `&mut self` the tick needs.
    fn ensure_self_destruct_tick(&mut self, cx: &mut Context<Self>) {
        let now_ms = unix_ms_now();
        let Some(chat_id) = self
            .session()
            .and_then(|session| session.open_chat)
            .filter(|_| {
                self.session()
                    .is_some_and(|session| session.open_chat_has_live_self_destruct(now_ms))
            })
        else {
            return;
        };
        if self.self_destruct_tick_chat == Some(chat_id) {
            return;
        }
        self.self_destruct_tick_chat = Some(chat_id);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let cont = this
                    .update(cx, |this, cx| {
                        let now_ms = unix_ms_now();
                        let still_open = this.session().and_then(|s| s.open_chat) == Some(chat_id);
                        let still_live = this.session().is_some_and(|session| {
                            session.open_chat_has_live_self_destruct(now_ms)
                        });
                        if still_open && still_live {
                            cx.notify();
                            true
                        } else {
                            if this.self_destruct_tick_chat == Some(chat_id) {
                                this.self_destruct_tick_chat = None;
                            }
                            false
                        }
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
        })
        .detach();
    }

    /// Phase C1: 1s tick while a call is tracked, keeping the overlay's
    /// ringing / connected clock fresh. Mirrors the Phase A1 slow-mode
    /// tick (at most one task; exits when no call is active).
    fn ensure_call_tick(&mut self, cx: &mut Context<Self>) {
        let call_active = self.session().is_some_and(|s| s.active_call.is_some());
        if !call_active || self.call_tick_active {
            return;
        }
        self.call_tick_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let cont = this
                    .update(cx, |this, cx| {
                        let still_active = this.session().is_some_and(|s| s.active_call.is_some());
                        if still_active {
                            cx.notify();
                            true
                        } else {
                            this.call_tick_active = false;
                            false
                        }
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.call_tick_active = false;
            });
        })
        .detach();
    }

    /// Phase A1: whether the viewer may change slow mode in a supergroup:
    /// creator, or administrator with the explicit `can_restrict_members`
    /// right (`setChatSlowModeDelay` requirement, schema 1.8.67 line
    /// 13551). Shared by the info-panel selector and `set_slow_mode_delay`
    /// (defense in depth — the request must never fire unauthorized).
    fn can_change_slow_mode(&self, supergroup_id: i64) -> bool {
        let session = match self.session() {
            Some(session) => session,
            None => return false,
        };
        let status = session.supergroup_own_status(supergroup_id);
        status == Some(ChannelMemberStatus::Creator)
            || (status == Some(ChannelMemberStatus::Administrator)
                && session.supergroup_can_restrict_members(supergroup_id))
    }

    /// Phase A1: admin slow-mode control (`setChatSlowModeDelay`, TDLib
    /// 1.8.67 line 13551) from the group info panel. The new delay
    /// arrives via `updateSupergroupFullInfo`; the panel re-renders then.
    fn set_slow_mode_delay(
        &mut self,
        chat_id: ChatId,
        slow_mode_delay: i32,
        cx: &mut Context<Self>,
    ) {
        let supergroup_id = self
            .session()
            .and_then(|s| match s.chats.get(&chat_id.0)?.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel: false,
                } => Some(supergroup_id),
                _ => None,
            });
        if supergroup_id.is_none_or(|id| !self.can_change_slow_mode(id)) {
            self.status_note = "slow mode needs the restrict-members admin right".into();
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live
                .driver
                .set_chat_slow_mode_delay(chat_id, slow_mode_delay)
            {
                Ok(_) => self.status_note = "slow mode updated".into(),
                Err(_) => self.status_note = "could not change slow mode".into(),
            }
        } else {
            self.status_note = "slow mode needs a live connection (demo)".into();
        }
        cx.notify();
    }

    fn kill_shared_player(&mut self) {
        if let Some(mut child) = self.voice_player.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn stop_voice_playback(&mut self) {
        if self.playing_voice.is_some() {
            self.kill_shared_player();
        }
        // Save the position before clearing the id: clear_playback_state
        // reads active_playback_id().
        self.clear_playback_state();
        self.playing_voice = None;
    }

    fn stop_audio_playback(&mut self) {
        if self.playing_audio.is_some() {
            self.kill_shared_player();
        }
        self.clear_playback_state();
        self.playing_audio = None;
        self.pending_audio_play = None;
    }

    /// Drop the seek-bar state for the active track (clock, slider entity,
    /// scrub preview). Idempotent: both stop functions call it. The
    /// remembered per-message position in `playback_positions` is kept so a
    /// stopped row still shows where it got to.
    fn clear_playback_state(&mut self) {
        if let (Some(message_id), Some(clock)) =
            (self.active_playback_id(), self.playback_clock.as_ref())
        {
            self.playback_positions
                .insert(message_id, clock.elapsed_secs());
        }
        self.playback_clock = None;
        self.playback_path = None;
        self.seek_slider = None;
        self.seek_scrubbing = false;
        self.seek_preview_secs = None;
    }

    /// Message id of the active (playing or paused) track, if any.
    fn active_playback_id(&self) -> Option<MessageId> {
        self.playing_voice.or(self.playing_audio)
    }

    /// Kind of the active track, for status notes.
    fn active_playback_kind(&self) -> PlaybackKind {
        if self.playing_voice.is_some() {
            PlaybackKind::Voice
        } else {
            PlaybackKind::Audio
        }
    }

    /// Mark the given row as the active track: sets `playing_voice` /
    /// `playing_audio`, starts the playback clock at `offset_secs`, builds
    /// the seek slider entity, and starts the progress tick. Does not spawn
    /// ffplay — the caller does that (the screenshot demo fakes playback
    /// without a subprocess).
    fn begin_track_playback(
        &mut self,
        kind: PlaybackKind,
        message_id: MessageId,
        duration_secs: f64,
        offset_secs: f64,
        cx: &mut Context<Self>,
    ) {
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.stop_viewer_video();
        match kind {
            PlaybackKind::Voice => self.playing_voice = Some(message_id),
            PlaybackKind::Audio => self.playing_audio = Some(message_id),
        }
        let mut clock = PlaybackClock::new(duration_secs);
        clock.seek(offset_secs);
        clock.resume();
        self.playback_clock = Some(clock);
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(duration_secs.max(0.1) as f32)
                .step(0.1)
                .default_value(offset_secs.clamp(0.0, duration_secs.max(0.0)) as f32)
        });
        cx.subscribe(&slider, |this, _entity, event: &SliderEvent, cx| {
            this.on_seek_event(event, cx);
        })
        .detach();
        self.seek_slider = Some(slider);
        self.seek_scrubbing = false;
        self.seek_preview_secs = None;
        self.spawn_playback_tick(cx);
    }

    /// Spawn ffplay for `path`, seeking to `offset_secs` first when positive.
    /// ffplay takes no seek commands on stdin, so seeking restarts the player
    /// with `-ss` (input seeking — fast on local files; see DECISIONS.md).
    /// Returns true when the child spawned.
    fn spawn_ffplay(&mut self, path: &std::path::Path, offset_secs: f64) -> bool {
        self.kill_shared_player();
        let mut command = Command::new("ffplay");
        command.args(["-nodisp", "-autoexit", "-loglevel", "quiet"]);
        if offset_secs > 0.05 {
            command.arg("-ss").arg(format!("{offset_secs:.1}"));
        }
        match command
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => {
                self.voice_player = Some(child);
                true
            }
            Err(_) => false,
        }
    }

    /// Restart the active track's player at `offset_secs` (seek while playing).
    fn restart_player_at(&mut self, offset_secs: f64) {
        if let Some(path) = self.playback_path.clone() {
            self.spawn_ffplay(&path, offset_secs);
        }
    }

    /// Pause the active track: freeze the clock, kill ffplay, keep the row
    /// active so the seek bar stays interactive and Play resumes from here.
    fn pause_active_playback(&mut self) {
        let id = self.active_playback_id();
        if let Some(clock) = self.playback_clock.as_mut() {
            clock.pause();
            if let Some(id) = id {
                self.playback_positions.insert(id, clock.elapsed_secs());
            }
        }
        self.kill_shared_player();
    }

    /// Resume the active track from the frozen clock position.
    fn resume_active_playback(&mut self) {
        let offset = self.playback_clock.as_ref().map(|c| c.elapsed_secs());
        let path = self.playback_path.clone();
        match (offset, path) {
            (Some(offset), Some(path)) => {
                self.spawn_ffplay(&path, offset);
                if let Some(clock) = self.playback_clock.as_mut() {
                    clock.resume();
                }
            }
            _ => {
                self.stop_voice_playback();
                self.stop_audio_playback();
            }
        }
    }

    /// `SliderEvent` sink for the active row's seek slider.
    fn on_seek_event(&mut self, event: &SliderEvent, cx: &mut Context<Self>) {
        match event {
            SliderEvent::Change(value) => {
                // Drag (or track click) in progress: show the preview in the
                // time label, but don't touch the player until Release.
                self.seek_scrubbing = true;
                self.seek_preview_secs = Some(f64::from(value.end()));
                cx.notify();
            }
            SliderEvent::Release(value) => {
                self.seek_scrubbing = false;
                self.seek_preview_secs = None;
                self.seek_active_to(f64::from(value.end()), cx);
            }
        }
    }

    /// Apply a finished seek: clamp, move the clock, and restart ffplay at
    /// the new offset when the track is playing. Seeking while paused just
    /// moves the frozen position (no player restart).
    fn seek_active_to(&mut self, secs: f64, cx: &mut Context<Self>) {
        let Some(clock) = self.playback_clock.as_mut() else {
            return;
        };
        clock.seek(secs);
        let offset = clock.elapsed_secs();
        let was_playing = clock.is_playing();
        if let Some(id) = self.active_playback_id() {
            self.playback_positions.insert(id, offset);
        }
        if was_playing {
            self.restart_player_at(offset);
            self.status_note = format!(
                "{} — seek {}",
                match self.active_playback_kind() {
                    PlaybackKind::Voice => "playing voice note",
                    PlaybackKind::Audio => "playing audio",
                },
                format_voice_duration(offset as i32)
            );
        }
        cx.notify();
    }

    /// 250 ms progress tick while a track is active: re-renders so the seek
    /// bar advances, and auto-stops when the clock reaches the duration
    /// (ffplay `-autoexit` exits on its own; this clears our state to match).
    fn spawn_playback_tick(&mut self, cx: &mut Context<Self>) {
        if self.playback_tick {
            return;
        }
        self.playback_tick = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        let active = this.active_playback_id().is_some();
                        if !active {
                            return false;
                        }
                        let finished = this
                            .playback_clock
                            .as_ref()
                            .is_some_and(|clock| clock.is_playing() && clock.finished());
                        if finished && !this.seek_scrubbing {
                            // Capture the id first: the stops below save the
                            // (now end-of-track) position via clear_playback_state,
                            // then reset to 0.0 so replay-after-finish starts at the top.
                            let finished_id = this.active_playback_id();
                            this.stop_voice_playback();
                            this.stop_audio_playback();
                            if let Some(id) = finished_id {
                                this.playback_positions.insert(id, 0.0);
                            }
                            this.status_note = "playback finished".into();
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.playback_tick = false;
            });
        })
        .detach();
    }

    /// Push the playback clock into the seek slider entity so the thumb
    /// follows elapsed time. Called at the top of `render` (the tick has no
    /// `&mut Window`, which `SliderState::set_value` needs). Skipped while
    /// scrubbing so the user's drag is never fought.
    fn sync_seek_slider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.seek_scrubbing {
            return;
        }
        if let (Some(slider), Some(clock)) =
            (self.seek_slider.as_ref(), self.playback_clock.as_ref())
        {
            let value = clock.elapsed_secs().clamp(0.0, clock.duration_secs()) as f32;
            // `set_value` calls `cx.notify()` unconditionally — only push when
            // the value actually changed, otherwise this render-triggered sync
            // loops at frame rate instead of the tick cadence.
            let changed = slider.read(cx).value() != SliderValue::Single(value);
            if changed {
                slider.update(cx, |state, cx| {
                    state.set_value(value, window, cx);
                });
            }
        }
    }

    /// View model for one audio/voice row's seek bar.
    fn seek_bar_view(&self, message_id: MessageId, duration_secs: f64) -> SeekBarView {
        let active = self.active_playback_id() == Some(message_id);
        if active {
            let display = self.seek_preview_secs.or_else(|| {
                self.playback_clock
                    .as_ref()
                    .map(|clock| clock.elapsed_secs())
            });
            SeekBarView {
                slider: self.seek_slider.clone(),
                display_secs: display.unwrap_or(0.0),
                duration_secs,
                is_playing: self
                    .playback_clock
                    .as_ref()
                    .is_some_and(PlaybackClock::is_playing),
            }
        } else {
            SeekBarView {
                slider: None,
                display_secs: self
                    .playback_positions
                    .get(&message_id)
                    .copied()
                    .unwrap_or(0.0),
                duration_secs,
                is_playing: false,
            }
        }
    }

    fn toggle_voice_playback(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        file_id: FileId,
        listened: bool,
        duration_secs: f64,
        cx: &mut Context<Self>,
    ) {
        if self.playing_voice == Some(message_id) {
            // Active row: pause ↔ resume (the row stays active so the seek
            // bar keeps working and Play resumes from the frozen position).
            let playing = self
                .playback_clock
                .as_ref()
                .is_some_and(PlaybackClock::is_playing);
            if playing {
                self.pause_active_playback();
                self.status_note = "voice note paused".into();
            } else {
                self.resume_active_playback();
                self.status_note = "playing voice note".into();
            }
            cx.notify();
            return;
        }
        let path = self.session().and_then(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .map(str::to_string)
        });
        let Some(path) = path else {
            self.request_media_download(file_id, None, cx);
            return;
        };
        let roots = self.media_display_roots();
        let Some(safe) = sandboxed_display_path(&path, &roots) else {
            self.status_note = "voice file is outside the account files".into();
            cx.notify();
            return;
        };
        self.stop_video_playback();
        let offset = self
            .playback_positions
            .get(&message_id)
            .copied()
            .unwrap_or(0.0);
        self.begin_track_playback(PlaybackKind::Voice, message_id, duration_secs, offset, cx);
        self.playback_path = Some(safe.clone().into());
        if !listened {
            self.mark_voice_opened(chat_id, message_id);
        }
        self.spawn_ffplay(&safe, offset);
        self.status_note = if self.voice_player.is_some() {
            "playing voice note".into()
        } else {
            "playing voice note (no audio player)".into()
        };
        cx.notify();
    }

    fn toggle_audio_playback(
        &mut self,
        message_id: MessageId,
        file_id: FileId,
        duration_secs: f64,
        cx: &mut Context<Self>,
    ) {
        if self.playing_audio == Some(message_id) {
            // Active row: pause ↔ resume (see voice toggle).
            let playing = self
                .playback_clock
                .as_ref()
                .is_some_and(PlaybackClock::is_playing);
            if playing {
                self.pause_active_playback();
                self.status_note = "audio paused".into();
            } else {
                self.resume_active_playback();
                self.status_note = "playing audio".into();
            }
            cx.notify();
            return;
        }
        let path = self.session().and_then(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .map(str::to_string)
        });
        let Some(path) = path else {
            self.pending_audio_play = Some((message_id, file_id, duration_secs));
            self.request_media_download(file_id, None, cx);
            self.status_note = "downloading audio".into();
            return;
        };
        self.pending_audio_play = None;
        let roots = self.media_display_roots();
        let Some(safe) = sandboxed_display_path(&path, &roots) else {
            self.status_note = "audio file is outside the account files".into();
            cx.notify();
            return;
        };
        self.stop_video_playback();
        self.stop_animation_playback();
        let offset = self
            .playback_positions
            .get(&message_id)
            .copied()
            .unwrap_or(0.0);
        self.begin_track_playback(PlaybackKind::Audio, message_id, duration_secs, offset, cx);
        self.playback_path = Some(safe.clone().into());
        self.spawn_ffplay(&safe, offset);
        self.status_note = if self.voice_player.is_some() {
            "playing audio".into()
        } else {
            "playing audio (no audio player)".into()
        };
        cx.notify();
    }

    fn resume_pending_audio(&mut self, cx: &mut Context<Self>) {
        let Some((message_id, file_id, duration_secs)) = self.pending_audio_play else {
            return;
        };
        let ready = self.session().is_some_and(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .is_some()
        });
        if ready {
            self.toggle_audio_playback(message_id, file_id, duration_secs, cx);
        }
    }

    fn mark_voice_opened(&mut self, chat_id: ChatId, message_id: MessageId) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.open_voice_content(chat_id, message_id);
            return;
        }
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let json = format!(
            r#"{{"@type":"updateMessageContentOpened","chat_id":{},"message_id":{}}}"#,
            chat_id.0, message_id.0
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    fn gif_panel_open(&self) -> bool {
        self.session().is_some_and(|session| session.gifs.open)
    }

    fn toggle_gif_panel(&mut self, cx: &mut Context<Self>) {
        if self.voice_capture.is_some() {
            self.cancel_voice_recording(cx);
        }
        if self.gif_panel_open() {
            self.close_gif_panel(cx);
            return;
        }
        if self.sticker_panel_open() {
            self.close_sticker_panel(cx);
        }
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.open_gif_panel() {
                Ok(_) => "GIFs".into(),
                Err(_) => "could not open GIFs".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.gifs.open = true;
            self.status_note = "GIFs".into();
        }
        cx.notify();
    }

    fn close_gif_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_gif_panel();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.gifs.close();
        }
        self.status_note = "GIFs closed".into();
        cx.notify();
    }

    fn send_gif_pick(
        &mut self,
        file_id: FileId,
        duration: i32,
        width: i32,
        height: i32,
        cx: &mut Context<Self>,
    ) {
        let chat_id = self.session().and_then(|session| session.open_chat);
        let Some(chat_id) = chat_id else {
            return;
        };
        // Phase A1: slow-mode gate applies to GIF sends too.
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        let reply = self
            .pending_reply
            .as_ref()
            .filter(|reply| reply.chat_id == chat_id)
            .map(|reply| reply.message_id);
        let sent = if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.send_animation(
                chat_id,
                quill::telegram::requests::AnimationSend {
                    file_id,
                    duration,
                    width,
                    height,
                    reply_to: reply,
                    // Parity slice 4: the driver addresses the open topic
                    // from the session; the UI passes no topic.
                    topic_id: None,
                },
            ) {
                Ok(_) => "sending GIF".into(),
                Err(_) => "could not send GIF".into(),
            };
            self.status_note == "sending GIF"
        } else if self.demo_session.is_some() {
            self.apply_demo_gif(chat_id, file_id, duration, width, height, reply);
            self.status_note = "demo GIF applied locally (no live Telegram)".into();
            true
        } else {
            false
        };
        if sent {
            self.consume_sent_reply(chat_id, cx);
        }
        cx.notify();
    }

    fn sticker_panel_open(&self) -> bool {
        self.session().is_some_and(|session| session.stickers.open)
    }

    fn toggle_sticker_panel(&mut self, cx: &mut Context<Self>) {
        if self.voice_capture.is_some() {
            self.cancel_voice_recording(cx);
        }
        if self.sticker_panel_open() {
            self.close_sticker_panel(cx);
            return;
        }
        if self.gif_panel_open() {
            self.close_gif_panel(cx);
        }
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.open_sticker_panel() {
                Ok(_) => "stickers".into(),
                Err(_) => "could not open stickers".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.open = true;
            self.status_note = "stickers".into();
        }
        cx.notify();
    }

    fn close_sticker_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_sticker_panel();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.close();
        }
        self.status_note = "stickers closed".into();
        cx.notify();
    }

    fn select_sticker_set(&mut self, set_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.select_sticker_set(set_id) {
                Ok(_) => "sticker set".into(),
                Err(_) => "could not open sticker set".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.select_sticker_set(set_id);
            self.status_note = "sticker set".into();
        }
        cx.notify();
    }

    fn send_sticker_pick(
        &mut self,
        file_id: FileId,
        emoji: String,
        width: i32,
        height: i32,
        thumb: Option<(FileId, i32, i32)>,
        cx: &mut Context<Self>,
    ) {
        let chat_id = self.session().and_then(|session| session.open_chat);
        let Some(chat_id) = chat_id else {
            return;
        };
        // Phase A1: slow-mode gate applies to sticker sends too.
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        let reply = self
            .pending_reply
            .as_ref()
            .filter(|reply| reply.chat_id == chat_id)
            .map(|reply| reply.message_id);
        let sent = if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.send_sticker(
                chat_id,
                quill::telegram::requests::StickerSend {
                    file_id,
                    emoji: &emoji,
                    width,
                    height,
                    thumb,
                    reply_to: reply,
                    // Parity slice 4: the driver addresses the open topic
                    // from the session; the UI passes no topic.
                    topic_id: None,
                },
            ) {
                Ok(_) => "sticker sent".into(),
                Err(_) => "could not send sticker".into(),
            };
            self.status_note == "sticker sent"
        } else if self.demo_session.is_some() {
            self.apply_demo_sticker(chat_id, &emoji, file_id, reply);
            self.status_note = "sticker sent".into();
            true
        } else {
            false
        };
        if sent {
            self.consume_sent_reply(chat_id, cx);
        }
        cx.notify();
    }

    /// Sticker/GIF/voice sends consume the composer reply. Drop it from the UI
    /// and from the stored draft, and keep any unsent text.
    fn consume_sent_reply(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.pending_reply = None;
        if self.pending_edit.is_some() {
            self.saved_edit_reply = None;
            return;
        }
        let text = self.composer.read(cx).value().to_string();
        self.save_chat_draft(chat_id, &text, None, false, cx);
    }

    fn toggle_emoji_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: String,
        cx: &mut Context<Self>,
    ) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .toggle_message_reaction(chat_id, message_id, &emoji);
            self.status_note = match result {
                Ok(_) => "updating reaction…".into(),
                Err(_) => "could not update reaction".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_reaction_toggle(chat_id, message_id, &emoji);
            self.status_note = "reaction updated".into();
            cx.notify();
        }
    }

    fn apply_demo_reaction_toggle(&mut self, chat_id: ChatId, message_id: MessageId, emoji: &str) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let current = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| message.interaction_info.clone());
        let next = toggle_chosen_emoji_reaction(current.as_ref(), emoji);
        let json = interaction_info_update_json(chat_id, message_id, &next);
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    /// Phase 4.2: poll option tap → `setPollAnswer` through the live driver
    /// (same guard style as the other send methods). In screenshot demos the
    /// tap flips the chosen marks locally (no live Telegram); the fixture
    /// already carries voted counts.
    fn vote_on_poll(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_index: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let result = live
                .driver
                .send_poll_answer(chat_id, message_id, option_index);
            self.status_note = match result {
                Ok(_) => "voting…".into(),
                Err(_) => "could not vote".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_poll_vote(chat_id, message_id, option_index);
            self.status_note = "vote updated (demo)".into();
            cx.notify();
        }
    }

    /// Demo-only vote: resolve the tap with the same `poll_answer_for_tap`
    /// semantics as the live driver, then flip the chosen marks in place
    /// (counts stay as the fixture set them).
    fn apply_demo_poll_vote(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_index: usize,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let answer = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| match &message.content {
                MessageContent::Poll(poll) => Some(poll.poll.clone()),
                _ => None,
            })
            .and_then(|poll| quill::poll::poll_answer_for_tap(&poll, option_index));
        let Some(answer) = answer else {
            return;
        };
        if let Some(history) = session.histories.get_mut(&chat_id.0)
            && let Some(message) = history.messages.get_mut(&message_id.0)
            && let MessageContent::Poll(poll_content) = &mut message.content
        {
            let chosen: HashSet<i32> = answer.into_iter().collect();
            for (index, option) in poll_content.poll.options.iter_mut().enumerate() {
                option.is_chosen = chosen.contains(&(index as i32));
            }
        }
    }

    /// Phase 4.2: open the poll creation dialog above the composer.
    fn open_poll_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.poll_dialog = Some(PollDialog::new(window, cx));
        if let Some(dialog) = &self.poll_dialog {
            dialog
                .question_input
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    fn close_poll_dialog(&mut self, cx: &mut Context<Self>) {
        self.poll_dialog = None;
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Phase 6: Contacts tab, info panels, add-contact dialog.
    // ------------------------------------------------------------------

    /// Sidebar "Chats | Contacts" tabs (Ready mode). Other modes keep the
    /// plain "Chats" title.
    fn list_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.pane_mode() != PaneMode::Ready {
            return div().font_semibold().child("Chats").into_any_element();
        }
        let chats_active = !self.contacts_tab_open;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id("tab-chats")
                    .cursor_pointer()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_sm()
                    .font_medium()
                    .bg(if chats_active {
                        cx.theme().accent.opacity(0.15)
                    } else {
                        cx.theme().sidebar
                    })
                    .child("Chats")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_chats_tab(cx);
                    })),
            )
            .child(
                div()
                    .id("tab-contacts")
                    .cursor_pointer()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_sm()
                    .font_medium()
                    .bg(if chats_active {
                        cx.theme().sidebar
                    } else {
                        cx.theme().accent.opacity(0.15)
                    })
                    .child("Contacts")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_contacts_tab(cx);
                    })),
            )
            .into_any_element()
    }

    /// Phase 9.1: tdesktop-style active-stories tray above the chat list.
    /// Each entry shows the poster's avatar with an unread (accent) or read
    /// (muted) ring; tapping opens the story viewer on that chat's latest
    /// story (`getStory` prefetches any missing story details first).
    fn story_tray(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let entries: Vec<quill::telegram::envelope::ChatActiveStoriesView> = self
            .session()
            .map(|s| s.ordered_story_tray().into_iter().cloned().collect())
            .unwrap_or_default();
        if entries.is_empty() {
            return None;
        }
        let mut row = div()
            .id("story-tray")
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start()
            .gap_2()
            .px_3()
            .py_2();
        for entry in entries {
            let unread = entry.has_unread();
            let chat_id = entry.chat_id;
            let title = self
                .session()
                .and_then(|s| s.chats.get(&chat_id))
                .map(|chat| chat.title.clone())
                .unwrap_or_else(|| format!("Chat {chat_id}"));
            let latest_story = entry
                .stories
                .iter()
                .map(|info| info.story_id)
                .max()
                .unwrap_or(0);
            row = row.child(
                div()
                    .id(("story-tray-item", chat_id as u64))
                    .cursor_pointer()
                    .flex()
                    .flex_col()
                    .items_center()
                    .w(px(60.))
                    .gap_1()
                    .child(
                        div()
                            .rounded_full()
                            .p(px(2.))
                            .border_2()
                            .border_color(if unread {
                                cx.theme().accent
                            } else {
                                cx.theme().border
                            })
                            .child(initials_avatar(&title, 40.0)),
                    )
                    .child(div().text_xs().max_w(px(60.)).child(title))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_story_viewer(ChatId(chat_id), latest_story, cx);
                    })),
            );
        }
        Some(row.into_any_element())
    }

    fn open_chats_tab(&mut self, cx: &mut Context<Self>) {
        self.contacts_tab_open = false;
        cx.notify();
    }

    fn open_contacts_tab(&mut self, cx: &mut Context<Self>) {
        self.contacts_tab_open = true;
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_contacts()
        {
            self.status_note = format!("contacts request failed: {err:?}");
        }
        cx.notify();
    }

    /// Phase 7.1: folder tabs (`Main` + `updateChatFolders` folders) above
    /// the search field. Selecting a folder filters the chat list to
    /// `chatListFolder` chats; selecting it live also fires a single-shot
    /// `loadChats(chatListFolder)` so TDLib delivers the folder's chats.
    /// Only rendered when the account actually has folders.
    fn folder_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let folders: Vec<(i32, String)> = self
            .session()
            .map(|s| {
                s.chat_folders
                    .iter()
                    .map(|f| (f.id, f.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let selected = self.folder_tab;
        let mut row = div()
            .id("folder-tabs")
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_1();
        // Parity slice: the manage entry is always present so folders can
        // be created even when the account has none yet.
        let tabs: Vec<(Option<i32>, String)> = std::iter::once((None, "Main".to_string()))
            .chain(folders.into_iter().map(|(id, name)| (Some(id), name)))
            .collect();
        for (folder, name) in tabs {
            let active = selected == folder;
            let id = match folder {
                Some(folder_id) => format!("tab-folder-{folder_id}"),
                None => "tab-folder-main".to_string(),
            };
            row = row.child(
                div()
                    .id(id)
                    .cursor_pointer()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_xs()
                    .font_medium()
                    .bg(if active {
                        cx.theme().accent.opacity(0.15)
                    } else {
                        cx.theme().sidebar
                    })
                    .child(name)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_folder_tab(folder, cx);
                    })),
            );
        }
        row = row.child(
            Button::new("folder-manage")
                .label("⋯")
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.open_folder_manage(cx);
                })),
        );
        row.into_any_element()
    }

    fn open_folder_tab(&mut self, folder: Option<i32>, cx: &mut Context<Self>) {
        self.folder_tab = folder;
        self.contacts_tab_open = false;
        if let (Some(live), Some(folder_id)) = (self.live.as_mut(), folder)
            && let Err(err) = live.driver.load_folder_chats(folder_id)
        {
            self.status_note = format!("folder load failed: {err:?}");
        }
        cx.notify();
    }

    fn retry_contacts(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_contacts()
        {
            self.status_note = format!("contacts request failed: {err:?}");
        }
        cx.notify();
    }

    /// Contacts list for the Contacts tab: loading / error / empty /
    /// rows. A tap opens the user info panel.
    fn contacts_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<ContactRow> = self.session().map(|s| s.contact_rows()).unwrap_or_default();
        let failed = self.session().is_some_and(|s| s.contacts_error);
        let loading = self.session().is_some_and(|s| s.contacts.is_none()) && !failed;
        let mut list = div().id("contacts-list").flex().flex_col().gap_1();
        if failed {
            list = list
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Couldn’t load contacts."),
                )
                .child(
                    Button::new("contacts-retry")
                        .label("Retry")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.retry_contacts(cx);
                        })),
                );
        } else if loading {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Loading contacts…"),
            );
        } else if rows.is_empty() {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No contacts yet."),
            );
        } else {
            for row in rows {
                list = list.child(self.contact_row(&row, cx));
            }
        }
        list
    }

    fn contact_row(&self, row: &ContactRow, cx: &mut Context<Self>) -> impl IntoElement {
        let user_id = row.user_id;
        let name = row.name.clone();
        let status = row.status_text.clone();
        let selected =
            self.session().and_then(|s| s.open_info_panel) == Some(InfoPanelTarget::User(user_id));
        div()
            .id(("contact-row", user_id as u64))
            .px_2()
            .py_2()
            .rounded_md()
            .cursor_pointer()
            .bg(if selected {
                cx.theme().accent.opacity(0.15)
            } else {
                cx.theme().sidebar
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_user_panel(user_id, window, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(initials_avatar(&name, 32.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(div().font_medium().text_sm().child(name))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(status),
                            ),
                    ),
            )
    }

    fn open_user_panel(&mut self, user_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.open_info_panel_target(InfoPanelTarget::User(user_id), window, cx);
    }

    fn open_supergroup_panel(
        &mut self,
        supergroup_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_info_panel_target(InfoPanelTarget::Supergroup(supergroup_id), window, cx);
    }

    /// Open an info panel: set the target, then fetch its data on the live
    /// path (full info + profile photo for users, full info for
    /// supergroups). The demo path only sets the target — the fixture
    /// pre-seeds the data.
    fn open_info_panel_target(
        &mut self,
        target: InfoPanelTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            live.driver.set_info_panel(Some(target));
            let fetch = match target {
                InfoPanelTarget::User(user_id) => {
                    live.driver.fetch_user_full_info(user_id).map(|_| ())
                }
                InfoPanelTarget::Supergroup(supergroup_id) => live
                    .driver
                    .fetch_supergroup_full_info(supergroup_id)
                    .map(|_| ()),
            };
            if let Err(err) = fetch {
                self.status_note = format!("info request failed: {err:?}");
            } else if let InfoPanelTarget::User(user_id) = target
                && let Err(err) = live.driver.download_user_photo(user_id)
            {
                self.status_note = format!("photo download failed: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_info_panel = Some(target);
        }
        let _ = window;
        cx.notify();
    }

    fn close_info_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.set_info_panel(None);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_info_panel = None;
        }
        cx.notify();
    }

    /// Right-side info panel for the open `InfoPanelTarget` (user profile
    /// or group info). Rendered next to the conversation in the shell.
    fn info_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let target = self.session()?.open_info_panel?;
        let (title, content) = match target {
            InfoPanelTarget::User(user_id) => ("Contact info", self.user_info_panel(user_id, cx)),
            InfoPanelTarget::Supergroup(supergroup_id) => {
                ("Group info", self.supergroup_info_panel(supergroup_id, cx))
            }
        };
        Some(
            div()
                .id("info-panel")
                .w(px(300.))
                .h_full()
                .flex_shrink_0()
                .border_l_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().sidebar)
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .child(div().font_semibold().child(title))
                        .child(
                            div()
                                .id("info-panel-close")
                                .cursor_pointer()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .text_color(cx.theme().muted_foreground)
                                .child("✕")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_info_panel(cx);
                                })),
                        ),
                )
                // Phase B2: the content scrolls — the encryption-key
                // section can push a contact panel past the window
                // height.
                .child(
                    div()
                        .id("info-panel-body")
                        .flex_1()
                        .overflow_y_scroll()
                        .child(content),
                )
                .into_any_element(),
        )
    }

    /// Phase B2: the "Encryption key" section of a secret chat partner's
    /// info panel — the 12×12 fingerprint grid from `secretChat.key_hash`
    /// (schema 1.8.67 lines 2812–2813: 36 little-endian bytes → 144
    /// two-bit pixels in FFFFFF / D5E6F3 / 2D5775 / 2F99C9) with
    /// Telegram-style verification copy. A Ready record whose hash isn't
    /// 36 bytes yet (still resolving) shows a loading note instead of
    /// the grid — graceful missing-key handling.
    ///
    /// Security: `record.key_hash` bytes are passed straight into
    /// `key_fingerprint::key_hash_pixels`; only pixel indices / colors
    /// enter the element tree — raw key bytes never leave `Session`.
    fn encryption_key_section(
        &self,
        record: &ParsedSecretChat,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const CELL_PX: f32 = 18.0;
        let pixels = key_fingerprint::key_hash_pixels(&record.key_hash);
        let mut body = div()
            .flex()
            .flex_col()
            .w_full()
            .gap_2()
            .pt_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child("Encryption key"),
            );
        match pixels {
            Some(pixels) => {
                let mut grid = div().flex().flex_col();
                for row in pixels.chunks(key_fingerprint::KEY_GRID_SIZE) {
                    let mut line = div().flex().flex_row();
                    for pixel in row {
                        line = line.child(
                            div()
                                .w(px(CELL_PX))
                                .h(px(CELL_PX))
                                .bg(rgb(key_fingerprint::key_pixel_color(*pixel))),
                        );
                    }
                    grid = grid.child(line);
                }
                body = body
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(grid.border_1().border_color(cx.theme().border)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("If this image matches the one on your contact's device, your conversation is secure."),
                    );
            }
            None => {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Encryption key · still loading…"),
                );
            }
        }
        body.into_any_element()
    }

    /// User profile panel: photo (downloaded `userFullInfo.photo` size, or
    /// an initials avatar), name, status, username/phone rows, bio, and an
    /// Add contact affordance for known non-contacts.
    fn user_info_panel(&self, user_id: i64, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let user = session.and_then(|s| s.user(user_id)).cloned();
        let info = session.and_then(|s| s.user_full_info(user_id)).cloned();
        let name = user
            .as_ref()
            .map(|u| u.display_name())
            .unwrap_or_else(|| format!("User {user_id}"));
        let status = user
            .as_ref()
            .map(|u| u.status.display())
            .unwrap_or_default();
        let username = user
            .as_ref()
            .map(|u| u.username.clone())
            .unwrap_or_default();
        let phone = user
            .as_ref()
            .map(|u| u.phone_number.clone())
            .unwrap_or_default();
        let bio = info.as_ref().map(|i| i.bio.clone()).unwrap_or_default();
        let show_add = user.as_ref().is_some_and(|u| !u.is_contact && !u.is_bot);
        let roots = self.media_display_roots();
        let photo_path: Option<PathBuf> = session
            .and_then(|s| {
                info.as_ref()
                    .and_then(|i| i.photo_file_id)
                    .and_then(|id| s.files.get(&id))
                    .and_then(|file| file.usable_path())
            })
            .and_then(|path| sandboxed_display_path(path, &roots));
        let avatar: AnyElement = match photo_path {
            Some(path) => img(path)
                .id(("info-panel-photo", user_id as u64))
                .w(px(96.))
                .h(px(96.))
                .rounded_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => initials_avatar(&name, 96.).into_any_element(),
        };
        let mut detail_rows: Vec<(&str, String)> = Vec::new();
        if !username.is_empty() {
            detail_rows.push(("Username", format!("@{username}")));
        }
        if !phone.is_empty() {
            detail_rows.push(("Phone", phone));
        }
        let mut body = div()
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .p_4()
            .child(avatar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(div().text_lg().font_semibold().child(name.clone()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(status),
                    ),
            );
        for (label, value) in detail_rows {
            body = body.child(
                div()
                    .flex()
                    .w_full()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(label),
                    )
                    .child(div().text_sm().child(value)),
            );
        }
        if !bio.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Bio"),
                    )
                    .child(div().text_sm().child(bio)),
            );
        }
        if show_add {
            body = body.child(
                Button::new("info-panel-add-contact")
                    .label("Add contact")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_add_contact_dialog(user_id, window, cx);
                    })),
            );
        }
        // Phase B1: "Start secret chat" from a user profile — E2E chat
        // with a non-bot user (`createNewSecretChat`, schema 1.8.67 line
        // 13340). Not offered for bots or for yourself.
        let show_start_secret = user.as_ref().is_some_and(|u| !u.is_bot)
            && session
                .as_ref()
                .and_then(|s| s.my_user_id)
                .is_none_or(|me| me != user_id);
        if show_start_secret {
            body = body.child(
                Button::new("info-panel-start-secret")
                    .label("Start secret chat")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.start_secret_chat_for_user(user_id, cx);
                    })),
            );
        }
        // Phase C1: "Call" from a user profile — `createCall`
        // (audio-only; video needs transport, C3). Same gating as
        // secret chats: non-bot users, not yourself.
        let show_call = show_start_secret;
        if show_call {
            body = body.child(
                Button::new("info-panel-call")
                    .label("Call")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.start_call_for_user(user_id, cx);
                    })),
            );
        }
        // Phase B2: encryption-key section — only when the open chat is a
        // Ready secret chat with this user (the key is meaningful once
        // the session is established). Missing/short hashes render the
        // still-loading note inside the section.
        if let Some(record) = session
            .as_ref()
            .and_then(|s| s.open_ready_secret_chat_for_user(user_id))
        {
            body = body.child(self.encryption_key_section(record, cx));
        }
        body.into_any_element()
    }

    /// Supergroup/channel panel: title, member count, description.
    fn supergroup_info_panel(&self, supergroup_id: i64, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let (title, chat_id, is_channel) = session
            .and_then(|s| {
                s.chats.values().find_map(|chat| match chat.kind {
                    ChatKind::Supergroup {
                        supergroup_id: id,
                        is_channel,
                    } if id == supergroup_id => Some((chat.title.clone(), chat.id, is_channel)),
                    _ => None,
                })
            })
            .unwrap_or_else(|| {
                (
                    format!("Group {supergroup_id}"),
                    ChatId(supergroup_id),
                    false,
                )
            });
        let info = session
            .and_then(|s| s.supergroup_full_infos.get(&supergroup_id))
            .cloned();
        let description = info
            .as_ref()
            .map(|i| i.description.clone())
            .unwrap_or_default();
        let members = info.as_ref().map(|i| i.member_count).unwrap_or(0);
        let username = session
            .and_then(|s| s.supergroup_username(supergroup_id))
            .filter(|name| !name.is_empty())
            .map(|name| name.to_string());
        // Parity slice: the panel avatar reuses the chat-list avatar (photo
        // or colored initials).
        let roots = self.media_display_roots();
        let photo = session
            .and_then(|s| s.chat_photo_path(chat_id))
            .and_then(|path| sandboxed_display_path(path, &roots));
        let noun = if is_channel { "subscribers" } else { "members" };
        let mut body = div()
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .p_4()
            .child(chat_avatar(&title, chat_id.0, photo.as_deref(), 96.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(div().text_lg().font_semibold().child(title.clone()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(if members > 0 {
                                format!("{} {noun}", compact_count(members))
                            } else {
                                format!("{noun} unknown")
                            }),
                    )
                    .when_some(username, |this, name| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("@{name}")),
                        )
                    }),
            );
        if !description.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Description"),
                    )
                    .child(div().text_sm().child(description)),
            );
        }
        // Phase A1: slow-mode admin control (`setChatSlowModeDelay`,
        // schema 1.8.67 line 13551 — allowed values 0/5/10/30/60/300/900/
        // 3600, supergroups only, requires `can_restrict_members`).
        // Creators hold all rights implicitly; administrators need the
        // explicit `can_restrict_members` right (lines 2500/1092). The new
        // delay arrives via `updateSupergroupFullInfo`.
        if !is_channel && self.can_change_slow_mode(supergroup_id) {
            let current_delay = info.as_ref().map(|i| i.slow_mode_delay).unwrap_or(0);
            let mut value_row = div().id("slow-mode-values").flex().flex_wrap().gap_1();
            for (label, value) in [
                ("Off", 0),
                ("5s", 5),
                ("10s", 10),
                ("30s", 30),
                ("1m", 60),
                ("5m", 300),
                ("15m", 900),
                ("1h", 3600),
            ] {
                let label = if value == current_delay {
                    format!("✓ {label}")
                } else {
                    label.to_string()
                };
                value_row = value_row.child(
                    Button::new(format!("slow-mode-set-{value}"))
                        .label(label)
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_slow_mode_delay(chat_id, value, cx);
                        })),
                );
            }
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Slow mode"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Delay between messages for members"),
                    )
                    .child(value_row),
            );
        }
        body.into_any_element()
    }

    fn open_add_contact_dialog(
        &mut self,
        user_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (phone, first, last) = self
            .session()
            .and_then(|s| s.user(user_id))
            .map(|u| {
                (
                    u.phone_number.clone(),
                    u.first_name.clone(),
                    u.last_name.clone(),
                )
            })
            .unwrap_or_default();
        self.add_contact_dialog = Some(AddContactDialog::new(
            window, cx, user_id, &phone, &first, &last,
        ));
        if let Some(dialog) = &self.add_contact_dialog {
            dialog
                .phone_input
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    fn close_add_contact_dialog(&mut self, cx: &mut Context<Self>) {
        self.add_contact_dialog = None;
        cx.notify();
    }

    /// Submit the add-contact dialog. The phone field is required —
    /// `addContact` needs an `importedContact`, and Quill does not offer
    /// adding by bare user id.
    fn submit_add_contact_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self
            .add_contact_dialog
            .as_ref()
            .and_then(|dialog| dialog.draft(cx));
        let Some((user_id, phone, first, last)) = draft else {
            self.status_note = "Enter a phone number to add the contact.".into();
            cx.notify();
            return;
        };
        if let Some(live) = self.live.as_mut() {
            match live.driver.add_contact(user_id, &phone, &first, &last) {
                Ok(_) => {
                    self.add_contact_dialog = None;
                    self.status_note = "Contact add requested.".into();
                }
                Err(err) => {
                    self.status_note = format!("add contact failed: {err:?}");
                }
            }
        } else {
            // Demo: no driver — just close.
            self.add_contact_dialog = None;
            self.status_note = "Contact add requested.".into();
        }
        let _ = window;
        cx.notify();
    }

    /// Add-contact dialog overlay (phone + first/last name), centered over
    /// the shell like the media viewer.
    fn add_contact_dialog_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.add_contact_dialog.as_ref()?;
        let name = self
            .session()
            .and_then(|s| s.user(dialog.user_id))
            .map(|u| u.display_name())
            .unwrap_or_else(|| format!("User {}", dialog.user_id));
        Some(
            div()
                .id("add-contact-overlay")
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .id("add-contact-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .bg(rgba(0x000000e6))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_add_contact_dialog(cx);
                        })),
                )
                .child(
                    div()
                        .id("add-contact-panel")
                        .flex()
                        .flex_col()
                        .gap_2()
                        .p_4()
                        .w(px(360.))
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().sidebar)
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .child(format!("Add {name} to contacts")),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("Phone number"),
                                )
                                .child(Textarea::new(&dialog.phone_input).h(px(40.))),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("First name"),
                                )
                                .child(Textarea::new(&dialog.first_name_input).h(px(40.))),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("Last name"),
                                )
                                .child(Textarea::new(&dialog.last_name_input).h(px(40.))),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    Button::new("add-contact-submit")
                                        .label("Add contact")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.submit_add_contact_dialog(window, cx);
                                        })),
                                )
                                .child(
                                    Button::new("add-contact-cancel")
                                        .label("Cancel")
                                        .ghost()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.close_add_contact_dialog(cx);
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    /// Phase C1: call overlay — incoming / outgoing / active / ended
    /// call UI above everything else. **Signaling only**: when a call
    /// would need media, the card says so honestly (audio transport is
    /// the C2 libtgvoip spike, not faked here).
    fn call_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if session.active_call.is_none()
            && session.call_summary.is_none()
            && session.call_error.is_none()
        {
            return None;
        }
        Some(self.call_card(cx).into_any_element())
    }

    /// Phase C1: format a call clock as `m:ss`.
    fn call_clock(secs: u64) -> String {
        format!("{}:{:02}", secs / 60, secs % 60)
    }

    fn call_peer_name(&self, user_id: i64) -> String {
        self.session()
            .and_then(|s| s.user(user_id))
            .map(|u| u.display_name())
            .unwrap_or_else(|| format!("User {user_id}"))
    }

    fn call_card(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let active = session.and_then(|s| s.active_call.clone());
        let summary = session.and_then(|s| s.call_summary.clone());
        let error = session.and_then(|s| s.call_error.clone());

        let mut card = div()
            .id("call-card")
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .p_6()
            .w(px(360.))
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar);

        if let Some(call) = active {
            card = self.call_active_card(card, &call, error.as_deref(), cx);
        } else if let Some(summary) = summary {
            card = self.call_summary_card(card, &summary, cx);
        } else if let Some(error) = error {
            card =
                card.child(div().text_sm().font_semibold().child("Call failed"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(error),
                    )
                    .child(Button::new("call-error-dismiss").label("Dismiss").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.dismiss_call_error(cx);
                        }),
                    ));
        }

        div()
            .id("call-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("call-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .bg(rgba(0x000000e6)),
            )
            .child(card)
    }

    /// Phase C1: the live-call card (ringing / connecting / connected).
    /// The backdrop is not clickable — only the call buttons act.
    fn call_active_card(
        &self,
        card: Stateful<Div>,
        call: &ActiveCall,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let name = self.call_peer_name(call.user_id);
        let kind_line = if call.is_video {
            "Video call"
        } else {
            "Voice call"
        };
        let (status, clock): (String, Option<String>) = match &call.state {
            CallState::Pending { .. } => {
                let elapsed = call.started_at.elapsed().as_secs();
                if call.is_outgoing {
                    ("Calling…".to_string(), Some(Self::call_clock(elapsed)))
                } else {
                    ("Incoming call".to_string(), Some(Self::call_clock(elapsed)))
                }
            }
            CallState::ExchangingKeys => ("Connecting…".to_string(), None),
            CallState::Ready => (
                "Connected".to_string(),
                Some(Self::call_clock(call.connected_secs().max(0) as u64)),
            ),
            CallState::HangingUp => ("Hanging up…".to_string(), None),
            // A future state the pinned schema doesn't know: label it
            // honestly instead of pretending it means something else.
            CallState::Unknown(type_name) => (format!("Call state: {type_name}"), None),
            // Terminal states end the call in the reducer, so this arm
            // is unreachable — but never crash the overlay on it.
            CallState::Discarded { .. } | CallState::Error { .. } => ("Ending…".to_string(), None),
        };
        let mut card = card
            .child(initials_avatar(&name, 72.))
            .child(div().text_lg().font_semibold().child(name))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(kind_line),
            )
            .child(div().text_sm().child(status));
        if let Some(clock) = clock {
            card = card.child(div().text_2xl().font_semibold().child(clock));
        }
        // Honest no-transport note: the call can be "Connected" at the
        // signaling level while carrying no audio. Never fake a live
        // call. Every pre-connected card carries it — incoming ringing,
        // outgoing "Calling…", connecting — and the end screen repeats
        // the note below; accepting/placing starts no audio in this
        // build.
        let no_transport_note = matches!(
            call.state,
            CallState::Ready | CallState::ExchangingKeys | CallState::Pending { .. }
        );
        if no_transport_note {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Audio isn't connected — Quill's voice transport \
                         ships in Phase C2. This call carries no sound.",
                    ),
            );
        }
        if let Some(error) = error {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(rgb(0xe17076))
                    .child(error.to_string()),
            );
        }
        let mut buttons = div().flex().gap_2();
        match &call.state {
            CallState::Pending { .. } if !call.is_outgoing => {
                buttons = buttons
                    .child(
                        Button::new("call-accept")
                            .label("Accept")
                            .success()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.accept_incoming_call(cx);
                            })),
                    )
                    .child(
                        Button::new("call-decline")
                            .label("Decline")
                            .danger()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.hang_up_call(cx);
                            })),
                    );
            }
            CallState::Pending { .. } | CallState::ExchangingKeys => {
                buttons =
                    buttons.child(Button::new("call-cancel").label("Cancel").ghost().on_click(
                        cx.listener(|this, _, _, cx| {
                            this.hang_up_call(cx);
                        }),
                    ));
            }
            CallState::Ready | CallState::Unknown(_) => {
                buttons = buttons.child(
                    Button::new("call-hangup")
                        .label("Hang up")
                        .danger()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.hang_up_call(cx);
                        })),
                );
            }
            CallState::HangingUp | CallState::Discarded { .. } | CallState::Error { .. } => {}
        }
        card.child(buttons)
    }

    /// Phase C1: the call-end screen — reason line, duration, and the
    /// optional 1–5 rating card (`callStateDiscarded.need_rating`,
    /// schema 1.8.67, line 7078). `need_debug_information` /
    /// `need_log` are out of this slice, stated honestly.
    fn call_summary_card(
        &self,
        card: Stateful<Div>,
        summary: &CallSummary,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let name = self.call_peer_name(summary.user_id);
        let mut card = card
            .child(initials_avatar(&name, 72.))
            .child(div().text_lg().font_semibold().child(name))
            .child(div().text_sm().child(summary.end_line.clone()));
        if summary.duration_secs > 0 {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "Connected for {}",
                        Self::call_clock(summary.duration_secs.max(0) as u64)
                    )),
            );
        }
        // Always present: the end screen never implies the call carried
        // audio, even when it never connected.
        card = card.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(
                    "No audio was carried — voice transport isn't \
                     implemented yet (Phase C2).",
                ),
        );
        if summary.need_debug_information || summary.need_log {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Call diagnostics upload isn't implemented yet."),
            );
        }
        if summary.need_rating && !summary.rating_sent {
            card = card.child(div().text_sm().child("How was the call quality?"));
            let mut stars = div().flex().gap_2();
            for star in 1..=5 {
                stars = stars.child(
                    Button::new(format!("call-rate-{star}"))
                        .label(format!("{star} ★"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.rate_last_call(star, cx);
                        })),
                );
            }
            card = card.child(stars).child(
                Button::new("call-rate-skip")
                    .label("Skip")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dismiss_call_summary(cx);
                    })),
            );
        } else {
            if summary.rating_sent {
                card = card.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Thanks for your feedback"),
                );
            }
            card = card.child(Button::new("call-summary-close").label("Close").on_click(
                cx.listener(|this, _, _, cx| {
                    this.dismiss_call_summary(cx);
                }),
            ));
        }
        card
    }

    /// Parity slice: folder manage / editor / delete-confirm overlays.
    /// The editor replaces the manage list while open (modal flow).
    fn folder_overlays(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.folder_editor.is_some() {
            return Some(self.folder_editor_overlay(cx));
        }
        if self.folder_delete_confirm.is_some() {
            return Some(self.folder_delete_overlay(cx));
        }
        if self.folder_manage_open {
            return Some(self.folder_manage_overlay(cx));
        }
        None
    }

    fn folder_backdrop(&self, cx: &mut Context<Self>, id: &str) -> AnyElement {
        div()
            .id(format!("{id}-backdrop"))
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .bg(rgba(0x000000e6))
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_folder_manage(cx);
            }))
            .into_any_element()
    }

    fn folder_manage_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let tags_enabled = session.as_ref().is_some_and(|s| s.are_folder_tags_enabled);
        let folders: Vec<(i32, String, usize)> = session
            .as_ref()
            .map(|s| {
                s.chat_folders
                    .iter()
                    .map(|f| {
                        let count = s
                            .chats
                            .values()
                            .filter(|c| c.folder_positions.contains_key(&f.id))
                            .count();
                        (f.id, f.name.clone(), count)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut list = div().id("folder-manage-list").flex().flex_col().gap_1();
        if folders.is_empty() {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No folders yet. Create one to organize your chats."),
            );
        }
        for (index, (folder_id, name, count)) in folders.iter().enumerate() {
            let folder_id = *folder_id;
            let is_first = index == 0;
            let is_last = index + 1 == folders.len();
            let name_label = format!("{name} ({count})");
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .child(div().text_sm().font_medium().child(name_label))
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                Button::new(format!("folder-up-{folder_id}"))
                                    .label("↑")
                                    .ghost()
                                    .when(is_first, |this| this.disabled(true))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.move_folder(folder_id, true, cx);
                                    })),
                            )
                            .child(
                                Button::new(format!("folder-down-{folder_id}"))
                                    .label("↓")
                                    .ghost()
                                    .when(is_last, |this| this.disabled(true))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.move_folder(folder_id, false, cx);
                                    })),
                            )
                            .child(
                                Button::new(format!("folder-edit-{folder_id}"))
                                    .label("Edit")
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_folder_edit(folder_id, window, cx);
                                    })),
                            )
                            .child(
                                Button::new(format!("folder-delete-{folder_id}"))
                                    .label("Delete")
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_folder_delete(folder_id, cx);
                                    })),
                            ),
                    ),
            );
        }
        div()
            .id("folder-manage-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(self.folder_backdrop(cx, "folder-manage"))
            .child(
                div()
                    .id("folder-manage-panel")
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .w(px(440.))
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().sidebar)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_sm().font_semibold().child("Folders"))
                            .child(
                                Button::new("folder-manage-close")
                                    .label("Close")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_folder_manage(cx);
                                    })),
                            ),
                    )
                    .child(list)
                    .child(
                        div().flex().items_center().gap_2().child(
                            Button::new("folder-tags-toggle")
                                .label(if tags_enabled {
                                    "☑ Show folder tags"
                                } else {
                                    "☐ Show folder tags"
                                })
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_folder_tags_ui(cx);
                                })),
                        ),
                    )
                    .child(
                        Button::new("folder-create")
                            .label("New folder")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_folder_create(window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    fn folder_editor_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("folder-editor-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(self.folder_backdrop(cx, "folder-editor"))
            .child(self.folder_editor_panel(cx))
            .into_any_element()
    }

    /// Parity slice: create/edit folder form — name, include-type filters,
    /// per-chat include/exclude multi-select, exclude flags.
    fn folder_editor_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let dialog = self.folder_editor.as_ref();
        let title = match dialog.and_then(|d| d.folder_id) {
            Some(_) => "Edit folder",
            None => "New folder",
        };
        let mut panel = div()
            .id("folder-editor-panel")
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .w(px(480.))
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(div().text_sm().font_semibold().child(title));
        let Some(dialog) = dialog else {
            return panel.into_any_element();
        };
        panel = panel.child(Textarea::new(&dialog.name_input));
        if let Some(error) = dialog.error.clone() {
            panel = panel.child(
                div()
                    .id("folder-editor-error")
                    .text_xs()
                    .text_color(rgb(0xd44a3a))
                    .child(error),
            );
        }
        if dialog.fetch_pending {
            return panel
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading folder…"),
                )
                .into_any_element();
        }
        // Include-type filters.
        let include_filters = [
            (
                "Contacts",
                dialog.editor.include_contacts,
                "include-contacts",
            ),
            (
                "Non-contacts",
                dialog.editor.include_non_contacts,
                "include-non-contacts",
            ),
            ("Groups", dialog.editor.include_groups, "include-groups"),
            (
                "Channels",
                dialog.editor.include_channels,
                "include-channels",
            ),
            ("Bots", dialog.editor.include_bots, "include-bots"),
        ];
        let mut filters_row = div().flex().flex_row().flex_wrap().gap_1().child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child("Include types:"),
        );
        for (label, checked, key) in include_filters {
            let mark = if checked { "☑" } else { "☐" };
            filters_row = filters_row.child(
                Button::new(format!("folder-filter-{key}"))
                    .label(format!("{mark} {label}"))
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(dialog) = this.folder_editor.as_mut() {
                            match key {
                                "include-contacts" => {
                                    dialog.editor.include_contacts =
                                        !dialog.editor.include_contacts;
                                }
                                "include-non-contacts" => {
                                    dialog.editor.include_non_contacts =
                                        !dialog.editor.include_non_contacts;
                                }
                                "include-groups" => {
                                    dialog.editor.include_groups = !dialog.editor.include_groups;
                                }
                                "include-channels" => {
                                    dialog.editor.include_channels =
                                        !dialog.editor.include_channels;
                                }
                                _ => {
                                    dialog.editor.include_bots = !dialog.editor.include_bots;
                                }
                            }
                        }
                        cx.notify();
                    })),
            );
        }
        panel = panel.child(filters_row);
        // Per-chat include/exclude multi-select.
        let mut chats: Vec<(i64, String)> = self
            .session()
            .map(|s| {
                s.chats
                    .values()
                    .map(|c| (c.id.0, c.title.clone()))
                    .collect()
            })
            .unwrap_or_default();
        chats.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
        let mut chat_list = div()
            .id("folder-editor-chats")
            .flex()
            .flex_col()
            .gap_1()
            .overflow_y_scroll()
            .max_h(px(220.))
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child("Chats (include / exclude):"),
            );
        for (chat_id, chat_title) in chats {
            let included = dialog.editor.included.contains(&chat_id);
            let excluded = dialog.editor.excluded.contains(&chat_id);
            chat_list = chat_list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .child(div().text_sm().min_w_0().child(chat_title))
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                Button::new(("folder-include-chat", chat_id as u64))
                                    .label(if included { "☑ In" } else { "☐ In" })
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(dialog) = this.folder_editor.as_mut() {
                                            dialog.editor.toggle_included(chat_id);
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new(("folder-exclude-chat", chat_id as u64))
                                    .label(if excluded { "☑ Out" } else { "☐ Out" })
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(dialog) = this.folder_editor.as_mut() {
                                            dialog.editor.toggle_excluded(chat_id);
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        }
        panel = panel.child(chat_list);
        // Exclude flags.
        let exclude_flags = [
            ("Muted", dialog.editor.exclude_muted, "exclude-muted"),
            ("Read", dialog.editor.exclude_read, "exclude-read"),
            (
                "Archived",
                dialog.editor.exclude_archived,
                "exclude-archived",
            ),
        ];
        let mut exclude_row = div().flex().flex_row().flex_wrap().gap_1().child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child("Exclude:"),
        );
        for (label, checked, key) in exclude_flags {
            let mark = if checked { "☑" } else { "☐" };
            exclude_row = exclude_row.child(
                Button::new(format!("folder-exclude-flag-{key}"))
                    .label(format!("{mark} {label}"))
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(dialog) = this.folder_editor.as_mut() {
                            match key {
                                "exclude-muted" => {
                                    dialog.editor.exclude_muted = !dialog.editor.exclude_muted;
                                }
                                "exclude-read" => {
                                    dialog.editor.exclude_read = !dialog.editor.exclude_read;
                                }
                                _ => {
                                    dialog.editor.exclude_archived =
                                        !dialog.editor.exclude_archived;
                                }
                            }
                        }
                        cx.notify();
                    })),
            );
        }
        panel = panel.child(exclude_row).child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("folder-editor-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.folder_editor = None;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("folder-editor-save")
                        .label("Save")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.save_folder_editor(cx);
                        })),
                ),
        );
        panel.into_any_element()
    }

    fn folder_delete_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let confirm = self.folder_delete_confirm.as_ref();
        let mut panel = div()
            .id("folder-delete-panel")
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .w(px(400.))
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar);
        if let Some(confirm) = confirm {
            let leave_count = self
                .session()
                .and_then(|s| s.folder_chats_to_leave.get(&confirm.folder_id))
                .map(|ids| ids.len())
                .unwrap_or(0);
            let leave_label = if leave_count > 0 {
                format!(
                    "{} Also leave {leave_count} suggested chat{}",
                    if confirm.leave_with_folder {
                        "☑"
                    } else {
                        "☐"
                    },
                    if leave_count == 1 { "" } else { "s" },
                )
            } else {
                "Also leave suggested chats".to_string()
            };
            panel = panel
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(format!("Delete “{}”?", confirm.name)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Chats stay in your main list unless you leave them."),
                )
                .child(
                    Button::new("folder-delete-leave-toggle")
                        .label(leave_label)
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(confirm) = this.folder_delete_confirm.as_mut() {
                                confirm.leave_with_folder = !confirm.leave_with_folder;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("folder-delete-cancel")
                                .label("Cancel")
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.folder_delete_confirm = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("folder-delete-confirm")
                                .label("Delete")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.confirm_folder_delete(cx);
                                })),
                        ),
                );
        }
        div()
            .id("folder-delete-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(self.folder_backdrop(cx, "folder-delete"))
            .child(panel)
            .into_any_element()
    }

    fn add_poll_option_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.poll_dialog.as_mut() else {
            return;
        };
        if dialog.option_inputs.len() >= POLL_OPTIONS_MAX {
            return;
        }
        let index = dialog.option_inputs.len();
        dialog.option_inputs.push(cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(format!("Option {}", index + 1))
                .auto_grow(1, 2)
                .submit_on_enter(false)
        }));
        cx.notify();
    }

    fn remove_poll_option_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(dialog) = self.poll_dialog.as_mut() else {
            return;
        };
        if dialog.option_inputs.len() <= POLL_OPTIONS_MIN || index >= dialog.option_inputs.len() {
            return;
        }
        dialog.option_inputs.remove(index);
        cx.notify();
    }

    /// Phase 4.2: freeze the dialog, validate, and send `inputMessagePoll`
    /// through the driver (same `sendMessage` path as the composer). The
    /// pending reply (if any) is attached like a normal send.
    fn submit_poll_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = match self.poll_dialog.as_ref() {
            Some(dialog) => dialog.draft(cx),
            None => return,
        };
        if let Some(reason) = draft.validate() {
            self.status_note = reason.to_string();
            cx.notify();
            return;
        }
        let plan = self.session().and_then(|session| {
            let chat_id = session.open_chat?;
            let chat = session.chats.get(&chat_id.0)?;
            chat.can_post().then_some(chat_id)
        });
        let Some(chat_id) = plan else {
            self.status_note = "select a chat to send".into();
            cx.notify();
            return;
        };
        // Phase A1: slow-mode gate applies to polls too.
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        let reply_to = self.pending_reply.as_ref().map(|reply| reply.message_id);
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.send_poll_draft(chat_id, &draft, reply_to);
            match result {
                Ok(_) => {
                    self.poll_dialog = None;
                    self.pending_reply = None;
                    self.status_note = "sending poll…".into();
                }
                Err(_) => {
                    self.status_note = "could not send poll".into();
                }
            }
            cx.notify();
            return;
        }
        // Screenshot demos have no live driver; close the dialog honestly.
        let _ = window;
        self.poll_dialog = None;
        self.status_note = "polls need a live connection (demo)".into();
        cx.notify();
    }

    fn toggle_pin_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .toggle_pin_chat_message(chat_id, message_id);
            self.status_note = match result {
                Ok(_) => "updating pin…".into(),
                Err(_) => "could not update pin".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_pin_toggle(chat_id, message_id);
            self.status_note = "pin updated".into();
            cx.notify();
        }
    }

    fn apply_demo_pin_toggle(&mut self, chat_id: ChatId, message_id: MessageId) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let currently_pinned = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| message.is_pinned);
        let json = format!(
            r#"{{"@type":"updateMessageIsPinned","chat_id":{},"message_id":{},"is_pinned":{}}}"#,
            chat_id.0, message_id.0, !currently_pinned
        );
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    fn jump_to_pinned_message(&mut self, message_id: MessageId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.jump_to_chat_search_message(message_id) {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to pinned message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let _ = session.begin_chat_search_jump(message_id);
            self.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    fn unpin_from_banner(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .unpin_chat_message(chat_id, message_id);
            self.status_note = match result {
                Ok(_) => "unpinning…".into(),
                Err(_) => "could not unpin".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            let json = format!(
                r#"{{"@type":"updateMessageIsPinned","chat_id":{},"message_id":{},"is_pinned":false}}"#,
                chat_id.0, message_id.0
            );
            if let Some(session) = self.demo_session.as_mut() {
                let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
                if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
            self.status_note = "unpinned".into();
            cx.notify();
        }
    }

    fn close_mute_menu(&mut self, cx: &mut Context<Self>) {
        self.mute_menu_open = false;
        self.notif_sound_picker_open = false;
        cx.notify();
    }

    fn open_mute_menu(&mut self, cx: &mut Context<Self>) {
        self.mute_menu_open = true;
        self.status_note = "mute for…".into();
        cx.notify();
    }

    fn apply_chat_mute(&mut self, chat_id: ChatId, mute_for: i32, cx: &mut Context<Self>) {
        self.mute_menu_open = false;
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .set_chat_mute_for(chat_id, mute_for);
            self.status_note = match result {
                Ok(_) if mute_for == 0 => "unmuting…".into(),
                Ok(_) => "muting…".into(),
                Err(_) => "could not change mute".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_notification_settings(
                chat_id,
                |settings| {
                    *settings = settings.clone().with_mute_for(mute_for);
                },
                cx,
            );
            self.status_note = if mute_for == 0 {
                "unmuted".into()
            } else {
                "muted".into()
            };
            cx.notify();
        }
    }

    /// Phase B4: apply a chat TTL choice (`setChatMessageAutoDeleteTime`,
    /// schema 1.8.67 line 13454). Live: the driver validates the value
    /// rule and sends; the new value arrives as
    /// `updateChatMessageAutoDeleteTime` (no optimistic state change).
    /// Demo: apply the same update through the reducer so the screenshot
    /// fixture shows the new timer immediately.
    fn apply_chat_ttl(&mut self, chat_id: ChatId, secs: i32, cx: &mut Context<Self>) {
        self.ttl_picker_open = false;
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .set_chat_message_auto_delete_time(chat_id, secs);
            self.status_note = match result {
                Ok(_) if secs == 0 => "turning off timer…".into(),
                Ok(_) => "setting timer…".into(),
                Err(_) => "could not change timer".into(),
            };
            cx.notify();
            return;
        }
        if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.message_auto_delete_time = secs;
            }
            self.status_note = if secs == 0 {
                "timer off".into()
            } else {
                format!("timer {}", format_ttl_setting(secs)).into()
            };
            cx.notify();
        }
    }

    /// Parity slice: apply arbitrary notification-settings edits to the demo
    /// session (screenshot demos have no TDLib), via the same
    /// `updateChatNotificationSettings` reducer path live updates take.
    fn apply_demo_notification_settings(
        &mut self,
        chat_id: ChatId,
        edit: impl FnOnce(&mut ChatNotificationSettings),
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let mut current = session
            .chats
            .get(&chat_id.0)
            .map(|chat| chat.notification_settings.clone())
            .unwrap_or_default();
        edit(&mut current);
        let json = format!(
            r#"{{"@type":"updateChatNotificationSettings","chat_id":{},"notification_settings":{}}}"#,
            chat_id.0,
            notification_settings_json(&current)
        );
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
        cx.notify();
    }

    fn toggle_archive(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let archived = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.in_archive);
        if self.live.is_some() {
            let result = if archived {
                self.live
                    .as_mut()
                    .expect("live")
                    .driver
                    .unarchive_chat(chat_id)
            } else {
                self.live
                    .as_mut()
                    .expect("live")
                    .driver
                    .archive_chat(chat_id)
            };
            self.status_note = match result {
                Ok(_) if archived => "unarchiving…".into(),
                Ok(_) => "archiving…".into(),
                Err(_) => "could not change archive".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_archive(chat_id, !archived);
            self.status_note = if archived {
                "unarchived".into()
            } else {
                "archived".into()
            };
            cx.notify();
        }
    }

    // ------------------------------------------------------------------
    // Parity slice: folder management (create / edit / delete / reorder /
    // tags / per-chat membership).
    // ------------------------------------------------------------------

    fn open_folder_manage(&mut self, cx: &mut Context<Self>) {
        self.folder_manage_open = true;
        self.folder_editor = None;
        self.folder_delete_confirm = None;
        cx.notify();
    }

    fn close_folder_manage(&mut self, cx: &mut Context<Self>) {
        self.folder_manage_open = false;
        self.folder_editor = None;
        self.folder_delete_confirm = None;
        cx.notify();
    }

    fn open_folder_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.folder_editor = Some(FolderEditorDialog::new(window, cx, None));
        cx.notify();
    }

    fn open_folder_edit(&mut self, folder_id: i32, window: &mut Window, cx: &mut Context<Self>) {
        // Reuse the cached spec when the manage dialog just created it;
        // otherwise fetch the full folder for the prefill.
        let cached = self
            .session()
            .and_then(|s| s.folder_specs.get(&folder_id).cloned());
        let mut dialog = FolderEditorDialog::new(window, cx, Some(folder_id));
        if let Some(spec) = cached {
            dialog.prefill_from_spec(&spec, window, cx);
        } else if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_chat_folder(folder_id)
        {
            self.status_note = format!("could not load folder: {err:?}");
        }
        self.folder_editor = Some(dialog);
        cx.notify();
    }

    /// Edit flow: once the `getChatFolder` spec arrives, prefill the open
    /// editor (no-op for create, or when already prefilled).
    fn maybe_prefill_folder_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let folder_id = match self.folder_editor.as_ref() {
            Some(dialog) if dialog.fetch_pending => dialog.folder_id,
            _ => return,
        };
        let Some(folder_id) = folder_id else {
            return;
        };
        let Some(spec) = self
            .session()
            .and_then(|s| s.folder_specs.get(&folder_id).cloned())
        else {
            return;
        };
        if let Some(dialog) = self.folder_editor.as_mut() {
            dialog.prefill_from_spec(&spec, window, cx);
        }
        cx.notify();
    }

    fn save_folder_editor(&mut self, cx: &mut Context<Self>) {
        let (name, folder_id) = match self.folder_editor.as_ref() {
            Some(dialog) => (dialog.name(cx), dialog.folder_id),
            None => return,
        };
        if let Some(dialog) = self.folder_editor.as_mut() {
            dialog.editor.name = name;
            if let Some(err) = dialog.editor.validate() {
                dialog.error = Some(err.to_string());
                cx.notify();
                return;
            }
        }
        let spec = self
            .folder_editor
            .as_ref()
            .map(|dialog| dialog.editor.to_spec());
        let Some(spec) = spec else { return };
        let result = match self.live.as_mut() {
            Some(live) => match folder_id {
                Some(id) => live.driver.edit_chat_folder(id, &spec).map(|_| ()),
                None => live.driver.create_chat_folder(&spec).map(|_| ()),
            },
            None => {
                // Screenshot demo: apply locally so the manage dialog shows
                // the change without live Telegram.
                self.apply_demo_folder_save(folder_id, spec);
                Ok(())
            }
        };
        match result {
            Ok(()) => {
                self.folder_editor = None;
                self.status_note = if folder_id.is_some() {
                    "folder updated".into()
                } else {
                    "folder created".into()
                };
            }
            Err(err) => {
                if let Some(dialog) = self.folder_editor.as_mut() {
                    dialog.error = Some(format!("could not save folder: {err:?}"));
                }
            }
        }
        cx.notify();
    }

    fn open_folder_delete(&mut self, folder_id: i32, cx: &mut Context<Self>) {
        let name = self
            .session()
            .and_then(|s| s.chat_folders.iter().find(|f| f.id == folder_id))
            .map(|f| f.name.clone())
            .unwrap_or_else(|| format!("Folder {folder_id}"));
        self.folder_delete_confirm = Some(FolderDeleteConfirm {
            folder_id,
            name,
            leave_with_folder: false,
        });
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_chat_folder_chats_to_leave(folder_id)
        {
            self.status_note = format!("could not load folder chats: {err:?}");
        }
        cx.notify();
    }

    fn confirm_folder_delete(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.folder_delete_confirm.take() else {
            return;
        };
        let leave: Vec<i64> = if confirm.leave_with_folder {
            self.session()
                .and_then(|s| s.folder_chats_to_leave.get(&confirm.folder_id).cloned())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let result = match self.live.as_mut() {
            Some(live) => live
                .driver
                .delete_chat_folder(confirm.folder_id, &leave)
                .map(|_| ()),
            None => {
                self.apply_demo_folder_delete(confirm.folder_id);
                Ok(())
            }
        };
        self.status_note = match result {
            Ok(()) => "folder deleted".into(),
            Err(err) => format!("could not delete folder: {err:?}"),
        };
        if self.folder_tab == Some(confirm.folder_id) {
            self.folder_tab = None;
        }
        cx.notify();
    }

    fn move_folder(&mut self, folder_id: i32, up: bool, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let mut ids: Vec<i32> = session.chat_folders.iter().map(|f| f.id).collect();
        let Some(pos) = ids.iter().position(|&id| id == folder_id) else {
            return;
        };
        let target = if up { pos.saturating_sub(1) } else { pos + 1 };
        if target >= ids.len() || target == pos {
            return;
        }
        ids.swap(pos, target);
        let result = match self.live.as_mut() {
            Some(live) => live.driver.reorder_chat_folders(&ids).map(|_| ()),
            None => {
                self.apply_demo_folder_reorder(&ids);
                Ok(())
            }
        };
        self.status_note = match result {
            Ok(()) => "folders reordered".into(),
            Err(err) => format!("could not reorder folders: {err:?}"),
        };
        cx.notify();
    }

    fn toggle_folder_tags_ui(&mut self, cx: &mut Context<Self>) {
        let enabled = self
            .session()
            .map(|s| !s.are_folder_tags_enabled)
            .unwrap_or(true);
        let result = match self.live.as_mut() {
            Some(live) => live.driver.toggle_chat_folder_tags(enabled).map(|_| ()),
            None => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.are_folder_tags_enabled = enabled;
                }
                Ok(())
            }
        };
        self.status_note = match result {
            Ok(()) if enabled => "folder tags on".into(),
            Ok(()) => "folder tags off".into(),
            Err(err) => format!("could not toggle folder tags: {err:?}"),
        };
        cx.notify();
    }

    fn open_folder_menu(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.folder_menu_open = true;
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_chat_lists_to_add_chat(chat_id)
        {
            self.status_note = format!("could not load folder options: {err:?}");
        }
        cx.notify();
    }

    fn close_folder_menu(&mut self, cx: &mut Context<Self>) {
        self.folder_menu_open = false;
        cx.notify();
    }

    fn add_open_chat_to_folder(&mut self, chat_id: ChatId, folder_id: i32, cx: &mut Context<Self>) {
        let result = match self.live.as_mut() {
            Some(live) => live
                .driver
                .add_chat_to_folder(chat_id, folder_id)
                .map(|_| ()),
            None => Ok(()),
        };
        self.status_note = match result {
            Ok(()) => "adding chat to folder…".into(),
            Err(err) => format!("could not add chat to folder: {err:?}"),
        };
        self.folder_menu_open = false;
        cx.notify();
    }

    fn remove_open_chat_from_folder(
        &mut self,
        chat_id: ChatId,
        folder_id: i32,
        cx: &mut Context<Self>,
    ) {
        let result = match self.live.as_mut() {
            Some(live) => live.driver.remove_chat_from_folder(chat_id, folder_id),
            None => Ok(()),
        };
        self.status_note = match result {
            Ok(()) => "removing chat from folder…".into(),
            Err(err) => format!("could not remove chat from folder: {err:?}"),
        };
        self.folder_menu_open = false;
        cx.notify();
    }

    /// Parity slice: screenshot-demo folder create/edit (no live Telegram —
    /// apply to the demo session directly).
    fn apply_demo_folder_save(&mut self, folder_id: Option<i32>, spec: ChatFolderSpec) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        match folder_id {
            Some(id) => {
                if let Some(info) = session.chat_folders.iter_mut().find(|f| f.id == id) {
                    info.name = spec.name.clone();
                }
                session.folder_specs.insert(id, spec);
            }
            None => {
                let id = session.chat_folders.iter().map(|f| f.id).max().unwrap_or(0) + 1;
                session.chat_folders.push(ChatFolderInfo {
                    id,
                    name: spec.name.clone(),
                    icon_name: String::new(),
                    color_id: -1,
                });
                session.folder_specs.insert(id, spec);
            }
        }
    }

    /// Parity slice: screenshot-demo folder delete.
    fn apply_demo_folder_delete(&mut self, folder_id: i32) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        session.chat_folders.retain(|f| f.id != folder_id);
        session.folder_specs.remove(&folder_id);
        session.folder_chats_to_leave.remove(&folder_id);
        session.folder_chats_exhausted.remove(&folder_id);
    }

    /// Parity slice: screenshot-demo folder reorder.
    fn apply_demo_folder_reorder(&mut self, ids: &[i32]) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let order: HashMap<i32, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        session
            .chat_folders
            .sort_by_key(|f| order.get(&f.id).copied().unwrap_or(usize::MAX));
    }

    fn apply_demo_archive(&mut self, chat_id: ChatId, archive: bool) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let order = session
            .chats
            .get(&chat_id.0)
            .map(|chat| {
                if archive {
                    chat.order.max(1)
                } else {
                    chat.archive_order.max(1)
                }
            })
            .unwrap_or(1);
        let jsons = if archive {
            vec![
                format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"0","is_pinned":false}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatRemovedFromList","chat_id":{},"chat_list":{{"@type":"chatListMain"}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"chatListArchive"}},"order":"{order}","is_pinned":false}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatAddedToList","chat_id":{},"chat_list":{{"@type":"chatListArchive"}}}}"#,
                    chat_id.0
                ),
            ]
        } else {
            vec![
                format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"chatListArchive"}},"order":"0","is_pinned":false}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatRemovedFromList","chat_id":{},"chat_list":{{"@type":"chatListArchive"}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatAddedToList","chat_id":{},"chat_list":{{"@type":"chatListMain"}}}}"#,
                    chat_id.0
                ),
            ]
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        if let Some(session) = self.demo_session.as_mut() {
            for json in jsons {
                if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
        }
    }

    fn open_chat_id(&self) -> Option<ChatId> {
        self.live
            .as_ref()
            .and_then(|live| live.driver.session.open_chat)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .and_then(|session| session.open_chat)
            })
    }

    fn composer_reply_id(&self, chat_id: ChatId) -> Option<MessageId> {
        self.pending_reply.as_ref().and_then(|reply| {
            if reply.chat_id == chat_id {
                Some(reply.message_id)
            } else {
                None
            }
        })
    }

    fn note_open_draft(&mut self, delayed: bool, cx: &mut Context<Self>) {
        if self.pending_edit.is_some() {
            return;
        }
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        let text = self.composer.read(cx).value().to_string();
        let reply = self.composer_reply_id(chat_id);
        self.save_chat_draft(chat_id, &text, reply, delayed, cx);
    }

    fn leaving_draft_parts(&self, cx: &Context<Self>) -> (String, Option<MessageId>, u64) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let Some(chat_id) = self.open_chat_id() else {
            return (String::new(), None, now_ms);
        };
        if self.pending_edit.is_some() {
            let reply = self.saved_edit_reply.as_ref().and_then(|saved| {
                if saved.chat_id == chat_id {
                    Some(saved.message_id)
                } else {
                    None
                }
            });
            return (self.saved_edit_draft.clone(), reply, now_ms);
        }
        (
            self.composer.read(cx).value().to_string(),
            self.composer_reply_id(chat_id),
            now_ms,
        )
    }

    fn dismiss_cross_chat_state(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if self
            .pending_reply
            .as_ref()
            .is_some_and(|reply| reply.chat_id != chat_id)
        {
            self.pending_reply = None;
        }
        if self
            .pending_edit
            .as_ref()
            .is_some_and(|edit| edit.chat_id != chat_id)
        {
            self.pending_edit = None;
            self.saved_edit_draft.clear();
            self.saved_edit_reply = None;
        }
        if self
            .pending_delete
            .as_ref()
            .is_some_and(|confirm| confirm.chat_id != chat_id)
        {
            self.pending_delete = None;
        }
        if self
            .pending_forward
            .as_ref()
            .is_some_and(|draft| draft.from_chat_id != chat_id)
        {
            self.pending_forward = None;
            self.forward_picker_open = false;
        }
        if self
            .pending_react
            .is_some_and(|(react_chat, _)| react_chat != chat_id)
        {
            self.pending_react = None;
        }
        // Phase 3.3: the `/` menu never survives a chat switch.
        self.command_menu_open = false;
        self.command_menu_selected = 0;
        if self.voice_capture.is_some() {
            self.cancel_voice_recording(cx);
        }
    }

    fn flush_leaving_draft(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        let (text, reply) = if self.pending_edit.is_some() {
            let reply = self.saved_edit_reply.as_ref().and_then(|saved| {
                if saved.chat_id == chat_id {
                    Some(saved.message_id)
                } else {
                    None
                }
            });
            (self.saved_edit_draft.clone(), reply)
        } else {
            (
                self.composer.read(cx).value().to_string(),
                self.composer_reply_id(chat_id),
            )
        };
        self.save_chat_draft(chat_id, &text, reply, false, cx);
    }

    fn save_chat_draft(
        &mut self,
        chat_id: ChatId,
        text: &str,
        reply_to: Option<MessageId>,
        delayed: bool,
        cx: &mut Context<Self>,
    ) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if self.live.is_some() {
            let outcome = self.live.as_mut().and_then(|live| {
                live.driver
                    .note_composer_draft(chat_id, text, reply_to, now_ms, delayed)
                    .ok()
            });
            if let Some(DraftSaveOutcome::Debounced { token, delay }) = outcome {
                self.schedule_draft_commit(token, delay, cx);
            }
            return;
        }
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        if !session.accepts_composer_draft(chat_id) {
            return;
        }
        let stored = draft_text_to_store(text, reply_to.is_some()).map(str::to_string);
        let reply_to = stored.as_ref().and(reply_to);
        let draft = stored.map(|text| ChatDraft {
            text,
            reply_to_message_id: reply_to,
        });
        session.store_composer_draft(chat_id, draft);
    }

    fn schedule_draft_commit(&mut self, token: u64, delay: Duration, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            this.update(cx, |this, _cx| {
                if let Some(live) = this.live.as_mut() {
                    let _ = live.driver.commit_debounced_draft(token);
                }
            })
            .ok();
        })
        .detach();
    }

    fn forget_local_draft(&mut self, chat_id: ChatId) {
        if let Some(live) = self.live.as_mut() {
            live.driver.cancel_pending_draft();
            live.driver.session.store_composer_draft(chat_id, None);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.store_composer_draft(chat_id, None);
        }
    }

    fn restore_open_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_edit.is_some() {
            return;
        }
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        let restored = self.session().and_then(|session| {
            if session.draft_is_dirty(chat_id) {
                return None;
            }
            if !session.accepts_composer_draft(chat_id) {
                return Some((None, None));
            }
            let draft = session
                .chats
                .get(&chat_id.0)
                .and_then(|chat| chat.draft.clone());
            let preview = draft
                .as_ref()
                .and_then(|draft| draft.reply_to_message_id)
                .map(|id| {
                    let text = session
                        .histories
                        .get(&chat_id.0)
                        .and_then(|history| history.messages.get(&id.0))
                        .map(|message| message.content.preview())
                        .filter(|text| !text.is_empty())
                        .unwrap_or_else(|| "message".into());
                    (id, text)
                });
            Some((draft, preview))
        });
        let Some((draft, preview)) = restored else {
            return;
        };
        let text = draft
            .as_ref()
            .map(|draft| draft.text.clone())
            .unwrap_or_default();
        self.pending_reply =
            preview.map(|(id, preview)| ComposerReplyTo::new(chat_id, id, preview));
        self.composer
            .update(cx, |input, cx| input.set_value(&text, window, cx));
    }

    fn sync_composer_typing(&mut self, text: &str) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let editing = self.pending_edit.is_some();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let _ = live.driver.sync_outgoing_typing(text, editing, now_ms);
    }

    fn conversation_header(
        &self,
        title: &str,
        actions: Option<(ChatId, bool, bool, bool)>,
        typing: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (chat_id, muted, forever, archived) =
            actions.unwrap_or((ChatId(0), false, false, false));
        // Phase 6: the header title opens the info panel for private chats
        // (user profile) and supergroups/channels (group info). Other chat
        // kinds keep the plain title.
        let info_target = actions.and_then(|(chat_id, _, _, _)| {
            self.session()
                .and_then(|s| s.info_panel_target_for_chat(chat_id))
        });
        // Parity slice: channel/supergroup header extras — photo,
        // description snippet, primary @username, subscriber/member count,
        // and the linked discussion chat ("Discuss").
        let extras: Option<SupergroupHeaderExtras> = actions.and_then(|(chat_id, _, _, _)| {
            let session = self.session()?;
            let chat = session.chats.get(&chat_id.0)?;
            let (supergroup_id, is_channel) = match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel,
                } => (supergroup_id, is_channel),
                _ => return None,
            };
            let full = session.supergroup_full_infos.get(&supergroup_id).cloned();
            let roots = self.media_display_roots();
            let photo = session
                .chat_photo_path(chat_id)
                .and_then(|path| sandboxed_display_path(path, &roots));
            let username = session
                .supergroup_username(supergroup_id)
                .filter(|name| !name.is_empty())
                .map(|name| name.to_string());
            Some(SupergroupHeaderExtras {
                is_channel,
                photo,
                username,
                member_count: full.as_ref().map(|info| info.member_count),
                description_snippet: full.as_ref().and_then(|info| {
                    let snippet = description_snippet(&info.description, 120);
                    (!snippet.is_empty()).then_some(snippet)
                }),
                discussion_chat_id: session.discussion_chat_id(chat_id),
            })
        });
        let discuss_chat_id = extras.as_ref().and_then(|ex| ex.discussion_chat_id);
        let title_text = title.to_string();
        let muted_fg = cx.theme().muted_foreground;
        // Phase B4: chat-level auto-delete / self-destruct timer status
        // (`chat.message_auto_delete_time`, schema 1.8.67 lines 3616 /
        // 3627) — shown under the title when a timer is set.
        let ttl_line: Option<String> = actions.and_then(|(chat_id, _, _, _)| {
            self.session()
                .and_then(|s| s.chats.get(&chat_id.0))
                .and_then(|chat| chat.ttl_status_line())
        });
        // Status lines kept for every chat kind (typing / muted).
        let identity: AnyElement = match (info_target, extras) {
            (Some(InfoPanelTarget::Supergroup(supergroup_id)), Some(ex)) => {
                let mut meta: Vec<String> = Vec::new();
                if let Some(username) = &ex.username {
                    meta.push(format!("@{username}"));
                }
                if let Some(count) = ex.member_count.filter(|count| *count > 0) {
                    let noun = if ex.is_channel {
                        "subscribers"
                    } else {
                        "members"
                    };
                    meta.push(format!("{} {noun}", compact_count(count)));
                }
                let meta_line = meta.join(" · ");
                div()
                    .id("conversation-identity")
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_supergroup_panel(supergroup_id, window, cx);
                    }))
                    .child(chat_avatar(
                        &title_text,
                        chat_id.0,
                        ex.photo.as_deref(),
                        40.,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(div().font_semibold().child(title_text))
                            .when(!meta_line.is_empty(), |this| {
                                this.child(
                                    div()
                                        .id("conversation-meta")
                                        .text_xs()
                                        .text_color(muted_fg)
                                        .child(meta_line),
                                )
                            })
                            .when_some(ex.description_snippet, |this, snippet| {
                                this.child(
                                    div()
                                        .id("conversation-description")
                                        .text_xs()
                                        .text_color(muted_fg)
                                        .child(snippet),
                                )
                            })
                            .when(typing, |this| {
                                this.child(
                                    div()
                                        .id("peer-typing")
                                        .text_sm()
                                        .text_color(cx.theme().accent)
                                        .child("typing…"),
                                )
                            })
                            .when(muted && !typing, |this| {
                                this.child(div().text_xs().text_color(muted_fg).child(if forever {
                                    "Muted forever"
                                } else {
                                    "Muted"
                                }))
                            })
                            // Phase B4: chat-level auto-delete / self-destruct timer
                            // status line (hidden when no timer is set).
                            .when_some(ttl_line.clone(), |this, line| {
                                this.child(
                                    div()
                                        .id("conversation-ttl")
                                        .text_xs()
                                        .text_color(muted_fg)
                                        .child(line),
                                )
                            }),
                    )
                    .into_any_element()
            }
            (Some(InfoPanelTarget::User(user_id)), _) => div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(
                    div()
                        .id("conversation-title")
                        .font_semibold()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_user_panel(user_id, window, cx);
                        }))
                        .child(title_text),
                )
                .when(typing, |this| {
                    this.child(
                        div()
                            .id("peer-typing")
                            .text_sm()
                            .text_color(cx.theme().accent)
                            .child("typing…"),
                    )
                })
                .when(muted && !typing, |this| {
                    this.child(div().text_xs().text_color(muted_fg).child(if forever {
                        "Muted forever"
                    } else {
                        "Muted"
                    }))
                })
                // Phase B4: chat-level auto-delete / self-destruct timer
                // status line (hidden when no timer is set).
                .when_some(ttl_line.clone(), |this, line| {
                    this.child(
                        div()
                            .id("conversation-ttl")
                            .text_xs()
                            .text_color(muted_fg)
                            .child(line),
                    )
                })
                .into_any_element(),
            _ => div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(div().font_semibold().child(title_text))
                .when(typing, |this| {
                    this.child(
                        div()
                            .id("peer-typing")
                            .text_sm()
                            .text_color(cx.theme().accent)
                            .child("typing…"),
                    )
                })
                .when(muted && !typing, |this| {
                    this.child(div().text_xs().text_color(muted_fg).child(if forever {
                        "Muted forever"
                    } else {
                        "Muted"
                    }))
                })
                // Phase B4: chat-level auto-delete / self-destruct timer
                // status line (hidden when no timer is set).
                .when_some(ttl_line.clone(), |this, line| {
                    this.child(
                        div()
                            .id("conversation-ttl")
                            .text_xs()
                            .text_color(muted_fg)
                            .child(line),
                    )
                })
                .into_any_element(),
        };
        div()
            .id("conversation-header")
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(identity)
            .when(actions.is_some(), |this| {
                // Phase B1: secret chats get a "Close secret chat" action
                // (`closeSecretChat`, schema 1.8.67 line 15242) instead of
                // the folder picker — closing is permanent.
                let is_secret = self
                    .session()
                    .and_then(|s| s.chats.get(&chat_id.0))
                    .is_some_and(|c| matches!(c.kind, ChatKind::Secret { .. }));
                // Phase B4: the self-destruct timer picker is only usable
                // in Ready secret chats — Pending/Closed chats can't send
                // (`can_post` is false there), and the driver would
                // reject the request.
                let ttl_ready = self
                    .session()
                    .and_then(|s| s.chats.get(&chat_id.0))
                    .is_some_and(|c| matches!(c.kind, ChatKind::Secret { .. }) && c.can_post());
                let ttl_button_label = self
                    .session()
                    .and_then(|s| s.chats.get(&chat_id.0))
                    .map(|c| format!("⏱ {}", format_ttl_setting(c.message_auto_delete_time)))
                    .unwrap_or_else(|| "⏱ Off".to_string());
                this.child(
                    div()
                        .flex()
                        .gap_1()
                        // Phase B1: close a secret chat (confirm banner
                        // below, like delete confirm).
                        .when(is_secret, |this| {
                            this.child(
                                Button::new("chat-close-secret")
                                    .label("Close secret chat")
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_close_secret_chat_confirm(chat_id, cx);
                                    })),
                            )
                        })
                        // Phase B4: self-destruct timer picker for Ready
                        // secret chats (`setChatMessageAutoDeleteTime`,
                        // schema 1.8.67 line 13454).
                        .when(ttl_ready, |this| {
                            this.child(
                                Button::new("chat-ttl")
                                    .label(ttl_button_label.clone())
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.ttl_picker_open = !this.ttl_picker_open;
                                        cx.notify();
                                    })),
                            )
                        })
                        // Parity slice: jump to the linked discussion group
                        // (`linked_chat_id`) when the channel has one.
                        .when_some(discuss_chat_id, |this, discussion_id| {
                            this.child(
                                Button::new("chat-discuss")
                                    .label("Discuss")
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.select_listed_chat(ChatId(discussion_id), window, cx);
                                    })),
                            )
                        })
                        .child(
                            Button::new("chat-mute")
                                .label(if muted { "Unmute" } else { "Mute" })
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if muted {
                                        this.apply_chat_mute(chat_id, 0, cx);
                                    } else if this.mute_menu_open {
                                        this.close_mute_menu(cx);
                                    } else {
                                        this.open_mute_menu(cx);
                                    }
                                })),
                        )
                        .child(
                            Button::new("chat-archive")
                                .label(if archived { "Unarchive" } else { "Archive" })
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_archive(chat_id, cx);
                                })),
                        )
                        // Parity slice: per-chat folder picker — destinations
                        // come from `getChatListsToAddChat`, as the schema
                        // intends; removals go through `editChatFolder`
                        // (there is no `removeChatFromList` in 1.8.67).
                        .child(
                            Button::new("chat-folders")
                                .label("Folders")
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.folder_menu_open {
                                        this.folder_menu_open = false;
                                        cx.notify();
                                    } else {
                                        this.open_folder_menu(chat_id, cx);
                                    }
                                })),
                        ),
                )
            })
    }

    /// Parity slice: the tdesktop "Mute" submenu is now a per-chat
    /// notification settings panel — mute presets, message-preview toggle,
    /// notification-sound picker (`getSavedNotificationSounds`), and a link
    /// to the scope defaults dialog. Current state shows in the summary line.
    fn mute_menu_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let open_chat = session.as_ref().and_then(|s| s.open_chat);
        let open_chat_summary: Option<&ChatSummary> =
            open_chat.and_then(|id| session.as_ref()?.chats.get(&id.0));
        let chat_settings: ChatNotificationSettings = open_chat_summary
            .map(|chat| chat.notification_settings.clone())
            .unwrap_or_default();
        let muted = open_chat_summary
            .and_then(|chat| session.as_ref().map(|s| s.effective_muted(chat)))
            .unwrap_or_else(|| chat_settings.is_muted());
        let preview_on = open_chat_summary
            .and_then(|chat| session.as_ref().map(|s| s.effective_preview_allowed(chat)))
            .unwrap_or(chat_settings.use_default_show_preview || chat_settings.show_preview);
        let sound_label = self.notification_sound_label(&chat_settings);
        let saved_sounds: Vec<NotificationSound> = session
            .as_ref()
            .map(|s| s.saved_notification_sounds.clone())
            .unwrap_or_default();
        let status = format!(
            "{} \u{b7} Sound: {} \u{b7} Previews: {}",
            if muted {
                if chat_settings.is_muted_forever() {
                    "Muted forever"
                } else {
                    "Muted"
                }
            } else {
                "Unmuted"
            },
            sound_label,
            if preview_on { "on" } else { "off" },
        );

        let presets = [
            ("1 hour", MUTE_FOR_1_HOUR),
            ("8 hours", MUTE_FOR_8_HOURS),
            ("2 days", MUTE_FOR_2_DAYS),
            ("Forever", MUTE_FOREVER),
        ];
        let mut preset_row = div().id("mute-presets").flex().flex_wrap().gap_1();
        for (label, seconds) in presets {
            preset_row = preset_row.child(
                Button::new(format!("mute-for-{seconds}"))
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(chat_id) = open_chat {
                            this.apply_chat_mute(chat_id, seconds, cx);
                        }
                    })),
            );
        }

        let mut panel = div()
            .id("mute-menu")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child("Notifications"))
                    .child(
                        Button::new("close-mute-menu")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_mute_menu(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            )
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child("Mute for"),
            )
            .child(preset_row)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Show message preview in notifications"),
                    )
                    .child(
                        Button::new("notif-preview-toggle")
                            .label(if preview_on { "On" } else { "Off" })
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(chat_id) = open_chat {
                                    this.apply_chat_preview(chat_id, !preview_on, cx);
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Notification sound: {sound_label}")),
                    )
                    .child(
                        Button::new("notif-sound-picker-toggle")
                            .label(if self.notif_sound_picker_open {
                                "Hide"
                            } else {
                                "Change"
                            })
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notif_sound_picker_open = !this.notif_sound_picker_open;
                                if this.notif_sound_picker_open
                                    && let Some(live) = this.live.as_mut()
                                {
                                    let _ = live.driver.maybe_fetch_notification_sounds();
                                }
                                cx.notify();
                            })),
                    ),
            );
        if self.notif_sound_picker_open
            && let Some(chat_id) = open_chat
        {
            let current = if chat_settings.use_default_sound {
                SoundChoice::Default
            } else if chat_settings.sound_id == 0 {
                SoundChoice::Disabled
            } else {
                SoundChoice::Custom(chat_settings.sound_id)
            };
            panel = panel.child(self.notification_sound_picker(
                cx,
                SoundPickerTarget::Chat(chat_id),
                current,
                &saved_sounds,
            ));
        }
        panel.child(
            Button::new("notif-open-defaults")
                .label("Defaults for all chats\u{2026}")
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.notification_defaults_open = true;
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.maybe_fetch_scope_notification_settings();
                        let _ = live.driver.maybe_fetch_notification_sounds();
                    }
                    cx.notify();
                })),
        )
    }

    /// Phase B4: self-destruct / auto-delete timer picker below the
    /// conversation header. The picker is secret-chat-only (the header
    /// button is gated), so only the secret presets are reachable today;
    /// the non-secret day-multiple branch below is defensive, kept for
    /// the planned regular-chat picker follow-up
    /// (`setChatMessageAutoDeleteTime`, schema 1.8.67 line 13454).
    fn ttl_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let open_chat = session.as_ref().and_then(|s| s.open_chat);
        let open_chat_summary: Option<&ChatSummary> =
            open_chat.and_then(|id| session.as_ref()?.chats.get(&id.0));
        let is_secret =
            open_chat_summary.is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        let current = open_chat_summary
            .and_then(|chat| chat.ttl_status_line())
            .unwrap_or_else(|| "Off".to_string());
        let title = if is_secret {
            "Self-destruct timer"
        } else {
            "Auto-delete timer"
        };
        // Schema value rule (1.8.67 line 13454): secret chats accept
        // arbitrary seconds; other chats need day multiples.
        let presets: &[(&str, i32)] = if is_secret {
            &[
                ("Off", 0),
                ("5s", 5),
                ("30s", 30),
                ("1m", 60),
                ("1h", 3600),
                ("1d", 86400),
                ("1w", 604800),
            ]
        } else {
            &[("Off", 0), ("1d", 86400), ("1w", 604800), ("30d", 2592000)]
        };
        let mut preset_row = div().id("ttl-presets").flex().flex_wrap().gap_1();
        for (label, secs) in presets {
            let secs = *secs;
            let active =
                open_chat_summary.is_some_and(|chat| chat.message_auto_delete_time == secs);
            preset_row = preset_row.child(
                Button::new(format!("ttl-set-{secs}"))
                    .label(if active {
                        format!("● {label}")
                    } else {
                        (*label).to_string()
                    })
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(chat_id) = open_chat {
                            this.apply_chat_ttl(chat_id, secs, cx);
                        }
                    })),
            );
        }
        div()
            .id("ttl-picker")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child(title))
                    .child(
                        Button::new("close-ttl-picker")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.ttl_picker_open = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "Current: {current} — messages auto-delete after the timer; \
                         in secret chats the countdown starts once the message is viewed."
                    )),
            )
            .child(preset_row)
    }

    /// Parity slice: current sound choice for a chat, for the notifications
    /// panel summary and picker checkmarks.
    fn notification_sound_label(&self, settings: &ChatNotificationSettings) -> String {
        if settings.use_default_sound {
            return "Default".to_string();
        }
        if settings.sound_id == 0 {
            return "None".to_string();
        }
        self.session()
            .and_then(|s| {
                s.saved_notification_sounds
                    .iter()
                    .find(|sound| sound.id == settings.sound_id)
                    .map(|sound| sound.title.clone())
            })
            .unwrap_or_else(|| "Custom".to_string())
    }

    /// Parity slice: apply a sound choice to the open chat
    /// (`setChatNotificationSettings`).
    fn apply_chat_sound(
        &mut self,
        chat_id: ChatId,
        use_default_sound: bool,
        sound_id: i64,
        cx: &mut Context<Self>,
    ) {
        self.notif_sound_picker_open = false;
        if self.live.is_some() {
            let result = self.live.as_mut().expect("live").driver.set_chat_sound(
                chat_id,
                use_default_sound,
                sound_id,
            );
            self.status_note = match result {
                Ok(_) => "sound updated…".into(),
                Err(_) => "could not change sound".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_notification_settings(
                chat_id,
                |settings| {
                    settings.use_default_sound = use_default_sound;
                    settings.sound_id = sound_id;
                },
                cx,
            );
            self.status_note = "sound updated".into();
            cx.notify();
        }
    }

    /// Parity slice: apply a message-preview exception to the open chat
    /// (`setChatNotificationSettings`).
    fn apply_chat_preview(&mut self, chat_id: ChatId, show_preview: bool, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .set_chat_show_preview(chat_id, show_preview);
            self.status_note = match result {
                Ok(_) => "preview setting updated…".into(),
                Err(_) => "could not change preview".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_notification_settings(
                chat_id,
                |settings| {
                    settings.use_default_show_preview = false;
                    settings.show_preview = show_preview;
                },
                cx,
            );
            self.status_note = "preview setting updated".into();
            cx.notify();
        }
    }

    /// Parity slice: preview a saved notification sound immediately — play
    /// the MP3 when local, otherwise download it and play on completion
    /// (via `Session::pending_sound_plays`).
    fn preview_saved_sound(&mut self, sound_id: i64) {
        if let Some(live) = self.live.as_mut() {
            match live
                .driver
                .resolve_notification_sound(NotificationSoundKind::Custom(sound_id))
            {
                SoundResolution::DefaultTone => {
                    if let Some(command) = quill::notify::default_tone_command() {
                        self.spawn_sound_command(command);
                    }
                }
                SoundResolution::FilePath(path) => {
                    if let Some(command) =
                        quill::notify::file_sound_command(&path.to_string_lossy())
                    {
                        self.spawn_sound_command(command);
                    }
                }
                SoundResolution::Pending => {}
            }
        } else if let Some(command) = quill::notify::default_tone_command() {
            // Screenshot demo: no TDLib files exist — the tone stands in.
            self.spawn_sound_command(command);
        }
    }

    /// Parity slice: the saved-sound picker shared by the per-chat panel and
    /// the scope defaults dialog. `getSavedNotificationSounds` says: "If a
    /// sound isn't in the list, then default sound needs to be used" — so
    /// Default / None are always offered first.
    fn notification_sound_picker(
        &self,
        cx: &mut Context<Self>,
        target: SoundPickerTarget,
        current: SoundChoice,
        saved_sounds: &[NotificationSound],
    ) -> AnyElement {
        let list_id = match target {
            SoundPickerTarget::Chat(_) => "notif-sound-list".to_string(),
            SoundPickerTarget::Scope(scope) => format!("scope-sound-list-{scope:?}"),
        };
        let mut list = div().id(list_id).flex().flex_col().gap_1().py_1();
        list = list.child(self.sound_picker_row(
            cx,
            target,
            SoundChoice::Default,
            current,
            "Default",
            "Telegram default tone",
            None,
        ));
        list = list.child(self.sound_picker_row(
            cx,
            target,
            SoundChoice::Disabled,
            current,
            "None",
            "No sound",
            None,
        ));
        for sound in saved_sounds {
            let subtitle = format!("{} ({}s)", sound.title, sound.duration.max(0));
            list = list.child(self.sound_picker_row(
                cx,
                target,
                SoundChoice::Custom(sound.id),
                current,
                &sound.title,
                &subtitle,
                Some(sound.id),
            ));
        }
        list.into_any_element()
    }

    /// Parity slice: one row of the sound picker. The `▶` preview button
    /// plays the sound without selecting it.
    fn sound_picker_row(
        &self,
        cx: &mut Context<Self>,
        target: SoundPickerTarget,
        choice: SoundChoice,
        current: SoundChoice,
        title: &str,
        subtitle: &str,
        preview_sound_id: Option<i64>,
    ) -> AnyElement {
        let selected = choice == current;
        let row_id = match (target, choice) {
            (SoundPickerTarget::Chat(_), SoundChoice::Default) => "sound-pick-chat-default",
            (SoundPickerTarget::Chat(_), SoundChoice::Disabled) => "sound-pick-chat-none",
            (SoundPickerTarget::Chat(_), SoundChoice::Custom(_)) => "sound-pick-chat-custom",
            (SoundPickerTarget::Scope(_), SoundChoice::Default) => "sound-pick-scope-default",
            (SoundPickerTarget::Scope(_), SoundChoice::Disabled) => "sound-pick-scope-none",
            (SoundPickerTarget::Scope(_), SoundChoice::Custom(_)) => "sound-pick-scope-custom",
        };
        let mut row = div()
            .id(format!("{row_id}-row"))
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().accent.opacity(0.08)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().child(format!(
                        "{}{}",
                        if selected { "✓ " } else { "" },
                        title
                    )))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(subtitle.to_string()),
                    ),
            );
        if let Some(sound_id) = preview_sound_id {
            row = row.child(
                Button::new((row_id, sound_id as u64))
                    .label("▶")
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, _| {
                        this.preview_saved_sound(sound_id);
                    })),
            );
        }
        row.on_click(cx.listener(move |this, _, _, cx| {
            this.apply_sound_choice(target, choice, cx);
        }))
        .into_any_element()
    }

    /// Parity slice: apply a picker choice to its target.
    fn apply_sound_choice(
        &mut self,
        target: SoundPickerTarget,
        choice: SoundChoice,
        cx: &mut Context<Self>,
    ) {
        match target {
            SoundPickerTarget::Chat(chat_id) => {
                let (use_default_sound, sound_id) = match choice {
                    SoundChoice::Default => (true, 0),
                    SoundChoice::Disabled => (false, 0),
                    SoundChoice::Custom(id) => (false, id),
                };
                self.apply_chat_sound(chat_id, use_default_sound, sound_id, cx);
            }
            SoundPickerTarget::Scope(scope) => {
                let sound_id = match choice {
                    // Scope `-1` = app-dependent default (schema line 3368).
                    SoundChoice::Default => -1,
                    SoundChoice::Disabled => 0,
                    SoundChoice::Custom(id) => id,
                };
                self.apply_scope_sound(scope, sound_id, cx);
            }
        }
    }

    /// Parity slice: apply a scope's sound default
    /// (`setScopeNotificationSettings`).
    fn apply_scope_sound(
        &mut self,
        scope: NotificationSettingsScope,
        sound_id: i64,
        cx: &mut Context<Self>,
    ) {
        self.defaults_sound_picker = None;
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.sound_id = sound_id;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.status_note = match result {
                Ok(_) => "default sound updated…".into(),
                Err(_) => "could not change default sound".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the dialog reflects it.
            let mut settings = session
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.sound_id = sound_id;
            session.scope_notification_settings.insert(scope, settings);
            self.status_note = "default sound updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a scope's mute default.
    fn apply_scope_mute(
        &mut self,
        scope: NotificationSettingsScope,
        mute_for: i32,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.mute_for = mute_for;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.status_note = match result {
                Ok(_) if mute_for == 0 => "default unmuted…".into(),
                Ok(_) => "default mute updated…".into(),
                Err(_) => "could not change default mute".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let mut settings = session
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.mute_for = mute_for;
            session.scope_notification_settings.insert(scope, settings);
            self.status_note = "default mute updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a scope's preview default.
    fn apply_scope_preview(
        &mut self,
        scope: NotificationSettingsScope,
        show_preview: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.show_preview = show_preview;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.status_note = match result {
                Ok(_) => "default preview updated…".into(),
                Err(_) => "could not change default preview".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let mut settings = session
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.show_preview = show_preview;
            session.scope_notification_settings.insert(scope, settings);
            self.status_note = "default preview updated".into();
        }
        cx.notify();
    }

    /// Parity slice: the scope-defaults dialog overlay (private chats /
    /// groups / channels), each with mute presets, a preview toggle, and a
    /// sound picker. Entry point: "Defaults for all chats…" in the per-chat
    /// notifications panel.
    fn notification_defaults_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let saved_sounds: Vec<NotificationSound> = session
            .as_ref()
            .map(|s| s.saved_notification_sounds.clone())
            .unwrap_or_default();
        let mut body = div().flex().flex_col().gap_3();
        for scope in NotificationSettingsScope::ALL {
            body = body.child(self.scope_settings_section(cx, scope, &saved_sounds));
        }
        div()
            .id("notif-defaults-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("notif-defaults-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .bg(rgba(0x000000e6))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.notification_defaults_open = false;
                        this.defaults_sound_picker = None;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("notif-defaults-dialog")
                    .flex()
                    .flex_col()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .rounded_lg()
                    .bg(cx.theme().sidebar)
                    .border_1()
                    .border_color(cx.theme().border)
                    .min_w(px(420.))
                    .max_w(px(560.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().font_semibold().child("Notification defaults"))
                            .child(
                                Button::new("close-notif-defaults")
                                    .label("Close")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.notification_defaults_open = false;
                                        this.defaults_sound_picker = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Used when a chat keeps the default setting. \
                                 Changes apply via setScopeNotificationSettings.",
                            ),
                    )
                    .child(body),
            )
            .into_any_element()
    }

    /// Parity slice: one scope's section in the defaults dialog.
    fn scope_settings_section(
        &self,
        cx: &mut Context<Self>,
        scope: NotificationSettingsScope,
        saved_sounds: &[NotificationSound],
    ) -> AnyElement {
        let settings: ScopeNotificationSettings = self
            .session()
            .and_then(|s| s.scope_notification_settings.get(&scope).cloned())
            .unwrap_or_default();
        let loaded = self
            .session()
            .is_some_and(|s| s.scope_notification_settings.contains_key(&scope));
        let muted = settings.mute_for > 0;
        let sound_label = match settings.sound_id {
            -1 => "Default".to_string(),
            0 => "None".to_string(),
            id => saved_sounds
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.title.clone())
                .unwrap_or_else(|| "Custom".to_string()),
        };
        let current_choice = match settings.sound_id {
            -1 => SoundChoice::Default,
            0 => SoundChoice::Disabled,
            id => SoundChoice::Custom(id),
        };

        let presets = [
            ("Unmute", 0),
            ("1 hour", MUTE_FOR_1_HOUR),
            ("8 hours", MUTE_FOR_8_HOURS),
            ("2 days", MUTE_FOR_2_DAYS),
            ("Forever", MUTE_FOREVER),
        ];
        let mut preset_row = div().flex().flex_wrap().gap_1();
        for (label, seconds) in presets {
            preset_row = preset_row.child(
                Button::new(format!("scope-mute-{scope:?}-{seconds}"))
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_scope_mute(scope, seconds, cx);
                    })),
            );
        }

        let mut section = div()
            .id(format!("scope-section-{:?}", scope))
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().text_sm().child(scope.label()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if loaded {
                                if muted { "Muted" } else { "Not muted" }.to_string()
                            } else {
                                "Loading…".to_string()
                            }),
                    ),
            )
            .child(preset_row)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Show message preview"),
                    )
                    .child(
                        Button::new(format!("scope-preview-{:?}", scope))
                            .label(if settings.show_preview { "On" } else { "Off" })
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let current = this
                                    .session()
                                    .and_then(|s| {
                                        s.scope_notification_settings.get(&scope).cloned()
                                    })
                                    .unwrap_or_default();
                                this.apply_scope_preview(scope, !current.show_preview, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Sound: {sound_label}")),
                    )
                    .child(
                        Button::new(format!("scope-sound-{:?}", scope))
                            .label(if self.defaults_sound_picker == Some(scope) {
                                "Hide"
                            } else {
                                "Change"
                            })
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.defaults_sound_picker =
                                    if this.defaults_sound_picker == Some(scope) {
                                        None
                                    } else {
                                        Some(scope)
                                    };
                                cx.notify();
                            })),
                    ),
            );
        if self.defaults_sound_picker == Some(scope) {
            section = section.child(self.notification_sound_picker(
                cx,
                SoundPickerTarget::Scope(scope),
                current_choice,
                saved_sounds,
            ));
        }
        section.into_any_element()
    }

    /// Parity slice: per-chat folder picker below the header. Destinations
    /// come from `getChatListsToAddChat` (as the schema intends); current
    /// folder memberships render as remove rows (`editChatFolder` chain —
    /// there is no `removeChatFromList` in 1.8.67).
    fn folder_menu_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let open = session.as_ref().and_then(|s| s.open_chat);
        let mut panel = div()
            .id("folder-menu")
            .flex()
            .flex_col()
            .gap_1()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child("Chat folders"))
                    .child(
                        Button::new("close-folder-menu")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_folder_menu(cx);
                            })),
                    ),
            );
        let (Some(session), Some(chat_id)) = (session, open) else {
            return panel
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No chat open."),
                )
                .into_any_element();
        };
        let folder_name = |id: i32| {
            session
                .chat_folders
                .iter()
                .find(|f| f.id == id)
                .map(|f| f.name.clone())
                .unwrap_or_else(|| format!("Folder {id}"))
        };
        let Some(lists) = session.chat_lists_for_add.get(&chat_id.0) else {
            return panel
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading folder options…"),
                )
                .into_any_element();
        };
        if lists.is_empty() {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No folder destinations available for this chat."),
            );
        }
        for list in lists {
            if let ChatList::Unknown = list {
                continue;
            }
            let (label, id_suffix) = match list {
                ChatList::Main => ("Move to main list".to_string(), "main".to_string()),
                ChatList::Archive => ("Archive chat".to_string(), "archive".to_string()),
                ChatList::Folder(folder_id) => (
                    format!("Add to {}", folder_name(*folder_id)),
                    format!("add-{folder_id}"),
                ),
                // Filtered above; kept for exhaustiveness.
                ChatList::Unknown => (String::new(), "unknown".to_string()),
            };
            let list = *list;
            panel = panel.child(
                Button::new(format!("folder-menu-add-{id_suffix}"))
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| match list {
                        ChatList::Main => {
                            if let Some(live) = this.live.as_mut()
                                && let Err(err) = live.driver.unarchive_chat(chat_id)
                            {
                                this.status_note = format!("could not move chat: {err:?}");
                            }
                            this.folder_menu_open = false;
                            cx.notify();
                        }
                        ChatList::Archive => {
                            if let Some(live) = this.live.as_mut()
                                && let Err(err) = live.driver.archive_chat(chat_id)
                            {
                                this.status_note = format!("could not archive chat: {err:?}");
                            }
                            this.folder_menu_open = false;
                            cx.notify();
                        }
                        ChatList::Folder(folder_id) => {
                            this.add_open_chat_to_folder(chat_id, folder_id, cx);
                        }
                        ChatList::Unknown => {}
                    })),
            );
        }
        // Current folder memberships (positional) render as remove rows.
        if let Some(chat) = session.chats.get(&chat_id.0) {
            for folder_id in chat.folder_positions.keys() {
                let folder_id = *folder_id;
                panel = panel.child(
                    Button::new(format!("folder-menu-remove-{folder_id}"))
                        .label(format!("Remove from {}", folder_name(folder_id)))
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove_open_chat_from_folder(chat_id, folder_id, cx);
                        })),
                );
            }
        }
        panel.into_any_element()
    }

    /// Phase 3.1: bot info panel rendered below the conversation header when
    /// the open chat is a bot chat with cached `botInfo` (lazy
    /// `getUserFullInfo` on chat open). Shows the bot description and its
    /// command list; tapping a command inserts it into the composer (the
    /// full `/` command menu is 3.3). Returns `None` when there is no bot
    /// info to show.
    fn bot_info_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let open = session.open_chat?;
        let info: BotInfo = session.bot_info_for_chat(open)?.clone();
        if info.description.is_empty() && info.commands.is_empty() {
            return None;
        }
        let mut panel = div()
            .id("bot-info")
            .flex()
            .flex_col()
            .gap_1()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Bot"),
            );
        if !info.description.is_empty() {
            panel = panel.child(div().text_sm().child(info.description.clone()));
        }
        if !info.commands.is_empty() {
            let mut row = div().id("bot-commands").flex().flex_wrap().gap_1();
            for command in &info.commands {
                let name = command.command.clone();
                let label = if command.description.is_empty() {
                    format!("/{name}")
                } else {
                    format!("/{name} — {}", command.description)
                };
                row = row.child(
                    Button::new(format!("bot-command-{name}"))
                        .label(label)
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.insert_bot_command(&name, window, cx);
                        })),
                );
            }
            panel = panel.child(row);
        }
        Some(panel.into_any_element())
    }

    /// Phase 3.1: insert a tapped bot command into the composer. Empty
    /// composer → the bare command; otherwise appended after a space (3.3
    /// owns the full `/` menu).
    fn insert_bot_command(&mut self, command: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            let next =
                quill::composer::insert_bot_command_text(&input.value().to_string(), command);
            input.set_value(next, window, cx);
        });
        self.sync_command_menu(cx);
    }

    fn pinned_message_banner(
        &self,
        message: &HistoryMessage,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let chat_id = message.chat_id;
        let message_id = message.id;
        let preview = message.content.preview();
        div()
            .id("pinned-message-bar")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(rgb(0x161b22))
            .child(
                div()
                    .id("pinned-message-jump")
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .flex_1()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.jump_to_pinned_message(message_id, cx);
                    }))
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(rgb(0x58a6ff))
                            .child("Pinned message"),
                    )
                    .child(div().text_sm().text_color(rgb(0xc9d1d9)).child(preview)),
            )
            .child(
                Button::new("unpin-banner")
                    .label("Unpin")
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.unpin_from_banner(chat_id, message_id, cx);
                    })),
            )
    }

    fn reaction_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let target = self.pending_react;
        let (chat_id, message_id) = target.unwrap_or((ChatId(0), MessageId(0)));
        let chosen: Vec<String> = self
            .session()
            .and_then(|session| {
                session
                    .histories
                    .get(&chat_id.0)
                    .and_then(|history| history.messages.get(&message_id.0))
                    .map(|message| {
                        DEFAULT_EMOJI_REACTIONS
                            .iter()
                            .filter(|emoji| message.chosen_emoji(emoji))
                            .map(|emoji| (*emoji).to_string())
                            .collect()
                    })
            })
            .unwrap_or_default();
        let mut row = div().id("reaction-emoji-row").flex().flex_wrap().gap_1();
        for emoji in DEFAULT_EMOJI_REACTIONS {
            let own = chosen.iter().any(|picked| picked == *emoji);
            let picked = (*emoji).to_string();
            row = row.child(
                Button::new(format!("react-pick-{picked}"))
                    .label(if own {
                        format!("{picked} · yours")
                    } else {
                        picked.clone()
                    })
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_emoji_reaction(chat_id, message_id, picked.clone(), cx);
                    })),
            );
        }
        div()
            .id("reaction-picker")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child("React"))
                    .child(
                        Button::new("close-reaction-picker")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_reaction_picker(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Tap an emoji to react. Tap yours again to remove."),
            )
            .child(row)
    }

    fn gif_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = self
            .session()
            .map(|session| session.gifs.clone())
            .unwrap_or_default();
        let files = self
            .session()
            .map(|session| session.files.clone())
            .unwrap_or_default();
        let roots = self.media_display_roots();
        let mut grid = div().id("gif-grid").flex().flex_wrap().gap_2();
        for (index, animation) in panel.animations.iter().enumerate() {
            let file_id = animation.file_id;
            let duration = animation.duration;
            let width = animation.width;
            let height = animation.height;
            let display_id = animation.thumb_file_id.filter(|id| id.0 != 0);
            let path = display_id.and_then(|id| {
                files
                    .get(&id.0)
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))
            });
            let label = if animation.file_name.is_empty() {
                "GIF".to_string()
            } else {
                animation.file_name.clone()
            };
            let cell_id = format!("gif-pick-{index}-{file_id}", file_id = file_id.0);
            let cell = if let Some(path) = path {
                img(path)
                    .id(SharedString::from(cell_id.clone()))
                    .w(px(96.))
                    .h(px(72.))
                    .rounded_md()
                    .object_fit(ObjectFit::Cover)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.send_gif_pick(file_id, duration, width, height, cx);
                    }))
                    .with_fallback({
                        let label = label.clone();
                        move || {
                            div()
                                .w(px(96.))
                                .h(px(72.))
                                .rounded_md()
                                .bg(rgb(0x1f6feb))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(label.clone())
                                .into_any_element()
                        }
                    })
                    .into_any_element()
            } else {
                div()
                    .id(SharedString::from(cell_id))
                    .w(px(96.))
                    .h(px(72.))
                    .rounded_md()
                    .bg(rgb(0x1f6feb))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.send_gif_pick(file_id, duration, width, height, cx);
                    }))
                    .child(label)
                    .into_any_element()
            };
            grid = grid.child(cell);
        }
        let status = if panel.loading {
            "Loading saved GIFs…"
        } else if panel.failed {
            "Could not load saved GIFs."
        } else if panel.animations.is_empty() {
            "No saved GIFs."
        } else {
            "Tap a GIF to send it."
        };
        div()
            .id("gif-picker")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child("GIFs"))
                    .child(
                        Button::new("close-gif-picker")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_gif_panel(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            )
            .child(grid)
    }

    fn sticker_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = self
            .session()
            .map(|session| session.stickers.clone())
            .unwrap_or_default();
        let files = self
            .session()
            .map(|session| session.files.clone())
            .unwrap_or_default();
        let roots = self.media_display_roots();
        let mut sets = div().id("sticker-set-row").flex().flex_wrap().gap_1();
        for set in &panel.sets {
            let set_id = set.id;
            let selected = panel.selected_set_id == Some(set_id);
            let title = if set.title.is_empty() {
                set.name.clone()
            } else {
                set.title.clone()
            };
            sets = sets.child(
                Button::new(format!("sticker-set-{set_id}"))
                    .label(if selected {
                        format!("{title} · open")
                    } else {
                        title
                    })
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_sticker_set(set_id, cx);
                    })),
            );
        }
        let mut grid = div().id("sticker-grid").flex().flex_wrap().gap_2();
        for sticker in &panel.stickers {
            let file_id = sticker.file_id;
            let emoji = sticker.emoji.clone();
            let width = sticker.width;
            let height = sticker.height;
            let thumb = sticker
                .thumb_file_id
                .filter(|id| id.0 != 0)
                .map(|id| (id, sticker.thumb_width, sticker.thumb_height));
            let display_id = sticker.thumb_file_id.filter(|id| id.0 != 0).or_else(|| {
                (sticker.format == quill::telegram::envelope::StickerFormat::Webp && file_id.0 != 0)
                    .then_some(file_id)
            });
            let path = display_id.and_then(|id| {
                files
                    .get(&id.0)
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))
            });
            let label = if emoji.is_empty() {
                "Sticker".to_string()
            } else {
                emoji.clone()
            };
            let cell_id = format!("sticker-pick-{}-{}", sticker.set_id, sticker.id);
            let cell = if let Some(path) = path {
                img(path)
                    .id(SharedString::from(cell_id.clone()))
                    .w(px(72.))
                    .h(px(72.))
                    .rounded_md()
                    .object_fit(ObjectFit::Contain)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.send_sticker_pick(file_id, emoji.clone(), width, height, thumb, cx);
                    }))
                    .with_fallback({
                        let label = label.clone();
                        move || {
                            div()
                                .w(px(72.))
                                .h(px(72.))
                                .rounded_md()
                                .bg(rgb(0x444c56))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(label.clone())
                                .into_any_element()
                        }
                    })
                    .into_any_element()
            } else {
                div()
                    .id(SharedString::from(cell_id))
                    .w(px(72.))
                    .h(px(72.))
                    .rounded_md()
                    .bg(rgb(0x21262d))
                    .border_1()
                    .border_color(rgb(0x8b949e))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.send_sticker_pick(file_id, emoji.clone(), width, height, thumb, cx);
                    }))
                    .child(label)
                    .into_any_element()
            };
            grid = grid.child(cell);
        }
        let status = if panel.loading_sets || panel.loading_set {
            "Loading installed sticker sets…"
        } else if panel.failed {
            "Could not load stickers."
        } else if panel.sets.is_empty() {
            "No installed sticker sets."
        } else if panel.stickers.is_empty() {
            "This set is empty."
        } else {
            "Tap a sticker to send it."
        };
        div()
            .id("sticker-picker")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child("Stickers"))
                    .child(
                        Button::new("close-sticker-picker")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_sticker_panel(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            )
            .child(sets)
            .child(grid)
    }

    fn composer_edit_banner(
        &self,
        edit: &ComposerEdit,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let preview = edit.original_text.clone();
        let kind = match edit.kind {
            quill::composer::ComposerEditKind::Text => "Editing message",
            quill::composer::ComposerEditKind::Caption => "Editing caption",
        };
        div()
            .id("composer-edit-header")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(0xd29922))
            .bg(rgb(0x21262d))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(rgb(0xd29922))
                            .child(kind),
                    )
                    .child(div().text_sm().text_color(rgb(0xc9d1d9)).child(preview)),
            )
            .child(
                Button::new("cancel-edit")
                    .label("Cancel")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.clear_edit(window, cx);
                    })),
            )
    }

    fn delete_confirm_banner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("delete-confirm")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(0xf85149))
            .bg(rgb(0x3d1f1f))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(rgb(0xf85149))
                            .child("Delete this message?"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xc9d1d9))
                            .child("Deletes for everyone (official desktop default)."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("cancel-delete")
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_delete(cx);
                            })),
                    )
                    .child(
                        Button::new("confirm-delete")
                            .label("Delete")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_delete(cx);
                            })),
                    ),
            )
    }

    /// Phase B1: "Close secret chat" confirm banner, styled like the
    /// delete confirm. Closing is permanent — the chat can never send
    /// again once `secretChatStateClosed` lands.
    fn close_secret_chat_confirm_banner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("close-secret-confirm")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(0xf85149))
            .bg(rgb(0x3d1f1f))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(rgb(0xf85149))
                            .child("Close this secret chat?"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xc9d1d9))
                            .child("Closing is permanent — the encrypted session ends and no new messages can be sent."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("cancel-close-secret")
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_close_secret_chat(cx);
                            })),
                    )
                    .child(
                        Button::new("confirm-close-secret")
                            .label("Close chat")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_close_secret_chat(cx);
                            })),
                    ),
            )
    }

    fn composer_reply_banner(
        &self,
        reply: &ComposerReplyTo,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let preview = reply.preview.clone();
        div()
            .id("composer-reply-quote")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x58a6ff))
            .bg(rgb(0x21262d))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(rgb(0x58a6ff))
                            .child("Replying to"),
                    )
                    .child(div().text_sm().text_color(rgb(0xc9d1d9)).child(preview)),
            )
            .child(
                Button::new("cancel-reply")
                    .label("Cancel")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.clear_reply(cx);
                    })),
            )
    }

    /// Phase 4.2: the poll creation dialog, rendered above the composer.
    /// Question field, dynamic option rows (2–10), anonymous / multiple-answers
    /// toggles, Create / Cancel. Quiz correct-option marking stays out of this
    /// slice (documented in DECISIONS.md).
    fn poll_dialog_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.poll_dialog.as_ref()?;
        let mut panel = div()
            .id("poll-dialog")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x58a6ff))
            .bg(rgb(0x161b22))
            .child(div().text_sm().font_semibold().child("New poll"))
            .child(Textarea::new(&dialog.question_input).h(px(64.)));
        for (index, input) in dialog.option_inputs.iter().enumerate() {
            let mut row = div()
                .id(("poll-dialog-option", index as u64))
                .flex()
                .items_center()
                .gap_2()
                .child(div().flex_1().child(Textarea::new(input).h(px(40.))));
            if dialog.option_inputs.len() > POLL_OPTIONS_MIN {
                row = row.child(
                    Button::new(format!("poll-remove-option-{index}"))
                        .label("✕")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove_poll_option_row(index, cx);
                        })),
                );
            }
            panel = panel.child(row);
        }
        if dialog.option_inputs.len() < POLL_OPTIONS_MAX {
            panel = panel.child(
                Button::new("poll-add-option")
                    .label("Add option")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.add_poll_option_row(window, cx);
                    })),
            );
        }
        let anonymous = dialog.is_anonymous;
        let multiple = dialog.allows_multiple_answers;
        panel =
            panel
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("poll-toggle-anonymous")
                                .label(if anonymous {
                                    "☑ Anonymous voting"
                                } else {
                                    "☐ Anonymous voting"
                                })
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(dialog) = this.poll_dialog.as_mut() {
                                        dialog.is_anonymous = !dialog.is_anonymous;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("poll-toggle-multiple")
                                .label(if multiple {
                                    "☑ Multiple answers"
                                } else {
                                    "☐ Multiple answers"
                                })
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(dialog) = this.poll_dialog.as_mut() {
                                        dialog.allows_multiple_answers =
                                            !dialog.allows_multiple_answers;
                                    }
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(Button::new("poll-create").label("Create poll").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.submit_poll_dialog(window, cx);
                            }),
                        ))
                        .child(Button::new("poll-cancel").label("Cancel").ghost().on_click(
                            cx.listener(|this, _, _, cx| {
                                this.close_poll_dialog(cx);
                            }),
                        )),
                );
        Some(panel.into_any_element())
    }

    fn forward_success_banner(
        &self,
        result: &ForwardResult,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let label = result.success_label();
        let detail = format!(
            "{} → {} (ids {})",
            result.from_chat_id.0,
            result.dest_title,
            result
                .forwarded_ids
                .iter()
                .map(|id| id.0.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        div()
            .id("forward-success")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x3fb950))
            .bg(rgb(0x1b3324))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(rgb(0x3fb950))
                            .child(label),
                    )
                    .child(div().text_sm().text_color(rgb(0xc9d1d9)).child(detail)),
            )
            .child(
                Button::new("dismiss-forward-success")
                    .label("Dismiss")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.forward_result = None;
                        cx.notify();
                    })),
            )
    }

    fn forward_selection_banner(
        &self,
        draft: &ForwardDraft,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let count = draft.count();
        let label = if count == 1 {
            "1 message selected".to_string()
        } else {
            format!("{count} messages selected")
        };
        div()
            .id("forward-selection")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x58a6ff))
            .bg(rgb(0x21262d))
            .child(div().text_sm().text_color(rgb(0xc9d1d9)).child(label))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("clear-forward-selection")
                            .label("Clear")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.clear_forward(window, cx);
                            })),
                    )
                    .child(
                        Button::new("open-forward-picker")
                            .label("Forward")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_forward_picker(window, cx);
                            })),
                    ),
            )
    }

    /// Phase 4.5: fullscreen media viewer overlay. The backdrop is a
    /// separate sibling painted behind the panel (not an ancestor), so a
    /// click on the panel never bubbles into the backdrop's close handler —
    /// "click outside" works regardless of click-bubbling semantics. Esc
    /// closes through `cancel_search`; the header close button is the third
    /// path. Videos show their thumbnail (no in-viewer playback — the
    /// history row's Play path is unchanged); secret/spoiler media never
    /// reach the viewer (filtered in `collect_media_items`).
    fn media_viewer_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let item = self
            .media_viewer
            .current()
            .cloned()
            .unwrap_or_else(|| MediaViewerItem {
                chat_id: ChatId(0),
                message_id: MessageId(0),
                kind: MediaViewerKind::Photo,
                display_file_ids: Vec::new(),
                download_file_id: FileId(0),
                play_file_id: None,
                duration_secs: None,
                mime_type: None,
                start_timestamp: None,
                caption: String::new(),
                caption_entities: Vec::new(),
                duration_label: None,
            });
        let (position, total) = self.media_viewer.position().unwrap_or((0, 0));
        let files: HashMap<i32, ParsedFile> =
            self.session().map(|s| s.files.clone()).unwrap_or_default();
        let downloading: HashSet<i32> = self
            .session()
            .map(|s| s.downloading.clone())
            .unwrap_or_default();
        let roots = self.media_display_roots();
        let thumb_path = viewer_display_path(&item, &files, &roots);
        // Parity slice 5: when the clip's frames are extracted and decoded,
        // the viewer shows the pre-loaded frame matching the playback clock
        // — real in-viewer video (`ImageSource::Render` resolves
        // synchronously, so the 125 ms tick animates without a per-frame
        // async load). Otherwise it falls back to the thumbnail (or the
        // loading status).
        let frame: Option<Arc<RenderImage>> =
            if item.kind == MediaViewerKind::Video && !self.viewer_video_frames.is_empty() {
                let elapsed = self
                    .viewer_clock
                    .as_ref()
                    .map(|clock| clock.elapsed_secs())
                    .unwrap_or(0.0);
                // Clamp, don't wrap: frames cover (duration − start_timestamp),
                // so the tail of the clock holds the last frame instead of
                // replaying early frames.
                let idx = ((elapsed * self.viewer_video_fps) as usize)
                    .min(self.viewer_video_frames.len() - 1);
                self.viewer_video_frames.get(idx).cloned()
            } else {
                None
            };
        let row_id = item.message_id.0 as u64;
        let downloading_now = item
            .display_file_ids
            .iter()
            .chain(std::iter::once(&item.download_file_id))
            .any(|id| file_is_downloading(*id, &files, &downloading));
        let kind_label = item.kind.label();
        let header_label = if total > 1 {
            format!("{kind_label} {position} of {total}")
        } else {
            kind_label.to_string()
        };
        // Parity slice 5: the visual lives in a fixed 720×480 frame; scroll
        // zooms (1×–8×, frame-center kept) and drag pans when zoomed.
        let zoom = self.viewer_zoom;
        let (frame_w, frame_h) = Self::VIEWER_FRAME;
        let (zoom_w, zoom_h) = (frame_w * zoom.zoom, frame_h * zoom.zoom);
        let (pan_x, pan_y) = zoom.pan;
        let content: AnyElement = {
            // Pre-decoded video frame and thumbnail both render through
            // `img`; the frame is an `ImageSource::Render` (synchronous),
            // the thumbnail a path (async-loaded once, then cached).
            let source: Option<ImageSource> = frame
                .map(ImageSource::from)
                .or_else(|| thumb_path.map(ImageSource::from));
            if let Some(source) = source {
                img(source)
                    .id(("media-viewer-img", row_id))
                    .w(px(zoom_w))
                    .h(px(zoom_h))
                    .object_fit(ObjectFit::Contain)
                    .bg(rgb(0x0d1117))
                    .with_fallback(move || {
                        div()
                            .w(px(zoom_w))
                            .h(px(zoom_h))
                            .bg(rgb(0x0d1117))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(0xffffff))
                            .child(format!("{kind_label} — could not render"))
                            .into_any_element()
                    })
                    .into_any_element()
            } else {
                let status = match (&item.duration_label, downloading_now) {
                    (Some(duration), true) => format!("Video · {duration} — downloading…"),
                    (Some(duration), false) => format!("Video · {duration} — not downloaded"),
                    (None, true) => format!("{kind_label} — downloading…"),
                    (None, false) => format!("{kind_label} — not downloaded"),
                };
                div()
                    .id(("media-viewer-loading", row_id))
                    .w(px(zoom_w))
                    .h(px(zoom_h))
                    .bg(rgb(0x0d1117))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().text_sm().text_color(rgb(0xffffff)).child(status))
                    .into_any_element()
            }
        };
        let visual = {
            let view = cx.entity().downgrade();
            let scroll_view = view.clone();
            let down_view = view.clone();
            let move_view = view.clone();
            let up_view = view;
            div()
                .id(("media-viewer-visual", row_id))
                .relative()
                .w(px(frame_w))
                .h(px(frame_h))
                .overflow_hidden()
                .rounded_md()
                .bg(rgb(0x0d1117))
                .child(
                    div()
                        .absolute()
                        .left(px(pan_x))
                        .top(px(pan_y))
                        .w(px(zoom_w))
                        .h(px(zoom_h))
                        .child(content),
                )
                .on_scroll_wheel(move |event, _window, cx| {
                    let dy = match event.delta {
                        ScrollDelta::Pixels(p) => f32::from(p.y),
                        ScrollDelta::Lines(l) => l.y,
                    };
                    if let Some(view) = scroll_view.upgrade() {
                        view.update(cx, |this, cx| this.viewer_zoom_scroll(dy, cx));
                    }
                })
                .on_mouse_down(MouseButton::Left, move |event, _window, cx| {
                    let pos = (f32::from(event.position.x), f32::from(event.position.y));
                    if let Some(view) = down_view.upgrade() {
                        view.update(cx, |this, cx| {
                            this.viewer_drag = Some(pos);
                            cx.notify();
                        });
                    }
                })
                .on_mouse_move(move |event, _window, cx| {
                    let pos = (f32::from(event.position.x), f32::from(event.position.y));
                    if let Some(view) = move_view.upgrade() {
                        view.update(cx, |this, cx| {
                            if let Some((lx, ly)) = this.viewer_drag {
                                this.viewer_pan_drag(pos.0 - lx, pos.1 - ly, cx);
                                this.viewer_drag = Some(pos);
                            }
                        });
                    }
                })
                .on_mouse_up(MouseButton::Left, move |_event, _window, cx| {
                    if let Some(view) = up_view.upgrade() {
                        view.update(cx, |this, cx| {
                            this.viewer_drag = None;
                            cx.notify();
                        });
                    }
                })
                // Double-click resets zoom/pan to fit.
                .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                    if event.click_count() >= 2 {
                        this.viewer_reset_zoom(cx);
                    }
                }))
        };
        // Parity slice 5: video transport under the visual. ffplay runs
        // `-nodisp` for audio only (no GPUI video element in this stack);
        // the decoded video frames render in-viewer above. The overlay
        // shows Play/Pause plus elapsed/total, or a download CTA while the
        // clip is not local.
        let video_controls: Option<AnyElement> =
            (item.kind == MediaViewerKind::Video).then(|| {
                let clip_path = self.viewer_clip_path(&item);
                if let Some(_clip) = clip_path {
                    let playing = self
                        .viewer_clock
                        .as_ref()
                        .is_some_and(|clock| clock.is_playing())
                        && self.viewer_video == Some(item.message_id);
                    let elapsed = self
                        .viewer_clock
                        .as_ref()
                        .map(|clock| clock.elapsed_secs())
                        .unwrap_or(0.0);
                    let total = item.duration_secs.unwrap_or(0) as f64;
                    let label = format!(
                        "{} / {}",
                        format_voice_duration(elapsed as i32),
                        format_voice_duration(total as i32)
                    );
                    // While ffmpeg extracts frames the thumbnail stays up;
                    // the Play button appears once frames are ready.
                    let extracting = self.viewer_extracting;
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(if extracting {
                            div()
                                .text_sm()
                                .text_color(rgb(0xffffff))
                                .child("Loading video…")
                                .into_any_element()
                        } else {
                            Button::new(("media-viewer-play", row_id))
                                .label(if playing { "❚❚ Pause" } else { "▶ Play" })
                                .ghost()
                                .text_color(rgb(0xffffff))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_viewer_video(cx);
                                }))
                                .into_any_element()
                        })
                        .child(div().text_sm().text_color(rgb(0xffffff)).child(label))
                        .into_any_element()
                } else {
                    let clip_downloading = item
                        .play_file_id
                        .is_some_and(|id| file_is_downloading(id, &files, &downloading));
                    let play_id = item.play_file_id;
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_sm().text_color(rgb(0xffffff)).child(
                            if clip_downloading {
                                "Video — downloading clip…"
                            } else {
                                "Video — clip not downloaded"
                            },
                        ))
                        .when_some(play_id.filter(|_| !clip_downloading), |this, id| {
                            this.child(
                                Button::new(("media-viewer-download", row_id))
                                    .label("Download")
                                    .ghost()
                                    .text_color(rgb(0xffffff))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.viewer_pending_play = Some((item.message_id, id));
                                        this.request_media_download(id, None, cx);
                                    })),
                            )
                        })
                        .into_any_element()
                }
            });
        // Parity slice 5: zoom controls share a row with the video
        // transport — − / % / + / Reset, then Play/Pause + elapsed/total.
        let zoom_pct = format!("{}%", (zoom.zoom * 100.0).round() as i32);
        let mut transport = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new(("media-viewer-zoom-out", row_id))
                    .label("−")
                    .ghost()
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.viewer_zoom_step(false, cx);
                    })),
            )
            .child(div().text_sm().text_color(rgb(0xffffff)).child(zoom_pct))
            .child(
                Button::new(("media-viewer-zoom-in", row_id))
                    .label("+")
                    .ghost()
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.viewer_zoom_step(true, cx);
                    })),
            )
            .child(
                Button::new(("media-viewer-zoom-reset", row_id))
                    .label("Reset")
                    .ghost()
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.viewer_reset_zoom(cx);
                    })),
            );
        if let Some(controls) = video_controls {
            transport = transport.child(controls);
        }
        let transport = transport.into_any_element();
        let caption: Option<AnyElement> = (!item.caption.is_empty()).then(|| {
            rich_text_line(
                &item.caption,
                &item.caption_entities,
                (item.chat_id.0, row_id),
                true,
                &self.spoiler_revealed,
                cx,
            )
        });
        div()
            .id("media-viewer-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("media-viewer-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .bg(rgba(0x000000e6))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_media_viewer(cx);
                    })),
            )
            .child(
                div()
                    .id("media-viewer-panel")
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .p_4()
                    .max_w(px(800.))
                    .max_h_full()
                    .child(
                        div()
                            .flex()
                            .w(px(720.))
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .font_semibold()
                                    .text_color(rgb(0xffffff))
                                    .child(header_label),
                            )
                            .child(
                                div()
                                    .id("media-viewer-close")
                                    .cursor_pointer()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .text_color(rgb(0xffffff))
                                    .child("Close")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_media_viewer(cx);
                                    })),
                            ),
                    )
                    .child(visual)
                    .child(transport)
                    .when_some(caption, |this, caption| {
                        this.child(div().text_color(rgb(0xffffff)).child(caption))
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("media-viewer-prev")
                                    .label("‹ Prev")
                                    .ghost()
                                    .text_color(rgb(0xffffff))
                                    .disabled(position <= 1)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.step_media_viewer(-1, cx);
                                    })),
                            )
                            .child(
                                Button::new("media-viewer-next")
                                    .label("Next ›")
                                    .ghost()
                                    .text_color(rgb(0xffffff))
                                    .disabled(position >= total)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.step_media_viewer(1, cx);
                                    })),
                            ),
                    ),
            )
    }

    /// Phase 9.2: the viewer's own-story interaction counters
    /// (`storyInteractionInfo`, `schema/td_api.tl:6712`) — only rendered
    /// when TDLib populated them (`story.can_get_interactions`) and at
    /// least one counter is nonzero.
    fn story_viewer_counts(&self) -> Option<String> {
        let story = self.current_story()?;
        if !story.can_get_interactions {
            return None;
        }
        let info = story.interaction_info.filter(|info| info.any_nonzero())?;
        let mut parts = Vec::new();
        if info.view_count > 0 {
            parts.push(format!("👁 {}", info.view_count));
        }
        if info.reaction_count > 0 {
            parts.push(format!("❤️ {}", info.reaction_count));
        }
        if info.forward_count > 0 {
            parts.push(format!("↩ {}", info.forward_count));
        }
        Some(parts.join(" · "))
    }

    /// Phase 9.2: the emoji picker popover fed by `getStoryAvailableReactions`
    /// (`availableReactions`, `schema/td_api.tl:13802`). One tap sets the
    /// reaction via `setStoryReaction` (`schema/td_api.tl:13809`).
    fn story_reaction_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let reactions = self
            .session()
            .and_then(|session| session.story_available_reactions.clone())
            .unwrap_or_default();
        let mut picker = div()
            .id("story-reaction-picker")
            .flex()
            .flex_row()
            .flex_wrap()
            .justify_center()
            .gap_1()
            .max_w(px(360.))
            .p_2()
            .rounded_md()
            .bg(rgb(0x161b22))
            .border_1()
            .border_color(rgb(0x30363d));
        if reactions.is_empty() {
            picker = picker.child(
                div()
                    .text_sm()
                    .text_color(rgb(0x8b949e))
                    .child("Loading reactions…"),
            );
        }
        for (index, reaction) in reactions.iter().enumerate() {
            let emoji = reaction.emoji.clone();
            picker = picker.child(
                div()
                    .id(("story-reaction-option", index))
                    .cursor_pointer()
                    .text_2xl()
                    .p_1()
                    .child(emoji.clone())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pick_story_reaction(&emoji, cx);
                    })),
            );
        }
        picker.into_any_element()
    }

    /// Phase 9.2: reaction / reply / delete affordances under the viewer
    /// visual, plus the reaction picker popover and the reply input row.
    /// The quick-react toggles ❤; Reply is gated on
    /// `story.can_be_replied`; Delete on `story.can_be_deleted`.
    fn story_action_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let story = self.current_story();
        let chosen = story
            .as_ref()
            .and_then(|story| story.chosen_reaction_emoji.clone());
        let can_reply = story.as_ref().is_some_and(|story| story.can_be_replied);
        let can_delete = story.as_ref().is_some_and(|story| story.can_be_deleted);
        let quick_label = if chosen.as_deref() == Some("❤") {
            "❤️ ✓"
        } else {
            "❤️"
        };
        let mut row = div().flex().gap_2().items_center().justify_center();
        row = row.child(
            Button::new("story-quick-react")
                .label(quick_label)
                .ghost()
                .text_color(rgb(0xffffff))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.quick_react_story(cx);
                })),
        );
        row = row.child(
            Button::new("story-react-picker")
                .label("React…")
                .ghost()
                .text_color(rgb(0xffffff))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_story_reaction_picker(cx);
                })),
        );
        if can_reply {
            row = row.child(
                Button::new("story-reply")
                    .label("Reply")
                    .ghost()
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_story_reply(cx);
                    })),
            );
        }
        if can_delete {
            row = row.child(
                Button::new("story-delete")
                    .label("Delete")
                    .ghost()
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.delete_story_viewer(cx);
                    })),
            );
        }
        let mut column = div().flex().flex_col().gap_2().items_center().child(row);
        if self.story_reaction_picker_open {
            column = column.child(self.story_reaction_picker(cx));
        }
        if self.story_reply_open {
            column =
                column.child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .w(px(360.))
                        .child(
                            div()
                                .flex_1()
                                .child(Textarea::new(&self.story_reply_input).h(px(40.))),
                        )
                        .child(Button::new("story-reply-send").label("Send").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.send_story_reply(window, cx);
                            }),
                        )),
                );
        }
        column.into_any_element()
    }

    /// Phase 9.1: fullscreen story overlay, modeled on
    /// `media_viewer_overlay`: poster name + "Story N of M" header, the
    /// photo (video shows its thumbnail; live/unsupported show a
    /// placeholder), the caption, and Prev / Next / Close controls. The
    /// backdrop click and Escape (see `cancel_search`) close it.
    fn story_viewer_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let item = self
            .story_viewer
            .current()
            .cloned()
            .unwrap_or_else(|| StoryViewerItem {
                chat_id: ChatId(0),
                story_id: 0,
                kind: StoryViewerKind::Unsupported,
                display_file_ids: Vec::new(),
                download_file_id: FileId(0),
                caption: String::new(),
                caption_entities: Vec::new(),
                duration_label: None,
                is_live: false,
            });
        let (position, total) = self.story_viewer.position().unwrap_or((0, 0));
        let poster = self
            .session()
            .and_then(|s| s.chats.get(&item.chat_id.0))
            .map(|chat| chat.title.clone())
            .unwrap_or_else(|| format!("Chat {}", item.chat_id.0));
        let files: HashMap<i32, ParsedFile> =
            self.session().map(|s| s.files.clone()).unwrap_or_default();
        let downloading: HashSet<i32> = self
            .session()
            .map(|s| s.downloading.clone())
            .unwrap_or_default();
        let roots = self.media_display_roots();
        let path = story_viewer_display_path(&item, &files, &roots);
        let downloading_now = item
            .display_file_ids
            .iter()
            .chain(std::iter::once(&item.download_file_id))
            .any(|id| file_is_downloading(*id, &files, &downloading));
        let kind_label = item.kind.label();
        let header_label = if total > 1 {
            format!("{poster} · {kind_label} {position} of {total}")
        } else {
            format!("{poster} · {kind_label}")
        };
        let visual: AnyElement = if let Some(path) = path {
            img(path)
                .id(("story-viewer-img", item.story_id as u64))
                .w(px(360.))
                .h(px(640.))
                .rounded_md()
                .object_fit(ObjectFit::Contain)
                .bg(rgb(0x0d1117))
                .with_fallback(move || {
                    div()
                        .w(px(360.))
                        .h(px(640.))
                        .rounded_md()
                        .bg(rgb(0x0d1117))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(0xffffff))
                        .child(format!("{kind_label} — could not render"))
                        .into_any_element()
                })
                .into_any_element()
        } else {
            let status = if downloading_now {
                format!("{kind_label} — downloading…")
            } else {
                format!("{kind_label} — not downloaded")
            };
            let status = match (&item.duration_label, downloading_now) {
                (Some(duration), _) => format!("Video · {duration} — {status}"),
                _ => status,
            };
            let status = if matches!(
                item.kind,
                StoryViewerKind::Live | StoryViewerKind::Unsupported
            ) {
                format!("{kind_label} — not supported in this slice")
            } else {
                status
            };
            div()
                .id(("story-viewer-loading", item.story_id as u64))
                .w(px(360.))
                .h(px(640.))
                .rounded_md()
                .bg(rgb(0x0d1117))
                .flex()
                .items_center()
                .justify_center()
                .child(div().text_sm().text_color(rgb(0xffffff)).child(status))
                .into_any_element()
        };
        let caption: Option<AnyElement> = (!item.caption.is_empty()).then(|| {
            rich_text_line(
                &item.caption,
                &item.caption_entities,
                (item.chat_id.0, item.story_id as u64),
                true,
                &self.spoiler_revealed,
                cx,
            )
        });
        // Phase 9.2: own-story interaction counters under the caption.
        let counts: Option<String> = self.story_viewer_counts();
        div()
            .id("story-viewer-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("story-viewer-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .bg(rgba(0x000000e6))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_story_viewer(cx);
                    })),
            )
            .child(
                div()
                    .id("story-viewer-panel")
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .p_4()
                    .max_w(px(480.))
                    .max_h_full()
                    .child(
                        div()
                            .flex()
                            .w(px(360.))
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .font_semibold()
                                    .text_color(rgb(0xffffff))
                                    .child(header_label),
                            )
                            .child(
                                div()
                                    .id("story-viewer-close")
                                    .cursor_pointer()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .text_color(rgb(0xffffff))
                                    .child("Close")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_story_viewer(cx);
                                    })),
                            ),
                    )
                    .child(visual)
                    .when_some(caption, |this, caption| {
                        this.child(div().text_color(rgb(0xffffff)).child(caption))
                    })
                    .when_some(counts, |this, counts| {
                        this.child(div().text_xs().text_color(rgb(0x8b949e)).child(counts))
                    })
                    .child(self.story_action_row(cx))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("story-viewer-prev")
                                    .label("‹ Prev")
                                    .ghost()
                                    .text_color(rgb(0xffffff))
                                    .disabled(position <= 1)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.step_story_viewer(-1, cx);
                                    })),
                            )
                            .child(
                                Button::new("story-viewer-next")
                                    .label("Next ›")
                                    .ghost()
                                    .text_color(rgb(0xffffff))
                                    .disabled(position >= total)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.step_story_viewer(1, cx);
                                    })),
                            ),
                    ),
            )
    }

    fn forward_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.forward_search_input.read(cx).value().to_string();
        let draft = self.pending_forward.clone();
        let count = draft.as_ref().map(|d| d.count()).unwrap_or(0);
        let from_title = draft
            .as_ref()
            .and_then(|d| {
                self.session()
                    .and_then(|s| s.chats.get(&d.from_chat_id.0).map(|c| c.title.clone()))
            })
            .unwrap_or_else(|| "this chat".into());
        let dests: Vec<(ChatId, String)> = self
            .session()
            .map(|session| {
                session
                    .forward_destinations(&query)
                    .into_iter()
                    .map(|chat| (chat.id, chat.title.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let heading = if count == 1 {
            format!("Forward 1 message from {from_title}")
        } else {
            format!("Forward {count} messages from {from_title}")
        };
        let mut list = div()
            .id("forward-dest-list")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(220.));
        if dests.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No matching loaded chats."),
            );
        } else {
            for (id, title) in dests {
                list = list.child(forward_dest_row(id, title, cx));
            }
        }
        div()
            .id("forward-picker")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x58a6ff))
            .bg(rgb(0x161b22))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(rgb(0x58a6ff))
                            .child("Forward to…"),
                    )
                    .child(
                        Button::new("cancel-forward-picker")
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_forward_picker(window, cx);
                            })),
                    ),
            )
            .child(div().text_xs().text_color(rgb(0xc9d1d9)).child(heading))
            .child(Textarea::new(&self.forward_search_input).h(px(36.)))
            .child(list)
    }

    fn jump_to_replied_message(&mut self, message_id: MessageId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.jump_to_replied_message(message_id) {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let _ = session.begin_chat_search_jump(message_id);
            self.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    fn open_chat_search_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pane_mode() != PaneMode::Ready {
            return;
        }
        let opened = if let Some(live) = self.live.as_mut() {
            match live.driver.open_chat_search() {
                Ok(true) => {
                    self.status_note = "search in chat".into();
                    true
                }
                Ok(false) => {
                    self.status_note = "select a chat to search in conversation".into();
                    false
                }
                Err(_) => {
                    self.status_note = "could not search in chat".into();
                    false
                }
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            if session.open_chat_search() {
                self.status_note = "search in chat".into();
                true
            } else {
                self.status_note = "select a chat to search in conversation".into();
                false
            }
        } else {
            false
        };
        if opened {
            self.chat_search_input
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    fn close_chat_search_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_chat_search();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.close_chat_search();
        }
        self.chat_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.status_note = "in-chat search closed".into();
        cx.notify();
    }

    fn sync_chat_search_query(&mut self, query: &str, cx: &mut Context<Self>) {
        if self.pane_mode() != PaneMode::Ready || !self.chat_search_is_open() {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.set_chat_search_query(query) {
                Ok(ChatSearchQueryOutcome::Sent(_)) => {
                    self.status_note = "searching in chat…".into()
                }
                Ok(ChatSearchQueryOutcome::Debounced { token }) => {
                    self.schedule_chat_search_commit(token, cx);
                }
                Ok(ChatSearchQueryOutcome::Unchanged) if query.trim().is_empty() => {
                    self.status_note = "search in chat".into();
                }
                Ok(ChatSearchQueryOutcome::Unchanged) => {}
                Err(_) => self.status_note = "could not search in chat".into(),
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            let trimmed = query.trim();
            if session.chat_search.open
                && session.chat_search.query == trimmed
                && !matches!(session.chat_search.status, SearchStatus::Closed)
            {
                cx.notify();
                return;
            }
            session.apply_local_chat_search_filter(query);
        }
        cx.notify();
    }

    fn schedule_chat_search_commit(&mut self, token: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            this.update(cx, |this, cx| {
                if let Some(live) = this.live.as_mut() {
                    match live.driver.commit_debounced_chat_search(token) {
                        Ok(Some(_)) => this.status_note = "searching in chat…".into(),
                        Ok(None) => {}
                        Err(_) => this.status_note = "could not search in chat".into(),
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn jump_selected_chat_search_hit(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.jump_selected_chat_search_hit() {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(id) = session.chat_search.selected_hit().map(|hit| hit.message_id) {
                let _ = session.begin_chat_search_jump(id);
            }
            self.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    fn jump_chat_search_message(&mut self, message_id: MessageId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.jump_to_chat_search_message(message_id) {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let _ = session.begin_chat_search_jump(message_id);
            self.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    fn chat_search_newer(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.chat_search_newer() {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(id) = session.chat_search.select_newer() {
                let _ = session.begin_chat_search_jump(id);
            }
            self.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    fn chat_search_older(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.chat_search_older() {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(id) = session.chat_search.select_older() {
                let _ = session.begin_chat_search_jump(id);
            }
            self.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    fn chat_search_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let status = session
            .map(|s| s.chat_search.status)
            .unwrap_or(SearchStatus::Closed);
        let query = session
            .map(|s| s.chat_search.query.clone())
            .unwrap_or_default();
        let position = session
            .map(|s| s.chat_search.position_label())
            .unwrap_or_default();
        let jump_note = session.map(chat_search_jump_note).unwrap_or_default();
        let hits: Vec<(MessageId, String, bool)> = session
            .map(|s| {
                s.chat_search
                    .hits
                    .iter()
                    .enumerate()
                    .map(|(i, hit)| {
                        (
                            hit.message_id,
                            hit.preview.clone(),
                            s.chat_search.selected == Some(i),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let caption = match status {
            SearchStatus::Idle => "Type to search this chat.".to_string(),
            SearchStatus::Searching => format!("Searching “{query}”…"),
            SearchStatus::Ready => {
                if jump_note.is_empty() {
                    format!("Results for “{query}”")
                } else {
                    format!("Results for “{query}” · {jump_note}")
                }
            }
            SearchStatus::Empty => format!("No messages match “{query}”."),
            SearchStatus::Failed => "Search in chat failed.".to_string(),
            SearchStatus::Closed => String::new(),
        };
        div()
            .id("chat-search")
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("chat-search-field")
                            .flex_1()
                            .child(Textarea::new(&self.chat_search_input).h(px(36.))),
                    )
                    .child(
                        Button::new("chat-search-close")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_chat_search_ui(window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("chat-search-older")
                            .label("Older")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.chat_search_older(cx);
                            })),
                    )
                    .child(
                        Button::new("chat-search-newer")
                            .label("Newer")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.chat_search_newer(cx);
                            })),
                    )
                    .when(!position.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(position),
                        )
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(caption),
            )
            .when(!hits.is_empty(), |this| {
                let mut list = div().id("chat-search-hits").flex().flex_col().gap_1();
                for (message_id, preview, selected) in hits {
                    list = list.child(search_result_row(
                        ("chat-search-hit", message_id.0 as u64),
                        preview,
                        if selected {
                            "Jump · selected".into()
                        } else {
                            "Jump".into()
                        },
                        cx,
                        move |this, _window, cx| {
                            this.jump_chat_search_message(message_id, cx);
                        },
                    ));
                }
                this.child(list)
            })
    }

    fn sync_search_query(&mut self, query: &str, cx: &mut Context<Self>) {
        if self.pane_mode() != PaneMode::Ready {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.set_search_query(query) {
                Ok(SearchQueryOutcome::Sent(_)) => self.status_note = "searching…".into(),
                Ok(SearchQueryOutcome::Debounced { token }) => {
                    self.schedule_search_commit(token, cx);
                }
                Ok(SearchQueryOutcome::Unchanged) if query.trim().is_empty() => {
                    self.status_note = "search chats and messages".into();
                }
                Ok(SearchQueryOutcome::Unchanged) => {}
                Err(_) => self.status_note = "could not search".into(),
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            let trimmed = query.trim();
            if session.search.open
                && session.search.query == trimmed
                && !matches!(session.search.status, SearchStatus::Closed)
            {
                cx.notify();
                return;
            }
            session.apply_local_search_filter(query);
        }
        cx.notify();
    }

    fn schedule_search_commit(&mut self, token: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            this.update(cx, |this, cx| {
                if let Some(live) = this.live.as_mut() {
                    match live.driver.commit_debounced_search(token) {
                        Ok(Some(_)) => this.status_note = "searching…".into(),
                        Ok(None) => {}
                        Err(_) => this.status_note = "could not search".into(),
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn activate_first_search_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chat = self
            .session()
            .and_then(|session| session.search.chat_ids.first().copied());
        let message = self.session().and_then(|session| {
            session
                .search
                .messages
                .first()
                .map(|hit| (hit.chat_id, hit.message_id))
        });
        if let Some(chat_id) = chat {
            self.select_search_chat(chat_id, window, cx);
        } else if let Some((chat_id, message_id)) = message {
            self.select_search_message(chat_id, message_id, window, cx);
        }
    }

    fn select_search_chat(&mut self, chat_id: ChatId, window: &mut Window, cx: &mut Context<Self>) {
        let (text, reply, now_ms) = self.leaving_draft_parts(cx);
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live
                .driver
                .select_search_chat(chat_id, &text, reply, now_ms)
            {
                Ok(_) => "chat selected".into(),
                Err(_) => "could not open chat".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(prev) = session.open_chat {
                if prev != chat_id {
                    let stored = draft_text_to_store(&text, reply.is_some()).map(str::to_string);
                    let reply_to = stored.as_ref().and(reply);
                    let draft = stored.map(|body| ChatDraft {
                        text: body,
                        reply_to_message_id: reply_to,
                    });
                    session.store_composer_draft(prev, draft);
                }
            }
            session.close_search();
            session.open_chat(chat_id);
            self.status_note = "chat selected".into();
        }
        self.dismiss_cross_chat_state(chat_id, cx);
        self.restore_open_draft(window, cx);
        self.search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    fn select_search_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (text, reply, now_ms) = self.leaving_draft_parts(cx);
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live
                .driver
                .select_search_message(chat_id, message_id, &text, reply, now_ms)
            {
                Ok(_) => "opened chat".into(),
                Err(_) => "could not open chat".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(prev) = session.open_chat {
                if prev != chat_id {
                    let stored = draft_text_to_store(&text, reply.is_some()).map(str::to_string);
                    let reply_to = stored.as_ref().and(reply);
                    let draft = stored.map(|body| ChatDraft {
                        text: body,
                        reply_to_message_id: reply_to,
                    });
                    session.store_composer_draft(prev, draft);
                }
            }
            session.promote_search_message(chat_id, message_id);
            session.close_search();
            session.open_chat(chat_id);
            self.status_note = "opened chat".into();
        }
        self.dismiss_cross_chat_state(chat_id, cx);
        self.restore_open_draft(window, cx);
        self.search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    fn sidebar_search_field(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("sidebar-search")
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id("sidebar-search-field")
                    .flex_1()
                    .on_click(cx.listener(|this, _, window, cx| {
                        if !this.search_is_open() {
                            this.open_search_ui(window, cx);
                        }
                    }))
                    .child(Textarea::new(&self.search_input).h(px(40.))),
            )
            .when(self.search_is_open(), |this| {
                this.child(Button::new("search-clear").label("Clear").ghost().on_click(
                    cx.listener(|this, _, window, cx| {
                        this.cancel_search(window, cx);
                    }),
                ))
            })
    }

    fn search_results(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let status = session
            .map(|s| s.search.status)
            .unwrap_or(SearchStatus::Closed);
        let recents = session.is_some_and(|s| s.search.recents);
        let query = session.map(|s| s.search.query.clone()).unwrap_or_default();
        let chat_ids: Vec<ChatId> = session
            .map(|s| s.search.chat_ids.clone())
            .unwrap_or_default();
        let messages: Vec<(ChatId, MessageId, String, String)> = session
            .map(|s| {
                s.search
                    .messages
                    .iter()
                    .map(|hit| {
                        let title = s
                            .chats
                            .get(&hit.chat_id.0)
                            .map(|c| c.title.clone())
                            .unwrap_or_else(|| format!("chat {}", hit.chat_id.0));
                        (hit.chat_id, hit.message_id, title, hit.preview.clone())
                    })
                    .collect()
            })
            .unwrap_or_default();
        let chats: Vec<(ChatId, String, String)> = session
            .map(|s| {
                chat_ids
                    .into_iter()
                    .map(|id| {
                        s.chats
                            .get(&id.0)
                            .map(|chat| (chat.id, chat.title.clone(), chat.sidebar_preview()))
                            .unwrap_or_else(|| (id, format!("chat {}", id.0), String::new()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Phase 7.2: `searchPublicChats` hits (public username/title lookup).
        // TDLib excludes known chats from these results, so they are shown
        // as their own section rather than merged into `Chats`.
        let public_chats: Vec<(ChatId, String, String)> = session
            .map(|s| {
                s.search
                    .public_chat_ids
                    .iter()
                    .map(|id| {
                        s.chats
                            .get(&id.0)
                            .map(|chat| (chat.id, chat.title.clone(), chat.sidebar_preview()))
                            .unwrap_or_else(|| (*id, format!("chat {}", id.0), String::new()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let hint = match status {
            SearchStatus::Idle => "Type to search chats and messages.".to_string(),
            SearchStatus::Searching if recents => "Loading recent chats…".to_string(),
            SearchStatus::Searching => format!("Searching “{query}”…"),
            SearchStatus::Ready if recents => String::new(),
            SearchStatus::Ready => format!("Results for “{query}”"),
            SearchStatus::Empty => format!("No chats or messages match “{query}”."),
            SearchStatus::Failed => "Search failed.".to_string(),
            SearchStatus::Closed => String::new(),
        };
        let chat_heading = if recents { "Recent" } else { "Chats" };
        div()
            .id("search-results")
            .flex()
            .flex_col()
            .gap_2()
            .when(!hint.is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(hint),
                )
            })
            .when(!chats.is_empty(), |this| {
                let mut block = div()
                    .id("search-chats")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().font_semibold().child(chat_heading));
                for (id, title, preview) in chats {
                    block = block.child(search_result_row(
                        ("search-chat", id.0 as u64),
                        title,
                        preview,
                        cx,
                        move |this, window, cx| this.select_search_chat(id, window, cx),
                    ));
                }
                this.child(block)
            })
            .when(!public_chats.is_empty(), |this| {
                let mut block = div()
                    .id("search-public-chats")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().font_semibold().child("Public chats"));
                for (id, title, preview) in public_chats {
                    block = block.child(search_result_row(
                        ("search-public-chat", id.0 as u64),
                        title,
                        preview,
                        cx,
                        move |this, window, cx| this.select_search_chat(id, window, cx),
                    ));
                }
                this.child(block)
            })
            .when(!messages.is_empty(), |this| {
                let mut block = div()
                    .id("search-messages")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().font_semibold().child("Messages"));
                for (chat_id, message_id, title, preview) in messages {
                    block = block.child(search_result_row(
                        ("search-msg", message_id.0 as u64),
                        title,
                        preview,
                        cx,
                        move |this, window, cx| {
                            this.select_search_message(chat_id, message_id, window, cx);
                        },
                    ));
                }
                this.child(block)
            })
    }
}

fn search_result_row(
    id: (&'static str, u64),
    title: String,
    preview: String,
    cx: &mut Context<QuillApp>,
    on_pick: impl Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .px_2()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().accent.opacity(0.12)))
        .on_click(cx.listener(move |this, _, window, cx| on_pick(this, window, cx)))
        .child(div().font_medium().child(title))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(preview),
        )
}

fn bootstrap_connect(
    credentials: Option<TelegramCredentials>,
) -> (
    ConnectUiStatus,
    Option<LiveConnect>,
    String,
    AuthorizationState,
) {
    let auth_demo = AuthorizationState::WaitPhoneNumber;
    match evaluate_gate(credentials.is_some()) {
        ConnectGate::Blocked(ConnectBlocker::MissingCredentials) => (
            ConnectUiStatus::NeedCredentials,
            None,
            ConnectBlocker::MissingCredentials.user_message().into(),
            auth_demo,
        ),
        ConnectGate::Blocked(ConnectBlocker::MissingTdjson) => (
            ConnectUiStatus::NeedTdjson,
            None,
            ConnectBlocker::MissingTdjson.user_message().into(),
            auth_demo,
        ),
        ConnectGate::Blocked(other) => (
            ConnectUiStatus::RestoreBlocked(other.user_message()),
            None,
            other.user_message().into(),
            auth_demo,
        ),
        ConnectGate::Ready { .. } => {
            let Some(credentials) = credentials else {
                return (
                    ConnectUiStatus::NeedCredentials,
                    None,
                    ConnectBlocker::MissingCredentials.user_message().into(),
                    auth_demo,
                );
            };
            let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
            let store = live_secret_store();
            match start_live_connect(credentials, store.as_ref(), sink) {
                Ok(live) => (
                    ConnectUiStatus::Live,
                    Some(live),
                    "TDLib connected — waiting for authorization updates".into(),
                    auth_demo,
                ),
                Err(ConnectBlocker::MissingTdjson) => (
                    ConnectUiStatus::NeedTdjson,
                    None,
                    ConnectBlocker::MissingTdjson.user_message().into(),
                    auth_demo,
                ),
                Err(err) => (
                    ConnectUiStatus::RestoreBlocked(err.user_message()),
                    None,
                    err.user_message().into(),
                    auth_demo,
                ),
            }
        }
    }
}

fn live_status_for(auth: &AuthorizationState) -> String {
    match auth {
        AuthorizationState::WaitTdlibParameters => {
            "TDLib connected — waiting for authorization updates".into()
        }
        AuthorizationState::WaitPhoneNumber => "enter phone number".into(),
        AuthorizationState::WaitCode { .. } => "enter the verification code from Telegram".into(),
        AuthorizationState::WaitPassword { .. } => {
            "enter your two-step verification password".into()
        }
        AuthorizationState::Ready => "signed in — cloud + secret chats".into(),
        AuthorizationState::WaitOtherDeviceConfirmation => {
            "confirm on another device (QR payload is not logged)".into()
        }
        AuthorizationState::LoggingOut => "signing out".into(),
        AuthorizationState::Closing => "TDLib is closing".into(),
        AuthorizationState::Closed => "session closed".into(),
        other => view_for(other).body,
    }
}

impl Render for QuillApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Phase 8.1: feed OS window focus into the notification decision, then
        // dispatch any notifications the reducer queued since the last frame.
        if let Some(live) = self.live.as_mut() {
            live.driver.session.app_active = window.is_window_active();
        }
        self.flush_notifications(window, cx);
        // Phase A1: keep the slow-mode countdown ticking while the open
        // chat is gated (spawns at most one 1s task per open chat).
        self.ensure_slow_mode_tick(cx);
        // Phase B3: keep self-destruct countdown badges fresh while the
        // open chat has a live `self_destruct_in` timer (same 1s task
        // pattern as slow mode).
        self.ensure_self_destruct_tick(cx);
        // Phase C1: keep the call overlay's ringing / connected clock
        // fresh while a call is tracked (same 1s task pattern).
        self.ensure_call_tick(cx);
        // Phase 9.1: resolve a tapped story whose `story` response landed
        // since the click (`getStory` prefetch finished).
        // Parity slice: prefill the folder editor once its `getChatFolder`
        // spec arrives.
        self.maybe_prefill_folder_editor(window, cx);
        if let Some((chat_id, story_id)) = self.pending_story_open {
            let ready = self
                .session()
                .is_some_and(|s| s.stories.contains_key(&(chat_id, story_id)));
            if ready {
                self.pending_story_open = None;
                self.rebuild_story_viewer(ChatId(chat_id), story_id, cx);
            }
        }
        // Phase 9.2: the `updateStoryPostSucceeded` reducer queued poster
        // chats whose active stories should be refreshed (an own story
        // posted from another client appears in the tray this way).
        if let Some(live) = self.live.as_mut() {
            let chats: Vec<i64> = live.driver.session.story_tray_refresh.drain().collect();
            for chat_id in chats {
                let _ = live.driver.get_chat_active_stories(ChatId(chat_id));
            }
        }
        // Phase 9.2: a story that vanished from the cache while being
        // viewed was deleted (`updateStoryDeleted`) — close the viewer.
        let current_deleted = self.story_viewer.current().is_some_and(|item| {
            self.session().is_some_and(|session| {
                !session
                    .stories
                    .contains_key(&(item.chat_id.0, item.story_id))
            })
        });
        if current_deleted {
            self.story_viewer.close();
            self.story_reaction_picker_open = false;
            self.story_reply_open = false;
            self.status_note = "Story deleted".into();
        }
        // Phase 4.6: push the playback clock into the seek slider entity so
        // the thumb follows elapsed time (the tick has no `&mut Window`).
        self.sync_seek_slider(window, cx);
        let auth_state = self.current_auth();
        let auth = view_for(&auth_state);
        let inputs_live = self.live.is_some() || self.demo_auth_inputs;
        let show_phone = inputs_live && matches!(auth.action, AuthAction::EnterPhone);
        let show_code = inputs_live && matches!(auth.action, AuthAction::EnterCode);
        let show_password = inputs_live && matches!(auth.action, AuthAction::EnterPassword);
        div()
            .flex()
            .flex_col()
            .size_full()
            .relative()
            .bg(cx.theme().background)
            .on_action(cx.listener(|this, _: &QuitApp, window, cx| {
                let _ = this;
                window.remove_window();
                cx.quit();
            }))
            .on_action(cx.listener(|this, _: &FocusComposer, window, cx| {
                this.composer
                    .update(cx, |input, cx| input.focus(window, cx));
            }))
            .on_action(cx.listener(|this, _: &FocusSidebar, window, cx| {
                window.focus(&this.focus_sidebar, cx);
            }))
            .on_action(cx.listener(|this, _: &LoadOlder, _, cx| {
                this.load_older_action(cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSearch, window, cx| {
                this.open_search_ui(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenChatSearch, window, cx| {
                this.open_chat_search_ui(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ChatSearchNewer, _, cx| {
                this.chat_search_newer(cx);
            }))
            .on_action(cx.listener(|this, _: &ChatSearchOlder, _, cx| {
                this.chat_search_older(cx);
            }))
            .on_action(cx.listener(|this, _: &CancelSearch, window, cx| {
                this.cancel_search(window, cx);
            }))
            // Parity slice 5: left/right step the media viewer; `0` resets
            // zoom. The handlers no-op unless the viewer is open, and only
            // then stop propagation — otherwise the keystroke still reaches
            // text inputs (composer caret movement keeps working).
            .on_action(cx.listener(|this, _: &ViewerPrev, _, cx| {
                if this.media_viewer.is_open() {
                    this.step_media_viewer(-1, cx);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &ViewerNext, _, cx| {
                if this.media_viewer.is_open() {
                    this.step_media_viewer(1, cx);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &ViewerZoomReset, _, cx| {
                if this.media_viewer.is_open() {
                    this.viewer_reset_zoom(cx);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &ViewerZoomIn, _, cx| {
                if this.media_viewer.is_open() {
                    this.viewer_zoom_step(true, cx);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &ViewerZoomOut, _, cx| {
                if this.media_viewer.is_open() {
                    this.viewer_zoom_step(false, cx);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &SubmitPhone, window, cx| {
                this.submit_phone(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SubmitCode, window, cx| {
                this.submit_code(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SubmitPassword, window, cx| {
                this.submit_password(window, cx);
            }))
            .child(title_bar(
                self.pane_mode(),
                self.live.is_some(),
                self.search_is_open(),
                self.chat_search_is_open(),
                cx,
            ))
            .child(
                div()
                    .id("quill-shell")
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(&auth, show_phone, show_code, show_password, cx))
                    .child(self.conversation(cx))
                    // Phase 6: user / group info panel beside the conversation.
                    .when_some(self.info_panel(cx), |this, panel| this.child(panel)),
            )
            .child(status_bar(
                &auth,
                &self.connect_status,
                &self.status_note,
                cx,
            ))
            .when(self.media_viewer.is_open(), |this| {
                this.child(self.media_viewer_overlay(cx))
            })
            // Phase 9.1: story viewer overlay above the media viewer.
            .when(self.story_viewer.is_open(), |this| {
                this.child(self.story_viewer_overlay(cx))
            })
            // Phase 6: add-contact dialog above everything else.
            .when_some(self.add_contact_dialog_overlay(cx), |this, overlay| {
                this.child(overlay)
            })
            // Parity slice: folder manage / editor / delete-confirm above
            // everything else.
            .when_some(self.folder_overlays(cx), |this, overlay| {
                this.child(overlay)
            })
            // Parity slice: scope-default notification settings dialog.
            .when(self.notification_defaults_open, |this| {
                this.child(self.notification_defaults_overlay(cx))
            })
            // Phase C1: call overlay above everything else.
            .when_some(self.call_overlay(cx), |this, overlay| this.child(overlay))
    }
}

impl QuillApp {
    fn conversation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.pane_mode();
        let history = match mode {
            PaneMode::Synthetic => div()
                .id("conversation-history")
                .flex()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .child(self.chat.clone())
                .into_any_element(),
            PaneMode::Connecting => pane_placeholder(
                "Not signed in",
                "Chat list and sending unlock after authorization is Ready. This window is not showing a fake inbox.",
                cx,
            )
            .into_any_element(),
            PaneMode::Ready => self.session_history(cx).into_any_element(),
        };
        let composer = match mode {
            PaneMode::Synthetic => Some(true),
            PaneMode::Connecting => None,
            PaneMode::Ready => {
                let session = self.session();
                let open = session.and_then(|s| s.open_chat);
                let chat = open.and_then(|id| session.and_then(|s| s.chats.get(&id.0)));
                // Parity slice 4: posting into a forum topic is supported —
                // `sendMessage` carries `topic_id = messageTopicForum`
                // (schema 1.8.67, lines 12200 / 3004). Closed topics and
                // chats without the basic send permission keep the composer
                // hidden.
                let topic = open.and_then(|id| session.and_then(|s| s.open_topic_info(id)));
                let in_topic = session.is_some_and(|s| s.open_topic.is_some());
                let can_post = match (chat, topic) {
                    (Some(c), Some(t)) => c.can_post() && !t.is_closed && c.can_send_basic_messages,
                    (Some(c), None) if !in_topic => c.can_post(),
                    // In a topic whose info hasn't loaded yet: hide the
                    // composer until it arrives (the note says "Loading
                    // topic…").
                    _ => false,
                };
                if can_post { Some(true) } else { None }
            }
        };
        let composer_note: Option<String> = match mode {
            PaneMode::Connecting => Some("Sign in to send messages.".to_string()),
            PaneMode::Ready if composer.is_none() => {
                let open = self.session().and_then(|s| s.open_chat);
                let in_topic = self.session().is_some_and(|s| s.open_topic.is_some());
                let is_channel = open.is_some_and(|id| {
                    self.session()
                        .and_then(|s| s.chats.get(&id.0).map(|c| c.is_channel()))
                        .unwrap_or(false)
                });
                if is_channel {
                    // The join/leave footer replaces the plain note for channels.
                    None
                } else if in_topic {
                    // Parity slice 4: closed topics and a missing send
                    // permission hide the composer with an explanatory note.
                    let topic_closed = open.is_some_and(|id| {
                        self.session()
                            .and_then(|s| s.open_topic_info(id))
                            .is_some_and(|t| t.is_closed)
                    });
                    let send_allowed = open.is_some_and(|id| {
                        self.session()
                            .and_then(|s| s.chats.get(&id.0))
                            .is_some_and(|c| c.can_post() && c.can_send_basic_messages)
                    });
                    if topic_closed {
                        Some("This topic is closed — new messages are disabled.".to_string())
                    } else if !send_allowed {
                        Some("You don't have permission to post in this topic.".to_string())
                    } else {
                        // The topic's info hasn't loaded yet; the composer
                        // appears once it arrives.
                        Some("Loading topic…".to_string())
                    }
                } else if open.is_none() {
                    Some("Select a supported cloud chat to send.".to_string())
                } else {
                    // Phase B1: secret chats that can't send yet explain why
                    // instead of the generic unsupported note.
                    self.secret_composer_note()
                        .or_else(|| {
                            self.session()
                                .and_then(|s| {
                                    open.and_then(|id| {
                                        s.chats.get(&id.0).and_then(|c| c.kind.gate_reason())
                                    })
                                })
                                .map(str::to_string)
                        })
                        .or(Some(
                            "This conversation type is not supported yet.".to_string(),
                        ))
                }
            }
            _ => None,
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(history)
            .when(composer.is_some(), |this| {
                let show_attach = matches!(mode, PaneMode::Ready) && self.pending_edit.is_none();
                let chips: Vec<String> = if show_attach {
                    self.pending_attachments
                        .iter()
                        .map(|att| match att.kind {
                            AttachmentKind::Photo => format!("Photo · {}", att.file_name),
                            AttachmentKind::Document => format!("Document · {}", att.file_name),
                            AttachmentKind::Video => format!("Video · {}", att.file_name),
                            AttachmentKind::VideoNote => format!("Video note · {}", att.file_name),
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                this.child(
                    div()
                        .p_3()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .flex()
                        .flex_col()
                        .gap_2()
                        .when(self.voice_capture.is_some(), |this| {
                            this.child(self.voice_record_bar(cx))
                        })
                        .when(show_attach, |box_| {
                            box_.child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        Button::new("attach-photo").label("Attach photo").on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.attach_local(AttachmentKind::Photo, cx);
                                            }),
                                        ),
                                    )
                                    .child(
                                        Button::new("attach-file").label("Attach file").on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.attach_local(AttachmentKind::Document, cx);
                                            }),
                                        ),
                                    )
                                    .child(
                                        Button::new("attach-video").label("Attach video").on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.attach_local(AttachmentKind::Video, cx);
                                            }),
                                        ),
                                    )
                                    .child(
                                        Button::new("attach-video-note")
                                            .label("Video note")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.attach_local(AttachmentKind::VideoNote, cx);
                                            })),
                                    )
                                    .child(Button::new("open-poll-dialog").label("Poll").on_click(
                                        cx.listener(|this, _, window, cx| {
                                            this.open_poll_dialog(window, cx);
                                        }),
                                    ))
                                    .child(
                                        Button::new("open-gifs")
                                            .label(if self.gif_panel_open() {
                                                "GIFs open"
                                            } else {
                                                "GIFs"
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_gif_panel(cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("open-stickers")
                                            .label(if self.sticker_panel_open() {
                                                "Stickers open"
                                            } else {
                                                "Stickers"
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_sticker_panel(cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("record-voice")
                                            .label(if self.voice_capture.is_some() {
                                                "Recording"
                                            } else {
                                                "Voice"
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.start_voice_recording(cx);
                                            })),
                                    )
                                    .when(!self.pending_attachments.is_empty(), |row| {
                                        row.child(
                                            Button::new("clear-attach").label("Clear").on_click(
                                                cx.listener(|this, _, _, cx| {
                                                    this.clear_attachment(cx);
                                                }),
                                            ),
                                        )
                                    })
                                    // Phase B3: self-destruct timer picker —
                                    // only for photo/video in private chats
                                    // (the only combination TDLib accepts
                                    // `self_destruct_type` for).
                                    .when(self.self_destruct_picker_visible(), |row| {
                                        let label = self.self_destruct_button_label();
                                        row.child(
                                            Button::new("self-destruct-cycle")
                                                .label(label)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.cycle_composer_self_destruct(cx);
                                                })),
                                        )
                                    }),
                            )
                        })
                        .when(!chips.is_empty(), |this| {
                            let album = chips.len() >= 2;
                            let mut row =
                                div().id("composer-attach-chip").flex().flex_wrap().gap_2();
                            for (index, label) in chips.into_iter().enumerate() {
                                row = row.child(
                                    div()
                                        .id(("composer-attach-item", index as u64))
                                        .px_3()
                                        .py_2()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(rgb(0x8b949e))
                                        .bg(rgb(0x21262d))
                                        .child(div().text_sm().font_medium().child(label))
                                        .child(div().text_xs().text_color(rgb(0xc9d1d9)).child(
                                            if album {
                                                "album · picked locally"
                                            } else {
                                                "ready to send · picked locally"
                                            },
                                        )),
                                );
                            }
                            this.child(row)
                        })
                        .when_some(self.forward_result.clone(), |this, result| {
                            this.child(self.forward_success_banner(&result, cx))
                        })
                        .when(
                            self.pending_forward.is_some() && !self.forward_picker_open,
                            |this| {
                                this.when_some(self.pending_forward.clone(), |this, draft| {
                                    this.child(self.forward_selection_banner(&draft, cx))
                                })
                            },
                        )
                        .when_some(self.pending_delete.clone(), |this, _| {
                            this.child(self.delete_confirm_banner(cx))
                        })
                        // Phase B1: close-secret-chat confirm banner.
                        .when_some(self.pending_close_secret_chat, |this, _| {
                            this.child(self.close_secret_chat_confirm_banner(cx))
                        })
                        .when_some(self.pending_edit.clone(), |this, edit| {
                            this.child(self.composer_edit_banner(&edit, cx))
                        })
                        .when_some(self.pending_reply.clone(), |this, reply| {
                            this.child(self.composer_reply_banner(&reply, cx))
                        })
                        // Phase 4.2: poll creation dialog above the composer.
                        .when_some(self.poll_dialog_panel(cx), |this, panel| this.child(panel))
                        // Phase 3.3: `/` command menu above the composer.
                        .when_some(self.command_menu_dropdown(cx), |this, panel| {
                            this.child(panel)
                        })
                        // Phase A1: slow-mode countdown. The composer stays
                        // usable (typing is fine) but sends are blocked
                        // until the wait expires; `ensure_slow_mode_tick`
                        // re-renders every second so the number counts down.
                        .when_some(self.slow_mode_wait_secs(), |this, wait| {
                            this.child(
                                div()
                                    .id("slow-mode-banner")
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(0x8b949e))
                                    .bg(rgb(0x21262d))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_medium()
                                            .child(format!("Slow mode · wait {wait}s")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0xc9d1d9))
                                            .child("sending is paused until the timer expires"),
                                    ),
                            )
                        })
                        .child(Textarea::new(&self.composer).h(px(88.))),
                )
            })
            .when_some(composer_note, |this, note| {
                this.child(
                    div()
                        .p_3()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(note),
                )
            })
            .when_some(self.channel_footer(cx), |this, footer| this.child(footer))
    }

    /// Join/leave footer for an open broadcast channel (Phase 2.3). Admins
    /// with posting rights see the composer; the footer keeps the leave
    /// affordance and notes the posting state. Non-admins keep the 2.2
    /// behavior (composer hidden).
    fn channel_footer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let open = session.open_chat?;
        let chat = session.chats.get(&open.0)?;
        if !chat.is_channel() {
            return None;
        }
        let status = chat.my_member_status;
        let footer = div()
            .p_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .flex()
            .items_center()
            .gap_3();
        match status {
            None => Some(
                footer
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Checking channel membership…"),
                    )
                    .into_any_element(),
            ),
            Some(ChannelMemberStatus::Left) => {
                Some(
                    footer
                        .child(Button::new("channel-join").label("Join channel").on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.join_channel(open, cx);
                            }),
                        ))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("Join to follow new posts."),
                        )
                        .into_any_element(),
                )
            }
            Some(ChannelMemberStatus::Member) => Some(
                footer
                    .child(
                        Button::new("channel-leave")
                            .label("Leave channel")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.leave_channel(open, cx);
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Posting in channels is admin-only."),
                    )
                    .into_any_element(),
            ),
            Some(ChannelMemberStatus::Administrator) => {
                let can_post = chat.channel_admin_can_post();
                let note = if can_post {
                    format!("Posting as {}.", chat.title)
                } else {
                    "You are an admin, but posting is disabled for you.".to_string()
                };
                Some(
                    footer
                        .child(
                            Button::new("channel-leave")
                                .label("Leave channel")
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.leave_channel(open, cx);
                                })),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(note),
                        )
                        .into_any_element(),
                )
            }
            Some(ChannelMemberStatus::Creator) => Some(
                footer
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Posting as {}.", chat.title)),
                    )
                    .into_any_element(),
            ),
            Some(ChannelMemberStatus::Banned) => Some(
                footer
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("You are banned from this channel."),
                    )
                    .into_any_element(),
            ),
            Some(ChannelMemberStatus::Restricted) | Some(ChannelMemberStatus::Unknown) => Some(
                footer
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Channel membership is unknown."),
                    )
                    .into_any_element(),
            ),
        }
    }

    /// `joinChat` for the open public channel. Demo sessions flip the status
    /// locally (no live driver).
    fn join_channel(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.join_channel(chat_id) {
                Ok(()) => "joining channel…".into(),
                Err(_) => "could not join channel".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.set_member_status(ChannelMemberStatus::Member, None);
            }
            self.status_note = "joined channel (demo)".into();
        }
        cx.notify();
    }

    /// `leaveChat` for the open channel. Demo sessions flip the status locally.
    fn leave_channel(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.leave_channel(chat_id) {
                Ok(()) => "leaving channel…".into(),
                Err(_) => "could not leave channel".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.set_member_status(ChannelMemberStatus::Left, None);
            }
            self.status_note = "left channel (demo)".into();
        }
        cx.notify();
    }

    fn voice_record_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let seconds = self
            .voice_capture
            .as_ref()
            .map(|capture| capture.elapsed_secs())
            .unwrap_or(0);
        let bars = self
            .voice_capture
            .as_ref()
            .map(|capture| capture.bars.clone())
            .unwrap_or_default();
        div()
            .id("voice-record-bar")
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(0xf85149))
            .bg(rgb(0x21262d))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_medium()
                            .text_color(rgb(0xffffff))
                            .child(format!(
                                "Recording voice · {}",
                                format_voice_duration(seconds)
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(Button::new("cancel-voice").label("Cancel").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.cancel_voice_recording(cx);
                                }),
                            ))
                            .child(
                                Button::new("send-voice")
                                    .label("Send")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.send_voice_recording(window, cx);
                                    })),
                            ),
                    ),
            )
            .child(waveform_row(0, &bars))
    }

    /// Phase B1: the composer note for a secret chat that can't send —
    /// Pending ("Waiting for X to come online…", schema 1.8.67 line 2797:
    /// "waiting for the other user to get online"), Closed ("Secret chat
    /// closed"), or still resolving ("Loading secret chat…"). `None`
    /// when the open chat is not a secret chat or is Ready (the composer
    /// shows then).
    fn secret_composer_note(&self) -> Option<String> {
        let session = self.session()?;
        let open = session.open_chat?;
        let chat = session.chats.get(&open.0)?;
        let ChatKind::Secret { user_id, .. } = &chat.kind else {
            return None;
        };
        match &chat.secret_state {
            Some(SecretChatState::Ready) => None,
            Some(SecretChatState::Pending) => {
                let name = session
                    .user(user_id.0)
                    .map(|u| u.display_name())
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| "your contact".to_string());
                Some(format!("🔒 Waiting for {name} to come online…"))
            }
            Some(SecretChatState::Closed) => Some("🔒 Secret chat closed".to_string()),
            Some(SecretChatState::Unknown(_)) | None => Some("🔒 Loading secret chat…".to_string()),
        }
    }

    fn session_history(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let open = session.and_then(|s| s.open_chat);
        let title = open
            .and_then(|id| session.and_then(|s| s.chats.get(&id.0).map(|c| c.title.clone())))
            .unwrap_or_else(|| "No chat selected".into());
        let supported = open
            .and_then(|id| session.and_then(|s| s.chats.get(&id.0).map(|c| c.supported())))
            .unwrap_or(false);
        let gate = open.and_then(|id| {
            session.and_then(|s| s.chats.get(&id.0).and_then(|c| c.kind.gate_reason()))
        });
        let messages: Vec<HistoryMessage> = open
            .and_then(|id| session.and_then(|s| s.histories.get(&id.0)))
            .map(|h| h.ordered().into_iter().cloned())
            .into_iter()
            .flatten()
            .collect();
        let chat = open.and_then(|id| session.and_then(|s| s.chats.get(&id.0)));
        let files: HashMap<i32, ParsedFile> = session.map(|s| s.files.clone()).unwrap_or_default();
        let downloading: std::collections::HashSet<i32> =
            session.map(|s| s.downloading.clone()).unwrap_or_default();
        let media_roots = self.media_display_roots();
        let sender_name = title.clone();
        let chat_search_open = session.is_some_and(|s| s.chat_search.open);
        let highlight_id = session.and_then(|s| match s.chat_search.jump {
            ChatSearchJump::Ready { message_id } | ChatSearchJump::Loading { message_id } => {
                Some(message_id)
            }
            ChatSearchJump::None | ChatSearchJump::Missing { .. } => None,
        });
        let pinned = session.and_then(|s| s.open_chat_pinned_message()).cloned();
        let chat_actions = open.and_then(|id| {
            session.and_then(|s| s.chats.get(&id.0)).and_then(|chat| {
                chat.supported().then_some((
                    chat.id,
                    chat.is_muted(),
                    chat.notification_settings.is_muted_forever(),
                    chat.in_archive,
                ))
            })
        });
        let peer_typing = chat.is_some_and(|c| c.is_peer_typing());
        // Phase 5.1: forum supergroups render a topic list instead of the
        // general history; a selected topic renders its own history.
        let is_forum = chat.is_some_and(|c| c.is_forum_chat());
        let open_topic = session.and_then(|s| s.open_topic);
        let topic_info = open.and_then(|id| session.and_then(|s| s.open_topic_info(id)));
        let topic_messages: Vec<HistoryMessage> = match (open, open_topic) {
            (Some(id), Some(topic_id)) => session
                .and_then(|s| s.topic_histories.get(&(id.0, topic_id)))
                .map(|h| h.ordered().into_iter().cloned().collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        div()
            .id("conversation-history")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(self.conversation_header(&title, chat_actions, peer_typing, cx))
            .when_some(topic_info.clone(), |this, info| {
                this.child(self.forum_topic_strip(&info, cx))
            })
            .when_some(self.bot_info_panel(cx), |this, panel| this.child(panel))
            .when(self.mute_menu_open, |this| {
                this.child(self.mute_menu_panel(cx))
            })
            // Phase B4: self-destruct / auto-delete timer picker below
            // the header.
            .when(self.ttl_picker_open, |this| {
                this.child(self.ttl_picker_panel(cx))
            })
            // Parity slice: per-chat folder picker below the header.
            .when(self.folder_menu_open, |this| {
                this.child(self.folder_menu_panel(cx))
            })
            .when_some(pinned, |this, message| {
                this.child(self.pinned_message_banner(&message, cx))
            })
            .when(self.forward_picker_open, |this| {
                this.child(self.forward_picker_panel(cx))
            })
            .when(self.pending_react.is_some(), |this| {
                this.child(self.reaction_picker_panel(cx))
            })
            .when(self.gif_panel_open(), |this| {
                this.child(self.gif_picker_panel(cx))
            })
            .when(self.sticker_panel_open(), |this| {
                this.child(self.sticker_picker_panel(cx))
            })
            .when(chat_search_open, |this| {
                this.child(self.chat_search_bar(cx))
            })
            .child(if let Some(reason) = gate {
                if self.sponsored_demo {
                    // Fixture/proof surface only: the demo channel renders its
                    // sponsored rows. The live path renders history normally.
                    self.sponsored_rows_pane(cx).into_any_element()
                } else {
                    pane_placeholder("Unsupported chat", reason, cx).into_any_element()
                }
            } else if open.is_none() {
                pane_placeholder(
                    "Select a chat",
                    "The main list is driven by loadChats + updateNewChat / updateChatPosition.",
                    cx,
                )
                .into_any_element()
            } else if !supported {
                pane_placeholder(
                    "Unsupported chat",
                    "This conversation type is not supported yet.",
                    cx,
                )
                .into_any_element()
            } else if is_forum && open_topic.is_none() {
                // Phase 5.1: opening a forum supergroup shows its topics.
                self.forum_topics_pane(open, cx).into_any_element()
            } else if is_forum {
                // Phase 5.1: per-topic history — same history component,
                // fed from the topic history store (`searchChatMessages`
                // with `topic_id`).
                if topic_messages.is_empty() {
                    pane_placeholder(
                        "No messages in this topic yet",
                        "Topic history arrives via searchChatMessages with topic_id.",
                        cx,
                    )
                    .into_any_element()
                } else {
                    self.history_message_list(
                        "topic-history",
                        &topic_messages,
                        chat,
                        &sender_name,
                        highlight_id,
                        &files,
                        &downloading,
                        &media_roots,
                        cx,
                    )
                }
            } else if messages.is_empty() {
                pane_placeholder(
                    "No messages yet",
                    "History arrives via getChatHistory and updates.",
                    cx,
                )
                .into_any_element()
            } else {
                self.history_message_list(
                    "session-history",
                    &messages,
                    chat,
                    &sender_name,
                    highlight_id,
                    &files,
                    &downloading,
                    &media_roots,
                    cx,
                )
            })
    }

    /// Phase 5.1: the shared history row list used by both chat history and
    /// per-topic history (topic view = same component with a `topic_id`
    /// filter). `messages` are rendered oldest-first.
    fn history_message_list(
        &self,
        id: &'static str,
        messages: &[HistoryMessage],
        chat: Option<&ChatSummary>,
        sender_name: &str,
        highlight_id: Option<MessageId>,
        files: &HashMap<i32, ParsedFile>,
        downloading: &std::collections::HashSet<i32>,
        media_roots: &[PathBuf],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let mut list = div()
            .id(id)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px_3()
            .pt_2()
            .gap_1();
        let groups = quill::album::group_media_albums(
            messages,
            |message| message.media_album_id,
            |message| message.is_outgoing,
            |message| quill::album::is_album_media(&message.content),
        );
        for group in groups {
            let quill::album::HistoryGroup::Single(message) = group else {
                if let quill::album::HistoryGroup::Album { album_id, messages } = group {
                    list = list.child(album_history_row(
                        album_id,
                        &messages,
                        &files,
                        &downloading,
                        &media_roots,
                        chat,
                        &sender_name,
                        cx,
                    ));
                }
                continue;
            };
            let label = if message.is_outgoing {
                let receipt = chat
                    .map(|summary| summary.outbox_receipt(&message))
                    .unwrap_or(OutboxReceipt::Sent);
                outgoing_status_label(message.pending, receipt).to_string()
            } else {
                sender_name.to_string()
            };
            let highlighted = highlight_id == Some(message.id);
            let selected_forward = self
                .pending_forward
                .as_ref()
                .is_some_and(|draft| draft.contains(message.id));
            let quote_preview = session.and_then(|s| s.reply_quote_preview(&message));
            let forward_from = message
                .forward_info
                .as_ref()
                .and_then(|info| session.map(|s| s.forward_from_label(info)));
            let reaction_open = self
                .pending_react
                .is_some_and(|(chat, id)| chat == message.chat_id && id == message.id);
            // Phase 4.6: audio/voice rows get a seek-bar view model.
            let seek_bar = match &message.content {
                MessageContent::VoiceNote(note) => {
                    Some(self.seek_bar_view(message.id, f64::from(note.duration)))
                }
                MessageContent::Audio(audio) => {
                    Some(self.seek_bar_view(message.id, f64::from(audio.duration)))
                }
                _ => None,
            };
            let animation_playing = self.playing_animation == Some(message.id);
            let animation_frame = if animation_playing {
                self.animation_frames
                    .get(self.animation_frame)
                    .cloned()
                    .or_else(|| self.animation_frames.first().cloned())
            } else {
                None
            };
            let video_playing = self.playing_video == Some(message.id);
            let video_frame = if video_playing {
                self.video_frames
                    .get(self.video_frame)
                    .cloned()
                    .or_else(|| self.video_frames.first().cloned())
            } else {
                None
            };
            let row = session_history_row(
                &message,
                &files,
                &downloading,
                &media_roots,
                label,
                quote_preview,
                forward_from,
                selected_forward,
                reaction_open,
                seek_bar,
                animation_playing,
                animation_frame,
                video_playing,
                video_frame,
                &self.spoiler_revealed,
                // Phase B4: secret chats word the timer-change service
                // row as "Self-destruct".
                chat.is_some_and(|c| matches!(c.kind, ChatKind::Secret { .. })),
                cx,
            );
            list = list.child(
                div()
                    .when(highlighted || selected_forward, |this| {
                        this.rounded_lg()
                            .border_2()
                            .border_color(if selected_forward {
                                rgb(0x3fb950)
                            } else {
                                rgb(0x58a6ff)
                            })
                            .px_1()
                    })
                    .child(row),
            );
        }
        list.into_any_element()
    }

    /// Phase 5.1: strip shown above a topic's history — back to the topic
    /// list plus the topic name (and its unread count).
    fn forum_topic_strip(&self, info: &ForumTopic, cx: &mut Context<Self>) -> impl IntoElement {
        let badge = unread_badge_text(info.unread_count);
        let topic_name = info.name.clone();
        div()
            .id("forum-topic-strip")
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                Button::new("forum-topic-back")
                    .label("\u{2039} Topics")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.deselect_topic_ui(cx);
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(div().font_semibold().child(topic_name))
                    .when(
                        info.is_general && !info.name.eq_ignore_ascii_case("general"),
                        |this| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("General"),
                            )
                        },
                    )
                    .when(info.is_pinned, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Pinned"),
                        )
                    })
                    .when(info.is_closed, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Closed"),
                        )
                    }),
            )
            .when_some(badge, |this, label| {
                this.child(unread_badge(label, ChatId(info.forum_topic_id as i64)))
            })
    }

    /// Phase 5.1: the topic list for a forum supergroup — one row per
    /// topic with name, unread badge, and a cheap last-message preview.
    /// Selecting a row opens the per-topic history.
    fn forum_topics_pane(&self, open: Option<ChatId>, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let topics: Vec<ForumTopic> = open
            .map(|id| {
                session
                    .map(|s| s.ordered_forum_topics(id))
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let mut list = div()
            .id("forum-topics")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px_3()
            .pt_2()
            .gap_1();
        if topics.is_empty() {
            return pane_placeholder(
                "No topics yet",
                "The topic list arrives via getForumTopics.",
                cx,
            )
            .into_any_element();
        }
        list = list.child(
            div()
                .id("forum-topics-count")
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{} topics", topics.len())),
        );
        for topic in topics {
            let topic_id = topic.forum_topic_id;
            let badge = unread_badge_text(topic.unread_count);
            let name = if topic.name.is_empty() {
                format!("Topic {}", topic.forum_topic_id)
            } else {
                topic.name.clone()
            };
            let preview = topic.last_message_preview.clone();
            list = list.child(
                div()
                    .id(("forum-topic-row", topic_id as u64))
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(cx.theme().sidebar)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_topic_ui(topic_id, cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .min_w_0()
                                    .child(div().font_medium().min_w_0().child(name))
                                    .when(
                                        topic.is_general
                                            && !topic.name.eq_ignore_ascii_case("general"),
                                        |this| {
                                            this.child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child("General"),
                                            )
                                        },
                                    )
                                    .when(topic.is_pinned, |this| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Pinned"),
                                        )
                                    })
                                    .when(topic.is_closed, |this| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Closed"),
                                        )
                                    }),
                            )
                            .when_some(badge, |this, label| {
                                this.child(unread_badge(label, ChatId(topic_id as i64)))
                            }),
                    )
                    .when(!preview.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(preview),
                        )
                    }),
            );
        }
        list.into_any_element()
    }

    /// Phase 5.1: select a forum topic (live: `searchChatMessages` with
    /// `topic_id`; demo: the seeded session takes it directly).
    fn select_topic_ui(&mut self, forum_topic_id: i32, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .select_topic(forum_topic_id);
            self.status_note = match result {
                Ok(_) => "topic selected".into(),
                Err(_) => "could not open topic".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat_id) = session.open_chat {
                session.select_topic(chat_id, forum_topic_id);
            }
        }
        cx.notify();
    }

    /// Phase 5.1: leave the topic view, back to the forum's topic list.
    fn deselect_topic_ui(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            self.live.as_mut().expect("live").driver.deselect_topic();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.deselect_topic();
        }
        self.status_note = "back to topics".into();
        cx.notify();
    }

    /// Fixture/proof surface for `ReadySponsored`: the demo channel renders
    /// its `getChatSponsoredMessages` rows with Sponsored / Recommended labels
    /// instead of history.
    fn sponsored_rows_pane(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let open = session.and_then(|s| s.open_chat);
        let rows: Vec<SponsoredMessage> = session
            .map(|s| s.open_sponsored_rows().into_iter().cloned().collect())
            .unwrap_or_default();
        let files: HashMap<i32, ParsedFile> = session.map(|s| s.files.clone()).unwrap_or_default();
        let downloading: std::collections::HashSet<i32> =
            session.map(|s| s.downloading.clone()).unwrap_or_default();
        let media_roots = self.media_display_roots();
        let report = session.and_then(|s| s.sponsored_report.clone());
        let outcome = session.and_then(|s| s.last_sponsored_report.clone());
        let chat_id = open.unwrap_or(ChatId(0));
        let mut list = div()
            .id("sponsored-rows")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px_3()
            .pt_2()
            .gap_2();
        if let Some(outcome) = outcome {
            let message = outcome.user_message().to_string();
            list = list.child(
                div()
                    .id("sponsored-report-outcome")
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().sidebar)
                    .child(div().text_sm().child(message))
                    .child(
                        Button::new("sponsored-outcome-dismiss")
                            .label("Dismiss")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismiss_sponsored_outcome_ui(cx);
                            })),
                    ),
            );
        }
        if let Some(flight) = report {
            list = list.child(self.sponsored_report_panel(&flight, cx));
        }
        if rows.is_empty() {
            list = list.child(
                div()
                    .id("sponsored-empty")
                    .text_sm()
                    .child("No sponsored messages for this chat."),
            );
        }
        for message in &rows {
            list = list.child(sponsored_message_row(
                chat_id,
                message,
                &files,
                &downloading,
                &media_roots,
                &self.spoiler_revealed,
                cx,
            ));
        }
        div()
            .id("sponsored-pane")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(list)
    }

    /// `reportSponsoredResultOptionRequired` picker.
    fn sponsored_report_panel(
        &self,
        flight: &SponsoredReportFlight,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut options = div()
            .id("sponsored-report-options")
            .flex()
            .flex_col()
            .gap_1();
        for option in &flight.options {
            let option_id = option.id.clone();
            options = options.child(
                Button::new(format!("sponsored-report-option-{}", option.id))
                    .label(option.text.clone())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pick_sponsored_report_option(&option_id, cx);
                    })),
            );
        }
        div()
            .id("sponsored-report-picker")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(div().font_semibold().child(flight.title.clone()))
            .child(options)
            .child(
                Button::new("sponsored-report-cancel")
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dismiss_sponsored_report_ui(cx);
                    })),
            )
    }

    /// Start `reportChatSponsoredMessage` for a sponsored row. Live: the
    /// driver sends the request with an empty option id. Demo: inject
    /// `reportSponsoredResultOptionRequired` through the same reducer.
    fn report_sponsored_message_ui(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live
                .driver
                .report_sponsored_message(chat_id, message_id, "")
            {
                Ok(Some(_)) => "reporting sponsored message…".into(),
                Ok(None) => "report not available for this row".into(),
                Err(_) => "could not send report".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if session
                .begin_sponsored_report(chat_id, message_id)
                .is_some()
            {
                let extra =
                    session.request(RequestPurpose::ReportChatSponsoredMessage, Some(chat_id));
                let json = format!(
                    r#"{{"@type":"reportSponsoredResultOptionRequired","@extra":"{}","title":"Why are you reporting this ad?","options":[{{"@type":"reportOption","id":"bWlzLWxlYWQ=","text":"Misleading or scam"}},{{"@type":"reportOption","id":"c3BhbQ==","text":"Spam"}}]}}"#,
                    extra.0
                );
                let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
                if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                    session.apply(owned);
                }
                self.status_note = "demo — report options injected".into();
            } else {
                self.status_note = "report not available for this row".into();
            }
        }
        cx.notify();
    }

    /// Send the chosen `reportOption` id for the in-flight sponsored report.
    /// Live: the driver sends the follow-up request. Demo: resolve with
    /// `reportSponsoredResultOk` through the same reducer.
    fn pick_sponsored_report_option(&mut self, option_id: &str, cx: &mut Context<Self>) {
        let Some(flight) = self.session().and_then(|s| s.sponsored_report.clone()) else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.report_sponsored_message(
                flight.chat_id,
                flight.message_id,
                option_id,
            ) {
                Ok(_) => "report sent…".into(),
                Err(_) => "could not send report".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let extra = session.request(
                RequestPurpose::ReportChatSponsoredMessage,
                Some(flight.chat_id),
            );
            let json = format!(
                r#"{{"@type":"reportSponsoredResultOk","@extra":"{}"}}"#,
                extra.0
            );
            let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
            if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                session.apply(owned);
            }
            self.status_note = "demo — report sent".into();
        }
        cx.notify();
    }

    fn dismiss_sponsored_report_ui(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            session.dismiss_sponsored_report();
        } else if let Some(live) = self.live.as_mut() {
            live.driver.session.dismiss_sponsored_report();
        }
        cx.notify();
    }

    fn dismiss_sponsored_outcome_ui(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            session.clear_sponsored_report_outcome();
        } else if let Some(live) = self.live.as_mut() {
            live.driver.session.clear_sponsored_report_outcome();
        }
        cx.notify();
    }

    fn sidebar(
        &self,
        auth: &AuthView,
        show_phone: bool,
        show_code: bool,
        show_password: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mode = self.pane_mode();
        let mut list = div()
            .id("sidebar")
            .track_focus(&self.focus_sidebar)
            .w(px(280.))
            .h_full()
            .p_3()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .flex()
            .flex_col()
            .gap_2()
            .child(self.list_tabs(cx))
            .when(!self.contacts_tab_open, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(chat_list_caption(mode, self.session(), self.folder_tab)),
                )
            });
        match mode {
            PaneMode::Synthetic => {
                list = list
                    .child(static_chat_row(
                        "Ada Lovelace",
                        "Mixed-height history",
                        true,
                        cx,
                    ))
                    .child(static_chat_row("RTL / emoji samples", "שלום 👨‍👩‍👧‍👦", false, cx));
            }
            PaneMode::Connecting => {
                list = list.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No chat list until Ready."),
                );
            }
            PaneMode::Ready => {
                if self.contacts_tab_open {
                    list = list.child(self.contacts_list(cx));
                } else {
                    list = list.child(self.folder_tabs(cx));
                    list = list.child(self.sidebar_search_field(cx));
                    // Phase 9.1: tdesktop-style active-stories tray above the
                    // chat rows; omitted for the contacts tab.
                    if let Some(tray) = self.story_tray(cx) {
                        list = list.child(tray);
                    }
                    if self.search_is_open() {
                        list = list.child(self.search_results(cx));
                    } else {
                        let open = self.session().and_then(|s| s.open_chat);
                        let folder = self.folder_tab;
                        // Parity slice: folder names + tags flag for chat-row
                        // chips.
                        let (folder_names, show_folder_tags) = self
                            .session()
                            .map(|s| {
                                (
                                    s.chat_folders
                                        .iter()
                                        .map(|f| (f.id, f.name.clone()))
                                        .collect::<Vec<_>>(),
                                    s.are_folder_tags_enabled,
                                )
                            })
                            .unwrap_or_default();
                        let chats: Vec<ChatSummary> = self
                            .session()
                            .map(|s| match folder {
                                Some(folder_id) => s
                                    .ordered_folder_chats(folder_id)
                                    .into_iter()
                                    .cloned()
                                    .collect(),
                                None => s.ordered_chats().into_iter().cloned().collect(),
                            })
                            .unwrap_or_default();
                        if chats.is_empty() {
                            let loading = self.session().is_some_and(|s| !s.chats_exhausted);
                            list = list.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if loading && folder.is_none() {
                                        "Loading chats…"
                                    } else if folder.is_some() {
                                        "No chats in this folder yet."
                                    } else {
                                        "No chats in the main list."
                                    }),
                            );
                        }
                        // Parity slice: chat photos resolve here (once per
                        // render) and are sandboxed before display.
                        let media_roots = self.media_display_roots();
                        let photo_for = |chat: &ChatSummary| {
                            self.session()
                                .and_then(|s| s.chat_photo_path(chat.id))
                                .and_then(|path| sandboxed_display_path(path, &media_roots))
                        };
                        for chat in chats {
                            let selected = open == Some(chat.id);
                            let photo = photo_for(&chat);
                            list = list.child(session_chat_row(
                                &chat,
                                selected,
                                &folder_names,
                                show_folder_tags,
                                photo.as_deref(),
                                cx,
                            ));
                        }
                        // Parity slice: folder chats page eagerly — the driver
                        // re-requests `loadChats(chatListFolder)` after each
                        // `ok` until a 404 marks the folder exhausted, the
                        // same pattern as the main list. No "Load more"
                        // button: paging is automatic, not user-triggered.
                        // Archive stays as-is under the main list; a folder
                        // tab shows only that folder's chats.
                        if folder.is_none() {
                            let archived: Vec<ChatSummary> = self
                                .session()
                                .map(|s| s.ordered_archived_chats().into_iter().cloned().collect())
                                .unwrap_or_default();
                            if !archived.is_empty() {
                                list = list.child(
                                    div()
                                        .id("archive-section")
                                        .mt_2()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("Archived"),
                                );
                                for chat in archived {
                                    let selected = open == Some(chat.id);
                                    let photo = photo_for(&chat);
                                    list = list.child(session_chat_row(
                                        &chat,
                                        selected,
                                        &folder_names,
                                        show_folder_tags,
                                        photo.as_deref(),
                                        cx,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
        list = list
            .child(div().mt_4().font_semibold().child("Authorization"))
            .child(div().text_sm().child(auth.title))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(auth.body.clone()),
            )
            .child(auth_action_note(auth, &self.connect_status))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.status_note.clone()),
            );
        list.when(show_phone, |this| {
            this.child(div().mt_2().font_semibold().text_sm().child("Phone"))
                .child(Textarea::new(&self.phone_input).h(px(40.)))
                .child(
                    Button::new("submit-phone")
                        .label("Submit phone")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_phone(window, cx);
                        })),
                )
        })
        .when(show_code, |this| {
            this.child(div().mt_2().font_semibold().text_sm().child("Code"))
                .child(Textarea::new(&self.code_input).h(px(40.)))
                .child(
                    Button::new("submit-code")
                        .label("Submit code")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_code(window, cx);
                        })),
                )
        })
        .when(show_password, |this| {
            this.child(
                div()
                    .mt_2()
                    .font_semibold()
                    .text_sm()
                    .child("Two-step password"),
            )
            .child(Textarea::new(&self.password_input).h(px(40.)))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Sent to TDLib only — never logged"),
            )
            .child(
                Button::new("submit-password")
                    .label("Submit password")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.submit_password(window, cx);
                    })),
            )
        })
    }
}

fn seed_ready_chats_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::ReadyChats)
}

fn seed_ready_unread_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::UnreadBadge)
}

fn seed_ready_unread_read_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::AfterMarkRead)
}

/// Phase B1: secret chat lifecycle fixture (injected, no live Telegram) —
/// an `updateSecretChat` (Ready) arrives *before* `updateNewChat`
/// (`chatTypeSecret`), exactly as the schema guarantees (td_api.tl line
/// 10740); the chat opens with three E2E messages and the composer live.
/// The 🔒 badge shows in the chat-list row.
fn apply_ready_secret_chat(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 41i64;
    let user_id = 41i64;
    let secret_chat_id = 7i32;
    let jsons = [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{user_id},"first_name":"Zed","last_name":"Hopper","usernames":{{"@type":"usernames","active_usernames":["zedhopper"],"disabled_usernames":[],"editable_username":"zedhopper","collectible_usernames":[]}},"phone_number":"+15550101041","status":{{"@type":"userStatusOnline","expires":9999999999}},"is_contact":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSecretChat","secret_chat":{{"@type":"secretChat","id":{secret_chat_id},"user_id":{user_id},"state":{{"@type":"secretChatStateReady"}},"is_outbound":true,"key_hash":"","layer":144}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Zed","type":{{"@type":"chatTypeSecret","secret_chat_id":{secret_chat_id},"user_id":{user_id}}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"85","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":401,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"is_outgoing":false,"date":1700000100,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"This chat is end-to-end encrypted.","entities":[]}}}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":402,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"is_outgoing":false,"date":1700000160,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Only the two devices in this chat can read it.","entities":[]}}}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":403,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":999}},"is_outgoing":true,"date":1700000220,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Exactly — the lock in the chat list marks it.","entities":[]}}}}}}}}"#
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));
}

/// Phase C1: incoming-call fixture — a pending incoming voice call
/// from Zed (user 41), so the call overlay renders its incoming-call
/// card. Injected, no live Telegram.
fn apply_ready_call(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let user_id = 41i64;
    let jsons = [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{user_id},"first_name":"Zed","last_name":"Hopper","usernames":{{"@type":"usernames","active_usernames":["zedhopper"],"disabled_usernames":[],"editable_username":"zedhopper","collectible_usernames":[]}},"phone_number":"+15550101041","status":{{"@type":"userStatusOnline","expires":9999999999}},"is_contact":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Phase B2: key verification UI fixture — the Ready secret chat (id 41)
/// with Zed (user 41), but with a real deterministic 36-byte `key_hash`
/// (base64; the B1 fixture left it empty), opened with E2E history, and
/// Zed's info panel open on the "Encryption key" 12×12 fingerprint grid.
fn apply_ready_key_verification(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 41i64;
    let user_id = 41i64;
    let secret_chat_id = 7i32;
    // Deterministic 36-byte fixture (xorshift32, seed 0x9E3779B9) — the
    // grid is fixed across captures. NOT a real key: injected demo data.
    let key_hash_b64 = "GUYMUT5VLuA6j7l7taiDAR9tM+Y30on50Cklur/t+/w57sWo";
    let jsons = [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{user_id},"first_name":"Zed","last_name":"Hopper","usernames":{{"@type":"usernames","active_usernames":["zedhopper"],"disabled_usernames":[],"editable_username":"zedhopper","collectible_usernames":[]}},"phone_number":"+15550101041","status":{{"@type":"userStatusOnline","expires":9999999999}},"is_contact":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSecretChat","secret_chat":{{"@type":"secretChat","id":{secret_chat_id},"user_id":{user_id},"state":{{"@type":"secretChatStateReady"}},"is_outbound":true,"key_hash":"{key_hash_b64}","layer":144}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Zed","type":{{"@type":"chatTypeSecret","secret_chat_id":{secret_chat_id},"user_id":{user_id}}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"85","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":401,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"is_outgoing":false,"date":1700000100,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Tap my name above, then compare the key image.","entities":[]}}}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":402,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"is_outgoing":false,"date":1700000160,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"If it matches on both devices, nobody else can read this.","entities":[]}}}}}}}}"#
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));
    session.open_info_panel = Some(InfoPanelTarget::User(user_id));
}

/// Phase B3: self-destructing media fixture — a Ready *private* (1:1
/// cloud) chat with Zed (user 41), opened with an incoming photo
/// carrying a live 60s `messageSelfDestructTypeTimer`
/// (`self_destruct_in` 45s at fixture time, so the badge shows a live
/// countdown) and an outgoing
/// `messageSelfDestructTypeImmediately` ("view once") photo. Private —
/// not secret — because TDLib only accepts per-media
/// `self_destruct_type` in `chatTypePrivate` chats (schema 1.8.67
/// lines 6117/6128, "private chats only"). Injected demo data.
fn apply_ready_self_destruct(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 11i64;
    let user_id = 41i64;
    let incoming = demo_file_json(91, &demo_thumb_png_path(), true);
    let outgoing = demo_file_json(92, &demo_thumb_png_path(), true);
    let jsons = [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{user_id},"first_name":"Zed","last_name":"Hopper","usernames":{{"@type":"usernames","active_usernames":["zedhopper"],"disabled_usernames":[],"editable_username":"zedhopper","collectible_usernames":[]}},"phone_number":"+15550101041","status":{{"@type":"userStatusOnline","expires":9999999999}},"is_contact":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Zed","type":{{"@type":"chatTypePrivate","user_id":{user_id}}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"85","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":901,"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{incoming},"width":240,"height":200,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"tap to view — 60s timer","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}},"self_destruct_type":{{"@type":"messageSelfDestructTypeTimer","self_destruct_time":60}},"self_destruct_in":45.0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":902,"chat_id":{chat_id},"is_outgoing":true,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{outgoing},"width":240,"height":200,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"one look only","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}},"self_destruct_type":{{"@type":"messageSelfDestructTypeImmediately"}},"self_destruct_in":0}}}}"#
        ),
        r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));
}

/// Phase B4: chat TTL fixture — the Ready secret chat with Zed (id 41)
/// carrying `message_auto_delete_time` 3600 (1h self-destruct), one
/// message with a live `auto_delete_in` countdown (3595.5s at fixture
/// time, so the chip shows a decaying value), and a
/// `messageChatSetMessageAutoDeleteTime` service row. Injected demo data.
fn apply_ready_chat_ttl(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 41i64;
    let user_id = 41i64;
    let secret_chat_id = 7i32;
    let jsons = [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{user_id},"first_name":"Zed","last_name":"Hopper","usernames":{{"@type":"usernames","active_usernames":["zedhopper"],"disabled_usernames":[],"editable_username":"zedhopper","collectible_usernames":[]}},"phone_number":"+15550101041","status":{{"@type":"userStatusOnline","expires":9999999999}},"is_contact":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSecretChat","secret_chat":{{"@type":"secretChat","id":{secret_chat_id},"user_id":{user_id},"state":{{"@type":"secretChatStateReady"}},"is_outbound":true,"key_hash":"","layer":144}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Zed","type":{{"@type":"chatTypeSecret","secret_chat_id":{secret_chat_id},"user_id":{user_id}}},"unread_count":0,"message_auto_delete_time":3600}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"85","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":601,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"is_outgoing":false,"date":1700000100,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"This one deletes itself an hour after you read it.","entities":[]}}}},"auto_delete_in":3595.5}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":602,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"is_outgoing":false,"date":1700000160,"content":{{"@type":"messageChatSetMessageAutoDeleteTime","message_auto_delete_time":3600,"from_user_id":{user_id}}}}}}}"#,
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":603,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":999}},"is_outgoing":true,"date":1700000220,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Got it — timer's on.","entities":[]}}}}}}}}"#
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));
}

fn apply_ready_drafts(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let json = r#"{"@type":"updateChatDraftMessage","chat_id":11,"draft_message":{"@type":"draftMessage","reply_to":{"@type":"inputMessageReplyToMessage","message_id":101,"quote":null,"checklist_task_id":0,"poll_option_id":""},"date":1700000000,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"meet at 6","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"30","is_pinned":true}]}"#;
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

fn apply_ready_link_preview(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb = demo_file_json(41, &demo_thumb_png_path(), true);
    let body = "see https://example.com/story";
    let url_at = body.find("https").unwrap();
    let url_len = "https://example.com/story".len();
    let text_json = serde_json::to_string(body).unwrap();
    let json = format!(
        r#"{{"@type":"updateMessageContent","chat_id":11,"message_id":101,"new_content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text_json},"entities":[{{"@type":"textEntity","offset":{url_at},"length":{url_len},"type":{{"@type":"textEntityTypeUrl"}}}}]}},"link_preview":{{"@type":"linkPreview","url":"https://example.com/story","display_url":"example.com","site_name":"Example","title":"A short story","description":{{"@type":"formattedText","text":"Telegram-style link preview for a private chat.","entities":[]}},"author":"","type":{{"@type":"linkPreviewTypeArticle","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":90,"height":90,"progressive_sizes":[]}}]}}}},"has_large_media":true,"show_large_media":false,"show_media_above_description":false,"skip_confirmation":true,"show_above_text":false,"instant_view_version":0}},"link_preview_options":null}}}}"#
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// `ReadyTextEntities` fixture (Phase 4.1): open a dedicated "Demo entities"
/// chat (id 14) holding a `messageText` with mixed entities
/// (bold/italic/underline/strikethrough/spoiler/code/preCode with a `rust`
/// language, incl. a nested bold-inside-italic run) plus a `messagePhoto`
/// whose caption carries bold + link entities — all through the normal
/// reducer, no live Telegram. Offsets are computed in UTF-16 code units via
/// the same helper the parser uses.
fn apply_ready_text_entities(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb = demo_file_json(51, &demo_thumb_png_path(), true);
    // A dedicated chat keeps the screenshot focused: just the two fixture
    // messages, both visible without scrolling.
    let chat_id = 14;
    let chat_json = format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Demo entities","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0}}}}"#
    );
    let position_json = format!(
        r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"50","is_pinned":false}}}}"#
    );
    for json in [chat_json, position_json] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));

    let body = "Bold italic bolditalic underline strike secret code https://example.com\nfn demo() {\n  42\n}";
    let span = |text: &str, needle: &str| -> (i32, i32) {
        let start = text.find(needle).expect("demo needle");
        let utf16_start = utf8_to_utf16_offset(text, start).expect("demo offset");
        let utf16_end = utf8_to_utf16_offset(text, start + needle.len()).expect("demo offset");
        (utf16_start, utf16_end - utf16_start)
    };
    let ent = |text: &str, needle: &str, type_json: &str| -> String {
        let (offset, length) = span(text, needle);
        format!(
            r#"{{"@type":"textEntity","offset":{offset},"length":{length},"type":{type_json}}}"#
        )
    };
    let bold = r#"{"@type":"textEntityTypeBold"}"#;
    let entities = [
        ent(body, "Bold", bold),
        ent(body, "italic", r#"{"@type":"textEntityTypeItalic"}"#),
        // Nested: bold inside italic renders bold italic.
        ent(body, "bolditalic", bold),
        ent(body, "bolditalic", r#"{"@type":"textEntityTypeItalic"}"#),
        ent(body, "underline", r#"{"@type":"textEntityTypeUnderline"}"#),
        ent(body, "strike", r#"{"@type":"textEntityTypeStrikethrough"}"#),
        ent(body, "secret", r#"{"@type":"textEntityTypeSpoiler"}"#),
        ent(body, "code", r#"{"@type":"textEntityTypeCode"}"#),
        ent(
            body,
            "https://example.com",
            r#"{"@type":"textEntityTypeUrl"}"#,
        ),
        ent(
            body,
            "fn demo() {\n  42\n}",
            r#"{"@type":"textEntityTypePreCode","language":"rust"}"#,
        ),
    ]
    .join(",");
    let text_json = serde_json::to_string(body).unwrap();
    let text_message = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":104,"chat_id":14,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text_json},"entities":[{entities}]}}}}}}}}"#
    );

    let caption = "A captioned photo: bold caption and a link https://example.com/pic";
    let caption_entities = [
        ent(caption, "bold caption", bold),
        ent(
            caption,
            "https://example.com/pic",
            r#"{"@type":"textEntityTypeUrl"}"#,
        ),
    ]
    .join(",");
    let caption_json = serde_json::to_string(caption).unwrap();
    let photo_message = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":105,"chat_id":14,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":{caption_json},"entities":[{caption_entities}]}},"has_spoiler":false,"is_secret":false}}}}}}"#
    );

    for json in [text_message, photo_message] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadyPoll` fixture (Phase 4.2): open a dedicated "Demo polls" chat
/// (id 15) with two injected `messagePoll` messages through the normal
/// reducer — an open regular poll with a voted option (percentage bars +
/// counts, the chosen option marked) and a closed quiz poll (results only,
/// no voting affordance, correct answer marked).
fn apply_ready_poll(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 15;
    let chat_json = format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Demo polls","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0}}}}"#
    );
    let position_json = format!(
        r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"50","is_pinned":false}}}}"#
    );
    for json in [chat_json, position_json] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));

    let formatted = |text: &str| -> String {
        let text_json = serde_json::to_string(text).unwrap();
        format!(r#"{{"@type":"formattedText","text":{text_json},"entities":[]}}"#)
    };
    let option = |id: &str, text: &str, voter_count: i32, vote_percentage: i32, is_chosen: bool| {
        let text_json = formatted(text);
        format!(
            r#"{{"@type":"pollOption","id":"{id}","text":{text_json},"voter_count":{voter_count},"vote_percentage":{vote_percentage},"is_chosen":{is_chosen}}}"#
        )
    };
    let poll_message = |message_id: i32,
                        poll_id: i64,
                        question: &str,
                        options: &str,
                        total_voter_count: i32,
                        is_anonymous: bool,
                        allows_multiple_answers: bool,
                        allows_revoting: bool,
                        is_closed: bool,
                        poll_type: &str| {
        let question_json = formatted(question);
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{message_id},"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messagePoll","poll":{{"@type":"poll","id":{poll_id},"question":{question_json},"options":[{options}],"total_voter_count":{total_voter_count},"is_anonymous":{is_anonymous},"allows_multiple_answers":{allows_multiple_answers},"allows_revoting":{allows_revoting},"is_closed":{is_closed},"type":{poll_type}}},"description":{{"@type":"formattedText","text":"","entities":[]}},"can_add_option":false}}}}}}"#
        )
    };

    // Open regular poll: the user voted for "Sushi place" (is_chosen).
    let open_options = [
        option("opt-sushi", "Sushi place", 12, 55, true),
        option("opt-pizza", "Pizza", 7, 32, false),
        option("opt-tacos", "Tacos", 3, 13, false),
    ]
    .join(",");
    let open_poll = poll_message(
        106,
        9001,
        "Where should we eat lunch?",
        &open_options,
        22,
        true,
        false,
        true,
        false,
        r#"{"@type":"pollTypeRegular"}"#,
    );

    // Closed quiz poll: correct answer is "Mars" (index 0), user answered
    // "Venus" (is_chosen) — results only, no voting affordance.
    let closed_options = [
        option("opt-mars", "Mars", 18, 72, false),
        option("opt-venus", "Venus", 7, 28, true),
    ]
    .join(",");
    let closed_poll = poll_message(
        107,
        9002,
        "Which planet is known as the Red Planet?",
        &closed_options,
        25,
        true,
        false,
        false,
        true,
        r#"{"@type":"pollTypeQuiz","correct_option_ids":[0],"explanation":{"@type":"formattedText","text":"","entities":[]}}"#,
    );

    for json in [open_poll, closed_poll] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadyLocation` fixture (Phase 4.3): open a dedicated "Demo places" chat
/// (id 16) and inject four messages through the normal reducer — a plain
/// `messageLocation` (coordinates + accuracy), a `messageLiveLocation`
/// (live period / expires / heading / proximity alert), a `messageVenue`
/// (title + address + provider), and a `messageContact` (name + phone +
/// vCard + user_id). All data is synthetic; no live Telegram.
fn apply_ready_location(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 16;
    let chat_json = format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Demo places","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0}}}}"#
    );
    let position_json = format!(
        r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"50","is_pinned":false}}}}"#
    );
    for json in [chat_json, position_json] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));

    let message = |message_id: i32, content: &str| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{message_id},"chat_id":{chat_id},"is_outgoing":false,"content":{content}}}}}"#
        )
    };

    // Plain location: San Francisco, ±15 m accuracy.
    let location = message(
        108,
        r#"{"@type":"messageLocation","location":{"@type":"location","latitude":37.7749,"longitude":-122.4194,"horizontal_accuracy":15}}"#,
    );
    // Live location: Paris, 15-minute live period, 10 minutes left,
    // heading 90°, 500 m proximity alert.
    let live_location = message(
        109,
        r#"{"@type":"messageLiveLocation","location":{"@type":"liveLocation","location":{"@type":"location","latitude":48.8566,"longitude":2.3522,"horizontal_accuracy":0},"live_period":900,"heading":90,"proximity_alert_radius":500},"expires_in":600}"#,
    );
    // Venue: Ferry Building, via foursquare.
    let venue = message(
        110,
        r#"{"@type":"messageVenue","venue":{"@type":"venue","location":{"@type":"location","latitude":37.7955,"longitude":-122.3937,"horizontal_accuracy":0},"title":"Ferry Building","address":"1 Ferry Building, San Francisco","provider":"foursquare","id":"4a1a2b3c","type":"Food"}}"#,
    );
    // Contact: Ada Lovelace with a vCard and a known Telegram user id.
    let contact = message(
        111,
        r#"{"@type":"messageContact","contact":{"@type":"contact","phone_number":"+14155550123","first_name":"Ada","last_name":"Lovelace","vcard":"BEGIN:VCARD\nFN:Ada Lovelace\nEND:VCARD","user_id":123456789}}"#,
    );

    for json in [location, live_location, venue, contact] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Phase 4.4: inject a "Demo dice" chat with three `messageDice` rolls —
/// an incoming 🎲 = 4, an outgoing 🎲 = 6, and an incoming 🎯 = 5.
/// All data is synthetic; no live Telegram.
fn apply_ready_dice(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 17;
    let chat_json = format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Demo dice","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0}}}}"#
    );
    let position_json = format!(
        r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"50","is_pinned":false}}}}"#
    );
    for json in [chat_json, position_json] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));

    let message = |message_id: i32, outgoing: bool, content: &str| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{message_id},"chat_id":{chat_id},"is_outgoing":{outgoing},"content":{content}}}}}"#
        )
    };

    // Incoming 🎲 = 4.
    let roll_one = message(
        112,
        false,
        r#"{"@type":"messageDice","emoji":"🎲","value":4,"success_animation_frame_number":0}"#,
    );
    // Outgoing 🎲 = 6.
    let roll_two = message(
        113,
        true,
        r#"{"@type":"messageDice","emoji":"🎲","value":6,"success_animation_frame_number":0}"#,
    );
    // Incoming 🎯 = 5.
    let roll_three = message(
        114,
        false,
        r#"{"@type":"messageDice","emoji":"🎯","value":5,"success_animation_frame_number":0}"#,
    );

    for json in [roll_one, roll_two, roll_three] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadySponsored` fixture: open the demo channel (id 13, now ungated) and
/// inject a `sponsoredMessages` response through the same reducer the live
/// `getChatSponsoredMessages` path uses — one Sponsored row, one Recommended.
/// `ReadyStories` fixture (Phase 9.1): active-story tray entries for the two
/// seeded demo chats plus full story details, all through the normal
/// reducer — chat 11 "Demo chat A": order 30, `max_read_story_id` 4, stories
/// 4 (video, read) and 5 (photo, unread); chat 12 "Demo chat B": order 20,
/// all read (muted ring in the tray). Story media points at the existing
/// `demo-thumb.png` fixture as completed downloads, so the viewer renders
/// immediately. The demo opens on chat 11's story 5 ("Photo 2 of 2").
fn apply_ready_stories(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let photo_file = demo_file_json(91, &demo_thumb_png_path(), true);
    let video_thumb_file = demo_file_json(92, &demo_thumb_png_path(), true);
    let video_file = demo_file_json(93, &demo_thumb_png_path(), true);
    let tray = |chat_id: i64, order: i64, max_read: i32, story_ids: &[i32]| -> String {
        let stories = story_ids
            .iter()
            .map(|id| {
                format!(
                    r#"{{"@type":"storyInfo","story_id":{id},"date":1700000000,"is_for_close_friends":false,"is_live":false}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"@type":"updateChatActiveStories","active_stories":{{"@type":"chatActiveStories","chat_id":{chat_id},"list":{{"@type":"storyListMain"}},"order":"{order}","can_be_archived":false,"max_read_story_id":{max_read},"stories":[{stories}]}}}}"#
        )
    };
    let caption = |text: &str| -> String {
        format!(
            r#"{{"@type":"formattedText","text":{},"entities":[]}}"#,
            serde_json::to_string(text).unwrap()
        )
    };
    let photo_story = |id: i32, chat_id: i64, text: &str, file: &str| -> String {
        format!(
            r#"{{"@type":"story","id":{id},"poster_chat_id":{chat_id},"date":1700000000,"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"y","photo":{file},"width":960,"height":1280,"progressive_sizes":[]}}]}}}},"caption":{}}}"#,
            caption(text),
        )
    };
    let jsons = [
        tray(11, 30, 4, &[4, 5]),
        tray(12, 20, 6, &[6]),
        // Chat 11, story 4: video story with a thumbnail (read).
        format!(
            r#"{{"@type":"story","id":4,"poster_chat_id":11,"date":1700000000,"content":{{"@type":"storyContentVideo","video":{{"@type":"storyVideo","duration":9.0,"video":{video_file},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":90,"height":120,"file":{video_thumb_file}}}}},"alternative_video":null}},"caption":{}}}"#,
            caption("Demo story — the video shows its thumbnail (playback is out of slice)."),
        ),
        // Chat 11, story 5: photo story (unread).
        photo_story(5, 11, "Demo story — full-size photo render.", &photo_file),
        // Chat 12, story 6: photo story (read).
        photo_story(6, 12, "Demo chat B story.", &photo_file),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadyStoryPost` fixture (Phase 9.2): same seed as `ReadyStories`, but
/// Demo chat A's photo story (id 5) is an *own* story — chosen ❤ reaction,
/// interaction counts, `can_be_deleted` / `can_be_replied` — and an
/// `availableReactions` response is injected through the reducer so the
/// reaction picker has options. The caption states the honest limitation:
/// the pinned TDLib 1.8.67 schema has no `sendStory` constructor, so a
/// photo-story composer cannot be built against it yet.
fn apply_ready_story_post(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let photo_file = demo_file_json(91, &demo_thumb_png_path(), true);
    let video_thumb_file = demo_file_json(92, &demo_thumb_png_path(), true);
    let video_file = demo_file_json(93, &demo_thumb_png_path(), true);
    let tray = |chat_id: i64, order: i64, max_read: i32, story_ids: &[i32]| -> String {
        let stories = story_ids
            .iter()
            .map(|id| {
                format!(
                    r#"{{"@type":"storyInfo","story_id":{id},"date":1700000000,"is_for_close_friends":false,"is_live":false}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"@type":"updateChatActiveStories","active_stories":{{"@type":"chatActiveStories","chat_id":{chat_id},"list":{{"@type":"storyListMain"}},"order":"{order}","can_be_archived":false,"max_read_story_id":{max_read},"stories":[{stories}]}}}}"#
        )
    };
    let caption = |text: &str| -> String {
        format!(
            r#"{{"@type":"formattedText","text":{},"entities":[]}}"#,
            serde_json::to_string(text).unwrap()
        )
    };
    let own_photo_story = format!(
        r#"{{"@type":"story","id":5,"poster_chat_id":11,"date":1700000000,"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"y","photo":{photo_file},"width":960,"height":1280,"progressive_sizes":[]}}]}}}},"chosen_reaction_type":{{"@type":"reactionTypeEmoji","emoji":"❤"}},"interaction_info":{{"@type":"storyInteractionInfo","view_count":42,"forward_count":3,"reaction_count":7,"recent_viewer_user_ids":[]}},"can_be_deleted":true,"can_be_replied":true,"can_get_interactions":true,"caption":{}}}"#,
        caption(
            "Phase 9.2: ❤ quick-react, reaction picker, reply and delete for own stories. Posting is blocked — pinned TDLib 1.8.67 has no sendStory constructor."
        ),
    );
    let jsons = [
        tray(11, 30, 4, &[4, 5]),
        tray(12, 20, 6, &[6]),
        // Chat 11, story 4: video story with a thumbnail (read).
        format!(
            r#"{{"@type":"story","id":4,"poster_chat_id":11,"date":1700000000,"content":{{"@type":"storyContentVideo","video":{{"@type":"storyVideo","duration":9.0,"video":{video_file},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":90,"height":120,"file":{video_thumb_file}}}}},"alternative_video":null}},"caption":{}}}"#,
            caption("Demo story — the video shows its thumbnail (playback is out of slice)."),
        ),
        own_photo_story,
        // Chat 12, story 6: photo story (read).
        format!(
            r#"{{"@type":"story","id":6,"poster_chat_id":12,"date":1700000000,"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"y","photo":{photo_file},"width":960,"height":1280,"progressive_sizes":[]}}]}}}},"caption":{}}}"#,
            caption("Demo chat B story."),
        ),
        // Seeded picker options (`getStoryAvailableReactions` response).
        r#"{"@type":"availableReactions","top_reactions":[{"@type":"availableReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"needs_premium":false},{"@type":"availableReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"needs_premium":false},{"@type":"availableReaction","type":{"@type":"reactionTypeEmoji","emoji":"🔥"},"needs_premium":false},{"@type":"availableReaction","type":{"@type":"reactionTypeEmoji","emoji":"🎉"},"needs_premium":false},{"@type":"availableReaction","type":{"@type":"reactionTypeEmoji","emoji":"😮"},"needs_premium":false},{"@type":"availableReaction","type":{"@type":"reactionTypeEmoji","emoji":"😢"},"needs_premium":false},{"@type":"availableReaction","type":{"@type":"reactionTypeEmoji","emoji":"😂"},"needs_premium":false},{"@type":"availableReaction","type":{"@type":"reactionTypeEmoji","emoji":"👏"},"needs_premium":false}],"recent_reactions":[],"popular_reactions":[],"allow_custom_emoji":false,"are_tags":false,"unavailability_reason":null}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadySponsored` fixture: open the demo channel (id 13, now ungated) and
/// inject a `sponsoredMessages` response through the same reducer the live
/// `getChatSponsoredMessages` path uses — one Sponsored row, one Recommended.
/// The fixture still swaps the history pane for the sponsored rows pane.
fn apply_ready_sponsored(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.open_chat(ChatId(13));
    let extra = session.request(RequestPurpose::GetChatSponsoredMessages, Some(ChatId(13)));
    let thumb = demo_file_json(61, &demo_thumb_png_path(), true);
    let json = format!(
        r#"{{"@type":"sponsoredMessages","@extra":"{}","messages_between":3,"messages":[{{"@type":"sponsoredMessage","message_id":9001,"is_recommended":false,"can_be_reported":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Sponsored demo row — tap Report to open the option picker.","entities":[]}}}},"sponsor":{{"@type":"advertisementSponsor","url":"https://example.com/promo","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}},"info":"Example Ads"}},"title":"Summer sale","button_text":"Shop now","accent_color_id":0,"background_custom_emoji_id":"0","additional_info":"Ad by Example"}},{{"@type":"sponsoredMessage","message_id":9002,"is_recommended":true,"can_be_reported":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"Recommended demo photo row.","entities":[]}},"has_spoiler":false,"is_secret":false}},"sponsor":{{"@type":"advertisementSponsor","url":"https://example.com/pick","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}},"info":"Curated"}},"title":"Editors' pick","button_text":"Learn more","accent_color_id":0,"background_custom_emoji_id":"0","additional_info":""}}]}}"#,
        extra.0,
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// Shared seed for the forum-topics screenshot fixtures: forum supergroup
/// (id 16) via `updateNewChat` + `updateChatPosition`, marked a forum via
/// `updateSupergroup`, with a three-topic `getForumTopics` response
/// (General pinned + unread, Announcements with a preview, Random closed)
/// injected through the same reducer the live path uses.
fn seed_forum_chat_16(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let topic_json = |id: i32,
                      name: &str,
                      general: bool,
                      closed: bool,
                      pinned: bool,
                      unread: i32,
                      preview: Option<&str>| {
        let last_message = match preview {
            Some(text) => format!(
                r#""last_message":{{"id":{}, "chat_id":16, "is_outgoing":false, "content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{}","entities":[]}}}}}}"#,
                9000 + id,
                text
            ),
            None => r#""last_message":null"#.to_string(),
        };
        format!(
            r#"{{"info":{{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":{id},"name":"{name}","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"creation_date":1,"creator_id":{{"@type":"messageSenderUser","user_id":6}},"is_general":{general},"is_outgoing":false,"is_closed":{closed},"is_hidden":false,"is_name_implicit":false}},{last_message},"order":"{order}","is_pinned":{pinned},"unread_count":{unread},"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{{"@type":"chatNotificationSettings"}},"draft_message":null}}"#,
            id = id,
            name = name,
            general = general,
            closed = closed,
            pinned = pinned,
            unread = unread,
            order = 900 - id,
        )
    };
    let jsons = [
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Demo forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":3}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":16,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"25","is_pinned":false}}"#.to_string(),
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true}}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(16)));
    let topics = [
        topic_json(
            1,
            "General",
            true,
            false,
            true,
            3,
            Some("Pinned: please read the rules before posting."),
        ),
        topic_json(
            2,
            "Announcements",
            false,
            false,
            false,
            0,
            Some("v2.1 is rolling out this week."),
        ),
        topic_json(3, "Random", false, true, false, 0, None),
    ]
    .join(",");
    let json = format!(
        r#"{{"@type":"forumTopics","@extra":"{}","total_count":3,"topics":[{topics}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
        extra.0,
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// `ReadyForumTopics` fixture: the demo opens the forum with no topic
/// selected, so the topic list shows.
fn apply_ready_forum_topics(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    seed_forum_chat_16(session, sink, seq);
    session.open_chat(ChatId(16));
}

/// `ReadyTopicPost` fixture (parity slice 4): the forum's General topic is
/// open with a two-message injected history and the composer enabled — the
/// composer now posts into the topic via `sendMessage` with
/// `topic_id = messageTopicForum`.
fn apply_ready_topic_post(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    seed_forum_chat_16(session, sink, seq);
    session.open_chat(ChatId(16));
    session.select_topic(ChatId(16), 1);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 1);
    let json = format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":0,"messages":[{{"id":101,"chat_id":16,"is_outgoing":false,"topic_id":{{"@type":"messageTopicForum","forum_topic_id":1}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Welcome to General — say hello!","entities":[]}}}}}},{{"id":102,"chat_id":16,"is_outgoing":true,"topic_id":{{"@type":"messageTopicForum","forum_topic_id":1}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Hello from the new topic composer.","entities":[]}}}}}}]}}"#,
        extra.0,
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// `ReadyContacts` fixture: inject three users via `updateUser` (Ada online
/// and already a contact, Zed last-week and *not* a contact so the Add
/// affordance shows, Noor recently seen and a contact), a `getContacts`
/// `users` response through the same reducer the live path uses, and a
/// `userFullInfo` response with a bio for Zed. Opens the user info panel
/// for Zed; the demo block opens the contacts sidebar tab.
fn apply_ready_contacts(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let user_json = |id: i64,
                     first: &str,
                     last: &str,
                     phone: &str,
                     username: &str,
                     contact: bool,
                     status: &str| {
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{first}","last_name":"{last}","usernames":{{"@type":"usernames","active_usernames":["{username}"],"disabled_usernames":[],"editable_username":"{username}","collectible_usernames":[]}},"phone_number":"{phone}","status":{status},"is_contact":{contact},"type":{{"@type":"userTypeRegular"}}}}}}"#,
            id = id,
            first = first,
            last = last,
            phone = phone,
            username = username,
            contact = contact,
            status = status,
        )
    };
    let jsons = [
        user_json(
            31,
            "Ada",
            "Lovelace",
            "+15550101031",
            "adalove",
            true,
            r#"{"@type":"userStatusOnline","expires":9999999999}"#,
        ),
        user_json(
            32,
            "Zed",
            "Hopper",
            "+15550101032",
            "zedhopper",
            false,
            r#"{"@type":"userStatusLastWeek","by_my_privacy_settings":false}"#,
        ),
        user_json(
            33,
            "Noor",
            "Haddad",
            "+15550101033",
            "noorhaddad",
            true,
            r#"{"@type":"userStatusRecently","by_my_privacy_settings":false}"#,
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let extra = session.request(RequestPurpose::GetContacts, None);
    let json = format!(
        r#"{{"@type":"users","@extra":"{}","total_count":3,"user_ids":[31,32,33]}}"#,
        extra.0,
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
    let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, 32);
    let json = format!(
        r#"{{"@type":"userFullInfo","@extra":"{}","block_list":null,"bio":{{"@type":"formattedText","text":"Demo bio — systems programmer, occasional keyboard builder. This panel comes from the cached userFullInfo slice (Phase 6).","entities":[]}},"bot_info":null}}"#,
        extra.0,
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
    session.open_info_panel = Some(InfoPanelTarget::User(32));
}

/// `ReadyFolders` fixture (Phase 7.1): inject `updateChatFolders` with two
/// folders ("Work" id 1, "News" id 2) and `updateChatPosition` folder
/// positions for the seeded demo chats — chat 11 (Demo chat A) and chat 13
/// (Demo channel) go to "News", chat 12 (Demo chat B) goes to "Work".
/// The demo block then selects the "News" folder tab.
fn apply_ready_folders(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let folders = r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":1,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]},"animate_custom_emoji":false},"icon":{"@type":"chatFolderIcon","name":"Work"},"color_id":2,"is_shareable":false,"has_my_invite_links":false},{"@type":"chatFolderInfo","id":2,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"News","entities":[]},"animate_custom_emoji":false},"icon":{"@type":"chatFolderIcon","name":"Channels"},"color_id":4,"is_shareable":false,"has_my_invite_links":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#;
    if let Some(owned) = copy_and_parse(folders, seq, &dyn_sink) {
        session.apply(owned);
    }
    for (chat_id, folder_id, order) in [(11, 2, "70"), (13, 2, "60"), (12, 1, "65")] {
        let json = format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListFolder","chat_folder_id":{folder_id}}},"order":"{order}","is_pinned":false}}}}"#
        );
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadyChatAvatars` fixture (parity slice): chat-list avatars and the
/// channel/supergroup header — all injected through the normal reducer, no
/// live Telegram.
///
/// - `updateChatPhoto` gives "Demo chat A" (private, id 11) and the demo
///   channel (id 13) downloaded `chatPhotoInfo.small` thumbnails (the
///   shared demo-thumb fixture, marked completed).
/// - "Demo chat B" (private, id 12), "Demo basic group" (id 14) and "Demo
///   discussion" (supergroup, id 16) keep no photo → colored-initial
///   fallbacks.
/// - `updateSupergroup` caches the channel's primary `@username`
///   (`demochannel`).
/// - A `getSupergroupFullInfo` round-trip seeds the channel description,
///   12,345 subscribers and `linked_chat_id: 16`, so the header shows the
///   description snippet, the count, and the "Discuss" affordance.
/// - The channel is opened with two broadcast posts.
fn apply_ready_chat_avatars(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb_path = demo_thumb_png_path();
    let chat_photo = |file_id: i32| {
        let small = demo_file_json(file_id, &thumb_path, true);
        format!(
            r#"{{"@type":"chatPhotoInfo","small":{small},"big":null,"minithumbnail":null,"has_animation":false,"is_personal":false}}"#
        )
    };
    let position = |chat_id: i64, order: &str| {
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#
        )
    };
    let usernames = |names: &[&str]| {
        let active = names
            .iter()
            .map(|n| serde_json::to_string(n).unwrap())
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"@type":"usernames","active_usernames":[{active}],"disabled_usernames":[],"editable_username":{first},"collectible_usernames":[]}}"#,
            first = serde_json::to_string(names.first().copied().unwrap_or("")).unwrap(),
        )
    };
    let full_info_extra = session.request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, 13);
    let group_full_info_extra =
        session.request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, 16);
    let description = "Demo channel — product updates, release notes, and the occasional meme. New posts every weekday morning.";
    let description_json = serde_json::to_string(description).unwrap();
    let group_description_json =
        serde_json::to_string("The discussion group for the Demo channel.").unwrap();
    let views = |count: i32| {
        format!(
            r#""interaction_info":{{"@type":"messageInteractionInfo","view_count":{count},"forward_count":7,"reply_info":null,"reactions":null}}"#
        )
    };
    let post = |id: i64, text: &str, view_count: i32| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":13,"sender_id":{{"@type":"messageSenderChat","chat_id":13}},"is_outgoing":false,"is_channel_post":true,{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}}}"#,
            views(view_count),
        )
    };
    let jsons = [
        // Photos for the private chat and the channel.
        format!(
            r#"{{"@type":"updateChatPhoto","chat_id":11,"photo":{}}}"#,
            chat_photo(91)
        ),
        format!(
            r#"{{"@type":"updateChatPhoto","chat_id":13,"photo":{}}}"#,
            chat_photo(93)
        ),
        // A basic group (initials fallback) and the discussion supergroup.
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Demo basic group","type":{"@type":"chatTypeBasicGroup","basic_group_id":14},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Demo discussion","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#.to_string(),
        position(14, "25"),
        position(16, "5"),
        // Channel + discussion-group metadata.
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":13,"usernames":{},"is_forum":false,"is_channel":true}}}}"#,
            usernames(&["demochannel"])
        ),
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"usernames":null,"is_forum":false,"is_channel":false}}"#.to_string(),
        format!(
            r#"{{"@type":"supergroupFullInfo","@extra":"{}","description":{description_json},"member_count":12345,"linked_chat_id":16}}"#,
            full_info_extra.0,
        ),
        format!(
            r#"{{"@type":"supergroupFullInfo","@extra":"{}","description":{group_description_json},"member_count":42,"linked_chat_id":0}}"#,
            group_full_info_extra.0,
        ),
        post(201, "Broadcast one — channel post from the channel itself.", 12345),
        post(202, "Broadcast two — a second post with fewer views.", 987),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(13));
}

fn seed_ready_media_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::Media)
}

fn seed_ready_send_media_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::SendMedia)
}

/// `ReadyChannels` fixture: open the demo channel (id 13) and inject broadcast
/// posts through the normal reducer — `sender_id: messageSenderChat`,
/// `is_channel_post: true`, `interaction_info.view_count` — plus the
/// `getMe`/`getChatMember` pair that leaves the viewer as a non-member, so
/// the composer is hidden and the Join footer shows. A later
/// `updateMessageInteractionInfo` proves view counts update live.
fn apply_ready_channels(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.open_chat(ChatId(13));
    let me_extra = session.request(RequestPurpose::GetMe, None);
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(ChatId(13)));
    let views = |count: i32| {
        format!(
            r#""interaction_info":{{"@type":"messageInteractionInfo","view_count":{count},"forward_count":7,"reply_info":null,"reactions":null}}"#
        )
    };
    let post = |id: i64, text: &str, view_count: i32| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":13,"sender_id":{{"@type":"messageSenderChat","chat_id":13}},"is_outgoing":false,"is_channel_post":true,{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}}}"#,
            views(view_count),
        )
    };
    let jsons = [
        format!(
            r#"{{"@type":"user","@extra":"{}","id":777,"first_name":"Demo","last_name":"Viewer","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}"#,
            me_extra.0,
        ),
        format!(
            r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"tag":"","inviter_user_id":0,"joined_chat_date":0,"status":{{"@type":"chatMemberStatusLeft"}}}}"#,
            member_extra.0,
        ),
        post(201, "Broadcast one — channel post from the channel itself.", 12345),
        post(202, "Broadcast two — a second post with fewer views.", 987),
        // Live view-count bump on the first post.
        r#"{"@type":"updateMessageInteractionInfo","chat_id":13,"message_id":201,"interaction_info":{"@type":"messageInteractionInfo","view_count":12402,"forward_count":7,"reply_info":null,"reactions":null}}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadyBotChat` fixture (Phase 3.1): a private chat with a bot user —
/// `updateUser` marks user 21 `userTypeBot` (schema 1.8.67 line 816) —
/// opened with history, plus a `getUserFullInfo` round-trip whose
/// `userFullInfo` response caches `botInfo` (description + commands) so the
/// bot panel renders under the header. Bot chats ride the ordinary
/// private-chat path, so the composer stays visible.
fn apply_ready_bot_chat(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.open_chat(ChatId(21));
    let info_extra = session.request(RequestPurpose::GetUserFullInfo, Some(ChatId(21)));
    let jsons = [
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Demo","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#
            .to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":21,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"40","is_pinned":false}}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hi! I'm Demo Bot. Tap a command below to try it.","entities":[]}}}}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":302,"chat_id":21,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"/start","entities":[]}}}}"#
            .to_string(),
        format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"A demo bot","description":"Demo Bot answers questions and shows how the info panel looks. It understands /start, /help and /ping.","commands":[{{"@type":"botCommand","command":"start","description":"Start the bot","is_ephemeral":false}},{{"@type":"botCommand","command":"help","description":"Show help","is_ephemeral":false}},{{"@type":"botCommand","command":"ping","description":"Check latency","is_ephemeral":false}}]}}}}"#,
            info_extra.0,
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadyBotKeyboard` fixture (Phase 3.2): like `apply_ready_bot_chat`,
/// but the bot's message carries a `replyMarkupInlineKeyboard` (schema 1.8.67
/// line 3855): a URL row, a callback + switchInline row, and a copy-text +
/// unknown-type row (the unknown button renders disabled).
fn apply_ready_bot_keyboard(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.open_chat(ChatId(21));
    let info_extra = session.request(RequestPurpose::GetUserFullInfo, Some(ChatId(21)));
    let keyboard_message = r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Visit site","icon_custom_emoji_id":0,"style":{"@type":"buttonStylePrimary"},"type":{"@type":"inlineKeyboardButtonTypeUrl","url":"https://example.com"}}],[{"@type":"inlineKeyboardButton","text":"Vote","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleSuccess"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AQID"}},{"@type":"inlineKeyboardButton","text":"Search here","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeSwitchInline","query":"cats","target_chat":{"@type":"targetChatCurrent"}}}],[{"@type":"inlineKeyboardButton","text":"Copy code","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeCopyText","text":"PROMO-42"}},{"@type":"inlineKeyboardButton","text":"Mystery","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeQuantum"}}]],"force_reply":false},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Tap a button below — this message has an inline keyboard.","entities":[]}}}}"#.to_string();
    let jsons = [
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Demo","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#
            .to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":21,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"40","is_pinned":false}}"#
            .to_string(),
        keyboard_message,
        format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"A demo bot","description":"Demo Bot answers questions and shows how the info panel looks. It understands /start, /help and /ping.","commands":[{{"@type":"botCommand","command":"start","description":"Start the bot","is_ephemeral":false}}]}}}}"#,
            info_extra.0,
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadyBotCommandMenu` fixture (Phase 3.3): like `apply_ready_bot_chat`,
/// plus a global-scope `botCommands` response through the real
/// `getCommands` reducer path, so the `/` menu shows the bot-specific
/// commands and a "Global" section below.
fn apply_ready_bot_command_menu(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    apply_ready_bot_chat(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let cmd_extra = session.request(RequestPurpose::GetCommands, Some(ChatId(21)));
    let json = format!(
        r#"{{"@type":"botCommands","@extra":"{}","bot_user_id":21,"commands":[{{"@type":"botCommand","command":"settings","description":"Tweak the bot","is_ephemeral":false}}]}}"#,
        cmd_extra.0,
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// `ReadyChannelsAdmin` fixture (Phase 2.3): like `apply_ready_channels`,
/// but the `getMe`/`getChatMember` pair leaves the viewer as an administrator
/// with `rights.can_post_messages: true` (full `chatAdministratorRights`
/// block per schema 1.8.67), so the composer is visible above the broadcast
/// posts. A later `updateMessageInteractionInfo` proves view counts update
/// live.
fn apply_ready_channels_admin(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.open_chat(ChatId(13));
    let me_extra = session.request(RequestPurpose::GetMe, None);
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(ChatId(13)));
    let views = |count: i32| {
        format!(
            r#""interaction_info":{{"@type":"messageInteractionInfo","view_count":{count},"forward_count":7,"reply_info":null,"reactions":null}}"#
        )
    };
    let post = |id: i64, text: &str, view_count: i32| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":13,"sender_id":{{"@type":"messageSenderChat","chat_id":13}},"is_outgoing":false,"is_channel_post":true,{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}}}"#,
            views(view_count),
        )
    };
    // `chatMemberStatusAdministrator can_be_edited:Bool
    // rights:chatAdministratorRights` — full rights block, `can_post_messages`
    // true (TDLib 1.8.67 `chatAdministratorRights` field order).
    let admin_status = r#"{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{"@type":"chatAdministratorRights","can_manage_chat":true,"can_change_info":true,"can_post_messages":true,"can_edit_messages":true,"can_delete_messages":true,"can_invite_users":true,"can_restrict_members":true,"can_pin_messages":true,"can_manage_topics":true,"can_promote_members":true,"can_manage_video_chats":true,"can_post_stories":false,"can_edit_stories":false,"can_delete_stories":false,"can_manage_direct_messages":true,"can_manage_tags":false,"can_send_welcome_messages":false,"is_anonymous":false}}"#;
    let jsons = [
        format!(
            r#"{{"@type":"user","@extra":"{}","id":777,"first_name":"Demo","last_name":"Viewer","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}"#,
            me_extra.0,
        ),
        format!(
            r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"tag":"","inviter_user_id":0,"joined_chat_date":0,"status":{}}}"#,
            member_extra.0, admin_status,
        ),
        post(201, "Broadcast one — channel post from the channel itself.", 12345),
        post(202, "Broadcast two — a second post with fewer views.", 987),
        // Live view-count bump on the first post.
        r#"{"@type":"updateMessageInteractionInfo","chat_id":13,"message_id":201,"interaction_info":{"@type":"messageInteractionInfo","view_count":12402,"forward_count":7,"reply_info":null,"reactions":null}}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn chat_search_jump_note(session: &Session) -> String {
    match session.chat_search.jump {
        ChatSearchJump::None => String::new(),
        ChatSearchJump::Loading { .. } => "loading around message…".into(),
        ChatSearchJump::Ready { message_id } => format!("jumped to {}", message_id.0),
        ChatSearchJump::Missing { .. } => "message deleted or inaccessible".into(),
    }
}

fn apply_ready_search(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.open_search();
    let search_gen = session.search.begin_query("hello");
    let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
    let jsons = [
        format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[11]}}"#,
            chats_extra.0
        ),
        format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":101,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}}}]}}"#,
            messages_extra.0
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn apply_ready_search_in_chat(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    assert!(session.open_chat_search());
    let search_gen = session.chat_search.begin_query("hello");
    let extra =
        session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11), search_gen);
    let json = format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":90,"messages":[{{"id":101,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}}},{{"id":103,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Two more waiting.","entities":[]}}}}}}]}}"#,
        extra.0
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
    let _ = session.begin_chat_search_jump(MessageId(101));
}

fn apply_ready_reply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let json = r#"{"@type":"updateNewMessage","message":{"id":104,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Got it — quoting you.","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":101,"quote":{"@type":"textQuote","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]},"position":0,"is_manual":false},"checklist_task_id":0,"poll_option_id":""}}}"#;
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

fn apply_ready_forward(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let incoming = r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}},"forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginHiddenUser","sender_name":"Ada Lovelace"},"date":1700000000,"source":null,"public_service_announcement_type":""}}}"#;
    if let Some(owned) = copy_and_parse(incoming, seq, &dyn_sink) {
        session.apply(owned);
    }
    let extra = session.request(RequestPurpose::ForwardMessages, Some(ChatId(12)));
    session.in_flight_forward = Some(quill::state::ForwardFlight {
        extra,
        dest_chat_id: ChatId(12),
        from_chat_id: ChatId(11),
        requested: 2,
    });
    let json = format!(
        r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":80,"chat_id":12,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":11}},"date":1}}}},{{"id":81,"chat_id":12,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Reply from the session reducer.","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":11}},"date":1}}}}]}}"#,
        extra.0
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

fn apply_ready_reactions(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let extra = session.request(RequestPurpose::AddMessageReaction, Some(ChatId(11)));
    let jsons = [
        format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        r#"{"@type":"updateMessageInteractionInfo","chat_id":11,"message_id":101,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":3,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]},{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn notification_settings_json(settings: &ChatNotificationSettings) -> String {
    format!(
        r#"{{"@type":"chatNotificationSettings","use_default_mute_for":{},"mute_for":{},"use_default_sound":{},"sound_id":"{}","use_default_show_preview":{},"show_preview":{},"use_default_mute_stories":{},"mute_stories":{},"use_default_story_sound":{},"story_sound_id":"{}","use_default_show_story_poster":{},"show_story_poster":{},"use_default_disable_pinned_message_notifications":{},"disable_pinned_message_notifications":{},"use_default_disable_mention_notifications":{},"disable_mention_notifications":{}}}"#,
        settings.use_default_mute_for,
        settings.mute_for,
        settings.use_default_sound,
        settings.sound_id,
        settings.use_default_show_preview,
        settings.show_preview,
        settings.use_default_mute_stories,
        settings.mute_stories,
        settings.use_default_story_sound,
        settings.story_sound_id,
        settings.use_default_show_story_poster,
        settings.show_story_poster,
        settings.use_default_disable_pinned_message_notifications,
        settings.disable_pinned_message_notifications,
        settings.use_default_disable_mention_notifications,
        settings.disable_mention_notifications
    )
}

fn apply_ready_gifs(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let frame = demo_media_allowlist()
        .join("demo-gif-1.png")
        .to_string_lossy()
        .into_owned();
    let clip = demo_media_allowlist()
        .join("demo-gif.gif")
        .to_string_lossy()
        .into_owned();
    let thumb = demo_file_json(61, &frame, true);
    let pending = demo_file_json(62, "", false);
    let local_clip = demo_file_json(63, &clip, true);
    let history = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":501,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageAnimation","animation":{{"@type":"animation","duration":1,"width":240,"height":140,"file_name":"demo-gif.gif","mime_type":"image/gif","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"animation":{local_clip}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let waiting = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":502,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageAnimation","animation":{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"saved.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":null,"animation":{pending}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    for json in [history, waiting] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.gifs.open = true;
    session.gifs.loading = true;
    let extra = session.request(RequestPurpose::GetSavedAnimations, None);
    let saved = format!(
        r#"{{"@type":"animations","@extra":"{}","animations":[{{"@type":"animation","duration":1,"width":240,"height":140,"file_name":"demo-gif.gif","mime_type":"image/gif","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"animation":{local_clip}}},{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"saved.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":null,"animation":{pending}}}]}}"#,
        extra.0
    );
    if let Some(owned) = copy_and_parse(&saved, seq, &dyn_sink) {
        session.apply(owned);
    }
}

fn apply_ready_video(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb_path = demo_thumb_png_path();
    let clip_path = demo_media_allowlist()
        .join("demo-gif.gif")
        .to_string_lossy()
        .into_owned();
    let thumb = demo_file_json(71, &thumb_path, true);
    let clip = demo_file_json(72, &clip_path, true);
    let pending = demo_file_json(73, "", false);
    let playing = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":601,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":12,"width":640,"height":360,"file_name":"clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"Beach clip","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let waiting = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":602,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":3,"width":320,"height":180,"file_name":"pending.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":false,"minithumbnail":null,"thumbnail":null,"video":{pending}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [playing, waiting, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn apply_ready_audio(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let cover_path = demo_thumb_png_path();
    let track_path = demo_media_allowlist()
        .join("demo-voice.ogg")
        .to_string_lossy()
        .into_owned();
    let cover = demo_file_json(101, &cover_path, true);
    let track = demo_file_json(102, &track_path, true);
    let pending = demo_file_json(103, "", false);
    let playing = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":801,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageAudio","audio":{{"@type":"audio","duration":214,"title":"Night Drive","performer":"Ada Lovelace","file_name":"night.mp3","mime_type":"audio/mpeg","album_cover_minithumbnail":null,"album_cover_thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":90,"height":90,"file":{cover}}},"external_album_covers":[],"audio":{track}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
    );
    let waiting = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":802,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageAudio","audio":{{"@type":"audio","duration":45,"title":"Untitled","performer":"","file_name":"pending.mp3","mime_type":"audio/mpeg","album_cover_minithumbnail":null,"album_cover_thumbnail":null,"external_album_covers":[],"audio":{pending}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [playing, waiting, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Demo fixture (Parity slice 5): inject a downloaded video message into
/// chat 11 of a Media-seeded demo session. The thumbnail (file 97) and the
/// full clip (file 96, `demo-clip.mp4`) are both local/completed so the
/// viewer opens straight onto playback. The 12 s duration is fixture data
/// for the screenshot — the real fixture clip is 1 s.
fn apply_ready_video_viewer(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let clip_path = demo_media_allowlist()
        .join("demo-clip-12s.mp4")
        .to_string_lossy()
        .into_owned();
    let thumb_path = demo_thumb_png_path();
    let clip = demo_file_json(96, &clip_path, true);
    let thumb = demo_file_json(97, &thumb_path, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":204,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":12,"width":320,"height":180,"file_name":"demo-clip-12s.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"Demo clip","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

fn apply_ready_video_note(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb_path = demo_thumb_png_path();
    let clip_path = demo_media_allowlist()
        .join("demo-clip.mp4")
        .to_string_lossy()
        .into_owned();
    let thumb = demo_file_json(91, &thumb_path, true);
    let clip = demo_file_json(92, &clip_path, true);
    let pending = demo_file_json(93, "", false);
    let playing = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":611,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":8,"waveform":"","length":240,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":120,"height":120,"file":{thumb}}},"speech_recognition_result":null,"video":{clip}}},"is_viewed":false,"is_secret":false}}}}}}"#
    );
    let waiting = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":612,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":3,"waveform":"","length":240,"minithumbnail":null,"thumbnail":null,"speech_recognition_result":null,"video":{pending}}},"is_viewed":true,"is_secret":false}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [playing, waiting, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn apply_ready_video_note_send(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let clip_path = demo_media_allowlist()
        .join("demo-video-note.mp4")
        .to_string_lossy()
        .into_owned();
    let thumb_path = demo_thumb_png_path();
    let clip = demo_file_json(94, &clip_path, true);
    let thumb = demo_file_json(95, &thumb_path, true);
    let sent = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":721,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":1,"waveform":"","length":240,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":240,"file":{thumb}}},"speech_recognition_result":null,"video":{clip}}},"is_viewed":true,"is_secret":false}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [sent, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn apply_ready_video_send(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let clip_path = demo_media_allowlist()
        .join("demo-clip.mp4")
        .to_string_lossy()
        .into_owned();
    let thumb_path = demo_thumb_png_path();
    let clip = demo_file_json(81, &clip_path, true);
    let thumb = demo_file_json(82, &thumb_path, true);
    let sent = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":701,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":1,"width":320,"height":180,"file_name":"demo-clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"Sent clip","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [sent, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn apply_ready_albums(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let wide = demo_file_json(71, &demo_thumb_png_path(), true);
    let left = demo_file_json(
        72,
        &demo_media_allowlist()
            .join("demo-gif-1.png")
            .to_string_lossy(),
        true,
    );
    let right = demo_file_json(
        73,
        &demo_media_allowlist()
            .join("demo-gif-2.png")
            .to_string_lossy(),
        true,
    );
    let own_photo = demo_file_json(74, &demo_thumb_png_path(), true);
    let own_clip = demo_file_json(
        75,
        &demo_media_allowlist()
            .join("demo-clip.mp4")
            .to_string_lossy(),
        true,
    );
    let own_thumb = demo_file_json(76, &demo_thumb_png_path(), true);
    let received_a = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":801,"chat_id":11,"is_outgoing":false,"media_album_id":"77001","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{wide},"width":640,"height":200,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let received_b = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":802,"chat_id":11,"is_outgoing":false,"media_album_id":"77001","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{left},"width":200,"height":200,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let received_c = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":803,"chat_id":11,"is_outgoing":false,"media_album_id":"77001","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{right},"width":200,"height":220,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"From Ada","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let sent_photo = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":811,"chat_id":11,"is_outgoing":true,"media_album_id":"77002","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{own_photo},"width":240,"height":200,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let sent_video = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":812,"chat_id":11,"is_outgoing":true,"media_album_id":"77002","content":{{"@type":"messageVideo","video":{{"@type":"video","duration":1,"width":320,"height":180,"file_name":"demo-clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{own_thumb}}},"video":{own_clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"Sent album","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [
        received_a,
        received_b,
        received_c,
        sent_photo,
        sent_video,
        drop_seed.to_string(),
    ] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn apply_ready_stickers(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb_path = demo_thumb_png_path();
    let loaded = demo_file_json(41, &thumb_path, true);
    let pending = demo_file_json(43, "", false);
    let history = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":401,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageSticker","is_premium":false,"sticker":{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatPng"}},"width":128,"height":128,"file":{loaded}}},"sticker":{pending}}}}}}}}}"#
    );
    if let Some(owned) = copy_and_parse(&history, seq, &dyn_sink) {
        session.apply(owned);
    }
    session.stickers.open = true;
    session.stickers.loading_sets = true;
    let sets_extra = session.request(RequestPurpose::GetInstalledStickerSets, None);
    let sets = format!(
        r#"{{"@type":"stickerSets","@extra":"{}","total_count":1,"sets":[{{"@type":"stickerSetInfo","id":"77","title":"Demo stickers","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"size":2,"covers":[]}}]}}"#,
        sets_extra.0
    );
    if let Some(owned) = copy_and_parse(&sets, seq, &dyn_sink) {
        session.apply(owned);
    }
    session.mark_sticker_set_loading();
    let set_extra = session.request(RequestPurpose::GetStickerSet, None);
    let smile = demo_file_json(41, &thumb_path, true);
    let wave = demo_file_json(44, "", false);
    let set = format!(
        r#"{{"@type":"stickerSet","@extra":"{}","id":"77","title":"Demo stickers","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"stickers":[{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatPng"}},"width":128,"height":128,"file":{smile}}},"sticker":{pending}}},{{"@type":"sticker","id":"9002","set_id":"77","width":512,"height":512,"emoji":"👋","format":{{"@type":"stickerFormatTgs"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{wave}}}],"emojis":[]}}"#,
        set_extra.0
    );
    if let Some(owned) = copy_and_parse(&set, seq, &dyn_sink) {
        session.apply(owned);
    }
}

fn apply_ready_voice(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let path = demo_media_allowlist()
        .join("demo-voice.ogg")
        .to_string_lossy()
        .into_owned();
    let wave = voice::waveform_base64(&[4, 16, 28, 12, 8, 20, 6, 18, 10, 24, 8, 14]);
    let incoming_file = demo_file_json(81, &path, true);
    let outgoing_file = demo_file_json(82, &path, true);
    let wave_json = serde_json::to_string(&wave).unwrap_or_else(|_| "\"\"".into());
    let incoming = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":90,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":12,"waveform":{wave_json},"mime_type":"audio/ogg","speech_recognition_result":null,"voice":{incoming_file}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"is_listened":false}}}}}}"#
    );
    let outgoing = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":91,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":3,"waveform":{wave_json},"mime_type":"audio/ogg","speech_recognition_result":null,"voice":{outgoing_file}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"is_listened":true}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [incoming, outgoing, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn apply_ready_typing(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let json = r#"{"@type":"updateChatAction","chat_id":11,"topic_id":null,"sender_id":{"@type":"messageSenderUser","user_id":11},"action":{"@type":"chatActionTyping"}}"#;
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

fn apply_ready_mute_archive(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let muted = ChatNotificationSettings::default().with_mute_for(MUTE_FOREVER);
    let jsons = [
        format!(
            r#"{{"@type":"updateChatNotificationSettings","chat_id":11,"notification_settings":{}}}"#,
            notification_settings_json(&muted)
        ),
        r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"0","is_pinned":false}}"#
            .to_string(),
        r#"{"@type":"updateChatRemovedFromList","chat_id":12,"chat_list":{"@type":"chatListMain"}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"20","is_pinned":false}}"#
            .to_string(),
        r#"{"@type":"updateChatAddedToList","chat_id":12,"chat_list":{"@type":"chatListArchive"}}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadySlowMode` fixture (Phase A1): a dedicated supergroup
/// ("Slow-mode demo group", chat id 17) with slow mode enabled
/// (`slow_mode_delay: 30`, `slow_mode_delay_expires_in: 25.0`) and the
/// viewer as a plain member (no bypass), opened with two messages — all
/// through the normal reducer, no live Telegram. The composer shows the
/// "Slow mode · wait Ns" countdown and blocks sends until it expires.
fn apply_ready_slow_mode(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 17i64;
    let extra = session.request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, chat_id);
    let description_json = serde_json::to_string(
        "Demo group with slow mode on: members wait 30 seconds between messages.",
    )
    .unwrap();
    let jsons = [
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Slow-mode demo group","type":{{"@type":"chatTypeSupergroup","supergroup_id":{chat_id},"is_channel":false}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"80","is_pinned":false}}}}"#
        ),
        // The viewer is a plain member — no slow-mode bypass.
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{chat_id},"is_forum":false,"status":{{"@type":"chatMemberStatusMember"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"supergroupFullInfo","@extra":"{}","description":{description_json},"member_count":128,"slow_mode_delay":30,"slow_mode_delay_expires_in":25.0,"my_boost_count":0,"unrestrict_boost_count":0}}"#,
            extra.0,
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":301,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":501}},"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Slow mode is on in this group: 30 seconds between messages.","entities":[]}}}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":302,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":502}},"is_outgoing":false,"date":1700000060,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Type below and hit Enter: Quill blocks the send until the timer expires.","entities":[]}}}}}}}}"#
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));
}

/// Parity slice: notification-sounds screenshot fixture — saved sounds
/// (`getSavedNotificationSounds` answer), all three scope defaults
/// (`getScopeNotificationSettings` answers, correlated via
/// `request_for_scope`), and chat 11 on a custom saved sound.
fn apply_ready_notification_sound(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let sound_path = demo_media_allowlist()
        .join("demo-voice.ogg")
        .to_string_lossy()
        .into_owned();
    let sound_json = |id: i64, file_id: i32, title: &str, duration: i32| {
        format!(
            r#"{{"@type":"notificationSound","id":{id},"duration":{duration},"date":0,"title":{title},"data":"","sound":{file}}}"#,
            file = demo_file_json(file_id, &sound_path, true),
            title = serde_json::to_string(title).unwrap(),
        )
    };
    let sounds_extra = session.request(RequestPurpose::GetSavedNotificationSounds, None);
    let mut jsons = vec![
        format!(
            r#"{{"@type":"notificationSounds","notification_sounds":[{a},{b}],"@extra":"{extra}"}}"#,
            a = sound_json(1, 91, "Ding", 2),
            b = sound_json(2, 92, "Chime", 3),
            extra = sounds_extra.0,
        ),
        {
            let chat_settings = ChatNotificationSettings {
                use_default_mute_for: true,
                mute_for: 0,
                use_default_sound: false,
                sound_id: 1,
                use_default_show_preview: false,
                show_preview: true,
                ..Default::default()
            };
            format!(
                r#"{{"@type":"updateChatNotificationSettings","chat_id":11,"notification_settings":{}}}"#,
                notification_settings_json(&chat_settings)
            )
        },
    ];
    for scope in NotificationSettingsScope::ALL {
        let extra = session.request_for_scope(RequestPurpose::GetScopeNotificationSettings, scope);
        jsons.push(format!(
            r#"{{"@type":"scopeNotificationSettings","mute_for":0,"sound_id":"-1","show_preview":true,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"-1","use_default_show_story_poster":true,"show_story_poster":true,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false,"@extra":"{extra}"}}"#,
            extra = extra.0,
        ));
    }
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn apply_ready_pin(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let extra = session.request(RequestPurpose::PinChatMessage, Some(ChatId(11)));
    let jsons = [
        format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        r#"{"@type":"updateMessageIsPinned","chat_id":11,"message_id":101,"is_pinned":true}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn interaction_info_update_json(
    chat_id: ChatId,
    message_id: MessageId,
    info: &MessageInteractionInfo,
) -> String {
    let reactions = match &info.reactions {
        Some(list) if !list.reactions.is_empty() => {
            let items: Vec<String> = list
                .reactions
                .iter()
                .filter_map(|reaction| {
                    let emoji = reaction.reaction_type.emoji_text()?;
                    Some(format!(
                        r#"{{"@type":"messageReaction","type":{{"@type":"reactionTypeEmoji","emoji":{}}},"total_count":{},"is_chosen":{},"used_sender_id":null,"recent_sender_ids":[]}}"#,
                        serde_json::to_string(emoji).unwrap_or_else(|_| "\"\"".into()),
                        reaction.total_count,
                        reaction.is_chosen
                    ))
                })
                .collect();
            format!(
                r#"{{"@type":"messageReactions","reactions":[{}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}"#,
                items.join(",")
            )
        }
        _ => "null".into(),
    };
    format!(
        r#"{{"@type":"updateMessageInteractionInfo","chat_id":{},"message_id":{},"interaction_info":{{"@type":"messageInteractionInfo","view_count":{},"forward_count":{},"reply_info":null,"reactions":{reactions}}}}}"#,
        chat_id.0, message_id.0, info.view_count, info.forward_count
    )
}

#[derive(Clone, Copy)]
enum DemoSeed {
    ReadyChats,
    UnreadBadge,
    AfterMarkRead,
    Media,
    SendMedia,
}

fn seed_demo_session(sink: Arc<MemorySink>, kind: DemoSeed) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let seq = AtomicU64::new(0);
    let a_unread = match kind {
        DemoSeed::ReadyChats | DemoSeed::Media | DemoSeed::SendMedia => 1,
        DemoSeed::UnreadBadge => 3,
        DemoSeed::AfterMarkRead => 3,
    };
    let jsons = [
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#
            .to_string(),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":11,"title":"Demo chat A","type":{{"@type":"chatTypePrivate","user_id":11}},"unread_count":{a_unread},"last_read_inbox_message_id":100,"last_read_outbox_message_id":0}}}}"#
        ),
        r#"{"@type":"updateNewChat","chat":{"id":12,"title":"Demo chat B","type":{"@type":"chatTypePrivate","user_id":12},"unread_count":0,"last_read_inbox_message_id":40,"last_read_outbox_message_id":0}}"#
            .to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":11,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"30","is_pinned":true}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"20","is_pinned":false}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":13,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"10","is_pinned":false}}"#
            .to_string(),
        r#"{"@type":"updateChatLastMessage","chat_id":11,"last_message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"30","is_pinned":true}]}"#
            .to_string(),
        r#"{"@type":"updateChatLastMessage","chat_id":12,"last_message":{"id":40,"chat_id":12,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Later.","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"20","is_pinned":false}]}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":102,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Reply from the session reducer.","entities":[]}}}}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":103,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Two more waiting.","entities":[]}}}}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":40,"chat_id":12,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Later.","entities":[]}}}}"#
            .to_string(),
    ];
    for json in jsons {
        if matches!(kind, DemoSeed::Media | DemoSeed::SendMedia)
            && (json.contains(r#""id":101"#)
                || json.contains(r#""id":102"#)
                || json.contains(r#""id":103"#))
        {
            continue;
        }
        if let Some(owned) = copy_and_parse(&json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    match kind {
        DemoSeed::ReadyChats => {
            session.open_chat(ChatId(11));
        }
        DemoSeed::UnreadBadge => {
            session.open_chat(ChatId(12));
        }
        DemoSeed::AfterMarkRead => {
            let follow = [
                r#"{"@type":"updateNewMessage","message":{"id":104,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Still waiting on a receipt.","entities":[]}}}}"#,
                r#"{"@type":"updateChatReadInbox","chat_id":11,"last_read_inbox_message_id":103,"unread_count":0}"#,
                r#"{"@type":"updateChatReadOutbox","chat_id":11,"last_read_outbox_message_id":102}"#,
            ];
            for json in follow {
                if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
            session.open_chat(ChatId(11));
        }
        DemoSeed::Media => {
            let thumb_path = demo_thumb_png_path();
            let loaded = demo_file_json(21, &thumb_path, true);
            let pending = demo_file_json(22, "", false);
            let full_pending = demo_file_json(23, "", false);
            let doc = demo_file_json(24, "", false);
            let follow = [
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":201,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{loaded},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"Loaded photo","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
                ),
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":202,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{pending},"width":320,"height":240,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{full_pending},"width":800,"height":600,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"Pending photo","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
                ),
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":203,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"notes.txt","mime_type":"text/plain","document":{doc}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
                ),
                r#"{"@type":"updateChatLastMessage","chat_id":11,"last_message":{"id":203,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageDocument","document":{"@type":"document","file_name":"notes.txt","mime_type":"text/plain","document":{"@type":"file","id":24,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}},"caption":{"@type":"formattedText","text":"","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"30","is_pinned":true}]}"#
                    .to_string(),
            ];
            for json in follow {
                if let Some(owned) = copy_and_parse(&json, &seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
            session.open_chat(ChatId(11));
        }
        DemoSeed::SendMedia => {
            let thumb_path = demo_thumb_png_path();
            let photo_file = demo_file_json(31, &thumb_path, true);
            let doc_path = demo_media_allowlist()
                .join("demo-notes.txt")
                .to_string_lossy()
                .into_owned();
            let doc_file = demo_file_json(32, &doc_path, true);
            let follow = [
                r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Send me a photo?","entities":[]}}}}"#
                    .to_string(),
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":302,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{photo_file},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"Outgoing photo","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
                ),
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":303,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"demo-notes.txt","mime_type":"text/plain","document":{doc_file}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
                ),
                r#"{"@type":"updateChatLastMessage","chat_id":11,"last_message":{"id":303,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageDocument","document":{"@type":"document","file_name":"demo-notes.txt","mime_type":"text/plain","document":{"@type":"file","id":32,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":true,"download_offset":0,"downloaded_prefix_size":24,"downloaded_size":24},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}},"caption":{"@type":"formattedText","text":"","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"30","is_pinned":true}]}"#
                    .to_string(),
            ];
            for json in follow {
                if let Some(owned) = copy_and_parse(&json, &seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
            session.open_chat(ChatId(11));
        }
    }
    session
}

fn demo_thumb_png_path() -> String {
    demo_media_allowlist()
        .join("demo-thumb.png")
        .to_string_lossy()
        .into_owned()
}

fn demo_media_allowlist() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots/fixtures")
}

fn demo_file_json(id: i32, path: &str, completed: bool) -> String {
    format!(
        r#"{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":{path},"can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}"#,
        path = serde_json::to_string(path).unwrap(),
        completed = completed,
    )
}

fn pane_placeholder(
    title: &'static str,
    body: impl Into<SharedString>,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    div()
        .id(title)
        .flex()
        .flex_col()
        .flex_1()
        .p_6()
        .gap_2()
        .child(div().font_semibold().child(title))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(body.into()),
        )
}

fn chat_list_caption(
    mode: PaneMode,
    session: Option<&Session>,
    folder: Option<i32>,
) -> SharedString {
    match mode {
        PaneMode::Synthetic => "Synthetic".into(),
        PaneMode::Connecting => "Waiting for Ready".into(),
        PaneMode::Ready => {
            if session.is_some_and(|s| s.search.open) {
                if session.is_some_and(|s| s.search.recents) {
                    "Recent".into()
                } else {
                    "Search".into()
                }
            } else if let (Some(session), Some(folder_id)) = (session, folder) {
                // Phase 7.1: a selected folder tab names the caption.
                let name = session.folder_name(folder_id).unwrap_or("Folder");
                let n = session.ordered_folder_chats(folder_id).len();
                format!("{name} · {n}").into()
            } else {
                let n = session.map(|s| s.ordered_chats().len()).unwrap_or(0);
                format!("Main list · {n}").into()
            }
        }
    }
}

fn title_bar(
    mode: PaneMode,
    live: bool,
    search_open: bool,
    chat_search_open: bool,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    let title = match mode {
        PaneMode::Synthetic => "Quill — synthetic chat",
        PaneMode::Connecting if live => "Quill — live TDLib",
        PaneMode::Connecting => "Quill — connecting",
        PaneMode::Ready if live => "Quill — chats",
        PaneMode::Ready => "Quill — chats (demo)",
    };
    let show_cycle = mode == PaneMode::Synthetic;
    div()
        .id("title")
        .h(px(44.))
        .px_4()
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(div().font_semibold().child(title))
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .flex_none()
                .child(
                    Button::new("older")
                        .label("Load older")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.load_older_action(cx);
                        })),
                )
                .when(mode == PaneMode::Ready, |this| {
                    this.child(
                        Button::new("search")
                            .label(if search_open {
                                "Close search"
                            } else {
                                "Search"
                            })
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.search_is_open() {
                                    this.close_search_ui(window, cx);
                                } else {
                                    this.open_search_ui(window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("find-in-chat")
                            .label(if chat_search_open {
                                "Close find"
                            } else {
                                "Find in chat"
                            })
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.chat_search_is_open() {
                                    this.close_chat_search_ui(window, cx);
                                } else {
                                    this.open_chat_search_ui(window, cx);
                                }
                            })),
                    )
                })
                .when(show_cycle, |this| {
                    this.child(
                        Button::new("cycle-auth")
                            .label("Cycle auth")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| this.cycle_auth(cx))),
                    )
                }),
        )
}

fn static_chat_row(
    title: &'static str,
    preview: &'static str,
    selected: bool,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    div()
        .id(title)
        .px_2()
        .py_2()
        .rounded_md()
        .bg(if selected {
            cx.theme().accent.opacity(0.15)
        } else {
            cx.theme().sidebar
        })
        .child(div().font_medium().child(title))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(preview),
        )
}

/// Phase 6: circular initials avatar (contact rows, info panels) shown
/// when no downloaded profile photo is available.
fn initials_avatar(name: &str, size: f32) -> impl IntoElement {
    let initials: String = name
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .collect();
    let initials = if initials.is_empty() {
        "?".to_string()
    } else {
        initials
    };
    div()
        .w(px(size))
        .h(px(size))
        .rounded_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(0x2f81f7))
        .text_color(rgb(0xffffff))
        .text_sm()
        .font_semibold()
        .child(initials)
}

/// Parity slice: deterministic fallback color for chat avatars, so every
/// chat type has a stable, recognizable circle before (or without) a
/// downloaded photo. Telegram's own palette, keyed on the chat id.
fn chat_avatar_color(chat_id: i64) -> Rgba {
    const PALETTE: [u32; 8] = [
        0xe17076, // red
        0xfaa774, // orange
        0xa695e7, // violet
        0x7bc862, // green
        0x6ec9cb, // teal
        0x65aadd, // blue
        0xcc6cbf, // pink
        0xee7aa2, // rose
    ];
    let index = (chat_id.unsigned_abs() % PALETTE.len() as u64) as usize;
    rgb(PALETTE[index])
}

/// Parity slice: circular chat avatar — the downloaded `chat.photo.small`
/// thumbnail when available, otherwise colored initials keyed on the chat
/// id. Used by chat-list rows, the conversation header, and info panels.
fn chat_avatar(
    name: &str,
    chat_id: i64,
    photo_path: Option<&std::path::Path>,
    size: f32,
) -> impl IntoElement {
    match photo_path {
        Some(path) => img(path)
            .id(("chat-avatar-photo", chat_id as u64))
            .w(px(size))
            .h(px(size))
            .rounded_full()
            .flex_shrink_0()
            .object_fit(ObjectFit::Cover)
            .into_any_element(),
        None => {
            let initials: String = name
                .split_whitespace()
                .filter_map(|word| word.chars().next())
                .take(2)
                .collect();
            let initials = if initials.is_empty() {
                "?".to_string()
            } else {
                initials
            };
            div()
                .id(("chat-avatar-initials", chat_id as u64))
                .w(px(size))
                .h(px(size))
                .rounded_full()
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(chat_avatar_color(chat_id))
                .text_color(rgb(0xffffff))
                .text_sm()
                .font_semibold()
                .child(initials)
                .into_any_element()
        }
    }
}

/// Parity slice: compact subscriber/member counts for the header
/// ("12.3K", "1.2M").
fn compact_count(count: i32) -> String {
    if count >= 1_000_000 {
        let value = count as f64 / 1_000_000.0;
        return format!(
            "{}{}",
            if value >= 100.0 {
                format!("{}", value as i64)
            } else {
                format!("{value:.1}")
            },
            "M"
        );
    }
    if count >= 1_000 {
        let value = count as f64 / 1_000.0;
        return format!(
            "{}{}",
            if value >= 100.0 {
                format!("{}", value as i64)
            } else {
                format!("{value:.1}")
            },
            "K"
        );
    }
    count.to_string()
}

/// Parity slice: one-line description snippet for the channel/supergroup
/// header.
fn description_snippet(description: &str, max_chars: usize) -> String {
    let one_line: String = description.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max_chars {
        return one_line;
    }
    let truncated: String = one_line.chars().take(max_chars).collect();
    format!("{truncated}…")
}

fn session_chat_row(
    chat: &ChatSummary,
    selected: bool,
    // Parity slice: `(folder id, name)` for folder-tag chips.
    folders: &[(i32, String)],
    // Parity slice: show folder-tag chips (`are_folder_tags_enabled`).
    show_tags: bool,
    // Parity slice: sandboxed display path for the downloaded chat photo
    // (`chat.photo.small`), if any; the avatar falls back to colored
    // initials otherwise.
    photo_path: Option<&std::path::Path>,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    let id = chat.id;
    let title = chat.title.clone();
    let preview = chat.sidebar_preview();
    let badge = unread_badge_text(chat.unread_count);
    let tags: Vec<String> = if show_tags {
        folders
            .iter()
            .filter(|(folder_id, _)| chat.folder_positions.contains_key(folder_id))
            .map(|(_, name)| name.clone())
            .collect()
    } else {
        Vec::new()
    };
    div()
        .id(("chat-row", id.0 as u64))
        .px_2()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .bg(if selected {
            cx.theme().accent.opacity(0.15)
        } else {
            cx.theme().sidebar
        })
        .on_click(cx.listener(move |this, _, window, cx| {
            this.select_listed_chat(id, window, cx);
        }))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                // Parity slice: circular chat photo or colored initials for
                // every chat-list row / chat type.
                .child(chat_avatar(&title, id.0, photo_path, 40.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .min_w_0()
                                        .child(div().font_medium().min_w_0().child(title))
                                        .when(chat.is_muted(), |this| this.child(muted_badge(id)))
                                        .when(chat.is_forum_chat(), |this| {
                                            this.child(forum_badge(id))
                                        })
                                        // Phase B1: lock indicator for
                                        // secret chats (E2E).
                                        .when(
                                            matches!(chat.kind, ChatKind::Secret { .. }),
                                            |this| this.child(secret_badge(id)),
                                        ),
                                )
                                .when_some(badge, |this, label| {
                                    this.child(unread_badge(label, id))
                                }),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(preview),
                        ),
                ),
        )
        .when(!tags.is_empty(), |this| {
            this.child(div().flex().flex_row().flex_wrap().gap_1().pt_1().children(
                tags.into_iter().map(|name| {
                    div()
                        .px_1()
                        .rounded_sm()
                        .bg(cx.theme().accent.opacity(0.12))
                        .text_xs()
                        .child(name)
                }),
            ))
        })
}

/// One `sponsoredMessage` row: Sponsored / Recommended label, title, content,
/// sponsor button, and a Report button when `can_be_reported` is set.
/// Media reuses the history attachment helpers, so thumbs download at
/// priority 1 and a tap fetches the full file at priority 32.
fn sponsored_message_row(
    chat_id: ChatId,
    message: &SponsoredMessage,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message.message_id as u64;
    let header = div()
        .id(("sponsored-row-header", row_id))
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .id(("sponsored-row-label", row_id))
                .text_xs()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(0x1f6feb))
                .text_color(rgb(0xffffff))
                .child(message.kind_label()),
        )
        .child(div().font_semibold().text_sm().child(message.title.clone()));
    let content: Option<AnyElement> = match &message.content {
        MessageContent::Text(text) => Some(message_text_block(
            (chat_id.0, row_id),
            text,
            files,
            downloading,
            media_roots,
            revealed,
            cx,
        )),
        MessageContent::Photo(photo) => Some(photo_attachment(
            row_id,
            photo,
            files,
            downloading,
            media_roots,
            Some((chat_id, message.message_id)),
            None,
            cx,
        )),
        MessageContent::Animation(animation) => Some(animation_attachment(
            MessageId(message.message_id),
            animation,
            files,
            downloading,
            media_roots,
            false,
            None,
            Some((chat_id, message.message_id)),
            cx,
        )),
        MessageContent::Video(video) => Some(video_attachment(
            MessageId(message.message_id),
            video,
            files,
            downloading,
            media_roots,
            false,
            None,
            Some((chat_id, message.message_id)),
            None,
            cx,
        )),
        MessageContent::Document(doc) => Some(document_chip(
            row_id,
            doc,
            files,
            downloading,
            Some((chat_id, message.message_id)),
            cx,
        )),
        _ => None,
    };
    let mut row = div()
        .id(("sponsored-row", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .px_3()
        .py_2()
        .rounded_lg()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().sidebar)
        .child(header);
    if let Some(content) = content {
        row = row.child(content);
    }
    if !message.sponsor.info.is_empty() {
        row = row.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(message.sponsor.info.clone()),
        );
    }
    if !message.additional_info.is_empty() {
        row = row.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(message.additional_info.clone()),
        );
    }
    let url = message.sponsor.url.clone();
    if !message.button_text.is_empty() && !url.is_empty() {
        let label = message.button_text.clone();
        let sponsored_id = message.message_id;
        row = row.child(
            Button::new(format!("sponsored-open-{row_id}"))
                .label(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.click_sponsored_message(chat_id, sponsored_id, false, cx);
                    this.open_message_url(&url, cx);
                })),
        );
    }
    if message.can_be_reported {
        let id = message.message_id;
        row = row.child(
            Button::new(format!("sponsored-report-{row_id}"))
                .label("Report")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.report_sponsored_message_ui(chat_id, id, cx);
                })),
        );
    }
    row.into_any_element()
}

fn muted_badge(chat_id: ChatId) -> impl IntoElement {
    div()
        .id(("muted-badge", chat_id.0 as u64))
        .h(px(18.))
        .px_1()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(0x6e7681))
        .text_color(rgb(0xffffff))
        .text_xs()
        .font_semibold()
        .child("Muted")
}

/// Phase B1: lock indicator for secret chats in the chat list (E2E).
fn secret_badge(chat_id: ChatId) -> impl IntoElement {
    div()
        .id(("secret-badge", chat_id.0 as u64))
        .h(px(18.))
        .px_1()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(0x1f6feb))
        .text_color(rgb(0xffffff))
        .text_xs()
        .font_semibold()
        .child("🔒")
}

/// Phase 5.1: forum indicator for forum supergroups in the chat list.
fn forum_badge(chat_id: ChatId) -> impl IntoElement {
    div()
        .id(("forum-badge", chat_id.0 as u64))
        .h(px(18.))
        .px_1()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(0x8250df))
        .text_color(rgb(0xffffff))
        .text_xs()
        .font_semibold()
        .child("Topics")
}

fn unread_badge(label: String, chat_id: ChatId) -> impl IntoElement {
    div()
        .id(("unread-badge", chat_id.0 as u64))
        .h(px(20.))
        .min_w(px(20.))
        .px_1()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(0x1f6feb))
        .text_color(rgb(0xffffff))
        .text_xs()
        .font_semibold()
        .child(label)
}

fn album_history_row(
    album_id: i64,
    messages: &[&HistoryMessage],
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    chat: Option<&ChatSummary>,
    sender_name: &str,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let Some(first) = messages.first() else {
        return div().into_any_element();
    };
    let sizes: Vec<(i32, i32)> = messages
        .iter()
        .map(|message| quill::album::album_pixel_size(&message.content))
        .collect();
    let layout = quill::album::layout_media_group(
        &sizes,
        quill::album::ALBUM_MAX_WIDTH,
        quill::album::ALBUM_MIN_WIDTH,
        quill::album::ALBUM_SPACING,
    );
    let (box_w, box_h) = quill::album::layout_bounds(&layout);
    let mut mosaic = div()
        .id(("album", album_id as u64))
        .relative()
        .w(px(box_w as f32))
        .h(px(box_h as f32))
        .overflow_hidden()
        .rounded_md();
    for (index, message) in messages.iter().enumerate() {
        let Some(part) = layout.get(index) else {
            continue;
        };
        let tile = album_tile(message, part, files, downloading, media_roots, cx);
        mosaic = mosaic.child(tile);
    }
    let caption = messages.iter().rev().find_map(|message| {
        let text = match &message.content {
            MessageContent::Photo(photo) => photo.caption.clone(),
            MessageContent::Video(video) => video.caption.clone(),
            _ => String::new(),
        };
        if text.is_empty() { None } else { Some(text) }
    });
    let label = if first.is_outgoing {
        let receipt = chat
            .map(|summary| summary.outbox_receipt(first))
            .unwrap_or(OutboxReceipt::Sent);
        outgoing_status_label(first.pending, receipt).to_string()
    } else {
        sender_name.to_string()
    };
    let reply_target = ComposerReplyTo::new(
        first.chat_id,
        first.id,
        caption.clone().unwrap_or_else(|| "Album".into()),
    );
    let reply_btn = Button::new(format!("reply-album-{}", album_id))
        .label("Reply")
        .ghost()
        .on_click(cx.listener(move |this, _, window, cx| {
            this.begin_reply_to(reply_target.clone(), window, cx);
        }));
    let extra = div()
        .id(("album-extra", album_id as u64))
        .flex()
        .flex_col()
        .gap_1()
        .child(mosaic)
        .when_some(caption, |this, text| {
            this.child(div().text_sm().child(text))
        })
        .child(reply_btn)
        .into_any_element();
    session_bubble_quoted(
        album_id as u64,
        label,
        String::new(),
        first.is_outgoing,
        Some(extra),
        None,
    )
}

fn album_tile(
    message: &HistoryMessage,
    part: &quill::album::AlbumRect,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message.id.0 as u64;
    let chat_id = message.chat_id;
    let message_id = message.id;
    // Parity slice 5: tiles open the fullscreen media viewer. The Play
    // button inside video tiles keeps its history-row playback — the
    // component button stops propagation on mouse-down, so the tile click
    // never double-fires.
    let frame = div()
        .id(("album-tile", row_id))
        .absolute()
        .left(px(part.x as f32))
        .top(px(part.y as f32))
        .w(px(part.width as f32))
        .h(px(part.height as f32))
        .overflow_hidden()
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_media_viewer(chat_id, message_id, cx);
        }));
    match &message.content {
        MessageContent::Photo(photo) => {
            if let Some(path) = photo_display_path(photo, files, media_roots) {
                frame
                    .child(
                        img(path)
                            .id(("album-photo", row_id))
                            .w(px(part.width as f32))
                            .h(px(part.height as f32))
                            .object_fit(ObjectFit::Cover)
                            .with_fallback(|| {
                                div()
                                    .size_full()
                                    .bg(rgb(0x444c56))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child("Photo")
                                    .into_any_element()
                            }),
                    )
                    .into_any_element()
            } else {
                let open_id = photo.open_file_id().unwrap_or(FileId(0));
                let downloading_now = file_is_downloading(open_id, files, downloading);
                frame
                    .bg(rgb(0x444c56))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0xffffff))
                            .child(if downloading_now {
                                "Photo — downloading…"
                            } else {
                                "Photo"
                            }),
                    )
                    .into_any_element()
            }
        }
        MessageContent::Video(video) => {
            let play_id = video.play_file_id().unwrap_or(FileId(0));
            let thumb_id = video.thumb_file_id().unwrap_or(FileId(0));
            let mime = video.mime_type.clone();
            let start_timestamp = video.start_timestamp;
            let message_id = message.id;
            let visual = [thumb_id, play_id].into_iter().find_map(|id| {
                if id.0 == 0 {
                    return None;
                }
                files
                    .get(&id.0)
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, media_roots))
            });
            let duration = format_voice_duration(video.duration);
            let picture = if let Some(path) = visual {
                img(path)
                    .id(("album-video", row_id))
                    .w(px(part.width as f32))
                    .h(px(part.height as f32))
                    .object_fit(ObjectFit::Cover)
                    .with_fallback(|| div().size_full().bg(rgb(0x238636)).into_any_element())
                    .into_any_element()
            } else {
                div()
                    .size_full()
                    .bg(rgb(0x238636))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().text_xs().text_color(rgb(0xffffff)).child("Video"))
                    .into_any_element()
            };
            frame
                .child(picture)
                .child(
                    div()
                        .absolute()
                        .bottom(px(4.))
                        .left(px(4.))
                        .px_1()
                        .rounded_sm()
                        .bg(rgb(0x010409))
                        .text_xs()
                        .text_color(rgb(0xffffff))
                        .child(format!("Video · {duration}")),
                )
                .child(
                    Button::new(format!("album-play-{row_id}"))
                        .label("Play")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_video_playback(
                                message_id,
                                play_id,
                                mime.clone(),
                                start_timestamp,
                                None,
                                cx,
                            );
                        })),
                )
                .into_any_element()
        }
        _ => frame.into_any_element(),
    }
}

/// Compact view-count formatting for broadcast posts (`👁 1.2K`, `👁 3.4M`).
fn format_view_count(count: i32) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

/// Phase 3.2: render `replyMarkupInlineKeyboard` as a button grid under the
/// message it belongs to. Parsing is generic (bot chats, groups, channels
/// all inherit it). Buttons stretch to share each row's width, like Telegram
/// desktop; empty rows are skipped.
fn inline_keyboard(message: &HistoryMessage, cx: &mut Context<QuillApp>) -> Option<AnyElement> {
    let keyboard = message.reply_markup.as_ref()?;
    let message_id = message.id.0 as u64;
    let mut grid = div()
        .id(("inline-keyboard", message_id))
        .flex()
        .flex_col()
        .gap_1()
        .mt_2();
    let mut any = false;
    for (row_index, row) in keyboard.rows.iter().enumerate() {
        if row.is_empty() {
            continue;
        }
        any = true;
        let mut line = div()
            .id(format!("inline-keyboard-row-{message_id}-{row_index}"))
            .flex()
            .gap_1();
        for (button_index, button) in row.iter().enumerate() {
            line = line.child(
                inline_keyboard_button(
                    message.chat_id,
                    message.id,
                    row_index,
                    button_index,
                    button,
                    cx,
                )
                .flex_1(),
            );
        }
        grid = grid.child(line);
    }
    any.then(|| grid.into_any_element())
}

/// Phase 3.2: one inline keyboard button. `Url` opens in the OS browser
/// (same gate as URLs in message text); `Callback` sends
/// `getCallbackQueryAnswer`; `SwitchInline` inserts the query into the
/// current chat's composer; `CopyText` copies to the clipboard. Everything
/// else (login/WebApp/password/game/buy/user buttons and unknown types)
/// renders disabled — known-but-unsupported is honest, and a crash is never
/// an option.
fn inline_keyboard_button(
    chat_id: ChatId,
    message_id: MessageId,
    row_index: usize,
    button_index: usize,
    button: &InlineKeyboardButton,
    cx: &mut Context<QuillApp>,
) -> Button {
    let label = if button.text.is_empty() {
        match &button.kind {
            InlineKeyboardButtonType::Unknown { type_name } if !type_name.is_empty() => {
                format!("({type_name})")
            }
            _ => "(button)".to_string(),
        }
    } else {
        button.text.clone()
    };
    let element = Button::new(format!(
        "inline-btn-{}-{row_index}-{button_index}",
        message_id.0
    ))
    .label(label)
    .tooltip(button_tooltip(button));
    let element = match button.style {
        InlineKeyboardButtonStyle::Primary => element.primary(),
        InlineKeyboardButtonStyle::Danger => element.danger(),
        InlineKeyboardButtonStyle::Success => element.success(),
        InlineKeyboardButtonStyle::Link => element.link(),
        InlineKeyboardButtonStyle::Default => element.ghost(),
    };
    match &button.kind {
        InlineKeyboardButtonType::Url { url } => {
            let url = url.clone();
            element.on_click(cx.listener(move |this, _, _, cx| {
                this.open_message_url(&url, cx);
            }))
        }
        InlineKeyboardButtonType::Callback { data } => {
            let data = data.clone();
            element.on_click(cx.listener(move |this, _, _, cx| {
                this.press_inline_callback(chat_id, message_id, data.clone(), cx);
            }))
        }
        InlineKeyboardButtonType::SwitchInline { query, .. } => {
            let query = query.clone();
            element.on_click(cx.listener(move |this, _, window, cx| {
                this.insert_switch_inline_query(&query, window, cx);
            }))
        }
        InlineKeyboardButtonType::CopyText { text } => {
            let text = text.clone();
            element.on_click(cx.listener(move |this, _, _, cx| {
                this.copy_inline_text(&text, cx);
            }))
        }
        _ => element.disabled(true),
    }
}

/// Short hint for unsupported / unknown inline buttons.
fn button_tooltip(button: &InlineKeyboardButton) -> &'static str {
    match &button.kind {
        InlineKeyboardButtonType::Url { .. }
        | InlineKeyboardButtonType::Callback { .. }
        | InlineKeyboardButtonType::SwitchInline { .. }
        | InlineKeyboardButtonType::CopyText { .. } => "",
        InlineKeyboardButtonType::LoginUrl { .. } => "Login buttons are not supported yet",
        InlineKeyboardButtonType::WebApp { .. } => "Web App buttons are not supported yet",
        InlineKeyboardButtonType::CallbackWithPassword { .. } => {
            "Password-protected buttons are not supported yet"
        }
        InlineKeyboardButtonType::CallbackGame => "Game buttons are not supported yet",
        InlineKeyboardButtonType::Buy => "Payment buttons are not supported yet",
        InlineKeyboardButtonType::User { .. } => "User buttons are not supported yet",
        InlineKeyboardButtonType::Disabled => "This button is disabled",
        InlineKeyboardButtonType::Unknown { .. } => "Unsupported button",
    }
}

/// Phase 4.2: `messagePoll` row — the question, one tappable option row per
/// option with a percentage bar and voter count, chosen option(s) marked;
/// closed polls render results without voting affordance. Vote taps go
/// through `setPollAnswer` (`QuillApp::vote_on_poll`); counts refresh live
/// via `updatePoll`.
fn poll_body(
    chat_id: ChatId,
    message_id: MessageId,
    content: &PollContent,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let poll = &content.poll;
    let kind_label = match &poll.poll_type {
        PollType::Quiz { .. } => "Quiz",
        PollType::Regular => "Poll",
    };
    let mut body = div()
        .id(("poll", message_id.0 as u64))
        .flex()
        .flex_col()
        .gap_1()
        .mt_1()
        .child(div().text_sm().font_semibold().child(poll.question.clone()));
    if !content.description.is_empty() {
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(0x8b949e))
                .child(content.description.clone()),
        );
    }
    body = body.child(div().text_xs().text_color(rgb(0x8b949e)).child(format!(
        "{} · {} · {}",
        kind_label,
        voter_count_label(poll.total_voter_count),
        if poll.is_closed {
            "closed"
        } else if poll.is_anonymous {
            "anonymous"
        } else {
            "public"
        },
    )));
    for (index, option) in poll.options.iter().enumerate() {
        body = body.child(poll_option_row(
            chat_id, message_id, index, option, poll, cx,
        ));
    }
    body.into_any_element()
}

/// One poll option: text + percentage, a bar split by `flex_grow`
/// (no percentage widths in GPUI), the per-option voter count, and the
/// chosen / quiz-correct marks. Tappable while the poll is open.
fn poll_option_row(
    chat_id: ChatId,
    message_id: MessageId,
    index: usize,
    option: &PollOption,
    poll: &quill::telegram::Poll,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let fill = poll_bar_fraction(option.vote_percentage);
    let chosen = option.is_chosen;
    let votable = poll.can_vote();
    let quiz_correct = poll.is_closed
        && matches!(&poll.poll_type, PollType::Quiz { correct_option_ids }
            if correct_option_ids.contains(&(index as i32)));
    let option_label = if chosen {
        format!("✓ {}", option.text)
    } else {
        option.text.clone()
    };
    let option_label = if quiz_correct {
        format!("{option_label} · correct answer")
    } else {
        option_label
    };
    let stats = if option.voter_count > 0 {
        format!(
            "{}% · {} {}",
            option.vote_percentage,
            option.voter_count,
            if option.voter_count == 1 {
                "vote"
            } else {
                "votes"
            },
        )
    } else {
        format!("{}%", option.vote_percentage)
    };
    let mut row = div()
        .id(("poll-option", message_id.0 as u64 * 64 + index as u64))
        .flex()
        .flex_col()
        .gap_0p5()
        .px_3()
        .py_1p5()
        .rounded_md()
        .border_1()
        .border_color(if chosen { rgb(0x58a6ff) } else { rgb(0x30363d) })
        .bg(rgb(0x161b22))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_sm()
                        .text_color(if quiz_correct {
                            rgb(0x3fb950)
                        } else {
                            rgb(0xe6edf3)
                        })
                        .child(option_label),
                )
                .child(div().text_sm().text_color(rgb(0x8b949e)).child(stats)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .h(px(6.))
                .rounded_md()
                .overflow_hidden()
                .bg(rgb(0x0d1117))
                .child(
                    div()
                        .flex_grow(fill)
                        .bg(if chosen { rgb(0x1f6feb) } else { rgb(0x30363d) }),
                )
                .child(div().flex_grow(1.0 - fill)),
        );
    if votable {
        row = row
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.vote_on_poll(chat_id, message_id, index, cx);
            }));
    }
    row.into_any_element()
}

fn session_history_row(
    message: &HistoryMessage,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    label: String,
    quote_preview: Option<String>,
    forward_from: Option<String>,
    selected_forward: bool,
    reaction_open: bool,
    // Seek-bar view for audio/voice rows (`None` for other content).
    seek_bar: Option<SeekBarView>,
    animation_playing: bool,
    animation_frame: Option<PathBuf>,
    video_playing: bool,
    video_frame: Option<PathBuf>,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Phase B4: whether the row's chat is a secret chat — selects the
    // "Self-destruct" vs "Auto-delete" service-row wording.
    is_secret: bool,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    // Phase B4: timer-change service rows (`messageChatSetMessageAutoDeleteTime`,
    // schema 1.8.67 line 5387) render as a centered neutral notice — no
    // bubble, no reply/react/edit/delete controls.
    if let MessageContent::ChatTtlChanged { secs } = &message.content {
        return div()
            .id(("ttl-service-row", message.id.0 as u64))
            .flex()
            .justify_center()
            .py_1()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(chat_ttl_service_label(*secs, is_secret)),
            )
            .into_any_element();
    }
    let quote = message.reply_to.as_ref().and_then(|reply| {
        let preview = quote_preview.clone()?;
        Some(reply_quote_strip(message.id, reply.message_id, preview, cx))
    });
    let forward_strip = forward_from.map(|label| forward_from_strip(message.id, label));
    let header = match (forward_strip, quote) {
        (Some(fwd), Some(reply)) => Some(
            div()
                .id(("row-headers", message.id.0 as u64))
                .flex()
                .flex_col()
                .child(fwd)
                .child(reply)
                .into_any_element(),
        ),
        (Some(fwd), None) => Some(fwd),
        (None, Some(reply)) => Some(reply),
        (None, None) => None,
    };
    let reply_id = format!("reply-{}", message.id.0);
    let reply_target = ComposerReplyTo::new(message.chat_id, message.id, message.content.preview());
    let reply_btn = Button::new(reply_id)
        .label("Reply")
        .ghost()
        .on_click(cx.listener(move |this, _, window, cx| {
            this.begin_reply_to(reply_target.clone(), window, cx);
        }));
    let chat_id = message.chat_id;
    let message_id = message.id;
    let pending = message.pending;
    let forward_btn = ForwardDraft::from_message(chat_id, message_id, pending).map(|_| {
        Button::new(format!("forward-{}", message_id.0))
            .label("Forward")
            .ghost()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.begin_forward_one(chat_id, message_id, pending, window, cx);
            }))
    });
    let select_btn = ForwardDraft::from_message(chat_id, message_id, pending).map(|_| {
        Button::new(format!("select-forward-{}", message_id.0))
            .label(if selected_forward {
                "Selected"
            } else {
                "Select"
            })
            .ghost()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_forward_select(chat_id, message_id, pending, cx);
            }))
    });
    let edit_btn = ComposerEdit::from_own_content(
        message.chat_id,
        message.id,
        message.is_outgoing,
        message.pending,
        &message.content,
    )
    .map(|edit| {
        Button::new(format!("edit-{}", message.id.0))
            .label("Edit")
            .ghost()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.begin_edit(edit.clone(), window, cx);
            }))
    });
    let delete_btn = DeleteConfirm::own(
        message.chat_id,
        message.id,
        message.is_outgoing,
        message.pending,
    )
    .map(|confirm| {
        Button::new(format!("delete-{}", message.id.0))
            .label("Delete")
            .ghost()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.begin_delete(confirm.clone(), cx);
            }))
    });
    let react_btn = message.can_react().then(|| {
        Button::new(format!("react-{}", message_id.0))
            .label(if reaction_open { "Reacting" } else { "React" })
            .ghost()
            .on_click(cx.listener(move |this, _, _, cx| {
                if this
                    .pending_react
                    .is_some_and(|(chat, id)| chat == chat_id && id == message_id)
                {
                    this.close_reaction_picker(cx);
                } else {
                    this.open_reaction_picker(chat_id, message_id, cx);
                }
            }))
    });
    let pin_btn = message.can_pin().then(|| {
        let pinned = message.is_pinned;
        Button::new(format!("pin-{}", message_id.0))
            .label(if pinned { "Unpin" } else { "Pin" })
            .ghost()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_pin_message(chat_id, message_id, cx);
            }))
    });
    // Broadcast posts (Phase 2.2): eye glyph + compact view count, like the
    // official clients' post footer. Renders whenever views exist; only
    // channel posts carry a view count in practice.
    let views_footer = message
        .interaction_info
        .as_ref()
        .map(|info| info.view_count)
        .filter(|&count| count > 0)
        .map(|count| {
            div()
                .id(("row-views", message_id.0 as u64))
                .mt_1()
                .text_xs()
                .opacity(0.75)
                .child(format!("👁 {}", format_view_count(count)))
        });
    let chips = message.emoji_reaction_chips();
    let chip_row = (!chips.is_empty()).then(|| {
        let mut row = div()
            .id(("reaction-chips", message_id.0 as u64))
            .flex()
            .flex_wrap()
            .gap_1()
            .mt_1();
        for (index, chip) in chips.into_iter().enumerate() {
            let Some(label) = chip.chip_label() else {
                continue;
            };
            let emoji = chip.reaction_type.emoji_text().unwrap_or("").to_string();
            let chosen = chip.is_chosen;
            row = row.child(
                div()
                    .id(("reaction-chip", message_id.0 as u64 * 64 + index as u64))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_xs()
                    .cursor_pointer()
                    .when(chosen, |this| {
                        this.bg(rgb(0x1f6feb))
                            .text_color(rgb(0xffffff))
                            .border_1()
                            .border_color(rgb(0x58a6ff))
                    })
                    .when(!chosen, |this| {
                        this.bg(rgb(0x21262d))
                            .text_color(rgb(0xc9d1d9))
                            .border_1()
                            .border_color(rgb(0x8b949e))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_emoji_reaction(chat_id, message_id, emoji.clone(), cx);
                    }))
                    .child(label),
            );
        }
        row
    });
    let text_body = match &message.content {
        MessageContent::Text(text) => Some(message_text_block(
            (message.chat_id.0, message.id.0 as u64),
            text,
            files,
            downloading,
            media_roots,
            revealed,
            cx,
        )),
        _ => None,
    };
    let extra_media = match &message.content {
        MessageContent::Photo(photo) => Some(photo_attachment(
            message.id.0 as u64,
            photo,
            files,
            downloading,
            media_roots,
            None,
            Some((message.chat_id, message.id)),
            cx,
        )),
        MessageContent::Document(doc) => Some(document_chip(
            message.id.0 as u64,
            doc,
            files,
            downloading,
            None,
            cx,
        )),
        MessageContent::Sticker(sticker) => Some(sticker_attachment(
            message.id.0 as u64,
            sticker,
            files,
            downloading,
            media_roots,
            cx,
        )),
        MessageContent::VoiceNote(note) => Some(voice_note_row(
            message.chat_id,
            message.id,
            message.is_outgoing,
            note,
            files,
            downloading,
            seek_bar.as_ref().expect("voice row always has a seek view"),
            cx,
        )),
        MessageContent::Audio(audio) => Some(audio_row(
            message.id,
            audio,
            files,
            downloading,
            media_roots,
            seek_bar.as_ref().expect("audio row always has a seek view"),
            cx,
        )),
        MessageContent::Animation(animation) => Some(animation_attachment(
            message.id,
            animation,
            files,
            downloading,
            media_roots,
            animation_playing,
            animation_frame.as_deref(),
            None,
            cx,
        )),
        MessageContent::Video(video) => Some(video_attachment(
            message.id,
            video,
            files,
            downloading,
            media_roots,
            video_playing,
            video_frame.as_deref(),
            None,
            Some((message.chat_id, message.id)),
            cx,
        )),
        MessageContent::VideoNote(note) => Some(video_note_attachment(
            message.chat_id,
            message.id,
            message.is_outgoing,
            note,
            files,
            downloading,
            media_roots,
            video_playing,
            video_frame.as_deref(),
            cx,
        )),
        MessageContent::Poll(poll) => Some(poll_body(message.chat_id, message.id, poll, cx)),
        MessageContent::Location(location) => Some(location_row(
            message.id.0 as u64,
            &location.location,
            location.live.as_ref(),
            cx,
        )),
        MessageContent::Venue(venue) => Some(venue_row(message.id.0 as u64, venue, cx)),
        MessageContent::Contact(contact) => Some(contact_row(message.id.0 as u64, contact)),
        MessageContent::Dice(dice) => Some(dice_row(message.id.0 as u64, dice)),
        MessageContent::Text(_)
        | MessageContent::ChatTtlChanged { .. }
        | MessageContent::Unsupported { .. } => None,
    };
    let keyboard = inline_keyboard(message, cx);
    // Phase B3: self-destruct timer badge (`message.self_destruct_type` /
    // `message.self_destruct_in`, schema 1.8.67 lines 3146–3147). The
    // countdown decays locally against `unix_ms_now()` (same pattern as
    // the Phase A1 slow-mode countdown); TDLib removes the row via
    // `updateDeleteMessages` when the timer fires. The 1-second render
    // tick (see `ensure_self_destruct_tick`) keeps this fresh.
    let self_destruct_badge = message.self_destruct_badge(unix_ms_now()).map(|label| {
        div()
            .id(("self-destruct-badge", message.id.0 as u64))
            .mt_1()
            .text_xs()
            .text_color(rgb(0xffd479))
            .child(label)
    });
    // Phase B4: auto-delete countdown chip (`message.auto_delete_in`,
    // schema 1.8.67 line 3148) — "🗑 59m left", decaying on the same
    // 1-second tick as the self-destruct badge.
    let auto_delete_chip = message.auto_delete_chip(unix_ms_now()).map(|label| {
        div()
            .id(("auto-delete-chip", message.id.0 as u64))
            .mt_1()
            .text_xs()
            .text_color(rgb(0xffd479))
            .child(label)
    });
    let extra = Some(
        div()
            .id(("bubble-extra", message.id.0 as u64))
            .when_some(extra_media, |this, media| this.child(media))
            .when_some(self_destruct_badge, |this, badge| this.child(badge))
            .when_some(auto_delete_chip, |this, chip| this.child(chip))
            .when_some(keyboard, |this, keyboard| this.child(keyboard))
            .when_some(views_footer, |this, footer| this.child(footer))
            .when_some(chip_row, |this, chips| this.child(chips))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(reply_btn)
                    .when_some(react_btn, |this, btn| this.child(btn))
                    .when_some(pin_btn, |this, btn| this.child(btn))
                    .when_some(forward_btn, |this, btn| this.child(btn))
                    .when_some(select_btn, |this, btn| this.child(btn))
                    .when_some(edit_btn, |this, btn| this.child(btn))
                    .when_some(delete_btn, |this, btn| this.child(btn)),
            )
            .into_any_element(),
    );
    // Captions carry entities too (Phase 4.1); they render through the same
    // rich-text path as message text. Everything else about captions is
    // unchanged (order in the bubble, preview text, media attachments).
    let caption: Option<(&str, &[TextEntity])> = match &message.content {
        MessageContent::Photo(photo) => (!photo.caption.is_empty())
            .then_some((photo.caption.as_str(), photo.caption_entities.as_slice())),
        MessageContent::Document(doc) => (!doc.caption.is_empty())
            .then_some((doc.caption.as_str(), doc.caption_entities.as_slice())),
        MessageContent::Animation(animation) => (!animation.caption.is_empty()).then_some((
            animation.caption.as_str(),
            animation.caption_entities.as_slice(),
        )),
        MessageContent::Video(video) => (!video.caption.is_empty())
            .then_some((video.caption.as_str(), video.caption_entities.as_slice())),
        MessageContent::VoiceNote(note) => (!note.caption.is_empty())
            .then_some((note.caption.as_str(), note.caption_entities.as_slice())),
        MessageContent::Audio(audio) => (!audio.caption.is_empty())
            .then_some((audio.caption.as_str(), audio.caption_entities.as_slice())),
        _ => None,
    };
    let unsupported_body = match &message.content {
        MessageContent::Unsupported { type_name } => format!("({type_name})"),
        _ => String::new(),
    };
    if let Some(text_body) = text_body {
        return session_bubble_rich(
            message.id.0 as u64,
            label,
            message.is_outgoing,
            text_body,
            extra,
            header,
        );
    }
    if let Some((caption_text, caption_entities)) = caption {
        return session_bubble_rich(
            message.id.0 as u64,
            label,
            message.is_outgoing,
            rich_text_line(
                caption_text,
                caption_entities,
                (message.chat_id.0, message.id.0 as u64),
                true,
                revealed,
                cx,
            ),
            extra,
            header,
        );
    }
    session_bubble_quoted(
        message.id.0 as u64,
        label,
        unsupported_body,
        message.is_outgoing,
        extra,
        header,
    )
}

/// Monospace family for `code` / `pre` entity runs (Phase 4.1). The generic
/// family resolves through the platform font stack (fontconfig on Linux).
const MONO_FONT: &str = "monospace";

/// Render message text or a caption with text-entity styling (Phase 4.1).
///
/// Runs come from `styled_runs`, so nested entities combine additively
/// (bold inside italic renders bold italic). `code` is a monospace chip,
/// `pre` a full-width monospace block, and spoilers render as opaque blocks
/// until tapped — reveal state lives on the app, keyed by
/// `(row_id, run index, is_caption)`. Links keep their existing
/// accent-color + open-on-click behavior.
fn rich_text_line(
    text: &str,
    entities: &[TextEntity],
    msg_key: (i64, u64),
    is_caption: bool,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let mut line = div()
        .id(("msg-rich-text", msg_key.1 * 2 + is_caption as u64))
        .text_sm()
        .flex()
        .flex_wrap()
        .gap_0();
    for (index, run) in styled_runs(text, entities).into_iter().enumerate() {
        if run.text.is_empty() {
            continue;
        }
        let run_id = format!(
            "msg-run-{}-{}-{}-{index}",
            msg_key.0, msg_key.1, is_caption as u64
        );
        let style = &run.style;
        let revealed =
            !style.spoiler || revealed.contains(&(msg_key.0, msg_key.1, index as u64, is_caption));
        if !revealed {
            let key = (msg_key.0, msg_key.1, index as u64, is_caption);
            line = line.child(
                div()
                    .id(run_id)
                    .bg(rgb(0x444c56))
                    .rounded_sm()
                    .px_1()
                    .text_color(rgb(0x444c56))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.spoiler_revealed.insert(key);
                        cx.notify();
                    }))
                    .child(run.text),
            );
            continue;
        }
        let mut el = div().id(run_id);
        if style.bold {
            el = el.font_weight(FontWeight::BOLD);
        }
        if style.italic {
            el = el.italic();
        }
        if style.underline {
            el = el.underline();
        }
        if style.strikethrough {
            el = el.line_through();
        }
        if style.code || style.pre {
            el = el.font_family(MONO_FONT);
        }
        if style.pre {
            el = el
                .w_full()
                .bg(rgb(0x22262d))
                .rounded_md()
                .px_2()
                .py_1()
                .my_1();
        } else if style.code {
            el = el.bg(rgb(0x444c56)).rounded_sm().px_1();
        }
        if let Some(href) = run.href {
            el = el
                .text_color(rgb(0x9ecbff))
                .underline()
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_message_url(&href, cx);
                }));
        }
        line = line.child(el.child(run.text));
    }
    line.into_any_element()
}

/// Phase 4.1: plain/link message text (with optional link-preview card).
/// `msg_key` is (chat id, message id); the spoiler-reveal lookup needs the
/// chat id because message ids are only unique within a chat.
fn message_text_block(
    msg_key: (i64, u64),
    text: &quill::telegram::envelope::TextContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = msg_key.1;
    let line = rich_text_line(&text.text, &text.entities, msg_key, false, revealed, cx);
    let card = text.link_preview.as_ref().and_then(|preview| {
        preview
            .has_card()
            .then(|| link_preview_card(row_id, preview, files, downloading, media_roots, cx))
    });
    let above = text
        .link_preview
        .as_ref()
        .is_some_and(|preview| preview.show_above_text);
    let has_text = !text.text.is_empty();
    let mut block = div().id(("msg-text-block", row_id)).flex().flex_col();
    if above {
        block = block.when_some(card, |this, card| this.child(card));
        if has_text {
            block = block.child(line);
        }
    } else {
        if has_text {
            block = block.child(line);
        }
        block = block.when_some(card, |this, card| this.child(card));
    }
    block.into_any_element()
}

fn link_preview_card(
    row_id: u64,
    preview: &quill::telegram::envelope::LinkPreview,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let url = preview.url.clone();
    let site_empty = preview.site_name.is_empty();
    let title_empty = preview.title.is_empty();
    let description_empty = preview.description.is_empty();
    let site = preview.site_name.clone();
    let title = preview.title.clone();
    let description = preview.description.clone();
    let display = if preview.display_url.is_empty() {
        preview.url.clone()
    } else {
        preview.display_url.clone()
    };
    let thumb = preview.photo.as_ref().map(|photo| {
        preview_thumb(
            row_id,
            photo,
            preview.show_large_media,
            files,
            downloading,
            media_roots,
        )
    });
    let mut copy = div()
        .id(("link-preview-copy", row_id))
        .flex()
        .flex_col()
        .min_w_0()
        .gap_0();
    if !site_empty {
        copy = copy.child(
            div()
                .text_xs()
                .font_medium()
                .text_color(rgb(0x58a6ff))
                .child(site),
        );
    }
    if !title_empty {
        copy = copy.child(div().text_sm().font_medium().child(title));
    }
    if !description_empty {
        copy = copy.child(div().text_xs().text_color(rgb(0xc9d1d9)).child(description));
    }
    if site_empty && title_empty && description_empty && !display.is_empty() {
        copy = copy.child(div().text_xs().text_color(rgb(0x58a6ff)).child(display));
    }
    let body = if preview.show_large_media {
        let mut column = div()
            .id(("link-preview-large", row_id))
            .flex()
            .flex_col()
            .gap_1();
        if preview.show_media_above_description {
            column = column.when_some(thumb, |this, thumb| this.child(thumb));
            column = column.child(copy);
        } else {
            column = column.child(copy);
            column = column.when_some(thumb, |this, thumb| this.child(thumb));
        }
        column.into_any_element()
    } else {
        div()
            .id(("link-preview-small", row_id))
            .flex()
            .items_start()
            .gap_2()
            .child(copy.flex_1())
            .when_some(thumb, |this, thumb| this.child(thumb))
            .into_any_element()
    };
    div()
        .id(("link-preview", row_id))
        .mt_2()
        .px_2()
        .py_1()
        .rounded_md()
        .border_l_2()
        .border_color(rgb(0x58a6ff))
        .bg(rgb(0x161b22))
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_message_url(&url, cx);
        }))
        .child(body)
        .into_any_element()
}

fn preview_thumb(
    row_id: u64,
    photo: &quill::telegram::envelope::PhotoContent,
    large: bool,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
) -> AnyElement {
    let (w, h) = if large {
        (px(240.), px(140.))
    } else {
        (px(72.), px(72.))
    };
    if let Some(path) = photo_display_path(photo, files, media_roots) {
        return img(path)
            .id(("link-preview-img", row_id))
            .w(w)
            .h(h)
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .flex_shrink_0()
            .with_fallback(move || {
                div()
                    .w(w)
                    .h(h)
                    .rounded_md()
                    .bg(rgb(0x444c56))
                    .into_any_element()
            })
            .into_any_element();
    }
    let file_id = photo
        .thumb_size()
        .map(|size| size.file_id)
        .unwrap_or(FileId(0));
    let label = if file_is_downloading(file_id, files, downloading) {
        "…"
    } else {
        "Preview"
    };
    div()
        .id(("link-preview-ph", row_id))
        .w(w)
        .h(h)
        .rounded_md()
        .bg(rgb(0x444c56))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .child(div().text_xs().text_color(rgb(0xffffff)).child(label))
        .into_any_element()
}

fn forward_from_strip(row_id: MessageId, label: String) -> AnyElement {
    div()
        .id(("forward-from", row_id.0 as u64))
        .mt_1()
        .mb_1()
        .px_2()
        .py_1()
        .rounded_md()
        .border_l_2()
        .border_color(rgb(0x3fb950))
        .bg(rgb(0x161b22))
        .child(
            div()
                .text_xs()
                .font_medium()
                .text_color(rgb(0x3fb950))
                .child(label),
        )
        .into_any_element()
}

fn forward_dest_row(id: ChatId, title: String, cx: &mut Context<QuillApp>) -> impl IntoElement {
    div()
        .id(("forward-dest", id.0 as u64))
        .px_2()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .bg(cx.theme().sidebar)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.submit_forward_to(id, cx);
        }))
        .child(div().font_medium().child(title))
}

fn reply_quote_strip(
    row_id: MessageId,
    target_id: MessageId,
    preview: String,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    div()
        .id(("reply-quote", row_id.0 as u64))
        .mt_1()
        .mb_1()
        .px_2()
        .py_1()
        .rounded_md()
        .border_l_2()
        .border_color(rgb(0x58a6ff))
        .bg(rgb(0x161b22))
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.jump_to_replied_message(target_id, cx);
        }))
        .child(
            div()
                .text_xs()
                .font_medium()
                .text_color(rgb(0x58a6ff))
                .child("Reply"),
        )
        .child(div().text_xs().text_color(rgb(0xc9d1d9)).child(preview))
        .into_any_element()
}

fn photo_display_path(
    photo: &quill::telegram::envelope::PhotoContent,
    files: &HashMap<i32, ParsedFile>,
    roots: &[PathBuf],
) -> Option<PathBuf> {
    let mut ids = Vec::new();
    if let Some(size) = photo.thumb_size() {
        ids.push(size.file_id);
    }
    if let Some(size) = photo.largest_size() {
        ids.push(size.file_id);
    }
    for id in ids {
        if let Some(path) = files.get(&id.0).and_then(|f| f.usable_path())
            && let Some(safe) = sandboxed_display_path(path, roots)
        {
            return Some(safe);
        }
    }
    for size in &photo.sizes {
        if let Some(path) = files.get(&size.file_id.0).and_then(|f| f.usable_path())
            && let Some(safe) = sandboxed_display_path(path, roots)
        {
            return Some(safe);
        }
    }
    None
}

/// Phase 4.5: resolve the current viewer item's visual — first local
/// display candidate inside the media allowlist roots, same sandbox rule as
/// history rows (`sandboxed_display_path`).
fn viewer_display_path(
    item: &MediaViewerItem,
    files: &HashMap<i32, ParsedFile>,
    roots: &[PathBuf],
) -> Option<PathBuf> {
    item.display_file_ids.iter().find_map(|id| {
        files
            .get(&id.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, roots))
    })
}

fn story_viewer_display_path(
    item: &StoryViewerItem,
    files: &HashMap<i32, ParsedFile>,
    roots: &[PathBuf],
) -> Option<PathBuf> {
    item.display_file_ids.iter().find_map(|id| {
        files
            .get(&id.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, roots))
    })
}

fn file_is_downloading(
    file_id: FileId,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
) -> bool {
    downloading.contains(&file_id.0)
        || files
            .get(&file_id.0)
            .is_some_and(|f| f.local.is_downloading_active)
}

fn photo_attachment(
    row_id: u64,
    photo: &quill::telegram::envelope::PhotoContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    sponsored: Option<(ChatId, i64)>,
    // Phase 4.5: `(chat_id, message_id)` when a click should open the
    // fullscreen viewer (history rows only; album tiles and sponsored rows
    // pass `None`).
    viewer: Option<(ChatId, MessageId)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let open_id = photo.open_file_id().unwrap_or(FileId(0));
    if !photo.is_secret
        && !photo.has_spoiler
        && let Some(path) = photo_display_path(photo, files, media_roots)
    {
        return img(path)
            .id(("photo-img", row_id))
            .mt_2()
            .w(px(240.))
            .h(px(140.))
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .when_some(viewer, |this, (chat_id, message_id)| {
                this.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_media_viewer(chat_id, message_id, cx);
                    }))
            })
            .with_fallback(|| {
                div()
                    .w(px(240.))
                    .h(px(140.))
                    .rounded_md()
                    .bg(rgb(0x444c56))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Photo")
                    .into_any_element()
            })
            .into_any_element();
    }
    let (w, h) = photo
        .largest_size()
        .or_else(|| photo.thumb_size())
        .map(|s| (s.width, s.height))
        .unwrap_or((0, 0));
    let downloading_now = file_is_downloading(open_id, files, downloading);
    let ready = files
        .get(&open_id.0)
        .and_then(|f| f.usable_path())
        .is_some();
    let status = if photo.is_secret || photo.has_spoiler {
        photo.placeholder_label(downloading_now, ready)
    } else if downloading_now {
        "Photo — downloading…".into()
    } else if w > 0 && h > 0 {
        format!("Photo {w}×{h} — not downloaded")
    } else {
        "Photo — not downloaded".into()
    };
    let viewable = !photo.is_secret && !photo.has_spoiler;
    let viewer_open = viewable.then_some(viewer).flatten();
    let has_viewer_open = viewer_open.is_some();
    div()
        .id(("photo-ph", row_id))
        .mt_2()
        .w(px(240.))
        .h(px(88.))
        .rounded_md()
        .bg(rgb(0x444c56))
        .flex()
        .items_center()
        .justify_center()
        // Phase 4.5: viewable photos open the viewer (it triggers the
        // download when needed); spoiler photos keep the old
        // click-to-download placeholder, secret photos stay inert.
        .when_some(viewer_open, |this, (chat_id, message_id)| {
            this.cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_media_viewer(chat_id, message_id, cx);
                }))
        })
        .when(
            !has_viewer_open && photo.click_requests_download(),
            |this| {
                this.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.request_media_download(open_id, sponsored, cx);
                    }))
            },
        )
        .child(div().text_xs().text_color(rgb(0xffffff)).child(status))
        .into_any_element()
}

fn animation_attachment(
    message_id: MessageId,
    animation: &quill::telegram::envelope::AnimationContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    playing: bool,
    frame: Option<&std::path::Path>,
    sponsored: Option<(ChatId, i64)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let play_id = animation.play_file_id().unwrap_or(FileId(0));
    let thumb_id = animation.thumb_file_id().unwrap_or(FileId(0));
    let mime = animation.mime_type.clone();
    let play_label = if playing { "Pause" } else { "Play" };
    let visual = if playing {
        frame.and_then(|path| sandboxed_display_path(&path.to_string_lossy(), media_roots))
    } else {
        None
    };
    let visual = visual.or_else(|| {
        [thumb_id, play_id].into_iter().find_map(|id| {
            if id.0 == 0 {
                return None;
            }
            files
                .get(&id.0)
                .and_then(|file| file.usable_path())
                .and_then(|path| sandboxed_display_path(path, media_roots))
        })
    });
    let downloading_now = file_is_downloading(play_id, files, downloading)
        || file_is_downloading(thumb_id, files, downloading);
    let blocked = animation.is_secret || animation.has_spoiler;
    let picture = if !blocked && let Some(path) = visual {
        img(path)
            .id(("gif-img", row_id))
            .w(px(240.))
            .h(px(140.))
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .with_fallback(|| {
                div()
                    .w(px(240.))
                    .h(px(140.))
                    .rounded_md()
                    .bg(rgb(0x1f6feb))
                    .into_any_element()
            })
            .into_any_element()
    } else {
        let label = if blocked {
            "GIF".to_string()
        } else if downloading_now {
            "GIF — downloading…".into()
        } else if animation.width > 0 && animation.height > 0 {
            format!(
                "GIF {}×{} — not downloaded",
                animation.width, animation.height
            )
        } else {
            "GIF — not downloaded".into()
        };
        div()
            .id(("gif-ph", row_id))
            .w(px(240.))
            .h(px(140.))
            .rounded_md()
            .bg(rgb(0x1f6feb))
            .flex()
            .items_center()
            .justify_center()
            .child(div().text_xs().text_color(rgb(0xffffff)).child(label))
            .into_any_element()
    };
    div()
        .id(("gif-row", row_id))
        .mt_2()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div().relative().child(picture).child(
                div()
                    .absolute()
                    .top_1()
                    .left_1()
                    .px_1()
                    .rounded_sm()
                    .bg(rgb(0x0d1117))
                    .text_xs()
                    .text_color(rgb(0xffffff))
                    .child(if playing { "GIF · playing" } else { "GIF" }),
            ),
        )
        .child(
            Button::new(format!("gif-play-{row_id}"))
                .label(play_label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if blocked {
                        return;
                    }
                    if let Some((chat_id, sponsored_id)) = sponsored {
                        this.click_sponsored_message(chat_id, sponsored_id, true, cx);
                    }
                    this.toggle_animation_playback(message_id, play_id, mime.clone(), cx);
                })),
        )
        .into_any_element()
}

fn video_attachment(
    message_id: MessageId,
    video: &quill::telegram::envelope::VideoContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    playing: bool,
    frame: Option<&std::path::Path>,
    sponsored: Option<(ChatId, i64)>,
    // Phase 4.5: `(chat_id, message_id)` when a click should open the
    // fullscreen viewer (history rows only; sponsored rows pass `None`).
    // Secret/spoiler videos never get the handler.
    viewer: Option<(ChatId, MessageId)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let play_id = video.play_file_id().unwrap_or(FileId(0));
    let thumb_id = video.thumb_file_id().unwrap_or(FileId(0));
    let mime = video.mime_type.clone();
    let start_timestamp = video.start_timestamp;
    let play_label = if playing { "Pause" } else { "Play" };
    let duration = format_voice_duration(video.duration);
    let visual = if playing {
        frame.and_then(|path| sandboxed_display_path(&path.to_string_lossy(), media_roots))
    } else {
        None
    };
    let visual = visual.or_else(|| {
        [thumb_id, play_id].into_iter().find_map(|id| {
            if id.0 == 0 {
                return None;
            }
            files
                .get(&id.0)
                .and_then(|file| file.usable_path())
                .and_then(|path| sandboxed_display_path(path, media_roots))
        })
    });
    let downloading_now = file_is_downloading(play_id, files, downloading)
        || file_is_downloading(thumb_id, files, downloading);
    let blocked = video.is_secret || video.has_spoiler;
    let picture = if !blocked && let Some(path) = visual {
        img(path)
            .id(("video-img", row_id))
            .w(px(240.))
            .h(px(140.))
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .with_fallback(|| {
                div()
                    .w(px(240.))
                    .h(px(140.))
                    .rounded_md()
                    .bg(rgb(0x238636))
                    .into_any_element()
            })
            .into_any_element()
    } else {
        let label = if blocked {
            "Video".to_string()
        } else if downloading_now {
            "Video — downloading…".into()
        } else if video.width > 0 && video.height > 0 {
            format!("Video {}×{} — not downloaded", video.width, video.height)
        } else {
            "Video — not downloaded".into()
        };
        div()
            .id(("video-ph", row_id))
            .w(px(240.))
            .h(px(140.))
            .rounded_md()
            .bg(rgb(0x238636))
            .flex()
            .items_center()
            .justify_center()
            .child(div().text_xs().text_color(rgb(0xffffff)).child(label))
            .into_any_element()
    };
    div()
        .id(("video-row", row_id))
        .mt_2()
        .flex()
        .flex_col()
        .gap_1()
        .child({
            // Phase 4.5: clicking the visual opens the viewer (the viewer
            // triggers the download when nothing is local yet). The
            // Play/Pause button below keeps its own handler.
            let viewer_open = (!blocked).then_some(viewer).flatten();
            div()
                .id(("video-visual", row_id))
                .relative()
                .when_some(viewer_open, |this, (chat_id, message_id)| {
                    this.cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_media_viewer(chat_id, message_id, cx);
                        }))
                })
                .child(picture)
                .child(
                    div()
                        .absolute()
                        .top_1()
                        .left_1()
                        .px_1()
                        .rounded_sm()
                        .bg(rgb(0x0d1117))
                        .text_xs()
                        .text_color(rgb(0xffffff))
                        .child(if playing { "Video · playing" } else { "Video" }),
                )
                .child(
                    div()
                        .absolute()
                        .bottom_1()
                        .left_1()
                        .px_1()
                        .rounded_sm()
                        .bg(rgb(0x0d1117))
                        .text_xs()
                        .text_color(rgb(0xffffff))
                        .child(duration),
                )
        })
        .child(
            Button::new(format!("video-play-{row_id}"))
                .label(play_label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if blocked {
                        return;
                    }
                    if let Some((chat_id, sponsored_id)) = sponsored {
                        this.click_sponsored_message(chat_id, sponsored_id, true, cx);
                    }
                    this.toggle_video_playback(
                        message_id,
                        play_id,
                        mime.clone(),
                        start_timestamp,
                        None,
                        cx,
                    );
                })),
        )
        .into_any_element()
}

/// Round video note. tdesktop paints `history/view/media` round video as a
/// circle (JPEG thumb, duration, play). Quill uses the same ffmpeg frames as
/// `messageVideo`, clipped to a circle. Diameter on screen is fixed; schema
/// `length` is the sender's pixel size, shown when the file is not local yet.
fn video_note_attachment(
    chat_id: ChatId,
    message_id: MessageId,
    outgoing: bool,
    note: &quill::telegram::envelope::VideoNoteContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    playing: bool,
    frame: Option<&std::path::Path>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let play_id = note.play_file_id().unwrap_or(FileId(0));
    let thumb_id = note.thumb_file_id().unwrap_or(FileId(0));
    let play_label = if playing { "Pause" } else { "Play" };
    let duration = format_voice_duration(note.duration);
    let visual = if playing {
        frame.and_then(|path| sandboxed_display_path(&path.to_string_lossy(), media_roots))
    } else {
        None
    };
    let visual = visual.or_else(|| {
        [thumb_id, play_id].into_iter().find_map(|id| {
            if id.0 == 0 {
                return None;
            }
            files
                .get(&id.0)
                .and_then(|file| file.usable_path())
                .and_then(|path| sandboxed_display_path(path, media_roots))
        })
    });
    let downloading_now = file_is_downloading(play_id, files, downloading)
        || file_is_downloading(thumb_id, files, downloading);
    let blocked = note.is_secret;
    let unseen = !outgoing && !note.is_viewed && !playing;
    let badge = if playing {
        "Video note · playing"
    } else if unseen {
        "New · Video note"
    } else {
        "Video note"
    };
    let ring = if playing {
        rgb(0x3fb950)
    } else if unseen {
        rgb(0x58a6ff)
    } else {
        rgb(0x8b949e)
    };
    let picture = if !blocked && let Some(path) = visual {
        img(path)
            .id(("video-note-img", row_id))
            .size(px(200.))
            .rounded(px(100.))
            .object_fit(ObjectFit::Cover)
            .with_fallback(|| {
                div()
                    .size(px(200.))
                    .rounded(px(100.))
                    .bg(rgb(0x238636))
                    .into_any_element()
            })
            .into_any_element()
    } else {
        let label = if blocked {
            "Video note".to_string()
        } else if downloading_now {
            "Video note — downloading…".into()
        } else if note.length > 0 {
            format!("Video note {} — not downloaded", note.length)
        } else {
            "Video note — not downloaded".into()
        };
        div()
            .id(("video-note-ph", row_id))
            .size(px(200.))
            .rounded(px(100.))
            .bg(rgb(0x238636))
            .flex()
            .items_center()
            .justify_center()
            .px_2()
            .child(
                div()
                    .text_xs()
                    .text_center()
                    .text_color(rgb(0xffffff))
                    .child(label),
            )
            .into_any_element()
    };
    let viewed = note.is_viewed;
    div()
        .id(("video-note", row_id))
        .mt_2()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .relative()
                .size(px(200.))
                .rounded(px(100.))
                .overflow_hidden()
                .border_2()
                .border_color(ring)
                .child(picture)
                .child(
                    div()
                        .absolute()
                        .top(px(72.))
                        .left(px(16.))
                        .w(px(168.))
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .px_1()
                                .rounded_sm()
                                .bg(rgb(0x0d1117))
                                .text_xs()
                                .text_color(rgb(0xffffff))
                                .child(badge),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .bottom(px(28.))
                        .left(px(16.))
                        .w(px(168.))
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .px_1()
                                .rounded_sm()
                                .bg(rgb(0x0d1117))
                                .text_xs()
                                .text_color(rgb(0xffffff))
                                .child(duration),
                        ),
                ),
        )
        .child(
            Button::new(format!("video-note-play-{row_id}"))
                .label(play_label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if blocked {
                        return;
                    }
                    this.toggle_video_playback(
                        message_id,
                        play_id,
                        "video/mp4".into(),
                        0,
                        if viewed { None } else { Some(chat_id) },
                        cx,
                    );
                })),
        )
        .into_any_element()
}

fn sticker_attachment(
    row_id: u64,
    sticker: &quill::telegram::envelope::StickerContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let display_id = sticker.display_file_id().unwrap_or(FileId(0));
    let fallback_label = sticker_label(sticker);
    if let Some(path) = files
        .get(&display_id.0)
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots))
    {
        let fallback_label = fallback_label.clone();
        return img(path)
            .id(("sticker-img", row_id))
            .mt_2()
            .w(px(128.))
            .h(px(128.))
            .object_fit(ObjectFit::Contain)
            .with_fallback(move || {
                div()
                    .w(px(128.))
                    .h(px(128.))
                    .rounded_md()
                    .bg(rgb(0x444c56))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(fallback_label.clone())
                    .into_any_element()
            })
            .into_any_element();
    }
    let downloading_now = file_is_downloading(display_id, files, downloading);
    let label = if downloading_now {
        format!("{} — downloading…", sticker_label(sticker))
    } else if display_id.0 == 0 {
        sticker_label(sticker)
    } else {
        format!("{} — not downloaded", sticker_label(sticker))
    };
    div()
        .id(("sticker-ph", row_id))
        .mt_2()
        .w(px(128.))
        .h(px(88.))
        .rounded_md()
        .bg(rgb(0x444c56))
        .flex()
        .items_center()
        .justify_center()
        .when(display_id.0 != 0, |this| {
            this.cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.request_media_download(display_id, None, cx);
                }))
        })
        .child(div().text_xs().text_color(rgb(0xffffff)).child(label))
        .into_any_element()
}

fn sticker_label(sticker: &quill::telegram::envelope::StickerContent) -> String {
    if sticker.emoji.is_empty() {
        "Sticker".into()
    } else {
        format!("Sticker {}", sticker.emoji)
    }
}

fn waveform_row(row_key: u64, bars: &[u8]) -> impl IntoElement {
    let mut row = div()
        .id(("waveform", row_key))
        .flex()
        .items_end()
        .gap_0()
        .h(px(28.));
    let shown: Vec<u8> = if bars.is_empty() {
        vec![6, 10, 14, 8, 12]
    } else {
        bars.iter().copied().take(48).collect()
    };
    for (index, bar) in shown.into_iter().enumerate() {
        let h = 4.0 + f32::from(bar.min(31)) * 0.7;
        row = row.child(
            div()
                .id(("wave-bar", row_key * 64 + index as u64))
                .w(px(3.))
                .h(px(h))
                .rounded_sm()
                .bg(rgb(0x58a6ff)),
        );
    }
    row
}

/// Phase 4.6 seek bar (tdesktop-style): the interactive gpui-component
/// `Slider` on the active row — click-to-seek and drag, with the UI layer
/// restarting ffplay at the released offset via `-ss` — and a static
/// track + fill on every other audio/voice row.
fn seek_bar_element(row_key: u64, seek: &SeekBarView) -> AnyElement {
    const BAR: u32 = 0x58a6ff;
    if let Some(slider) = &seek.slider {
        div()
            .id(("seek-bar", row_key))
            .w_full()
            .child(Slider::new(slider).bg(rgb(BAR)).text_color(rgb(0xffffff)))
            .into_any_element()
    } else {
        div()
            .id(("seek-bar", row_key))
            .w_full()
            .h(px(6.))
            .rounded_full()
            .bg(rgb(0x30363d))
            .child(
                div()
                    .h_full()
                    .w(relative(seek.fraction() as f32))
                    .rounded_full()
                    .bg(rgb(BAR)),
            )
            .into_any_element()
    }
}

fn voice_note_row(
    chat_id: ChatId,
    message_id: MessageId,
    outgoing: bool,
    note: &quill::telegram::envelope::VoiceNoteContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    seek: &SeekBarView,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let file_id = note.file_id;
    let ready = files
        .get(&file_id.0)
        .and_then(|file| file.usable_path())
        .is_some();
    let downloading_now = file_is_downloading(file_id, files, downloading);
    let bars = voice::waveform_bars_from_bytes(&note.waveform);
    let listened = note.is_listened;
    let note_duration = f64::from(note.duration);
    let active = seek.slider.is_some();
    let play_label = if seek.is_playing {
        "Pause"
    } else if downloading_now {
        "Downloading"
    } else {
        "Play"
    };
    // Phase 4.6: the active row shows elapsed / total (tdesktop-style).
    let total = format_voice_duration(note.duration);
    let mut meta = if active {
        format!(
            "{} / {total}",
            format_voice_duration(seek.display_secs as i32)
        )
    } else {
        total
    };
    if !outgoing && !note.is_listened && !active {
        meta = format!("New · {meta}");
    }
    if !ready && !downloading_now {
        meta = format!("{meta} · not downloaded");
    } else if downloading_now {
        meta = format!("{meta} · downloading…");
    } else if seek.is_playing {
        meta = format!("Playing · {meta}");
    } else if active {
        meta = format!("{meta} · paused");
    }
    div()
        .id(("voice-note", message_id.0 as u64))
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(if active { rgb(0x3fb950) } else { rgb(0x8b949e) })
        .bg(rgb(0x21262d))
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new(format!("voice-play-{}", message_id.0))
                        .label(play_label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_voice_playback(
                                chat_id,
                                message_id,
                                file_id,
                                listened,
                                note_duration,
                                cx,
                            );
                        })),
                )
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(rgb(0xffffff))
                        .child("Voice message"),
                )
                .child(div().text_xs().text_color(rgb(0xffffff)).child(meta)),
        )
        .child(waveform_row(message_id.0 as u64, &bars))
        .child(seek_bar_element(message_id.0 as u64, seek))
        .into_any_element()
}

fn audio_row(
    message_id: MessageId,
    audio: &quill::telegram::envelope::AudioContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    seek: &SeekBarView,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let file_id = audio.file_id;
    let ready = files
        .get(&file_id.0)
        .and_then(|file| file.usable_path())
        .is_some();
    let downloading_now = file_is_downloading(file_id, files, downloading);
    let audio_duration = f64::from(audio.duration);
    let active = seek.slider.is_some();
    let play_label = if seek.is_playing {
        "Pause"
    } else if downloading_now {
        "Downloading"
    } else {
        "Play"
    };
    let title = if !audio.title.is_empty() {
        audio.title.clone()
    } else if !audio.file_name.is_empty() {
        audio.file_name.clone()
    } else {
        "Audio".to_string()
    };
    // Phase 4.6: the active row shows elapsed / total (tdesktop-style).
    let total = voice::format_voice_duration(audio.duration);
    let duration_label = if active {
        format!(
            "{} / {total}",
            voice::format_voice_duration(seek.display_secs as i32)
        )
    } else {
        total
    };
    let mut meta = duration_label;
    if !audio.performer.is_empty() {
        meta = format!("{} · {meta}", audio.performer);
    }
    if seek.is_playing {
        meta = format!("Playing · {meta}");
    } else if downloading_now {
        meta = format!("{meta} · downloading…");
    } else if !ready {
        meta = format!("{meta} · not downloaded");
    } else if active {
        meta = format!("{meta} · paused");
    }
    let cover_id = audio.cover_file_id().unwrap_or(FileId(0));
    let cover = files
        .get(&cover_id.0)
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots));
    let cover_box = if let Some(path) = cover {
        img(path)
            .id(("audio-cover", message_id.0 as u64))
            .w(px(56.))
            .h(px(56.))
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .with_fallback(|| {
                div()
                    .w(px(56.))
                    .h(px(56.))
                    .rounded_md()
                    .bg(rgb(0x444c56))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Audio")
                    .into_any_element()
            })
            .into_any_element()
    } else {
        div()
            .id(("audio-cover-ph", message_id.0 as u64))
            .w(px(56.))
            .h(px(56.))
            .rounded_md()
            .bg(rgb(0x444c56))
            .flex()
            .items_center()
            .justify_center()
            .child(div().text_xs().text_color(rgb(0xffffff)).child("Audio"))
            .into_any_element()
    };
    div()
        .id(("audio", message_id.0 as u64))
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(if active { rgb(0x3fb950) } else { rgb(0x8b949e) })
        .bg(rgb(0x21262d))
        .flex()
        .items_center()
        .gap_3()
        .child(cover_box)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(rgb(0xffffff))
                        .child(title),
                )
                .child(div().text_xs().text_color(rgb(0xc9d1d9)).child(meta))
                .child(seek_bar_element(message_id.0 as u64, seek))
                .child(
                    Button::new(format!("audio-play-{}", message_id.0))
                        .label(play_label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_audio_playback(message_id, file_id, audio_duration, cx);
                        })),
                ),
        )
        .into_any_element()
}

fn document_chip(
    row_id: u64,
    doc: &quill::telegram::envelope::DocumentContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    sponsored: Option<(ChatId, i64)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let file_id = doc.file_id;
    let file = files.get(&file_id.0);
    let ready = file.and_then(|f| f.usable_path()).is_some();
    let downloading_now = file_is_downloading(file_id, files, downloading);
    let size = file.map(|f| f.display_size()).unwrap_or(0);
    let size_label = format_bytes(size);
    let state = if ready {
        "ready"
    } else if downloading_now {
        "downloading…"
    } else {
        "not downloaded"
    };
    let mut detail = doc.mime_type.clone();
    if !size_label.is_empty() {
        if !detail.is_empty() {
            detail.push_str(" · ");
        }
        detail.push_str(&size_label);
    }
    if !detail.is_empty() {
        detail.push_str(" · ");
    }
    detail.push_str(state);
    let name = if doc.file_name.is_empty() {
        "Document".to_string()
    } else {
        doc.file_name.clone()
    };
    div()
        .id(("doc-chip", row_id))
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x8b949e))
        .bg(rgb(0x21262d))
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.request_media_download(file_id, sponsored, cx);
        }))
        .child(div().text_sm().font_medium().child(name))
        .child(div().text_xs().text_color(rgb(0xc9d1d9)).child(detail))
        .into_any_element()
}

fn format_bytes(n: i64) -> String {
    if n <= 0 {
        String::new()
    } else if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{} KB", n / 1024)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
}

/// Phase 4.3: `messageLocation` / `messageLiveLocation` row. A static map
/// placeholder chip (no live tiles): pin glyph, coordinate line, live
/// status when the message is a live location, and a tappable "Open map"
/// link. The link opens an OpenStreetMap URL through
/// `platform::open_external_url` (https only, scheme-gated — no `geo:`
/// or `tel:` schemes). Live re-rendering is out of scope: the
/// live-period/expires state is a static snapshot from parse time.
fn location_row(
    row_id: u64,
    location: &quill::telegram::envelope::GeoLocation,
    live: Option<&quill::telegram::envelope::LiveLocationState>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let url = location.open_street_map_url();
    let header = if live.is_some() {
        "📍 Live location"
    } else {
        "📍 Location"
    };
    let mut body = div()
        .id(("location-row", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x8b949e))
        .bg(rgb(0x21262d))
        .child(div().text_sm().font_medium().child(header))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0xc9d1d9))
                .child(location.coords_label()),
        );
    if let Some(live) = live {
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(0x58a6ff))
                .child(live.status_label()),
        );
    } else if location.accuracy_m > 0 {
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(0x8b949e))
                .child(format!("accuracy ±{} m", location.accuracy_m)),
        );
    }
    body.child(
        div()
            .id(("location-open-map", row_id))
            .text_sm()
            .text_color(rgb(0x58a6ff))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_message_url(&url, cx);
            }))
            .child("🗺 Open map"),
    )
    .into_any_element()
}

/// Phase 4.3: `messageVenue` row — venue title, address, optional provider
/// subtitle, and a tappable "Open map" link on the venue's coordinates
/// (same OpenStreetMap handling as `location_row`). The provider `id` /
/// `type` are not kept in the model (see `VenueContent`).
fn venue_row(
    row_id: u64,
    venue: &quill::telegram::envelope::VenueContent,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let url = venue.location.open_street_map_url();
    let title = if venue.title.is_empty() {
        "Venue".to_string()
    } else {
        venue.title.clone()
    };
    let mut body = div()
        .id(("venue-row", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x8b949e))
        .bg(rgb(0x21262d))
        .child(div().text_sm().font_medium().child(format!("📍 {title}")));
    if !venue.address.is_empty() {
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(0xc9d1d9))
                .child(venue.address.clone()),
        );
    }
    let subtitle = if venue.provider.is_empty() {
        venue.location.coords_label()
    } else {
        format!("{} · via {}", venue.location.coords_label(), venue.provider)
    };
    body.child(div().text_xs().text_color(rgb(0x8b949e)).child(subtitle))
        .child(
            div()
                .id(("venue-open-map", row_id))
                .text_sm()
                .text_color(rgb(0x58a6ff))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_message_url(&url, cx);
                }))
                .child("🗺 Open map"),
        )
        .into_any_element()
}

/// Phase 4.3: `messageContact` row — display name, phone number, and a
/// subtle "Telegram user" note when `user_id` is known. The phone number
/// is display-only: tapping it must not dial (`tel:` URLs are refused by
/// `open_external_url`'s scheme gate anyway). The vCard is kept in the
/// model but not rendered; there is no profile deep-link yet.
fn contact_row(row_id: u64, contact: &quill::telegram::envelope::ContactContent) -> AnyElement {
    let name = contact.display_name();
    let mut body = div()
        .id(("contact-row", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x8b949e))
        .bg(rgb(0x21262d))
        .child(div().text_sm().font_medium().child(format!(
            "👤 {}",
            if name.is_empty() { "Contact" } else { &name }
        )));
    if !contact.phone_number.is_empty() {
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(0xc9d1d9))
                .child(contact.phone_number.clone()),
        );
    }
    if contact.user_id != 0 {
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(0x8b949e))
                .child("Telegram user"),
        );
    }
    body.into_any_element()
}

/// Phase 4.4: `messageDice` row — the dice emoji rendered large plus the
/// rolled value, tdesktop-style. Static only: the `DiceStickers` roll
/// animation (and `success_animation_frame_number`) is out of scope for
/// this slice; the face glyph stands in for the final animation frame.
fn dice_row(row_id: u64, dice: &quill::telegram::envelope::DiceContent) -> AnyElement {
    div()
        .id(("dice-row", row_id))
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .mt_2()
        .px_3()
        .py_3()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x8b949e))
        .bg(rgb(0x21262d))
        .child(div().text_size(px(64.0)).child(dice.face().to_string()))
        .child(
            div()
                .text_sm()
                .font_medium()
                .child(format!("Rolled {}", dice.value)),
        )
        .into_any_element()
}

fn auth_action_note(auth: &AuthView, connect_status: &ConnectUiStatus) -> impl IntoElement {
    let gate = match connect_status {
        ConnectUiStatus::NeedCredentials => "need credentials",
        ConnectUiStatus::NeedTdjson => "need tdjson",
        ConnectUiStatus::RestoreBlocked(_) => "restore blocked",
        ConnectUiStatus::DemoWaitPhone => "demo wait-phone",
        ConnectUiStatus::DemoWaitCode => "demo wait-code",
        ConnectUiStatus::DemoWaitPassword => "demo wait-password",
        ConnectUiStatus::DemoReadyChats => "demo ready-chats",
        ConnectUiStatus::Live => "live TDLib",
    };
    let label = match &auth.action {
        AuthAction::UnsupportedHalt { reason } => format!("Blocked: {reason}"),
        AuthAction::Ready => format!("Ready ({gate})"),
        AuthAction::EnterPhone => format!("Phone entry ({gate})"),
        AuthAction::EnterCode => format!("Code entry ({gate})"),
        AuthAction::EnterPassword => format!("Password entry ({gate})"),
        AuthAction::ProvideParameters => format!("Sending TDLib parameters ({gate})"),
        other => format!("{other:?} ({gate})"),
    };
    div().text_xs().child(label)
}

fn connect_status_label(status: &ConnectUiStatus) -> String {
    match status {
        ConnectUiStatus::NeedCredentials => {
            "set TELEGRAM_API_ID / TELEGRAM_API_HASH (or local .env)".into()
        }
        ConnectUiStatus::NeedTdjson => {
            "credentials loaded · tdjson missing (QUILL_TDJSON_PATH / bundle)".into()
        }
        ConnectUiStatus::RestoreBlocked(msg) => format!("credentials loaded · {msg}"),
        ConnectUiStatus::DemoWaitPhone => {
            "credentials loaded · WaitPhoneNumber (screenshot demo)".into()
        }
        ConnectUiStatus::DemoWaitCode => "credentials loaded · WaitCode (screenshot demo)".into(),
        ConnectUiStatus::DemoWaitPassword => {
            "credentials loaded · WaitPassword (screenshot demo)".into()
        }
        ConnectUiStatus::DemoReadyChats => {
            "injected Ready · chat list + composer (screenshot demo)".into()
        }
        ConnectUiStatus::Live => "credentials loaded · TDLib live".into(),
    }
}

fn status_bar(
    auth: &AuthView,
    connect_status: &ConnectUiStatus,
    status_note: &str,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    div()
        .id("status")
        .h(px(28.))
        .px_3()
        .flex()
        .items_center()
        .border_t_1()
        .border_color(cx.theme().border)
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(format!(
            "Auth: {} · {} · {} · Keyboard: ⌘K search, ⌘F in chat, Esc cancel forward/delete/edit/reply/search, ⌘1 sidebar, ⌘L composer, ⌘↑ older · VoiceOver: macOS follow-up",
            auth.title,
            connect_status_label(connect_status),
            status_note
        ))
}

impl Focusable for QuillApp {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_sidebar.clone()
    }
}
