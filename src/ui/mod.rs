mod synthetic;

use gpui_kit::component::button::*;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::auth::{AuthAction, AuthView, view_for};
use quill::composer::{
    AttachmentKind, ComposerAttachment, ComposerEdit, ComposerReplyTo, ComposerSnapshot,
    DeleteConfirm, ForwardDraft, begin_edit_keeping_reply, cancel_edit_draft,
    cancel_edit_keeping_reply, cancel_forward_draft, cancel_reply_draft, draft_text_to_store,
    should_send_on_enter,
};
use quill::connect::{
    ChatSearchQueryOutcome, ConnectBlocker, ConnectGate, DraftSaveOutcome, LiveConnect,
    SEARCH_DEBOUNCE, SearchQueryOutcome, USER_DOWNLOAD_PRIORITY, evaluate_gate, start_live_connect,
};
use quill::credentials::TelegramCredentials;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{AccountKey, ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::platform::live_secret_store;
use quill::state::{
    ChatSearchJump, ChatSummary, ForwardResult, HistoryMessage, OutboxReceipt, RequestPurpose,
    SearchStatus, Session, outgoing_status_label, unread_badge_text,
};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    AuthorizationState, ChatDraft, ChatNotificationSettings, DEFAULT_EMOJI_REACTIONS,
    MUTE_FOR_1_HOUR, MUTE_FOR_2_DAYS, MUTE_FOR_8_HOURS, MUTE_FOREVER, MessageContent,
    MessageInteractionInfo, ParsedFile, toggle_chosen_emoji_reaction,
};
use quill::voice::{self, VoiceCapture, format_voice_duration};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
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
        SubmitPassword
    ]
);

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

