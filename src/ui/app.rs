//! QuillApp struct definition and core app queries.

use super::chat_row::{ChatListItem, ChatPreviewState};
use super::connect_ui::ConnectUiStatus;
use super::demo::demo_media_allowlist;
use super::history::HistoryShared;
use super::notification_settings::SoundPickerTarget;
use super::synthetic::SyntheticChat;
use super::*;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::message_scroller::MessageScrollerState;
use gpui_kit::component::theme::ThemeMode;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::connect::LiveConnect;
use quill::credentials::TelegramCredentials;
use quill::data_settings::{AutoDownloadNetSettings, NetworkKind};
use quill::diagnostics::MemorySink;
use quill::ids::{ChatId, MessageId};
use quill::settings::{AppearancePrefs, ChatPrefs};
use quill::state::Session;
use quill::telegram::envelope::AuthorizationState;
use quill::telegram::envelope::NotificationSettingsScope;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize};
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
    /// Chat-row swipe gesture state (`quill::chat_swipe`).
    pub(super) chat_swipe: super::chat_swipe_ui::ChatSwipeState,
    /// Stories UI state: the strip, viewer, composer, story page and their boxes.
    pub(super) stories: super::stories_state::StoryUi,
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
    pub(super) history_key: Option<(i64, Option<i32>, i64)>,
    /// "Jump to root" was asked while older replies were still loading.
    pub(super) thread_root_jump: bool,
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
    /// `(jump serial, start)` of the running jump-highlight fade.
    pub(super) highlight_fade: Option<(u64, std::time::Instant)>,
    /// B11: the reaction that just flew from the message (message, glyph, start).
    pub(super) reaction_fly: Option<super::history_fx::ReactionFly>,
    /// Floating date pill state (shown while scrolling the history).
    pub(super) scroll_date: super::history_fx::ScrollDate,
    /// Rows painted this frame: `(row, bounds, starts its day)`.
    pub(super) scroll_probe:
        std::rc::Rc<std::cell::RefCell<Vec<super::history_fx::ScrollProbeRow>>>,
    /// Top visible row of the last paint: `(row, its day separator is at
    /// the top edge)`.
    pub(super) scroll_top_probe: std::rc::Rc<std::cell::Cell<Option<(usize, bool)>>>,
    /// First and last row on screen at the last paint (the page keys scroll
    /// by this many rows).
    pub(super) scroll_view_probe: std::rc::Rc<std::cell::Cell<Option<(usize, usize)>>>,
    /// Composer state: menus, attachments, send options, reply and edit drafts (`ui/composer_state.rs`).
    pub(super) composer_ui: super::composer_state::ComposerUi,
    /// Emoji, sticker and GIF panel and their search fields (`ui/pickers_state.rs`).
    pub(super) pickers: super::pickers_state::PickerUi,
    pub(super) marketplace_open: bool,
    pub(super) marketplace_name_input: Entity<TextareaState>,
    pub(super) marketplace_comment_input: Entity<TextareaState>,
    pub(super) marketplace_private: bool,
    pub(super) marketplace_error: Option<String>,
    pub(super) registration_first_input: Entity<TextareaState>,
    pub(super) registration_last_input: Entity<TextareaState>,
    pub(super) accepted_registration_terms: Option<quill::telegram::envelope::RegistrationTerms>,
    pub(super) registration_notify_contacts: bool,
    pub(super) email_input: Entity<TextareaState>,
    pub(super) phone_input: Entity<TextareaState>,
    pub(super) signin: super::signin_ui::SignInUi,
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
    /// Search fields: global search and search in chat (`ui/search_state.rs`).
    pub(super) search_ui: super::search_state::SearchUi,
    /// Forwarding and the share box (`ui/share_state.rs`).
    pub(super) share: super::share_state::ShareUi,
    pub(super) auth_demo: AuthorizationState,
    pub(super) focus_sidebar: FocusHandle,
    pub(super) context_menu_focus: FocusHandle,
    pub(super) context_menu_was_open: bool,
    pub(super) context_menu_previous_focus: Option<FocusHandle>,
    pub(super) connect_status: ConnectUiStatus,
    pub(super) connection_generation: u64,
    /// The TDLib receive bridge stopped; shows the Closed / Retry card.
    pub(super) connection_lost: bool,
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
    /// One-to-one call UI: the call window, tick, sounds, images and notifications.
    pub(super) calls: super::calls_state::CallUi,
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
    pub(super) notify_clicks: Arc<Mutex<Vec<(ChatId, quill::notify::NotificationAction)>>>,
    /// Phase 8.1: in-flight OS notification workers; capped so a message
    /// burst cannot stack threads.
    pub(super) notify_inflight: Arc<AtomicUsize>,
    /// The frozen-account details dialog is open.
    pub(super) freeze_info_open: bool,
    /// The age verification prompt is open, and the user already started
    /// the verification (so turning on 18+ content goes to the server).
    pub(super) age_verify_open: bool,
    pub(super) age_verify_started: bool,
    /// Spell checking: the engine, results and the dictionaries box (`ui/spell_state.rs`).
    pub(super) spell: super::spell_state::SpellUi,
    /// Message menu, selection, links and per-message toggles in the history (`ui/message_state.rs`).
    pub(super) message_ui: super::message_state::MessageUi,
    /// Slice CL1: right-click chat-row context menu target + window
    /// position.
    pub(super) chat_menu: Option<ChatMenuState>,
    /// Right-click menu of the "Archived chats" row (window position).
    pub(super) archive_menu: Option<Point<Pixels>>,
    /// Contacts tab, stories menu, suggestions and search tabs.
    pub(super) global: super::chatlist_global::ChatlistGlobal,
    /// Pinned-chat drag in progress (or its release slide), see
    /// `quill::pin_reorder`; `pin_reorder_archived` says which pinned list.
    pub(super) pin_reorder: Option<quill::pin_reorder::PinReorder>,
    pub(super) pin_reorder_archived: bool,
    /// Where a pinned-row press started moving, until the 30px threshold.
    pub(super) pin_drag_anchor: Option<(i64, f32)>,
    /// Slice CL: the open peek preview — hovered/press-and-hold chat,
    /// or `None`. Transient; never an open chat.
    pub(super) chat_preview: Option<ChatPreviewState>,
    /// Profile layer opened from a sender avatar (tdesktop's
    /// `Info::LayerWidget`); presents `session.open_info_panel`.
    pub(super) profile_modal: Option<super::profile_modal::ProfileModal>,
    /// Slice CL: an in-progress long press on a chat-list row — the
    /// row's chat id + press start, for the peek preview.
    pub(super) preview_press: Option<(ChatId, Instant)>,
    /// Slice CL3: multi-select mode — checked chat ids. Non-empty while
    /// selecting; rows toggle the check instead of opening the chat and
    /// the select bar offers the bulk actions.
    pub(super) selected_chats: HashSet<i64>,
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
    /// Voice, music, GIF, sticker and inline video playback (`ui/playback_state.rs`).
    pub(super) playback: super::playback_state::PlaybackUi,
    /// Highest frame rate animated content asked for since the last
    /// clock tick (0: nothing animated rendered); see `frame_clock`.
    pub(super) animation_demand: std::cell::Cell<u32>,
    /// Chat-row online-dot and unread-badge animation state.
    pub(super) row_fx: std::cell::RefCell<quill::row_fx::RowFxMap>,
    /// What asked for the next tick: cached slices by entity id, `None`
    /// for `QuillApp` itself (see `frame_clock`).
    pub(super) animation_targets: std::cell::RefCell<std::collections::HashSet<Option<EntityId>>>,
    /// The next tick serves media playing with sound (allowed while the
    /// window is inactive).
    pub(super) animation_sound: std::cell::Cell<bool>,
    /// Redraws the TDLib poll asked for, batched by urgency
    /// (`notifications::PolledRedraw`).
    pub(super) polled_redraw: super::notifications::PolledRedraw,
    /// Whether the main window is active this frame: like tdesktop
    /// (`isGifPausedAtLeastFor` → `!widget()->isActive()`), animated
    /// stickers and emoji hold still while it isn't.
    pub(super) window_active: std::cell::Cell<bool>,
    /// The title last handed to the platform window (`window_chrome`).
    pub(super) window_title_shown: std::cell::RefCell<String>,
    /// Middle-click autoscroll over the history (`autoscroll_ui`).
    pub(super) autoscroll: super::autoscroll_ui::AutoscrollUi,
    /// Batch 4: the last `online` value sent to TDLib.
    pub(super) presence: quill::presence::PresenceSync,
    /// Batch 4: the attempts the user just terminated from the new-login
    /// alert ("New Login Prevented" box), until acknowledged.
    pub(super) login_prevented: Option<Vec<String>>,
    /// Batch 4: terms of service prompt state (decline flow, age check).
    pub(super) terms_step: TermsStep,
    pub(super) terms_age_ok: bool,
    pub(super) terms_age_error: bool,
    /// `media_display_roots`, computed once per frame (rows ask for it
    /// one by one, and it touches the file system).
    pub(super) media_roots_frame: std::cell::RefCell<Option<Vec<PathBuf>>>,
    pub(super) frame_clock_running: std::cell::Cell<bool>,
    /// History motion: new-message reveal and selection-mode fades.
    pub(super) motion: super::motion::MotionState,
    /// Cached child views (chat list, conversation) the frame clock can
    /// redraw on their own, and the chat list's animation layer; see
    /// `app_slice`.
    pub(super) slices: super::app_slice::Slices,
    /// Smooth reveal of a bot's streaming reply (`bot_stream`).
    pub(super) stream_reveal: std::cell::RefCell<super::bot_stream::StreamReveal>,
    /// Deleted messages still dissolving (`vanish`).
    pub(super) vanishing: std::cell::RefCell<Vec<super::vanish::Vanishing>>,
    /// Phase S2: storage-usage overlay (TGX Settings → Data and Storage).
    pub(super) storage_usage_open: bool,
    /// Settings → Appearance slice: client-side look-and-feel
    /// (theme/auto-night/accent/wallpaper/font-size/bubbles), persisted
    /// to `appearance_prefs.json`.
    pub(super) appearance: AppearancePrefs,
    /// Chat prefs slice: chat-composer behavior (send-key mode),
    /// persisted to `chat_prefs.json`.
    pub(super) chat_prefs: ChatPrefs,
    /// Translation: prefs (`translate_prefs.json`), the translate dialog,
    /// the bar's toast.
    pub(super) translate_ui: super::translate_ui::TranslateUi,
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
    /// Screenshot demo: the Appearance dialog shows only the accent, font
    /// family and power-saving sections.
    pub(super) appearance_power_screenshot: bool,
    /// Slice parity:platform-shortcuts-reference: the keyboard shortcuts
    /// reference dialog is on screen.
    pub(super) shortcuts_open: bool,
    /// `parity:proxy-settings`: proxy list / editor / link-confirm state.
    pub(super) proxy_ui: super::proxy::ProxyUi,
    pub(super) sticker_settings_open: bool,
    /// Settings → Appearance slice: last `(theme mode, accent)` pushed
    /// into the global component theme, so `apply_appearance` only
    /// notifies (re-renders) when something actually changed.
    pub(super) appearance_applied: Option<(ThemeMode, u32, bool, u16, String)>,
    /// The operating system's accent color (0xRRGGBB), when it reports one
    /// and has been read (`refresh_system_accent`).
    pub(super) system_accent: Option<u32>,
    /// `system_accent` was read at least once.
    pub(super) system_accent_probed: bool,
    /// Settings → Appearance: the searchable font family list.
    pub(super) font_picker: Entity<
        gpui_kit::component::select::SelectState<
            gpui_kit::component::select::SearchableVec<SharedString>,
        >,
    >,
    /// Settings → Appearance: the custom accent color field (kit
    /// `ColorSelect`), tdesktop's "custom" accent circle. Holds the last
    /// custom color; choosing one sets `appearance.accent_rgb`.
    pub(super) accent_picker: Entity<gpui_kit::component::color_picker::ColorPickerState>,
    /// Slice S3: Privacy settings overlay (TGX Settings → Privacy).
    pub(super) privacy_open: bool,
    /// B13: transient state of the privacy / security extras.
    pub(super) privacy_ui: super::privacy_extra::PrivacyUi,
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
    pub(super) storage_confirm: Option<StorageClear>,
    /// Batch 6: the file types ticked for "Clear selected".
    pub(super) storage_selected: std::collections::BTreeSet<&'static str>,
    /// Two-step verification box: its view, inputs and pending confirmation.
    pub(super) twofa: super::twofa_state::TwoStepUi,
    /// Slice A9: account lifecycle dialog (delete account + self-destruct
    /// TTL). Working state lives in the named module; this is the one
    /// field the dialog machinery reads.
    pub(super) account_lifecycle: AccountLifecycleState,
    /// Slice parity:auth-multi-account (UI): the Accounts dialog state
    /// (list / switch / add / remove). Working state lives in
    /// `accounts.rs`; this is the one field the dialog machinery reads.
    pub(super) accounts_ui: AccountsUiState,
    /// Local passcode: settings dialog, lock screen, auto-lock.
    pub(super) passcode_ui: super::passcode::PasscodeUi,
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
    /// tdesktop Mute submenu (1 hour / 8 hours / 2 days / Forever).
    pub(super) mute_menu_open: bool,
    /// The Mute submenu's "Custom..." duration row is expanded.
    pub(super) mute_custom_open: bool,
    /// The custom mute duration being edited (tdesktop `ChooseTimeWidget`).
    pub(super) mute_custom: quill::mute_menu::CustomMute,
    /// Phase B4: self-destruct / auto-delete timer picker below the
    /// conversation header (`setChatMessageAutoDeleteTime`).
    pub(super) ttl_picker_open: bool,
    /// The auto-delete picker's "Custom" stepper is expanded.
    pub(super) ttl_custom_open: bool,
    /// The custom auto-delete period being edited, in seconds.
    pub(super) ttl_custom_secs: i32,
    /// Video chat UI: its window, dialogs, composer, pin and push-to-talk.
    pub(super) group_call: super::group_call_state::GroupCallUi,
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
    /// Voice and round video recording (`ui/recording_state.rs`).
    pub(super) recording: super::recording_state::RecordingUi,
    /// Phase B3: the open chat whose self-destruct countdown badges are
    /// ticking (`Some` exactly while the 1s tick task runs). Mirrors
    /// `slow_mode_tick_chat`.
    pub(super) self_destruct_tick_chat: Option<ChatId>,
    /// The open chat whose live-location countdowns are being refreshed
    /// (`Some` exactly while that task runs; see `live_location_tick`).
    pub(super) live_location_tick_chat: Option<ChatId>,
    /// Cmd+Q hold detection (`macWarnBeforeQuit`) and its clock origin.
    pub(super) quit_guard: quill::quit_guard::QuitGuard,
    pub(super) quit_clock: Instant,
    /// Demo captures can't go full screen: show the stage anyway.
    pub(super) demo_group_stage: bool,
    pub(super) notification_sounds: super::audio::NotificationSounds,
    /// Slice P1: the payment checkout dialog's text inputs. The dialog
    /// renders from the session's `payment_form`; this holds the live
    /// text fields.
    pub(super) payment_dialog: Option<PaymentDialog>,
    /// Phase D3a: invite-link create dialog state.
    pub(super) invite_link_dialog: Option<InviteLinkDialog>,
    /// B8: the invite link whose "who joined" details are expanded.
    pub(super) invite_link_details: Option<(ChatId, String)>,
    /// B8: whether the revoked-links list is expanded.
    pub(super) revoked_links_open: bool,
    /// The invite link whose QR code is showing.
    pub(super) invite_link_qr: Option<(ChatId, String, Arc<RenderImage>)>,
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
    /// A typed link that needs the `Window` (`deep_link_routes`).
    pub(super) pending_deep_link_ui: Option<quill::deep_link_types::DeepLinkUi>,
    /// `parity:platform-deep-links`: resolved chat + action waiting for
    /// render (which owns the `Window`) to open it.
    pub(super) pending_deep_link_open: Option<(ChatId, quill::state::DeepLinkAction)>,
    /// Mini apps: the helper window, its launch and the open box
    /// (`ui/web_app_ui.rs`, `DialogKind::WebAppConfirm`).
    pub(super) mini_apps: super::web_app_ui::MiniApps,
    /// Slice G1: default chat permissions editor.
    pub(super) permissions_dialog: Option<PermissionsDialog>,
    /// Slice G1: public username editor.
    pub(super) username_dialog: Option<UsernameDialog>,
    /// Slice G1: restrict/ban dialog.
    pub(super) restrict_dialog: Option<RestrictDialog>,
    pub(super) ownership_dialog: Option<OwnershipDialog>,
    /// Slice G1: delete / leave / broadcast-upgrade / ban confirmations.
    pub(super) group_confirm_dialog: Option<GroupConfirmDialog>,
    /// Slice G2: forum-topic management dialog.
    pub(super) forum_manage_dialog: Option<ForumManageDialog>,
    /// Saved Messages: "Add Name" / "Edit Name" for a tag.
    pub(super) saved_tag_dialog: Option<super::saved_sublists::SavedTagDialog>,
    /// Slice G2: chat welcome-message editor.
    pub(super) welcome_dialog: Option<WelcomeDialog>,
    /// Slice G2: event-log search input for the info panel's
    /// "Recent actions" section (created lazily when the panel opens).
    pub(super) event_log_search: Option<Entity<TextareaState>>,
    /// Media viewer: zoom, video, controls and the photo editor (`ui/viewer_state.rs`).
    pub(super) viewer: super::viewer_state::ViewerUi,
    /// Whether the main window is currently excluded from screen capture.
    pub(super) capture_blocked: bool,
    /// The "screenshots can't be blocked here" note was dismissed (Linux).
    pub(super) capture_notice_dismissed: bool,
    /// The last status note printed by `QUILL_TRACE_STATUS`.
    pub(super) status_traced: String,
    /// Phase 9.1: `(chat_id, story_id)` the user tapped while the story's
    /// full content was still being fetched; resolved on the next render
    /// once the `story` response lands in the cache.
    pub(super) pending_story_open: Option<(i64, i32)>,
    /// The info card under the open forum topic's strip.
    pub(super) topic_info_open: bool,
    /// The info card under the open reply thread's root bar.
    pub(super) thread_info_open: bool,
    /// On a narrow window the forum's topic column replaces the chat list;
    /// this brings the list back until another forum opens.
    pub(super) forum_chats_peek: bool,
    /// The forum topic column is on screen this frame, so the conversation
    /// shows a hint instead of repeating the topic list.
    pub(super) forum_column_shown: bool,
    /// Phase 6: sidebar tab — `true` shows the contacts list instead of
    /// the chat list.
    pub(super) contacts_tab_open: bool,
    /// Phase C2i: sidebar tab — `true` shows the recent-calls list +
    /// call settings instead of the chat list. Mutually exclusive with
    /// `contacts_tab_open`.
    pub(super) calls_tab_open: bool,
    /// Phase C2i: rating detail draft for the call-end card — the star
    /// tap opens the problems checklist + comment field instead of
    /// sending immediately.
    pub(super) rating_detail: Option<RatingDetail>,
    /// Phase C2i: comment input for the rating detail card.
    pub(super) rating_comment_input: Entity<TextareaState>,
    /// Chat folders UI: the tab strip, editor, manager, share and invite boxes.
    pub(super) folders: super::folders_state::FolderUi,
    /// Phase 6: add-contact dialog (phone + first/last name) opened from
    /// the user info panel.
    pub(super) add_contact_dialog: Option<AddContactDialog>,
    /// Batch 8: "Block {name}" box opened from the chat action bar.
    pub(super) block_bar_dialog: Option<super::chat_bars::BlockBarDialog>,
    /// Batch 8: the join-requests box of this chat (from the requests bar).
    pub(super) join_requests_dialog: Option<super::chat_bars::JoinRequestsDialog>,
    /// A5: edit-profile dialog (name / bio / username / photo) opened
    /// from the user's own info panel.
    pub(super) edit_profile_dialog: Option<EditProfileDialog>,
    /// B10: edit-contact / birthday / personal-channel / share-contact
    /// dialog behind the profile panels.
    pub(super) profile_dialog: Option<ProfileDialog>,
    /// B7: group / channel settings dialog (topics, history, reactions,
    /// discussion group, ...).
    pub(super) group_settings_dialog: Option<GroupSettingsDialog>,
    /// B10: a profile photo gallery whose list was requested; the viewer
    /// opens when it lands (checked by the poll loop).
    pub(super) pending_profile_gallery: Option<i64>,
    /// Slice A6: vCard import dialog opened from the Contacts tab
    /// settings section.
    pub(super) import_contacts_dialog: Option<ImportContactsDialog>,
    /// Theme and wallpaper picker for a chat, or a `bg/` link preview.
    pub(super) chat_look_dialog: Option<super::chat_look_ui::ChatLookDialog>,
    /// The Archive menu's "How does it work?" box is open.
    pub(super) archive_hint_open: bool,
    /// Screenshot demo: the Appearance box shows only the window and tray switches.
    pub(super) window_settings_screenshot: bool,
    /// The "Export chat history" box.
    pub(super) chat_export_dialog: Option<super::chat_export_ui::ChatExportDraft>,
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
        if self.connection_lost {
            // The receive bridge died: treat it like an unexpected Closed.
            AuthorizationState::Closed
        } else if let Some(live) = self.live.as_ref() {
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
        if let Some(roots) = self.media_roots_frame.borrow().as_ref() {
            return roots.clone();
        }
        let roots = self.compute_media_display_roots();
        *self.media_roots_frame.borrow_mut() = Some(roots.clone());
        roots
    }

    fn compute_media_display_roots(&self) -> Vec<PathBuf> {
        let primary = if let Some(live) = self.live.as_ref() {
            live.driver.tdlib_media_roots()
        } else if self.demo_session.is_some() {
            let mut roots = vec![demo_media_allowlist()];
            roots.extend(super::demo::demo_stress_avatar_dir());
            roots.extend(super::demo::demo_stress_photo_dir());
            roots
        } else {
            Vec::new()
        };
        if primary.is_empty() {
            primary
        } else {
            quill::voice::with_capture_root(quill::video::with_viewer_frame_cache(
                quill::video::with_video_frame_cache(quill::animation::with_gif_frame_cache(
                    primary,
                )),
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
