//! QuillApp struct definition and core app queries.

use super::chat_row::{ChatListItem, ChatPreviewState};
use super::connect_ui::ConnectUiStatus;
use super::demo::demo_media_allowlist;
use super::history::HistoryShared;
use super::notification_settings::SoundPickerTarget;
use super::story_albums::StoryPrivacyEdit;
use super::synthetic::SyntheticChat;
use super::*;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::message_scroller::MessageScrollerState;
use gpui_kit::component::slider::SliderState;
use gpui_kit::component::theme::ThemeMode;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::{
    ComposerAttachment, ComposerEdit, ComposerReplyTo, ComposerScheduling, DeleteConfirm,
    ForwardDraft, PreviewMediaSize,
};
use quill::connect::LiveConnect;
use quill::credentials::TelegramCredentials;
use quill::data_settings::{AutoDownloadNetSettings, NetworkKind};
use quill::diagnostics::MemorySink;
use quill::ids::{ChatId, FileId, MessageId};
use quill::media_viewer::{MediaViewer, ViewerZoom};
use quill::playback::PlaybackClock;
use quill::settings::{AppearancePrefs, ChatPrefs};
use quill::state::{ForwardResult, Session};
use quill::story_composer::StoryComposer;
use quill::story_viewer::{StoryPlayback, StoryViewer};
use quill::telegram::envelope::AuthorizationState;
use quill::telegram::envelope::NotificationSettingsScope;
use quill::telegram::requests::SelfDestructSend;
use quill::video::VideoNoteCapture;
use quill::voice::VoiceCapture;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Child;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};
use std::time::Instant;
pub struct QuillApp {
    pub(super) update_state: quill::updater::UpdateState,
    pub(super) update_banner_dismissed: bool,
    pub(super) chat: Entity<SyntheticChat>,
    pub(super) composer: Entity<TextareaState>,
    /// kit Phase 7: the in-window menu bar (Linux/Windows; macOS uses the
    /// native menu bar via `cx.set_menus`). Entity-owned so it survives
    /// re-renders.
    pub(super) menu_bar: Entity<AppMenuBar>,
    /// kit Phase 3: chat-list virtualization — scroll handle owned by the
    /// app so scroll position survives re-renders (scroll restoration).
    pub(super) chat_list_scroll: VirtualListScrollHandle,
    /// kit Phase 3: chat-list virtualization — the flat item list the
    /// `VirtualList` renders (main rows + archive section).
    pub(super) chat_list_items: Vec<ChatListItem>,
    /// kit Phase 3: message-history virtualization — scroller state with
    /// tail-following, owned by the app so prepend/append keep the anchor.
    pub(super) history_scroller: Entity<MessageScrollerState>,
    /// kit Phase 3: per-row render inputs for the visible history window.
    /// Rebuilt each render; the `MessageScroller` renderer only builds
    /// elements for visible indices.
    pub(super) history_rows: Vec<HistoryRow>,
    /// Row indices the history list rendered in the last frame (the
    /// virtual list only builds on-screen rows plus a small overdraw).
    /// Drained into `viewMessages` reports on the next render.
    pub(super) rendered_history_rows: std::cell::RefCell<Vec<usize>>,
    /// Last `(chat, message ids)` reported as visible, to skip repeats.
    pub(super) reported_visible: Option<(ChatId, Vec<MessageId>)>,
    /// Window activity at the last render; rows only prompt a visibility
    /// report while the window is active.
    pub(super) history_window_active: bool,
    /// kit Phase 3: per-render shared inputs for history rows (files,
    /// downloads, media roots) so visible-row rendering doesn't re-clone.
    pub(super) history_shared: HistoryShared,
    /// kit Phase 3: `(open_chat_id, open_topic)` the scroller state was
    /// last synced for — a change means reset + scroll to bottom.
    pub(super) history_key: Option<(i64, Option<i32>)>,
    /// kit Phase 3: first/last message ids of the last-synced history, to
    /// tell appends apart from prepends without re-scanning.
    pub(super) history_ends: Option<(MessageId, MessageId)>,
    /// `HistoryState::window_epoch` the scroller last anchored for: a
    /// change (window replaced) re-anchors like opening the chat.
    pub(super) history_window_epoch: u64,
    /// The scroller must anchor once rows exist: the jump highlight, else
    /// the "Unread messages" divider, else the bottom.
    pub(super) history_anchor_pending: bool,
    /// The window stopped short of the latest message at the last render:
    /// rows appended since are a newer page, not live messages.
    pub(super) history_had_newer: bool,
    /// Chat list column width (drag its right edge; double-click resets).
    pub(super) sidebar_width: Pixels,
    /// A debounced `window_state.json` save is scheduled.
    pub(super) window_state_save_pending: bool,
    /// What `history_rows` were last built from (`None`: not cacheable).
    pub(super) history_rows_key: Option<super::conversation::HistoryRowsKey>,
    /// (ready files, downloading files) at the last history render; a
    /// change remeasures the virtualized rows (media grew in place).
    pub(super) history_media_signature: (usize, usize),
    /// kit Phase 3: last chat-search highlight the scroller jumped to —
    /// avoids re-scrolling every frame while the highlight is set.
    pub(super) last_highlight: Option<MessageId>,
    /// Phase 3.3: `/` command menu state. Open while the composer text
    /// ends with a `/`-led token and the open bot chat has commands;
    /// `command_menu_selected` is the highlighted row (Up/Down/Enter).
    pub(super) command_menu_open: bool,
    pub(super) command_menu_selected: usize,
    /// Highlighted row of the composer's `@` suggestions.
    pub(super) mention_selected: usize,
    /// Bots slice: `@botname query` inline-mode results dropdown above
    /// the composer. `inline_results_selected` is the highlighted row
    /// (Up/Down/Enter); `inline_query_token` debounces the
    /// `getInlineQueryResults` dispatch (100ms quiet window, TGX).
    /// `inline_query_armed` is the `(username, query)` a debounce timer
    /// is currently armed for — `poll_live` re-runs the inline progress
    /// check after every batch of session updates, and the armed identity
    /// keeps it from spawning a duplicate timer per poll.
    pub(super) inline_results_open: bool,
    pub(super) inline_results_selected: usize,
    pub(super) inline_query_token: u64,
    pub(super) inline_query_armed: Option<(String, String)>,
    /// The composer's emoji / sticker / GIF popover.
    pub(super) media_panel: super::media_panel::MediaPanel,
    pub(super) emoji_status_hours_input: Entity<TextareaState>,
    pub(super) emoji_set_search_input: Entity<TextareaState>,
    pub(super) emoji_search_input: Entity<TextareaState>,
    /// The reaction selector's own search (cleared on each open).
    pub(super) reaction_search_input: Entity<TextareaState>,
    pub(super) gif_search_input: Entity<TextareaState>,
    pub(super) marketplace_open: bool,
    pub(super) marketplace_name_input: Entity<TextareaState>,
    pub(super) marketplace_comment_input: Entity<TextareaState>,
    pub(super) marketplace_private: bool,
    pub(super) marketplace_error: Option<String>,
    pub(super) sticker_search_input: Entity<TextareaState>,
    pub(super) registration_first_input: Entity<TextareaState>,
    pub(super) registration_last_input: Entity<TextareaState>,
    pub(super) accepted_registration_terms: Option<quill::telegram::envelope::RegistrationTerms>,
    pub(super) registration_notify_contacts: bool,
    pub(super) email_input: Entity<TextareaState>,
    pub(super) phone_input: Entity<TextareaState>,
    pub(super) code_input: Entity<TextareaState>,
    pub(super) password_input: Entity<InputState>,
    /// Slice A10: recovery-code entry for 2FA password recovery. The code
    /// is never stored beyond the input widget — it is zeroized after
    /// submit (the A2 rule).
    pub(super) recovery_code_input: Entity<TextareaState>,
    /// Slice A10: the password screen is showing recovery-code entry
    /// instead of password entry. Pure UI state, reset when auth leaves
    /// WaitPassword.
    pub(super) recovery_mode: bool,
    pub(super) search_input: Entity<TextareaState>,
    pub(super) chat_search_input: Entity<TextareaState>,
    pub(super) forward_search_input: Entity<TextareaState>,
    pub(super) auth_demo: AuthorizationState,
    pub(super) focus_sidebar: FocusHandle,
    pub(super) context_menu_focus: FocusHandle,
    pub(super) context_menu_was_open: bool,
    pub(super) context_menu_previous_focus: Option<FocusHandle>,
    pub(super) connect_status: ConnectUiStatus,
    pub(super) connection_generation: u64,
    pub(super) live: Option<LiveConnect>,
    pub(super) status_note: String,
    /// The `status_note` text the toast last showed, and when it appeared:
    /// a changed note restarts the toast's timer.
    pub(super) status_seen: String,
    pub(super) status_shown_at: Option<std::time::Instant>,
    /// Slice auth-logout-warning: the startup credentials, kept so a
    /// `logOut`-driven Closed can restart the live connection and return
    /// the user to the login screen (same sensitivity class as the
    /// driver's own copy). Also read by the account switcher
    /// (parity:auth-multi-account) to reconnect as another account.
    pub(super) credentials: Option<TelegramCredentials>,
    /// Phase 1 (kit adoption): the note text a dismiss timer is already armed
    /// for. The permanent debug status bar is gone; `status_note` now shows
    /// as a kit notification (auto-dismissing) instead.    /// Screenshot / synthetic demo: show the matching auth field without a live client.
    pub(super) demo_auth_inputs: bool,
    /// Screenshot Ready list: same reducers as live, injected JSON only.
    pub(super) demo_session: Option<Session>,
    /// Phase C2c screenshot ReadyCallDevices demo: injected demo devices
    /// only (no live Telegram, no real hardware); `None` means the engine
    /// reported none.
    pub(super) demo_call_devices: Option<Vec<quill::calls::engine::MediaDevice>>,
    pub(super) demo_selected_devices: (Option<String>, Option<String>),
    /// Phase C2e: synthetic demo video frames for the screenshot
    /// fixture only — live frames come from the driver, never these.
    pub(super) demo_remote_frame: Option<quill::calls::engine::VideoFrame>,
    pub(super) demo_local_frame: Option<quill::calls::engine::VideoFrame>,
    /// Phase C2l: synthetic demo screen-share frame (peer side) for the
    /// screenshot fixture only — live frames come from the driver.
    pub(super) demo_screen_frame: Option<quill::calls::engine::VideoFrame>,
    /// Phase C2e: demo-mode camera pick (live picks go to the driver).
    pub(super) demo_selected_camera: Option<String>,
    /// Phase C2e/C2l: decoded video tiles cached by `(frame seq,
    /// is_screen)`, rebuilt only when the key changes. The peer's
    /// screen and camera streams share `seq` numbering (all demo
    /// fixtures use seq 0), so `is_screen` is part of the key — a
    /// seq-only key would cross-render.
    pub(super) call_remote_image: Option<((u64, bool), Arc<RenderImage>)>,
    pub(super) call_local_image: Option<((u64, bool), Arc<RenderImage>)>,
    /// Slice A1: decoded QR-login bitmap cached by link, rebuilt only when
    /// the link changes. The link itself is never logged.
    pub(super) qr_login_cache: Option<(String, Arc<RenderImage>)>,
    /// Phase C2g: group-call video tiles cached by
    /// `(group_call_id, user_id, is_screen)` → `(frame seq, image)`,
    /// rebuilt only when that slot's frame sequence changes.
    pub(super) group_video_images: HashMap<(i32, i64, bool), (u64, Arc<RenderImage>)>,
    /// Phase C2g: synthetic per-participant frames injected by the
    /// ready-group-call demo fixture, keyed `(user_id, is_screen)`.
    /// Injected demo data, not real media.
    pub(super) demo_group_frames: HashMap<(i64, bool), quill::calls::engine::VideoFrame>,
    pub(super) demo_seq: AtomicU64,
    pub(super) demo_sink: Arc<MemorySink>,
    /// Phase 8.1: chat ids whose OS notification was clicked (set by the
    /// notification worker threads); the next render focuses the chat.
    pub(super) notify_clicks: Arc<Mutex<Vec<ChatId>>>,
    /// Phase 8.1: in-flight OS notification workers; capped so a message
    /// burst cannot stack threads.
    pub(super) notify_inflight: Arc<AtomicUsize>,
    /// Local files the user explicitly attached (canonical paths via `pick`).
    /// One item sends with `sendMessage`. Two or more photos/videos send with
    /// `sendMessageAlbum`.
    pub(super) pending_attachments: Vec<ComposerAttachment>,
    /// Phase B3: the composer's self-destruct choice for photo/video
    /// sends (`inputMessagePhoto`/`inputMessageVideo`
    /// `self_destruct_type`, schema 1.8.67 lines 6117/6128 — private
    /// chats only). Cycles Off → 5s → 30s → 1m → View once via the
    /// picker button; captured into `ComposerSnapshot` at submit time.
    pub(super) composer_self_destruct: Option<SelfDestructSend>,
    /// MED4: caption-above-media toggle for photo/video sends
    /// (`show_caption_above_media`, schema 1.8.67 lines 6117/6128).
    /// Captured into `ComposerSnapshot` at submit time; reset after send.
    pub(super) composer_caption_above: bool,
    /// M1: silent-send toggle (`messageSendOptions.disable_notification`,
    /// schema 1.8.67 line 5934). Persists across sends until toggled.
    pub(super) composer_silent: bool,
    /// M1: link-preview toggle (`linkPreviewOptions.is_disabled`, schema
    /// 1.8.67 line 2237). Persists across sends; secret chats force it on.
    pub(super) composer_preview_disabled: bool,
    /// MED4b: `linkPreviewOptions.show_above_text` (schema:2236, TGX
    /// `onRequestToggleShowAbove`). Persists across sends like the
    /// disable toggle; hidden while the preview is off.
    pub(super) composer_preview_above: bool,
    /// MED4b: `linkPreviewOptions.force_small_media` /
    /// `force_large_media` (schema:2234-2235, TGX
    /// `onRequestToggleLargeMedia`). Persists across sends; the size
    /// button only renders when the prefetched preview actually offers
    /// large media.
    pub(super) composer_preview_media: PreviewMediaSize,
    /// MED4b: debounce token for the `getLinkPreview` prefetch — each
    /// keystroke bumps it so only the latest quiet window fires (schema:
    /// "Do not call this function too often"; TGX rate-limits 400ms).
    pub(super) composer_preview_token: u64,
    /// M1: scheduling choice (`messageSchedulingState*`, schema 1.8.67
    /// lines 5902/5905). Reset to `None` after each successful send.
    pub(super) composer_scheduling: ComposerScheduling,
    /// M1: the schedule picker popup above the composer.
    pub(super) schedule_popup_open: bool,
    /// parity:platform-spellcheck: the spellcheck engine (wordlist +
    /// user words, loaded once at startup).
    pub(super) spellchecker: quill::spellcheck::SpellChecker,
    /// parity:platform-spellcheck: misspellings in the current draft
    /// (cheap check_words pass, refreshed on every composer input).
    pub(super) spell_misspellings: Vec<quill::spellcheck::Misspelling>,
    /// parity:platform-spellcheck: suggestions parallel to
    /// `spell_misspellings`, computed when the panel opens (not per
    /// keystroke — distance-2 generation is too slow for the input path).
    pub(super) spell_suggestions: Vec<Vec<String>>,
    /// parity:platform-spellcheck: the corrections panel above the composer.
    pub(super) spellcheck_open: bool,
    /// M1: the scheduled-messages dialog (view/delete).
    pub(super) scheduled_dialog_open: bool,
    /// M2: the rich editor is open — the composer textarea is interpreted
    /// as block markup (`quill::rich::markup_to_blocks`) and sends via
    /// `inputMessageRichMessage`. Opened via the ⛶ button (visible after
    /// 3+ lines, per the anniversary post).
    pub(super) rich_editor_open: bool,
    /// M1: right-click context menu target + window position.
    pub(super) message_menu: Option<MessageMenuState>,
    /// Slice CL1: right-click chat-row context menu target + window
    /// position.
    pub(super) chat_menu: Option<ChatMenuState>,
    /// Slice CL: the open peek preview — hovered/press-and-hold chat,
    /// or `None`. Transient; never an open chat.
    pub(super) chat_preview: Option<ChatPreviewState>,
    /// Slice CL: an in-progress long press on a chat-list row — the
    /// row's chat id + press start, for the peek preview.
    pub(super) preview_press: Option<(ChatId, Instant)>,
    /// Slice CL3: multi-select mode — checked chat ids. Non-empty while
    /// selecting; rows toggle the check instead of opening the chat and
    /// the select bar offers the bulk actions.
    pub(super) selected_chats: HashSet<i64>,
    /// M1: swipe-to-reply press origin (chat, message, press x).
    pub(super) swipe_reply_start: Option<(ChatId, MessageId, Pixels)>,
    /// Same-chat reply draft (tdesktop `FieldHeader::replyToMessage`).
    pub(super) pending_reply: Option<ComposerReplyTo>,
    /// Chat whose draft should be cleared after `updateMessageSendSucceeded`
    /// if the composer is still empty.
    pub(super) clear_draft_on_success: Option<ChatId>,
    /// Own-message edit (tdesktop `FieldHeader::editMessage`).
    pub(super) pending_edit: Option<ComposerEdit>,
    /// Normal composer draft stashed while editing (`DraftType::Normal`).
    pub(super) saved_edit_draft: String,
    /// Reply that belonged to that normal draft. Restored with the text on cancel.
    pub(super) saved_edit_reply: Option<ComposerReplyTo>,
    /// Delete confirm (tdesktop `DeleteMessagesBox` / Unigram popup).
    pub(super) pending_delete: Option<DeleteConfirm>,
    /// B4: pending "Stop poll" confirm (`stopPoll`, schema 1.8.67 line
    /// 12953) — (chat, message, is_quiz) for the warning copy. TGX
    /// `StopPollWarn` / `StopQuizWarn`: nobody can vote afterwards and
    /// the action can't be undone.
    pub(super) pending_stop_poll: Option<(ChatId, MessageId, bool)>,
    /// Phase B1: pending "Close secret chat" confirm for the open chat
    /// (`closeSecretChat`, schema 1.8.67 line 15242).
    pub(super) pending_close_secret_chat: Option<ChatId>,
    /// Pinned bar position per chat: index into the pinned list, newest
    /// first. A click on the bar jumps there and steps to the next older.
    pub(super) pinned_cursor: HashMap<i64, usize>,
    /// Chats whose pinned bar was hidden, with the newest pinned message
    /// at the time: the bar returns when a newer message is pinned.
    pub(super) hidden_pinned: HashMap<i64, MessageId>,
    /// The open chat's pinned-messages list (bar's list button).
    pub(super) pinned_list_open: bool,
    /// Muted, looping inline players for visible videos and GIFs.
    pub(super) inline_videos: std::cell::RefCell<super::inline_video::InlineVideos>,
    /// Highest frame rate animated content asked for since the last
    /// clock tick (0: nothing animated rendered); see `frame_clock`.
    pub(super) animation_demand: std::cell::Cell<u32>,
    pub(super) frame_clock_running: std::cell::Cell<bool>,
    /// Smooth reveal of a bot's streaming reply (`bot_stream`).
    pub(super) stream_reveal: std::cell::RefCell<super::bot_stream::StreamReveal>,
    /// Phase S2: pending inline-bot warning for a `SwitchInline` press in
    /// a secret chat (TGX `SecretChatContextBotAlert`) — the stashed
    /// query is inserted on Confirm.
    pub(super) pending_inline_bot_alert: Option<String>,
    /// Phase S2: the inline-bot warning has been confirmed once this
    /// session (TGX `TUTORIAL_INLINE_SEARCH_SECRECY`, which persists;
    /// Quill keeps it per session — documented divergence).
    pub(super) inline_bot_alert_shown: bool,
    /// Phase S2: storage-usage overlay (TGX Settings → Data and Storage).
    pub(super) storage_usage_open: bool,
    /// Settings → Appearance slice: client-side look-and-feel
    /// (theme/auto-night/accent/wallpaper/font-size/bubbles), persisted
    /// to `appearance_prefs.json`.
    pub(super) appearance: AppearancePrefs,
    /// Chat prefs slice: chat-composer behavior (send-key mode),
    /// persisted to `chat_prefs.json`.
    pub(super) chat_prefs: ChatPrefs,
    /// Settings → Appearance slice: the dialog is on screen.
    pub(super) appearance_open: bool,
    pub(super) settings_open: bool,
    pub(super) settings_page: Option<&'static str>,
    /// Parity slice (platform-custom-keybindings): the rebindable action id
    /// currently capturing a keystroke, if any.
    pub(super) keybinding_capture: Option<String>,
    /// Parity slice (platform-custom-keybindings): a capture that was
    /// refused (fixed chrome or another rebindable action). The shortcuts
    /// chip keeps showing the chord that is actually bound.
    pub(super) keybinding_error: Option<(String, String)>,
    /// Parity slice (platform-custom-keybindings): focus handle for the
    /// keystroke-capture row.
    pub(super) keybinding_focus: FocusHandle,
    /// Parity slice (platform-custom-keybindings): saved shortcut overrides
    /// applied to the keymap once the live driver is ready.
    pub(super) keybindings_applied: bool,
    /// Screenshot proof for the keyboard-shortcuts section. The Appearance
    /// dialog then shows that section alone so the frame is the rebind UI.
    pub(super) keybindings_screenshot: bool,
    /// Slice parity:platform-shortcuts-reference: the keyboard shortcuts
    /// reference dialog is on screen.
    pub(super) shortcuts_open: bool,
    pub(super) sticker_settings_open: bool,
    /// Settings → Appearance slice: last `(theme mode, accent)` pushed
    /// into the global component theme, so `apply_appearance` only
    /// notifies (re-renders) when something actually changed.
    pub(super) appearance_applied: Option<(ThemeMode, u32, bool)>,
    /// Slice S3: Privacy settings overlay (TGX Settings → Privacy).
    pub(super) privacy_open: bool,
    /// Slice S3: per-rule editor overlay target (Privacy screen).
    pub(super) privacy_editor: Option<PrivacyEditorTarget>,
    /// Slice S3: exception list overlay — the rule and always/never kind.
    pub(super) privacy_exceptions: Option<(PrivacyEditorTarget, PrivacyExceptionKind)>,
    /// Slice S3: add-exception contact picker inside the exceptions overlay.
    pub(super) exception_picker_open: bool,
    /// Slice S3: block-user contact picker inside the Privacy overlay.
    pub(super) block_picker_open: bool,
    /// Slice S3: two-step Unblock confirm on the Privacy screen.
    pub(super) unblock_confirm: Option<i64>,
    /// Slice S4: the Data & Storage per-network editor — the network
    /// being edited plus its draft settings (saved or discarded
    /// explicitly, never applied optimistically).
    pub(super) data_storage_editor: Option<(NetworkKind, AutoDownloadNetSettings)>,
    /// Slice S4: the "Clear cache" button is awaiting its second,
    /// confirming tap.
    pub(super) data_storage_confirm_clear: bool,
    /// Slice A2: two-step verification overlay. `twofa_view` picks the
    /// status screen or one of the forms; the four textareas back the
    /// enable/change/disable/recovery-email forms. Passwords live in the
    /// inputs only and are cleared on submit/close — never on the session.
    pub(super) twofa_open: bool,
    pub(super) twofa_view: TwofaView,
    pub(super) twofa_current_password: Entity<InputState>,
    pub(super) twofa_new_password: Entity<InputState>,
    pub(super) twofa_hint: Entity<TextareaState>,
    pub(super) twofa_email: Entity<TextareaState>,
    /// Slice A2 fixup: local validation notice for the 2FA forms ("enter
    /// your current password") — the driver rejects doomed requests
    /// silently, so the form must speak before sending.
    pub(super) twofa_notice: Option<String>,
    /// Slice A9: account lifecycle dialog (delete account + self-destruct
    /// TTL). Working state lives in the named module; this is the one
    /// field the dialog machinery reads.
    pub(super) account_lifecycle: AccountLifecycleState,
    /// Slice parity:auth-multi-account (UI): the Accounts dialog state
    /// (list / switch / add / remove). Working state lives in
    /// `accounts.rs`; this is the one field the dialog machinery reads.
    pub(super) accounts_ui: AccountsUiState,
    /// Slice A3: Active Sessions overlay (TGX Settings → Devices /
    /// `SettingsSessionsController`).
    pub(super) sessions_open: bool,
    pub(super) device_qr_scanner: Option<super::device_qr::DeviceQrScanner>,
    pub(super) device_login_qr: Option<zeroize::Zeroizing<String>>,
    pub(super) device_link_notice: Option<&'static str>,
    /// Slice A3: pending terminate confirmation on the sessions overlay
    /// (TGX `TerminateSessionQuestion` / `TerminateIncompleteSessionQuestion`
    /// / `AreYouSureSessions`).
    pub(super) sessions_confirm: Option<SessionsConfirm>,
    /// Slice A4: Connected Websites overlay (TGX `SettingsWebsitesController`
    /// / `WebSessionsTitle` "Logged In with Telegram").
    pub(super) websites_open: bool,
    /// Slice A4: pending disconnect confirmation on the websites overlay
    /// (TGX `TerminateWebSessionQuestion` / `DisconnectAllWebsitesHint`).
    pub(super) websites_confirm: Option<WebsitesConfirm>,
    /// Slice CL2: chat-list category filter (TGX `ChatFilter` unread /
    /// archive categories, `MainController` pager categories). `All` is
    /// the unfiltered list; `Unread` filters to unread chats;
    /// `Archived` shows only the archive.
    pub(super) chat_filter: ChatListFilter,
    /// Phase S1: "New secret chat" contact-picker overlay (sidebar).
    pub(super) new_secret_picker_open: bool,
    /// tdesktop `Data::ForwardDraft` / history multi-select.
    pub(super) pending_forward: Option<ForwardDraft>,
    /// ShareBox / `ShowForwardMessagesBox` dest picker overlay.
    pub(super) forward_picker_open: bool,
    /// Last successful (or failed) `forwardMessages` result.
    pub(super) forward_result: Option<ForwardResult>,
    /// tdesktop hover React / Unigram ReactionButton picker (emoji only).
    /// The message menu's reaction strip is expanded to every reaction.
    pub(super) reactions_expanded: bool,
    /// tdesktop Mute submenu (1 hour / 8 hours / 2 days / Forever).
    pub(super) mute_menu_open: bool,
    /// Phase B4: self-destruct / auto-delete timer picker below the
    /// conversation header (`setChatMessageAutoDeleteTime`).
    pub(super) ttl_picker_open: bool,
    /// Phase C3a: voice-chat title rename dialog (`setVideoChatTitle`).
    pub(super) group_call_title_dialog: Option<GroupCallTitleDialog>,
    /// Phase C2h: `createVideoChat` start/schedule dialog.
    pub(super) group_call_start_dialog: Option<GroupCallStartDialog>,
    /// Phase C2h: in-call chat composer (sendGroupCallMessage).
    pub(super) group_call_composer: Entity<TextareaState>,
    /// Phase C2f: voice-chat invite picker overlay (contacts list).
    pub(super) group_call_invite_open: bool,
    /// Parity slice: the notifications panel's sound picker sub-view is open.
    pub(super) notif_sound_picker_open: bool,
    /// Parity slice (`parity:stories-notify-settings`): the notifications
    /// panel's story-sound picker sub-view is open.
    pub(super) story_sound_picker_open: bool,
    /// Parity slice: scope-default notification settings dialog is open.
    pub(super) notification_defaults_open: bool,
    /// Parity slice: which defaults-dialog section's sound picker is
    /// expanded (`None` = all collapsed).
    pub(super) defaults_sound_picker: Option<SoundPickerTarget>,
    /// Parity slice: which scope section's notification exceptions list is
    /// expanded in the defaults dialog (`None` = all collapsed).
    pub(super) defaults_exceptions_scope: Option<NotificationSettingsScope>,
    /// Parity slice: pending "Reset all" confirmation on the notification
    /// defaults dialog (`resetAllNotificationSettings` wipes every
    /// notification customization with no undo, so it gates behind an
    /// explicit confirm).
    pub(super) notifications_confirm: Option<NotificationsConfirm>,
    /// Parity slice: in-flight notification-sound workers; capped so a
    /// message burst cannot stack players.
    pub(super) notify_sound_inflight: Arc<AtomicUsize>,
    /// tdesktop `VoiceRecordBar` (click the record button to record in the
    /// current mode; Cancel / Esc asks for confirmation first).
    pub(super) voice_capture: Option<VoiceCapture>,
    /// MED2: in-progress round video-note camera capture (video mode).
    pub(super) video_note_capture: Option<VideoNoteCapture>,
    /// MED2: lock-to-record — a locked recording ignores Esc; only Send
    /// or Cancel (with confirmation) ends it.
    pub(super) record_locked: bool,
    /// MED2: the record bar is showing the discard-confirmation row.
    pub(super) record_discard_confirm: bool,
    pub(super) voice_tick: bool,
    /// Phase A1: the open chat whose slow-mode countdown is ticking
    /// (`Some` exactly while the 1s tick task runs). Mirrors `voice_tick`.
    pub(super) slow_mode_tick_chat: Option<ChatId>,
    /// Phase B3: the open chat whose self-destruct countdown badges are
    /// ticking (`Some` exactly while the 1s tick task runs). Mirrors
    /// `slow_mode_tick_chat`.
    pub(super) self_destruct_tick_chat: Option<ChatId>,
    /// Phase C1: whether the call-duration 1s tick task is running
    /// (keeps the overlay's ringing/connected clock fresh). Mirrors
    /// `voice_tick`.
    pub(super) call_tick_active: bool,
    /// History row whose voice note is playing.
    pub(super) playing_voice: Option<MessageId>,
    /// History row whose music file (`messageAudio`) is playing. Shares `voice_player`.
    pub(super) playing_audio: Option<MessageId>,
    /// Play was tapped before the track was local. Resume when `downloadFile` finishes.
    pub(super) pending_audio_play: Option<(MessageId, FileId, f64)>,
    pub(super) pending_voice_play: Option<(ChatId, MessageId, FileId, bool, f64)>,
    pub(super) voice_player: Option<Child>,
    /// Active audio/voice track's playback clock (playing or paused-with-offset).
    /// `Some` exactly when `playing_voice` or `playing_audio` is `Some` (Phase 4.6).
    pub(super) playback_clock: Option<PlaybackClock>,
    /// Sandbox-checked local path of the active track, for ffplay restarts on seek.
    pub(super) playback_path: Option<PathBuf>,
    /// Interactive seek slider bound to the active row (Phase 4.6).
    pub(super) seek_slider: Option<Entity<SliderState>>,
    /// True while the user is dragging the seek slider (Change without Release yet).
    pub(super) seek_scrubbing: bool,
    /// Drag preview position in seconds, shown in the time label while scrubbing.
    pub(super) seek_preview_secs: Option<f64>,
    /// Guard for the playback progress tick task.
    pub(super) playback_tick: bool,
    /// Last known position per message, so rows keep their seek bar fill (and
    /// resume from it) after pause/stop.
    pub(super) playback_positions: HashMap<MessageId, f64>,
    /// History row whose GIF is looping (tdesktop clip / Unigram player).
    pub(super) sticker_playback: super::sticker_playback::StickerPlayback,
    /// Animated custom emoji (smaller frames, more clips).
    pub(super) emoji_playback: super::sticker_playback::StickerPlayback,
    pub(super) playing_animation: Option<MessageId>,
    pub(super) autoplayed_gifs: std::collections::HashSet<MessageId>,
    pub(super) animation_frames: Vec<Arc<RenderImage>>,
    pub(super) animation_frame: usize,
    pub(super) animation_tick: bool,
    pub(super) animation_fps: f64,
    pub(super) animation_started_at: Option<Instant>,
    pub(super) animation_extract_child: Option<Arc<Mutex<Option<Child>>>>,
    pub(super) animation_extract_cancel: Option<Arc<AtomicBool>>,
    pub(super) animation_extract_epoch: u64,
    /// File whose extracted frames should be deleted when playback stops.
    pub(super) animation_cache_file: Option<i32>,
    /// Play was tapped before the clip was local. Resume when `downloadFile` finishes.
    pub(super) pending_gif_play: Option<(MessageId, FileId, String)>,
    /// History row whose video preview is looping.
    pub(super) playing_video: Option<MessageId>,
    pub(super) video_frames: Vec<PathBuf>,
    pub(super) video_frame: usize,
    pub(super) video_tick: bool,
    pub(super) video_cache_file: Option<i32>,
    /// Play was tapped before the video was local. Resume when `downloadFile` finishes.
    /// The last field is the chat to mark opened (`openMessageContent`) once playback starts.
    pub(super) pending_video_play: Option<(MessageId, FileId, String, i32, Option<ChatId>)>,
    /// `ReadySponsored` fixture surface: the demo channel renders sponsored rows
    /// instead of history. Normal live path unchanged.
    pub(super) sponsored_demo: bool,
    /// Phase 4.1: revealed text-entity spoilers, keyed by
    /// (chat id, message id, run index, is-caption block). Message ids are
    /// only unique within a chat, so the chat id is part of the key.
    pub(super) spoiler_revealed: HashSet<(i64, u64, u64, bool)>,
    /// Phase 4.2: poll creation dialog (open above the composer).
    pub(super) poll_dialog: Option<PollDialog>,
    /// Slice P1: the payment checkout dialog's text inputs. The dialog
    /// renders from the session's `payment_form`; this holds the live
    /// text fields.
    pub(super) payment_dialog: Option<PaymentDialog>,
    /// Phase D3a: invite-link create dialog state.
    pub(super) invite_link_dialog: Option<InviteLinkDialog>,
    /// Phase D3b: admin-management dialog state (promote picker /
    /// rights editor / demote confirm).
    pub(super) admin_dialog: Option<AdminDialog>,
    /// Slice G1: group/supergroup/channel creation dialog.
    pub(super) create_chat_dialog: Option<CreateChatDialog>,
    /// Slice G10: communities dialog state (create dialog + hub flag).
    pub(super) community_ui: CommunityUi,
    /// Slice G1: member-management dialog (tabs + add section).
    pub(super) member_dialog: Option<MemberDialog>,
    /// B1: password prompt for `inlineKeyboardButtonTypeCallbackWithPassword`.
    pub(super) callback_password_dialog: Option<CallbackPasswordDialog>,
    /// B1: `loginUrlInfoRequestConfirmation` domain/url awaiting user consent.
    pub(super) login_url_confirm: Option<LoginUrlConfirm>,
    /// `parity:platform-deep-links`: launch link from the CLI (`t.me` /
    /// `tg:`), set by `main.rs`. Fired once auth reaches Ready, then
    /// cleared (`pub(crate)` so the binary can set it).
    pub(crate) pending_deep_link: Option<String>,
    /// `parity:platform-deep-links`: TDLib's info / error text for the
    /// deep link, shown in a dialog (`DialogKind::DeepLinkInfo`).
    pub(super) deep_link_dialog: Option<String>,
    pub(super) deep_link_invite: Option<quill::state::DeepLinkState>,
    /// `parity:platform-deep-links`: resolved chat + action waiting for
    /// render (which owns the `Window`) to open it.
    pub(super) pending_deep_link_open: Option<(ChatId, quill::state::DeepLinkAction)>,
    /// B1: one-time custom keyboards the user already tapped
    /// (`(chat_id, message_id)`), hidden locally after use.
    pub(super) dismissed_keyboards: std::collections::HashSet<(i64, i64)>,
    /// Slice G1: default chat permissions editor.
    pub(super) permissions_dialog: Option<PermissionsDialog>,
    /// Slice G1: public username editor.
    pub(super) username_dialog: Option<UsernameDialog>,
    /// Slice G1: restrict/ban dialog.
    pub(super) restrict_dialog: Option<RestrictDialog>,
    /// Slice G1: delete / leave / broadcast-upgrade / ban confirmations.
    pub(super) group_confirm_dialog: Option<GroupConfirmDialog>,
    /// Slice G2: forum-topic management dialog.
    pub(super) forum_manage_dialog: Option<ForumManageDialog>,
    /// Slice G2: channel-post comment-thread viewer.
    pub(super) comment_thread_dialog: Option<CommentThreadDialog>,
    /// B4: poll voter-list viewer.
    pub(super) poll_voters_dialog: Option<PollVotersDialog>,
    /// Slice G2: chat welcome-message editor.
    pub(super) welcome_dialog: Option<WelcomeDialog>,
    /// Slice G2: event-log search input for the info panel's
    /// "Recent actions" section (created lazily when the panel opens).
    pub(super) event_log_search: Option<Entity<TextareaState>>,
    /// Slice G2: per-admin filter for the event log (client-side — TDLib's
    /// `chatEventLogFilters` has no user field, schema 1.8.67 line 7956).
    /// `None` shows all admins.
    pub(super) event_log_admin_filter: Option<i64>,
    /// Message text selected when the message menu opened, if the
    /// selection lies in that message (Quote & Reply, Copy Selected Text).
    pub(super) message_menu_selection: Option<String>,
    /// Phase 4.5: fullscreen media viewer (photo/video overlay).
    pub(super) media_viewer: MediaViewer,
    /// The photo editor over a pending photo attachment, when open.
    pub(super) photo_editor: Option<super::photo_editor::PhotoEditor>,
    /// Parity slice 5: zoom/pan of the viewer visual (reset on open/step).
    pub(super) viewer_zoom: ViewerZoom,
    /// The viewer's media frame for the current window size (zoom/pan
    /// math works in it).
    pub(super) viewer_frame: (f32, f32),
    /// Parity slice 5: drag-pan anchor — last mouse position in px while the
    /// left button is held over the zoomed visual.
    pub(super) viewer_drag: Option<(f32, f32)>,
    /// Parity slice 5: message whose video clip is playing in the viewer
    /// (ffplay child alive) or paused (clock frozen, no child).
    pub(super) viewer_video: Option<MessageId>,
    pub(super) pip_window: Option<WindowHandle<gpui_kit::component::Root>>,
    /// Sandbox-checked local path of the viewer's clip, for pause/resume
    /// ffplay restarts.
    pub(super) viewer_video_path: Option<PathBuf>,
    /// The viewer's ffplay child (audio-only `-nodisp`; the video frames
    /// render in-viewer). Killed when the viewer closes, steps, or pauses.
    pub(super) viewer_player: Option<Child>,
    /// Playback clock for the viewer's clip (elapsed/total + pause freeze).
    pub(super) viewer_clock: Option<PlaybackClock>,
    /// Decoded frames for the viewer's clip, rendered in-place (parity
    /// slice 5). Pre-decoded `RenderImage` handles: `img()` resolves
    /// `ImageSource::Render` synchronously, so the 125 ms tick can cycle
    /// frames without an async load round trip per frame. Empty until
    /// extraction + decode finish; the thumbnail shows meanwhile.
    pub(super) viewer_video_frames: Vec<Arc<RenderImage>>,
    /// The native player (AVFoundation on macOS) for the viewer clip; when
    /// set it replaces ffplay audio and the extracted frames.
    pub(super) viewer_native: Option<super::native_video::NativeVideo>,
    /// The last status note printed by `QUILL_TRACE_STATUS`.
    pub(super) status_traced: String,
    /// Frame rate of `viewer_video_frames`, for clock → frame-index mapping.
    pub(super) viewer_video_fps: f64,
    /// File ID whose frames are in `viewer_video_frames` (cache invalidation).
    pub(super) viewer_frame_cache_file: Option<i32>,
    /// Frame extraction in progress (async); the viewer shows the thumbnail
    /// with a "loading video" hint until frames land.
    pub(super) viewer_extracting: bool,
    /// Running ffmpeg viewer-frame extraction, published by the background
    /// task. Taken and killed when the viewer closes or steps; stale
    /// completions are dropped by `viewer_extract_epoch`.
    pub(super) viewer_extract_child: Option<Arc<Mutex<Option<Child>>>>,
    /// Shared cancellation flag for the in-flight extraction: set by
    /// `kill_viewer_extraction` so the worker can abort even if the UI
    /// kills before ffmpeg publishes its child into `viewer_extract_child`.
    pub(super) viewer_extract_cancel: Option<Arc<AtomicBool>>,
    /// Generation counter for viewer frame extraction: bumped on every new
    /// extraction and on cancel, so a late completion from an abandoned run
    /// is dropped silently (no error note, no playback).
    pub(super) viewer_extract_epoch: u64,
    /// Screenshot demo only: skip the async frame extraction in
    /// `maybe_autoplay_viewer_video` (the demo extracts + decodes
    /// synchronously itself for a deterministic capture).
    pub(super) viewer_demo_sync_frames: bool,
    /// Guard for the viewer's 250 ms elapsed tick.
    pub(super) viewer_tick: bool,
    /// Play was requested before the clip was local. Resumed from the poll
    /// loop when `downloadFile` finishes.
    pub(super) viewer_pending_play: Option<(MessageId, FileId)>,
    /// MED1: photo rotation in quarter-turns clockwise (photos only;
    /// reset on open/step). Rendered from `viewer_rotated`.
    pub(super) viewer_rotation: u8,
    /// MED1: rotated render of the viewer photo, keyed
    /// `(path, quarter-turns)`; decoded eagerly by the Rotate button so
    /// the overlay render stays allocation-free.
    pub(super) viewer_rotated: Option<(PathBuf, u8, Arc<RenderImage>)>,
    /// MED1: seek slider for the viewer video transport (created in
    /// `begin_viewer_video`, cleared in `stop_viewer_video`).
    pub(super) viewer_seek_slider: Option<Entity<SliderState>>,
    /// MED1: true while the viewer seek thumb is being dragged (the tick
    /// must not fight the drag).
    pub(super) viewer_seek_scrubbing: bool,
    /// MED1: drag preview position for the viewer seek slider.
    pub(super) viewer_seek_preview_secs: Option<f64>,
    /// MED1: volume slider for the viewer video transport (0–100%).
    pub(super) viewer_volume_slider: Option<Entity<SliderState>>,
    /// MED1: true while the viewer volume thumb is being dragged.
    pub(super) viewer_volume_scrubbing: bool,
    /// MED1: playback speed multiplier, 0.5–2.0 (TGX `PlaybackSpeed*`;
    /// applied via ffplay `atempo` + the playback clock rate).
    pub(super) playback_speed: f64,
    /// MED1: playback volume 0.0–1.0 (ffplay `-volume`); 0 is muted.
    pub(super) playback_volume: f32,
    /// MED1: last non-zero volume, restored by the mute toggle.
    pub(super) playback_unmuted_volume: f32,
    /// MED1: honest playback error for the active track — set instead of
    /// the old silent failure (e.g. ffplay missing, unsupported format).
    pub(super) playback_error: Option<String>,
    /// MED1: composer "group media" override for 2+ attachments; `None`
    /// follows `media_prefs.default_grouping()`.
    pub(super) composer_group_media: Option<bool>,
    /// Phase 9.1: fullscreen story viewer (active-story tray → overlay).
    pub(super) story_viewer: StoryViewer,
    /// Phase 9.6: playback clock + segmented progress bar for the viewer;
    /// `story_tick_active` guards the at-most-one 100ms tick task (same
    /// pattern as `ensure_call_tick`).
    pub(super) story_playback: StoryPlayback,
    pub(super) story_tick_active: bool,
    /// Phase 9.1: `(chat_id, story_id)` the user tapped while the story's
    /// full content was still being fetched; resolved on the next render
    /// once the `story` response lands in the cache.
    pub(super) pending_story_open: Option<(i64, i32)>,
    /// Phase 9.2: story reaction picker open above the viewer overlay.
    pub(super) story_reaction_picker_open: bool,
    /// Phase 9.2: story reply input open in the viewer overlay.
    pub(super) story_reply_open: bool,
    /// Phase 9.2: reply-to-story draft (the viewer overlay's reply row).
    pub(super) story_reply_input: Entity<TextareaState>,
    /// Phase 9.5: viewers panel open in the viewer overlay
    /// (`getStoryInteractions`).
    pub(super) story_viewers_open: bool,
    /// Phase 9.5: report flow UI open in the viewer overlay
    /// (`reportStory`).
    pub(super) story_report_open: bool,
    /// Phase 9.5: report details draft (the
    /// `reportStoryResultTextRequired` step).
    pub(super) story_report_text_input: Entity<TextareaState>,
    /// Phase 9.7: the chat story page overlay (albums / chat-page
    /// stories / archive); `None` when closed.
    pub(super) story_page: Option<StoryPage>,
    /// Phase 9.3: story posting composer state (pure) + its path /
    /// caption / user-search inputs.
    pub(super) story_composer: StoryComposer,
    pub(super) story_composer_path: Entity<TextareaState>,
    pub(super) story_composer_caption: Entity<TextareaState>,
    pub(super) story_composer_user_search: Entity<TextareaState>,
    /// Phase 9.4: story areas — link URL + suggested-reaction emoji
    /// inputs.
    pub(super) story_composer_link: Entity<TextareaState>,
    pub(super) story_composer_reaction: Entity<TextareaState>,
    /// Phase 9.5: cover-frame editor (viewer) — open on a video story
    /// with `can_be_edited`; the input takes seconds.
    pub(super) story_cover_target: Option<(i64, i32)>,
    pub(super) story_cover_input: Entity<TextareaState>,
    pub(super) story_cover_sent: bool,
    /// Phase 9.5: privacy editor (viewer) — open on a story with
    /// `can_set_privacy_settings`; reuses the 4-way privacy selector
    /// + contact checkboxes.
    pub(super) story_privacy_edit: Option<StoryPrivacyEdit>,
    pub(super) story_privacy_user_search: Entity<TextareaState>,
    pub(super) story_privacy_sent: bool,
    /// Phase 6: sidebar tab — `true` shows the contacts list instead of
    /// the chat list.
    pub(super) contacts_tab_open: bool,
    /// Phase C2i: sidebar tab — `true` shows the recent-calls list +
    /// call settings instead of the chat list. Mutually exclusive with
    /// `contacts_tab_open`.
    pub(super) calls_tab_open: bool,
    /// Phase C2i: pending "call again" / profile-call confirmation when
    /// the confirm-before-calling pref is on: `(user_id, is_video)`.
    pub(super) call_confirm: Option<(i64, bool)>,
    /// Phase C2i: rating detail draft for the call-end card — the star
    /// tap opens the problems checklist + comment field instead of
    /// sending immediately.
    pub(super) rating_detail: Option<RatingDetail>,
    /// Phase C2i: comment input for the rating detail card.
    pub(super) rating_comment_input: Entity<TextareaState>,
    /// Phase 7.1: selected folder tab (`None` = Main). Folder membership
    /// comes from chat positions (`chatListFolder`); the tab only filters.
    pub(super) folder_tab: Option<i32>,
    /// Phase 6: add-contact dialog (phone + first/last name) opened from
    /// the user info panel.
    pub(super) add_contact_dialog: Option<AddContactDialog>,
    /// A5: edit-profile dialog (name / bio / username / photo) opened
    /// from the user's own info panel.
    pub(super) edit_profile_dialog: Option<EditProfileDialog>,
    /// Slice A6: vCard import dialog opened from the Contacts tab
    /// settings section.
    pub(super) import_contacts_dialog: Option<ImportContactsDialog>,
    /// Parity slice: folder management (manage dialog / editor / delete
    /// confirm / per-chat folder menu).
    pub(super) folder_manage_open: bool,
    pub(super) folder_editor: Option<FolderEditorDialog>,
    pub(super) folder_delete_confirm: Option<FolderDeleteConfirm>,
    pub(super) folder_menu_open: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PaneMode {
    Synthetic,
    Connecting,
    Ready,
}

/// Slice CL2: chat-list category filter (TGX `ChatFilter` unread /
/// archive categories). View state on `QuillApp`, not the session —
/// it filters the already-loaded model, never the server query.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ChatListFilter {
    #[default]
    All,
    Unread,
    Archived,
    /// Parity slice `parity:communities-chatlist-mode`: community
    /// chat-list mode — the main list shows only the selected
    /// community's chats (membership from the cached
    /// `communityFullInfo.chats` pack; see `community_mode`).
    Community(i64),
}

pub(super) fn pane_placeholder(
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

impl Focusable for QuillApp {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_sidebar.clone()
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

    pub(super) fn current_auth(&self) -> AuthorizationState {
        if let Some(live) = self.live.as_ref() {
            live.driver.session.auth.clone()
        } else if let Some(session) = self.demo_session.as_ref() {
            session.auth.clone()
        } else {
            self.auth_demo.clone()
        }
    }

    pub(crate) fn session(&self) -> Option<&Session> {
        self.live
            .as_ref()
            .map(|live| &live.driver.session)
            .or(self.demo_session.as_ref())
    }

    pub(super) fn media_display_roots(&self) -> Vec<PathBuf> {
        let primary = if let Some(live) = self.live.as_ref() {
            live.driver.tdlib_media_roots()
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

    pub(super) fn pane_mode(&self) -> PaneMode {
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
}

impl Drop for QuillApp {
    fn drop(&mut self) {
        self.stop_animation_playback();
        self.stop_sticker_playback();
    }
}