pub struct QuillApp {
    chat: Entity<SyntheticChat>,
    composer: Entity<TextareaState>,
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
    /// Local file the user explicitly attached (canonical path via `pick`).
    pending_attachment: Option<ComposerAttachment>,
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
    /// tdesktop `VoiceRecordBar` (click mic to record; Esc / Cancel discards).
    voice_capture: Option<VoiceCapture>,
    voice_tick: bool,
    /// History row whose voice note is playing.
    playing_voice: Option<MessageId>,
    voice_player: Option<Child>,
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
    pending_video_play: Option<(MessageId, FileId, String, i32)>,
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
    /// Restored private-chat composer draft (`draftMessage`).
    ReadyDrafts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneMode {
    Synthetic,
    Connecting,
    Ready,
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
        cx.subscribe_in(
            &composer,
            window,
            |this, state, event: &InputEvent, window, cx| {
                let text = state.read(cx).value().to_string();
                this.sync_composer_typing(&text);
                this.note_open_draft(true, cx);
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(quill::composer::enter_event_from_kit(
                        *shift, *secondary, marked,
                    )) {
                        if !text.trim().is_empty() {
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
            Some(ScreenshotDemo::ReadyDrafts) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — restored private-chat draft".into(),
                    AuthorizationState::Ready,
                )
            }
            None => bootstrap_connect(credentials),
        };

        let mut pending_attachment = None;
        if matches!(demo, Some(ScreenshotDemo::ReadySendMedia)) {
            pending_attachment = ComposerAttachment::pick(
                &demo_media_allowlist().join("demo-notes.txt"),
                AttachmentKind::Document,
            );
        }

        let mut app = Self {
            chat,
            composer,
            phone_input,
            code_input,
            password_input,
            search_input,
            chat_search_input,
            forward_search_input,
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
            pending_attachment,
            pending_reply: None,
            clear_draft_on_success: None,
            pending_edit: None,
            saved_edit_draft: String::new(),
            saved_edit_reply: None,
            pending_delete: None,
            pending_forward: None,
            forward_picker_open: false,
            forward_result: None,
            pending_react: None,
            mute_menu_open: false,
            voice_capture: None,
            voice_tick: false,
            playing_voice: None,
            voice_player: None,
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
        };
        if matches!(demo, Some(ScreenshotDemo::ReadyChatsComposer)) {
            app.composer.update(cx, |input, cx| {
                input.set_value("hello from composer", window, cx);
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
        self.finish_successful_sends(cx);
        if progressed || send_failed {
            cx.notify();
        }
        self.resume_pending_gif(cx);
        self.resume_pending_video(cx);
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
            quill::video::with_video_frame_cache(quill::animation::with_gif_frame_cache(primary))
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
                    let attachment = self.pending_attachment.clone();
                    let snap = ComposerSnapshot::capture_with_attachment(
                        chat_id,
                        view_generation,
                        text,
                        attachment,
                    )
                    .with_reply(self.pending_reply.clone());
                    if snap.is_empty() {
                        self.status_note = "type a message or attach a file".into();
                        cx.notify();
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
                            self.pending_attachment = None;
                            self.pending_reply = None;
                            self.clear_draft_on_success = Some(chat_id);
                            self.composer
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            self.forget_local_draft(chat_id);
                            self.status_note = "sending…".into();
                        }
                        Err(_) => {
                            self.status_note = "could not send message".into();
                        }
                    }
                    cx.notify();
                    return;
                }
                if self.demo_session.is_some() {
                    let attachment = self.pending_attachment.clone();
                    let reply = self.pending_reply.clone();
                    self.apply_demo_outgoing(&text, attachment.as_ref(), reply.as_ref());
                    self.pending_attachment = None;
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
        // Explicit user action → pick. Prefer QUILL_ATTACH_PHOTO / QUILL_ATTACH_FILE
        // when set (live testing); otherwise the demo fixtures under docs/screenshots.
        // Never read paths from TDLib JSON for send.
        let env_key = match kind {
            AttachmentKind::Photo => "QUILL_ATTACH_PHOTO",
            AttachmentKind::Document => "QUILL_ATTACH_FILE",
        };
        let path = std::env::var_os(env_key)
            .map(PathBuf::from)
            .unwrap_or_else(|| match kind {
                AttachmentKind::Photo => demo_media_allowlist().join("demo-thumb.png"),
                AttachmentKind::Document => demo_media_allowlist().join("demo-notes.txt"),
            });
        match ComposerAttachment::pick(&path, kind) {
            Some(att) => {
                self.status_note = format!("attached {}", att.file_name);
                self.pending_attachment = Some(att);
            }
            None => {
                self.status_note = "could not attach file".into();
            }
        }
        cx.notify();
    }

    fn clear_attachment(&mut self, cx: &mut Context<Self>) {
        self.pending_attachment = None;
        self.status_note = "attachment cleared".into();
        cx.notify();
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
                        MessageContent::Sticker(_) | MessageContent::Unsupported { .. } => {}
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

    fn request_media_download(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        if file_id.0 == 0 {
            return;
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
                    let result = self.live.as_mut().expect("live").driver.fetch_history();
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
        if self.mute_menu_open {
            self.close_mute_menu(cx);
            return;
        }
        if self.pending_react.is_some() {
            self.close_reaction_picker(cx);
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
        self.pending_attachment = None;
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
            self.request_media_download(file_id, cx);
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
                self.stop_video_playback();
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
            self.pending_video_play = Some((message_id, file_id, mime, start_timestamp));
            self.request_media_download(file_id, cx);
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
                self.stop_animation_playback();
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
                self.status_note = "playing video".into();
            }
            _ => {
                self.status_note = "could not play video".into();
            }
        }
        cx.notify();
    }

    fn resume_pending_video(&mut self, cx: &mut Context<Self>) {
        let Some((message_id, file_id, mime, start_timestamp)) = self.pending_video_play.clone()
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
            self.toggle_video_playback(message_id, file_id, mime, start_timestamp, cx);
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

    fn stop_voice_playback(&mut self) {
        if let Some(mut child) = self.voice_player.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.playing_voice = None;
    }

    fn toggle_voice_playback(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        file_id: FileId,
        listened: bool,
        cx: &mut Context<Self>,
    ) {
        if self.playing_voice == Some(message_id) {
            self.stop_voice_playback();
            self.status_note = "voice note paused".into();
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
            self.request_media_download(file_id, cx);
            return;
        };
        let roots = self.media_display_roots();
        if sandboxed_display_path(&path, &roots).is_none() {
            self.status_note = "voice file is outside the account files".into();
            cx.notify();
            return;
        }
        self.stop_voice_playback();
        self.stop_video_playback();
        self.playing_voice = Some(message_id);
        if !listened {
            self.mark_voice_opened(chat_id, message_id);
        }
        match Command::new("ffplay")
            .args(["-nodisp", "-autoexit", "-loglevel", "quiet", &path])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => {
                self.voice_player = Some(child);
                self.status_note = "playing voice note".into();
            }
            Err(_) => {
                self.status_note = "playing voice note (no audio player)".into();
            }
        }
        cx.notify();
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
            self.apply_demo_notification(chat_id, mute_for);
            self.status_note = if mute_for == 0 {
                "unmuted".into()
            } else {
                "muted".into()
            };
            cx.notify();
        }
    }

    fn apply_demo_notification(&mut self, chat_id: ChatId, mute_for: i32) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let current = session
            .chats
            .get(&chat_id.0)
            .map(|chat| chat.notification_settings.clone())
            .unwrap_or_default();
        let settings = current.with_mute_for(mute_for);
        let json = format!(
            r#"{{"@type":"updateChatNotificationSettings","chat_id":{},"notification_settings":{}}}"#,
            chat_id.0,
            notification_settings_json(&settings)
        );
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
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
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(div().font_semibold().child(title.to_string()))
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
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(if forever { "Muted forever" } else { "Muted" }),
                        )
                    }),
            )
            .when(actions.is_some(), |this| {
                this.child(
                    div()
                        .flex()
                        .gap_1()
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
                        ),
                )
            })
    }

    fn mute_menu_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let chat_id = self.session().and_then(|session| session.open_chat);
        let presets = [
            ("1 hour", MUTE_FOR_1_HOUR),
            ("8 hours", MUTE_FOR_8_HOURS),
            ("2 days", MUTE_FOR_2_DAYS),
            ("Forever", MUTE_FOREVER),
        ];
        let mut row = div().id("mute-presets").flex().flex_wrap().gap_1();
        for (label, seconds) in presets {
            row = row.child(
                Button::new(format!("mute-for-{seconds}"))
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(chat_id) = chat_id {
                            this.apply_chat_mute(chat_id, seconds, cx);
                        }
                    })),
            );
        }
        div()
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
                    .child(div().font_semibold().child("Mute for"))
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
                    .child("1 hour, 8 hours, 2 days, or forever."),
            )
            .child(row)
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
        AuthorizationState::Ready => "signed in — cloud chats only".into(),
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(self.conversation(cx)),
            )
            .child(status_bar(
                &auth,
                &self.connect_status,
                &self.status_note,
                cx,
            ))
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
                let supported = open
                    .and_then(|id| session.and_then(|s| s.chats.get(&id.0).map(|c| c.supported())))
                    .unwrap_or(false);
                if supported { Some(true) } else { None }
            }
        };
        let composer_note = match mode {
            PaneMode::Connecting => Some("Sign in to send messages."),
            PaneMode::Ready if composer.is_none() => {
                if self.session().and_then(|s| s.open_chat).is_none() {
                    Some("Select a supported cloud chat to send.")
                } else {
                    Some("This chat type is gated until sponsored-content handling exists.")
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
                let chip = if show_attach {
                    self.pending_attachment.as_ref().map(|att| {
                        let label = match att.kind {
                            AttachmentKind::Photo => format!("Photo · {}", att.file_name),
                            AttachmentKind::Document => format!("Document · {}", att.file_name),
                        };
                        label
                    })
                } else {
                    None
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
                                    .when(self.pending_attachment.is_some(), |row| {
                                        row.child(
                                            Button::new("clear-attach").label("Clear").on_click(
                                                cx.listener(|this, _, _, cx| {
                                                    this.clear_attachment(cx);
                                                }),
                                            ),
                                        )
                                    }),
                            )
                        })
                        .when_some(chip, |this, label| {
                            this.child(
                                div()
                                    .id("composer-attach-chip")
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(0x8b949e))
                                    .bg(rgb(0x21262d))
                                    .child(div().text_sm().font_medium().child(label))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0xc9d1d9))
                                            .child("ready to send · picked locally"),
                                    ),
                            )
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
                        .when_some(self.pending_edit.clone(), |this, edit| {
                            this.child(self.composer_edit_banner(&edit, cx))
                        })
                        .when_some(self.pending_reply.clone(), |this, reply| {
                            this.child(self.composer_reply_banner(&reply, cx))
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
        div()
            .id("conversation-history")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(self.conversation_header(&title, chat_actions, peer_typing, cx))
            .when(self.mute_menu_open, |this| {
                this.child(self.mute_menu_panel(cx))
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
                pane_placeholder("Unsupported chat", reason, cx).into_any_element()
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
            } else if messages.is_empty() {
                pane_placeholder(
                    "No messages yet",
                    "History arrives via getChatHistory and updates.",
                    cx,
                )
                .into_any_element()
            } else {
                let mut list = div()
                    .id("session-history")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .px_3()
                    .pt_2()
                    .gap_1();
                for message in messages {
                    let label = if message.is_outgoing {
                        let receipt = chat
                            .map(|summary| summary.outbox_receipt(&message))
                            .unwrap_or(OutboxReceipt::Sent);
                        outgoing_status_label(message.pending, receipt).to_string()
                    } else {
                        sender_name.clone()
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
                    let voice_playing = self.playing_voice == Some(message.id);
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
                        voice_playing,
                        animation_playing,
                        animation_frame,
                        video_playing,
                        video_frame,
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
            })
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
            .child(div().font_semibold().child("Chats"))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(chat_list_caption(mode, self.session())),
            );
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
                list = list.child(self.sidebar_search_field(cx));
                if self.search_is_open() {
                    list = list.child(self.search_results(cx));
                } else {
                    let open = self.session().and_then(|s| s.open_chat);
                    let chats: Vec<ChatSummary> = self
                        .session()
                        .map(|s| s.ordered_chats().into_iter().cloned().collect())
                        .unwrap_or_default();
                    if chats.is_empty() {
                        let loading = self.session().is_some_and(|s| !s.chats_exhausted);
                        list = list.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(if loading {
                                    "Loading chats…"
                                } else {
                                    "No chats in the main list."
                                }),
                        );
                    }
                    for chat in chats {
                        let selected = open == Some(chat.id);
                        list = list.child(session_chat_row(&chat, selected, cx));
                    }
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
                            list = list.child(session_chat_row(&chat, selected, cx));
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

fn seed_ready_media_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::Media)
}

fn seed_ready_send_media_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::SendMedia)
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

fn chat_list_caption(mode: PaneMode, session: Option<&Session>) -> SharedString {
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

fn session_chat_row(
    chat: &ChatSummary,
    selected: bool,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    let id = chat.id;
    let title = chat.title.clone();
    let preview = chat.sidebar_preview();
    let badge = unread_badge_text(chat.unread_count);
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
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .min_w_0()
                        .child(div().font_medium().min_w_0().child(title))
                        .when(chat.is_muted(), |this| this.child(muted_badge(id))),
                )
                .when_some(badge, |this, label| this.child(unread_badge(label, id))),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(preview),
        )
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
    voice_playing: bool,
    animation_playing: bool,
    animation_frame: Option<PathBuf>,
    video_playing: bool,
    video_frame: Option<PathBuf>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
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
            message.id.0 as u64,
            text,
            files,
            downloading,
            media_roots,
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
            cx,
        )),
        MessageContent::Document(doc) => Some(document_chip(
            message.id.0 as u64,
            doc,
            files,
            downloading,
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
            voice_playing,
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
            cx,
        )),
        MessageContent::Text(_) | MessageContent::Unsupported { .. } => None,
    };
    let extra = Some(
        div()
            .id(("bubble-extra", message.id.0 as u64))
            .when_some(extra_media, |this, media| this.child(media))
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
    let body = match &message.content {
        MessageContent::Text(_) => String::new(),
        MessageContent::Unsupported { type_name } => format!("({type_name})"),
        MessageContent::Photo(photo) => photo.caption.clone(),
        MessageContent::Document(doc) => doc.caption.clone(),
        MessageContent::Sticker(_) => String::new(),
        MessageContent::Animation(animation) => animation.caption.clone(),
        MessageContent::Video(video) => video.caption.clone(),
        MessageContent::VoiceNote(note) => note.caption.clone(),
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
    session_bubble_quoted(
        message.id.0 as u64,
        label,
        body,
        message.is_outgoing,
        extra,
        header,
    )
}

fn message_text_block(
    row_id: u64,
    text: &quill::telegram::envelope::TextContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let runs = quill::text::link_runs(&text.text, &text.entities);
    let mut line = div()
        .id(("msg-text", row_id))
        .text_sm()
        .flex()
        .flex_wrap()
        .gap_0();
    if runs.is_empty() {
        line = line.child(text.text.clone());
    }
    for (index, run) in runs.into_iter().enumerate() {
        if let Some(href) = run.href.clone() {
            line = line.child(
                div()
                    .id(("msg-link", row_id * 32 + index as u64))
                    .text_color(rgb(0x9ecbff))
                    .underline()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_message_url(&href, cx);
                    }))
                    .child(run.text),
            );
        } else if !run.text.is_empty() {
            line = line.child(run.text);
        }
    }
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
        .when(photo.click_requests_download(), |this| {
            this.cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.request_media_download(open_id, cx);
                }))
        })
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
        .child(
            div()
                .relative()
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
                ),
        )
        .child(
            Button::new(format!("video-play-{row_id}"))
                .label(play_label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if blocked {
                        return;
                    }
                    this.toggle_video_playback(
                        message_id,
                        play_id,
                        mime.clone(),
                        start_timestamp,
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
                    this.request_media_download(display_id, cx);
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

fn voice_note_row(
    chat_id: ChatId,
    message_id: MessageId,
    outgoing: bool,
    note: &quill::telegram::envelope::VoiceNoteContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    playing: bool,
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
    let play_label = if playing {
        "Pause"
    } else if downloading_now {
        "Downloading"
    } else if ready {
        "Play"
    } else {
        "Play"
    };
    let mut meta = format_voice_duration(note.duration);
    if !outgoing && !note.is_listened && !playing {
        meta = format!("New · {meta}");
    }
    if !ready && !downloading_now {
        meta = format!("{meta} · not downloaded");
    } else if downloading_now {
        meta = format!("{meta} · downloading…");
    } else if playing {
        meta = format!("Playing · {meta}");
    }
    div()
        .id(("voice-note", message_id.0 as u64))
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(if playing {
            rgb(0x3fb950)
        } else {
            rgb(0x8b949e)
        })
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
                            this.toggle_voice_playback(chat_id, message_id, file_id, listened, cx);
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
        .into_any_element()
}

fn document_chip(
    row_id: u64,
    doc: &quill::telegram::envelope::DocumentContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
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
            this.request_media_download(file_id, cx);
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
