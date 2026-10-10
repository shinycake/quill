//! The `QuillApp` constructor; a screenshot demo (`ScreenshotDemo`) seeds
//! the session and runs its registered setup at the end.

use super::app::{ChatListFilter, QuillApp};
use super::connect_ui::ConnectUiStatus;
use super::history::HistoryShared;
use super::screenshot_demo::ScreenshotDemo;
use super::synthetic::SyntheticChat;
use super::*;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::message_scroller::MessageScrollerState;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::{ComposerScheduling, PreviewMediaSize, should_send_on_enter};
use quill::credentials::TelegramCredentials;
use quill::diagnostics::MemorySink;
use quill::media_viewer::{MediaViewer, ViewerZoom};
use quill::story_composer::StoryComposer;
use quill::story_viewer::{StoryPlayback, StoryViewer};
use quill::telegram::envelope::AuthorizationState;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::time::Duration;
impl QuillApp {
    pub fn new_with_demo(
        window: &mut Window,
        cx: &mut Context<Self>,
        credentials: Option<TelegramCredentials>,
        demo: Option<ScreenshotDemo>,
    ) -> Self {
        let chat = cx.new(SyntheticChat::new);
        // Send-key mode drives kit's newline-vs-submit behavior: plain
        // Enter submits only in Enter mode; in CtrlEnter mode it inserts
        // a newline and Ctrl/Cmd+Enter sends.
        let chat_prefs = Self::load_chat_prefs();
        let submit_on_enter = chat_prefs.send_key_mode == quill::composer::SendKeyMode::Enter;
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Write a message...")
                .auto_grow(1, 8)
                .submit_on_enter(submit_on_enter)
        });
        // Phase C2h: in-call group-chat composer for the voice-chat
        // overlay (sendGroupCallMessage).
        let group_call_composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Message the voice chat — Enter sends")
                .auto_grow(1, 3)
                .submit_on_enter(submit_on_enter)
        });
        // Phase C2i: comment field for the call-rating detail card.
        let rating_comment_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("What went wrong? (optional)")
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        let appearance_prefs = Self::load_appearance();
        let accent_picker = Self::new_accent_picker(appearance_prefs.accent_rgb, window, cx);
        let font_picker = Self::new_font_picker(&appearance_prefs.font_family, window, cx);
        let emoji_status_hours_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Custom duration in hours")
                .auto_grow(1, 1)
        });
        let emoji_set_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search emoji packs")
                .auto_grow(1, 1)
        });
        let emoji_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search emoji")
                .auto_grow(1, 1)
        });
        let reaction_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search emoji")
                .auto_grow(1, 1)
        });
        cx.subscribe(
            &reaction_search_input,
            |_this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe(&emoji_search_input, |_this, _, event: &InputEvent, cx| {
            // The panel's rows follow the query on the next render.
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        let dict_filter_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Filter languages")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        cx.subscribe(&dict_filter_input, |_this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        let gif_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search GIFs")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let sticker_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search stickers and sets")
                .auto_grow(1, 1)
        });
        let registration_first_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("First name")
                .auto_grow(1, 1)
        });
        let registration_last_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Last name (optional)")
                .auto_grow(1, 1)
        });
        let email_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Email address")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let phone_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Phone (+country code)")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let signin = super::signin_ui::SignInUi::new(window, cx);
        // Group the digits as the user types or pastes.
        cx.subscribe_in(
            &phone_input,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.signin_phone_changed(window, cx);
                }
            },
        )
        .detach();
        let code_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Verification code")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let password_input = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Two-step password")
                .submit_on_enter(true)
        });
        let recovery_code_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Recovery code from email")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        // Slice A2: two-step verification overlay inputs. Passwords live
        // here only and are cleared on submit/close — never on the
        // session.
        let twofa_current_password = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Current password")
                .submit_on_enter(false)
        });
        let twofa_new_password = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("New password")
                .submit_on_enter(false)
        });
        let twofa_hint = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Hint (optional)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let twofa_code = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Code")
                .submit_on_enter(false)
        });
        let twofa_email = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Recovery email")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let global = super::chatlist_global::ChatlistGlobal::new(window, cx);
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
        let share_comment_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Add a comment")
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        let story_reply_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Reply to story")
                .auto_grow(1, 3)
                .submit_on_enter(true)
        });
        // Phase 9.5: report details draft — shown when the server answers
        // `reportStoryResultTextRequired`.
        let story_report_text_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Report details (optional)")
                .auto_grow(1, 3)
                .submit_on_enter(true)
        });
        // Phase 9.3: story composer inputs — media path (path entry; no
        // native file-picker infrastructure yet), caption, and the
        // selected-users search.
        let story_composer_path = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("/path/to/photo.jpg")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let story_composer_caption = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Caption… (**bold** markup supported)")
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        let story_composer_user_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search contacts")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        // Phase 9.4: story area inputs — link sticker URL and
        // suggested-reaction emoji (space-separated).
        let story_composer_link = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("https://… (optional, Premium)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let story_composer_reaction = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("❤️ (optional, space-separated)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        // Phase 9.5: cover-frame seconds input (viewer cover editor) and
        // the privacy editor's contact search.
        let story_more_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let story_cover_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Cover frame time in seconds, e.g. 1.5")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let story_privacy_user_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search contacts")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        cx.subscribe_in(
            &composer,
            window,
            |this, state, event: &InputEvent, window, cx| {
                let mut text = state.read(cx).value().to_string();
                if matches!(event, InputEvent::Change) {
                    if let Some(replaced) = this.apply_instant_replace(&text, window, cx) {
                        text = replaced;
                    }
                    this.sync_composer_typing(&text);
                    this.note_open_draft(true, cx);
                }
                // Phase 3.3: the `/` menu tracks the composer text (Blur
                // dismisses it); Enter picks the highlighted command
                // instead of sending while the menu is open.
                match event {
                    InputEvent::Blur => {
                        this.close_command_menu(cx);
                        this.close_inline_results(cx);
                        this.close_mention_menu(cx);
                        this.close_suggest_menu(false, cx);
                    }
                    _ => {
                        this.sync_command_menu(cx);
                        this.sync_mention_menu(cx);
                        this.sync_suggest_menu(cx);
                        this.sync_inline_mode(cx);
                        // codex:spellcheck-native: shift underlines with
                        // the edit and debounce a background re-check.
                        this.sync_spellcheck(&text, cx);
                        this.sync_sticker_suggestions(&text, cx);
                        this.sync_animated_emoji_suggestion(&text, cx);
                    }
                }
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        this.chat_prefs.send_key_mode,
                    ) {
                        if this.pick_inline_result_selection(window, cx) {
                            // Enter was consumed by the inline results.
                        } else if this.pick_mention_selection(window, cx) {
                            // Enter completed the highlighted mention.
                        } else if this.pick_suggest_selection(window, cx) {
                            // Enter inserted the highlighted hashtag/emoji.
                        } else if this.pick_command_menu_selection(window, cx) {
                            // Enter was consumed by the open menu.
                        } else if !text.trim().is_empty()
                            || !this.pending_attachments.is_empty()
                            || this.forward_bar_here()
                        {
                            this.submit_composer(
                                quill::composer::send_text_on_enter(
                                    text,
                                    this.chat_prefs.send_key_mode,
                                ),
                                window,
                                cx,
                            );
                        }
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &group_call_composer,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        this.chat_prefs.send_key_mode,
                    ) {
                        this.send_group_call_message(window, cx);
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &email_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter,
                    ) {
                        this.submit_email(window, cx);
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
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter, // not a chat composer — the send-key setting does not apply
                    ) {
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
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter, // not a chat composer — the send-key setting does not apply
                    ) {
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
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter, // not a chat composer — the send-key setting does not apply
                    ) {
                        this.submit_password(window, cx);
                    }
                }
            },
        )
        .detach();
        auth_recovery::subscribe_recovery_input(&recovery_code_input, window, cx);
        cx.subscribe_in(
            &search_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                let text = state.read(cx).value().to_string();
                match event {
                    InputEvent::Change => this.sync_search_query(&text, cx),
                    InputEvent::Focus if !this.search_is_open() => this.open_search_ui(window, cx),
                    _ => {}
                }
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter, // not a chat composer — the send-key setting does not apply
                    ) {
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
                if matches!(event, InputEvent::Change) {
                    this.sync_chat_search_query(&text, cx);
                }
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter, // not a chat composer — the send-key setting does not apply
                    ) {
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
                if matches!(event, InputEvent::Change) {
                    let text = state.read(cx).value().to_string();
                    this.sync_share_search(&text, cx);
                }
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        quill::composer::SendKeyMode::Enter, // not a chat composer — the send-key setting does not apply
                    ) {
                        this.activate_first_forward_destination(cx);
                    }
                }
            },
        )
        .detach();

        let demo_sink = Arc::new(MemorySink::new());
        let mut demo_session = None;
        let (connect_status, live, status_note, auth_demo) = match demo {
            Some(d) => {
                let (seed, status, note, auth) = d.start();
                if let Some(seed) = seed {
                    demo_session = Some(seed(demo_sink.clone()));
                }
                (status, None, note, auth)
            }
            None => (
                ConnectUiStatus::Live,
                None,
                "Connecting to Telegram…".into(),
                AuthorizationState::WaitTdlibParameters,
            ),
        };

        let pending_attachments = demo.map_or_else(Vec::new, |d| d.attachments());

        let audio_output = super::audio::SharedOutput::default();
        super::audio::share_output_with_video(&audio_output);
        let (spellchecker, spell_info) = Self::new_spellchecker(true);
        let mut app = Self {
            update_state: if demo.is_none() {
                quill::update_install::startup_state()
            } else {
                quill::updater::UpdateState::Idle
            },
            update_banner_dismissed: false,
            chat,
            composer,
            // kit Phase 7: in-window menu bar (menus installed by
            // `setup_app_menus` at startup).
            menu_bar: AppMenuBar::new(cx),
            // kit Phase 3: chat list + message history virtualization.
            chat_list_scroll: VirtualListScrollHandle::new(),
            chat_list_items: Vec::new(),
            chat_swipe: Default::default(),
            story_strip: Default::default(),
            history_scroller: cx.new(|cx| MessageScrollerState::new(0, cx)),
            history_rows: Vec::new(),
            rendered_history_rows: std::cell::RefCell::new(Vec::new()),
            reported_visible: None,
            history_window_active: false,
            history_shared: HistoryShared::default(),
            history_key: None,
            thread_root_jump: false,
            history_ends: None,
            history_window_epoch: 0,
            history_anchor_pending: false,
            history_had_newer: false,
            sidebar_width: px(quill::settings::load_window_state()
                .map_or(quill::settings::DEFAULT_SIDEBAR_WIDTH, |state| {
                    state.sidebar_width
                })),
            window_state_save_pending: false,
            history_rows_key: None,
            history_media_signature: (0, 0),
            last_highlight: None,
            highlight_fade: None,
            reaction_fly: None,
            scroll_date: Default::default(),
            scroll_probe: Default::default(),
            scroll_top_probe: Default::default(),
            scroll_view_probe: Default::default(),
            group_call_composer,
            command_menu_open: false,
            command_menu_selected: 0,
            mention_selected: 0,
            suggest: super::composer_suggest::SuggestUi::load(),
            inline_results_open: false,
            inline_results_selected: 0,
            inline_query_token: 0,
            inline_query_armed: None,
            sticker_search_input,
            media_panel: super::media_panel::MediaPanel::default(),
            emoji_search_input,
            reaction_search_input,
            emoji_set_search_input,
            emoji_status_hours_input,
            gif_search_input,
            marketplace_open: false,
            marketplace_name_input: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Collectible gift name, e.g. PlushPepe-123")
            }),
            marketplace_comment_input: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Personal comment")
                    .submit_on_enter(false)
            }),
            marketplace_private: true,
            marketplace_error: None,
            registration_first_input,
            registration_last_input,
            accepted_registration_terms: None,
            registration_notify_contacts: false,
            email_input,
            phone_input,
            signin,
            code_input,
            password_input,
            recovery_code_input,
            recovery_mode: false,
            twofa_open: false,
            twofa_view: TwofaView::Status,
            twofa_current_password,
            twofa_new_password,
            twofa_hint,
            twofa_email,
            twofa_notice: None,
            twofa_code,
            twofa_confirm: None,
            account_lifecycle: AccountLifecycleState::new(window, cx),
            accounts_ui: AccountsUiState::new(window, cx),
            passcode_ui: super::passcode::PasscodeUi::new(window, cx),
            credentials,
            // Slice S3: privacy screen state.
            privacy_open: false,
            privacy_ui: super::privacy_extra::PrivacyUi::new(window, cx),
            privacy_editor: None,
            privacy_exceptions: None,
            exception_picker_open: false,
            block_picker_open: false,
            unblock_confirm: None,
            search_input,
            chat_search_input,
            forward_search_input,
            share_comment_input,
            story_reply_input,
            story_viewers_open: false,
            story_stats_open: false,
            topic_info_open: false,
            thread_info_open: false,
            story_report_open: false,
            story_report_text_input,
            story_page: None,
            story_composer: StoryComposer::default(),
            story_composer_path,
            story_composer_caption,
            story_composer_user_search,
            story_composer_link,
            story_composer_reaction,
            story_cover_target: None,
            story_cover_input,
            story_cover_sent: false,
            story_privacy_edit: None,
            story_privacy_user_search,
            story_native: std::cell::RefCell::new(None),
            story_native_key: None,
            story_native_failed: None,
            story_native_play_at: std::time::Instant::now(),
            story_native_paused_by_us: false,
            story_video_wait_since: None,
            story_muted: false,
            story_pause: Default::default(),
            close_friends_edit: None,
            close_friends_saving: false,
            story_share_open: false,
            story_more_search,
            story_notice: None,
            story_privacy_sent: false,
            auth_demo,
            focus_sidebar: cx.focus_handle(),
            context_menu_focus: cx.focus_handle(),
            context_menu_was_open: false,
            context_menu_previous_focus: None,
            connect_status,
            connection_generation: 0,
            connection_lost: false,
            live,
            status_note,
            status_seen: String::new(),
            status_shown_at: None,
            demo_auth_inputs: demo.is_some_and(|d| d.auth_inputs()),
            demo_session,
            demo_call_devices: None,
            demo_selected_devices: (None, None),
            demo_remote_frame: None,
            demo_local_frame: None,
            demo_screen_frame: None,
            demo_selected_camera: None,
            call_remote_image: None,
            call_local_image: None,
            qr_login_cache: None,
            group_video_images: HashMap::new(),
            demo_group_frames: HashMap::new(),
            demo_seq: AtomicU64::new(0),
            demo_sink,
            notify_clicks: Arc::new(Mutex::new(Vec::new())),
            notify_inflight: Arc::new(AtomicUsize::new(0)),
            call_notify_clicks: Arc::new(Mutex::new(Vec::new())),
            call_notified: None,
            pending_attachments,
            composer_self_destruct: None,
            composer_caption_above: false,
            composer_silent: false,
            composer_loud_chat: None,
            freeze_info_open: false,
            age_verify_open: false,
            age_verify_started: false,
            composer_preview_disabled: false,
            composer_preview_above: false,
            composer_preview_media: PreviewMediaSize::Auto,
            composer_preview_link: 0,
            edit_replace_as_file: false,
            composer_preview_token: 0,
            composer_prev_text: String::new(),
            composer_scheduling: ComposerScheduling::None,
            schedule_popup_open: false,
            schedule_picker: None,
            scheduled_dialog_open: false,
            scheduled_selected: Vec::new(),
            rich_editor_open: false,
            message_menu: None,
            chat_menu: None,
            archive_menu: None,
            global,
            pin_reorder: None,
            pin_reorder_archived: false,
            pin_drag_anchor: None,
            chat_preview: None,
            profile_modal: None,
            preview_press: None,
            selected_chats: HashSet::new(),
            swipe_reply_start: None,
            pending_reply: None,
            clear_draft_on_success: None,
            pending_edit: None,
            saved_edit_draft: String::new(),
            saved_edit_reply: None,
            pending_delete: None,
            pending_stop_poll: None,
            pending_close_secret_chat: None,
            pending_inline_bot_alert: None,
            inline_bot_alert_shown: false,
            forum_manage_dialog: None,
            saved_tag_dialog: None,
            fact_check_dialog: None,
            poll_voters_dialog: None,
            poll_add_option: None,
            checklist_dialog: None,
            share_content_dialog: None,
            welcome_dialog: None,
            event_log_search: None,
            storage_usage_open: false,
            appearance: appearance_prefs,
            chat_prefs,
            translate_ui: super::translate_ui::TranslateUi::load(),
            appearance_open: false,
            settings_open: false,
            settings_page: None,
            keybinding_capture: None,
            keybinding_error: None,
            keybinding_focus: cx.focus_handle(),
            keybindings_applied: false,
            keybindings_screenshot: false,
            appearance_power_screenshot: false,
            appearance_applied: None,
            system_accent: None,
            system_accent_probed: false,
            font_picker,
            accent_picker,
            // codex:spellcheck-native: platform engine + persisted app words.
            spellchecker,
            spell_info,
            dict_manager: Default::default(),
            dict_filter_input,
            spell_misspellings: Vec::new(),
            spell_checked_text: String::new(),
            spell_task: None,
            shortcuts_open: false,
            proxy_ui: Default::default(),
            sticker_settings_open: false,
            data_storage_editor: None,
            storage_confirm: None,
            storage_selected: Default::default(),
            sessions_open: false,
            device_qr_scanner: None,
            device_login_qr: None,
            device_link_notice: None,
            sessions_confirm: None,
            websites_open: false,
            websites_confirm: None,
            chat_filter: ChatListFilter::All,
            new_secret_picker_open: false,
            pending_forward: None,
            selection_anchor: None,
            selection_drag: None,
            selection_focus: None,
            drag_select_from: None,
            forward_picker_open: false,
            share_selection: quill::share_box::ShareSelection::default(),
            forward_bar_dest: None,
            send_as_open: false,
            forward_result: None,
            reactions_expanded: false,
            mute_menu_open: false,
            mute_custom_open: false,
            mute_custom: quill::mute_menu::CustomMute::default(),
            ttl_picker_open: false,
            ttl_custom_open: false,
            ttl_custom_secs: 86_400,
            pinned_cursor: HashMap::new(),
            hidden_pinned: HashMap::new(),
            pinned_list_open: false,
            inline_videos: Default::default(),
            animation_demand: Default::default(),
            row_fx: Default::default(),
            animation_targets: Default::default(),
            animation_sound: Default::default(),
            polled_redraw: super::notifications::PolledRedraw::new(std::time::Instant::now()),
            window_active: std::cell::Cell::new(true),
            window_title_shown: Default::default(),
            autoscroll: Default::default(),
            presence: Default::default(),
            login_prevented: None,
            terms_step: Default::default(),
            terms_age_ok: false,
            terms_age_error: false,
            media_roots_frame: Default::default(),
            frame_clock_running: Default::default(),
            motion: Default::default(),
            composer_link_dialog: None,
            composer_code_language: None,
            send_morph: Default::default(),
            slices: Default::default(),
            stream_reveal: Default::default(),
            vanishing: Default::default(),
            group_call_title_dialog: None,
            group_call_start_dialog: None,
            group_call_invite_open: false,
            notif_sound_picker_open: false,
            story_sound_picker_open: false,
            notification_defaults_open: false,
            defaults_sound_picker: None,
            defaults_exceptions_scope: None,
            notifications_confirm: None,
            voice_capture: None,
            video_note_capture: None,
            record_locked: false,
            record_discard_confirm: false,
            drop_paths: Vec::new(),
            drop_state: None,
            drop_preview: None,
            voice_tick: false,
            recording_auto_send: false,
            round_preview: Default::default(),
            slow_mode_tick_chat: None,
            self_destruct_tick_chat: None,
            call_tick_active: false,
            call_window: None,
            group_call_window: None,
            group_call_window_opening: false,
            group_call_window_closed_by_user: None,
            group_call_chat_shown: false,
            group_call_ptt: quill::calls::ptt::PushToTalk::new(),
            ptt_clock: std::time::Instant::now(),
            ptt_capture: false,
            global_ptt: Default::default(),
            global_ptt_polling: false,
            group_call_pin: quill::calls::tile_pin::TilePin::default(),
            demo_group_stage: false,
            call_window_opening: false,
            call_window_raised: false,
            call_window_closed_by_user: None,
            call_ended_at: None,
            call_sounds: super::call_sounds::CallSounds::new(audio_output.clone()),
            call_sound_marks: Default::default(),
            player: Default::default(),
            playing_voice: None,
            playing_audio: None,
            pending_audio_play: None,
            pending_voice_play: None,
            audio: super::audio::AudioEngine::new(audio_output.clone()),
            notification_sounds: super::audio::NotificationSounds::new(audio_output.clone()),
            playback_clock: None,
            playback_path: None,
            seek_slider: None,
            seek_scrubbing: false,
            seek_preview_secs: None,
            playback_tick: false,
            playback_positions: HashMap::new(),
            sticker_playback: Default::default(),
            emoji_playback: Default::default(),
            playing_animation: None,
            animation_frames: Vec::new(),
            autoplayed_gifs: Default::default(),
            animation_frame: 0,
            animation_tick: false,
            animation_fps: 8.0,
            animation_started_at: None,
            animation_extract_child: None,
            animation_extract_cancel: None,
            animation_extract_epoch: 0,
            animation_cache_file: None,
            pending_gif_play: None,
            playing_video: None,
            video_frames: Vec::new(),
            video_frame: 0,
            video_tick: false,
            video_cache_file: None,
            pending_video_play: None,
            sponsored_about_open: false,
            rendered_sponsored: std::cell::RefCell::new(Vec::new()),
            spoiler_revealed: HashSet::new(),
            poll_dialog: None,
            payment_dialog: None,
            invite_link_dialog: None,
            invite_link_details: None,
            revoked_links_open: false,
            admin_dialog: None,
            create_chat_dialog: None,
            member_dialog: None,
            callback_password_dialog: None,
            login_url_confirm: None,
            pending_deep_link: None,
            deep_link_dialog: None,
            deep_link_invite: None,
            pending_deep_link_ui: None,
            share_link_text: None,
            custom_emoji_card_seen: None,
            pending_media_seek: None,
            pending_deep_link_open: None,
            pending_link: None,
            right_clicked_link: None,
            message_menu_link: None,
            link_tooltip: None,
            open_link_confirm: None,
            link_popup: None,
            pending_viewer_seek: None,
            dismissed_keyboards: std::collections::HashSet::new(),
            collapsed_keyboards: std::collections::HashSet::new(),
            request_share: None,
            permissions_dialog: None,
            username_dialog: None,
            community_ui: CommunityUi::default(),
            restrict_dialog: None,
            ownership_dialog: None,
            group_confirm_dialog: None,
            message_menu_selection: None,
            message_menu_ui: super::message_menu_ui::MessageMenuUi::new(window, cx),
            media_viewer: MediaViewer::closed(),
            photo_editor: None,
            viewer_zoom: ViewerZoom::new(),
            viewer_frame: (720.0, 480.0),
            viewer_drag: None,
            viewer_video: None,
            pip_window: None,
            viewer_video_path: None,
            viewer_audio: super::audio::AudioEngine::new(audio_output.clone()),
            viewer_clock: None,
            capture_blocked: false,
            capture_notice_dismissed: false,
            viewer_tick: false,
            viewer_pending_play: None,
            viewer_orientation: Default::default(),
            viewer_rotated: None,
            viewer_open_gen: 0,
            viewer_last_activity: std::time::Instant::now(),
            viewer_controls_hidden: false,
            viewer_controls_gen: 0,
            viewer_over_controls: false,
            viewer_hide_timer: false,
            viewer_extra: Default::default(),
            viewer_seek_slider: None,
            viewer_seek_scrubbing: false,
            viewer_seek_preview_secs: None,
            viewer_volume_slider: None,
            viewer_volume_scrubbing: false,
            playback_speed: 1.0,
            playback_volume: 1.0,
            playback_unmuted_volume: 1.0,
            playback_error: None,
            composer_group_media: None,
            viewer_video_frames: Vec::new(),
            viewer_native: None,
            status_traced: String::new(),
            viewer_video_fps: 0.0,
            viewer_frame_cache_file: None,
            viewer_extracting: false,
            viewer_extract_child: None,
            viewer_extract_cancel: None,
            viewer_extract_epoch: 0,
            viewer_demo_sync_frames: false,
            story_viewer: StoryViewer::closed(),
            story_playback: StoryPlayback::default(),
            story_tick_active: false,
            pending_story_open: None,
            story_reaction_picker_open: false,
            story_reply_open: false,
            contacts_tab_open: false,
            calls_tab_open: false,
            call_confirm: None,
            rating_detail: None,
            rating_comment_input,
            folder_tab: None,
            folder_manage_open: false,
            folder_editor: None,
            folder_delete_confirm: None,
            folder_share: None,
            folder_invite: None,
            chat_look_dialog: None,
            folder_menu_open: false,
            folder_tab_menu: None,
            folder_new_chats_dialog: None,
            folder_limit_box: None,
            archive_hint_open: false,
            add_contact_dialog: None,
            block_bar_dialog: None,
            join_requests_dialog: None,
            edit_profile_dialog: None,
            profile_dialog: None,
            group_settings_dialog: None,
            pending_profile_gallery: None,
            import_contacts_dialog: None,
        };

        if let Some(demo) = demo {
            demo.setup(&mut app, window, cx);
        }

        let menu_app = cx.weak_entity();
        cx.intercept_keystrokes(move |event, window, cx| {
            // Shortcut capture owns the key. This interceptor is registered
            // first so it observes `keybinding_capture` before the capture
            // handler clears it.
            let capturing = menu_app
                .update(cx, |this, _| this.keybinding_capture_active())
                .unwrap_or(false);
            if !capturing {
                let menu_handled = menu_app
                    .update(cx, |this, cx| {
                        if this.message_menu.is_none()
                            && this.chat_menu.is_none()
                            && this.archive_menu.is_none()
                            && this.folder_tab_menu.is_none()
                        {
                            return false;
                        }
                        if event.keystroke.key == "escape" {
                            this.message_menu = None;
                            this.chat_menu = None;
                            this.archive_menu = None;
                            this.folder_tab_menu = None;
                            cx.notify();
                            return true;
                        }
                        super::keybindings::context_menu_captures_key(&event.keystroke)
                    })
                    .unwrap_or(false);
                if menu_handled {
                    cx.stop_propagation();
                    return;
                }
            }
            // Viewer playback keys (Space/K/J/L/Enter, tdesktop
            // `handleKeyPress`). A dialog over the viewer (delete
            // confirmation) keeps its own Enter and Space.
            if !capturing && !window.has_active_dialog(cx) {
                let viewer_handled = menu_app
                    .update(cx, |this, cx| {
                        this.handle_viewer_key(&event.keystroke, window, cx)
                    })
                    .unwrap_or(false);
                if viewer_handled {
                    cx.stop_propagation();
                    return;
                }
            }
            if capturing || event.keystroke.modifiers.modified() {
                return;
            }
            let handled = match event.keystroke.key.as_str() {
                "escape" => menu_app
                    .update(cx, |this, cx| {
                        this.close_context_menus(cx)
                            || this.close_media_viewer_on_escape(cx)
                            || this.close_media_panel(cx)
                            || this.close_inline_results(cx)
                            || this.close_mention_menu(cx)
                            || this.close_suggest_menu(true, cx)
                            || this.close_command_menu(cx)
                    })
                    .unwrap_or(false),
                "up" => menu_app
                    .update(cx, |this, cx| {
                        this.step_inline_results(-1, cx)
                            || this.step_mention_menu(-1, cx)
                            || this.step_suggest_menu(-1, false, cx)
                            || this.step_command_menu(-1, cx)
                    })
                    .unwrap_or(false),
                "down" => menu_app
                    .update(cx, |this, cx| {
                        this.step_inline_results(1, cx)
                            || this.step_mention_menu(1, cx)
                            || this.step_suggest_menu(1, false, cx)
                            || this.step_command_menu(1, cx)
                    })
                    .unwrap_or(false),
                // Tab completes the highlighted `@` / `#` / `:` suggestion.
                "tab" => menu_app
                    .update(cx, |this, cx| {
                        this.pick_mention_selection(window, cx)
                            || this.pick_suggest_selection(window, cx)
                    })
                    .unwrap_or(false),
                // The emoji strip is horizontal (tdesktop steps with
                // Left/Right too); the key keeps moving the caret otherwise.
                "left" => menu_app
                    .update(cx, |this, cx| this.step_suggest_menu(-1, true, cx))
                    .unwrap_or(false),
                "right" => menu_app
                    .update(cx, |this, cx| this.step_suggest_menu(1, true, cx))
                    .unwrap_or(false),
                _ => false,
            };
            if handled {
                cx.stop_propagation();
            }
        })
        .detach();
        // GPUI matches keybindings before `on_key_down`. While a shortcut
        // row is capturing, consume the key here so Escape cannot dismiss
        // Appearance and quit/close/other chords cannot fire underneath.
        // The element `on_key_down` also stops propagation for keys that
        // reach the bubble phase.
        let capture_app = cx.weak_entity();
        cx.intercept_keystrokes(move |event, _window, cx| {
            let keystroke = event.keystroke.clone();
            let armed = capture_app
                .update(cx, |this, _| this.keybinding_capture_active())
                .unwrap_or(false);
            if !armed {
                return;
            }
            cx.stop_propagation();
            capture_app
                .update(cx, |this, cx| {
                    this.handle_keybinding_capture(&keystroke, cx);
                })
                .ok();
        })
        .detach();
        // Settings > Calls > Push-to-talk: the next key becomes the shortcut.
        let ptt_app = cx.weak_entity();
        cx.intercept_keystrokes(move |event, _window, cx| {
            let armed = ptt_app
                .update(cx, |this, _| this.ptt_capture)
                .unwrap_or(false);
            if !armed || super::keybindings::is_modifier_key(&event.keystroke.key) {
                return;
            }
            cx.stop_propagation();
            let key = event.keystroke.key.clone();
            ptt_app
                .update(cx, |this, cx| this.capture_ptt_key(&key, cx))
                .ok();
        })
        .detach();
        // Up in an empty, focused composer edits the last own message
        // (Telegram Desktop). The textarea binds Up to a cursor move, and
        // bindings match before `on_key_down`, so intercept the keystroke.
        let edit_app = cx.weak_entity();
        cx.intercept_keystrokes(move |event, window, cx| {
            let keystroke = &event.keystroke;
            if keystroke.key != "up" || keystroke.modifiers.modified() {
                return;
            }
            let handled = edit_app
                .update(cx, |this, cx| this.try_edit_last_message(window, cx))
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        })
        .detach();
        // Fast buttons mode: keys 1 to 9 in an empty composer press the
        // last message's inline buttons (tdesktop `setupFastButtonMode`).
        let fast_app = cx.weak_entity();
        cx.intercept_keystrokes(move |event, window, cx| {
            let keystroke = &event.keystroke;
            let Some(index) =
                quill::fast_buttons::index_for_key(&keystroke.key, keystroke.modifiers.modified())
            else {
                return;
            };
            let handled = fast_app
                .update(cx, |this, cx| this.try_fast_button(index, window, cx))
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        })
        .detach();
        let notification_app = cx.weak_entity();
        cx.on_system_notification_response(move |response, cx| {
            if let Some((account, call_id)) =
                quill::notify_call::parse_call_notification_tag(&response.tag)
            {
                let _ = notification_app.update(cx, |this, cx| {
                    if this
                        .session()
                        .is_none_or(|s| account != format!("account:{}", s.account.0))
                    {
                        return;
                    }
                    let action = quill::notify_call::CallNotificationAction::from_id(
                        response.action_id.as_ref().map(|id| id.as_ref()),
                    );
                    if let Ok(mut clicks) = this.call_notify_clicks.lock() {
                        clicks.push((call_id, action));
                    }
                    cx.notify();
                });
                // Same as a chat click: a hidden window may never render on
                // its own, and the pick runs from `flush_notifications`.
                cx.activate(true);
                for window in cx.windows() {
                    let _ = window.update(cx, |_, window, _| window.activate_window());
                }
                return;
            }
            let Some((account, chat_id)) = quill::notify::parse_notification_tag(&response.tag)
            else {
                return;
            };
            let _ = notification_app.update(cx, |this, cx| {
                if this
                    .session()
                    .is_none_or(|s| account != format!("account:{}", s.account.0))
                {
                    return;
                }
                let action = quill::notify::NotificationAction::from_id(
                    response.action_id.as_ref().map(|id| id.as_ref()),
                );
                if let Ok(mut clicks) = this.notify_clicks.lock() {
                    clicks.push((chat_id, action));
                }
                cx.notify();
            });
            // A hidden (close-to-tray) or minimized window may never render
            // again on its own: bring the app forward from the click itself
            // so `flush_notifications` runs and opens the chat.
            cx.activate(true);
            for window in cx.windows() {
                let _ = window.update(cx, |_, window, _| window.activate_window());
            }
        });
        if demo.is_none() && !app.passcode_ui.deferred_connect {
            // With a local passcode the database key is wrapped: the
            // connection starts after the first unlock (tdesktop starts locked).
            app.start_connection(cx);
        }
        if demo.is_none() {
            app.spawn_passcode_tick(cx);
        }
        // Settings → Appearance: apply the persisted prefs (theme +
        // accent) before the first frame, then re-evaluate auto-night
        // (scheduled/system) once a minute. `apply_appearance` only
        // notifies when the effective theme actually changed, so the
        // tick is free when idle.
        if demo.is_some()
            && let Some(pct) = super::interface_zoom::demo_interface_scale()
        {
            app.appearance.interface_scale_pct = pct;
        }
        if app.appearance.system_accent && demo.is_none() {
            app.refresh_system_accent(cx);
        }
        app.apply_appearance(cx);
        app.init_slices(cx);
        // Animations stop behind another app and resume on activation:
        // update the gate now and redraw (the content asks for ticks again).
        cx.observe_window_activation(window, |this, window, cx| {
            let active = window.is_window_active() || super::frame_clock::assume_active();
            this.window_active.set(active);
            this.inline_videos.borrow_mut().set_window_active(active);
            // The system accent may have changed while another app was in
            // front.
            if active && this.appearance.system_accent {
                this.refresh_system_accent(cx);
                this.apply_appearance(cx);
            }
            cx.notify();
        })
        .detach();
        // Remember the window's geometry when the user moves or resizes it.
        cx.observe_window_bounds(window, |this, window, cx| {
            this.schedule_window_state_save(window, cx);
        })
        .detach();
        // Performance fixture: a steady stream of synthetic TDLib updates
        // (`demo_stream`), to measure what an idle signed-in window costs.
        if demo.is_some()
            && let Some(rate) = super::demo_stream::update_stream_rate()
        {
            app.spawn_demo_update_stream(rate, cx);
        }
        // Performance fixture: keep rendering at ~60 Hz so a profiler sees
        // steady-state frames (`QUILL_DEMO_STRESS_REDRAW=0`: only what the
        // app itself asks for, to measure idle animation cost).
        if demo.is_some()
            && super::demo::demo_stress_size().is_some()
            && std::env::var_os("QUILL_DEMO_STRESS_REDRAW").is_none_or(|v| v != "0")
        {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
        if demo.is_none()
            && app.appearance.check_updates_on_launch
            && app.update_state == quill::updater::UpdateState::Idle
        {
            app.check_for_updates(cx);
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(60))
                    .await;
                let alive = this
                    .update(cx, |this, cx| {
                        if this.appearance.system_accent {
                            this.refresh_system_accent(cx);
                        }
                        this.apply_appearance(cx)
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
        // The scroller notifies on every scroll: drives the floating date.
        cx.observe(&app.history_scroller, |this, _, cx| {
            this.note_history_scroll(cx);
        })
        .detach();
        app
    }

    /// Phase C2l: synthetic peer screen-share frame for the screenshot
    /// fixture — a 16:9 "desktop" test pattern (dark gradient, window
    /// rectangles, a taskbar strip) so the receive tile is visually
    /// distinct from the camera patterns. NOT a real share: screenshot
    /// demos only.
    pub(super) fn demo_screen_frame() -> quill::calls::engine::VideoFrame {
        const W: usize = 480;
        const H: usize = 270;
        let mut rgba = Vec::with_capacity(W * H * 4);
        for y in 0..H {
            for x in 0..W {
                let fx = x as f32 / (W - 1) as f32;
                let fy = y as f32 / (H - 1) as f32;
                // Dark blue-gray desktop gradient.
                let (mut r, mut g, mut b) = (
                    (24.0 + 20.0 * fx) as u8,
                    (32.0 + 28.0 * fy) as u8,
                    (52.0 + 30.0 * fx) as u8,
                );
                // Two window rectangles.
                let in_win = |x0: usize, y0: usize, x1: usize, y1: usize| {
                    x >= x0 && x < x1 && y >= y0 && y < y1
                };
                if in_win(40, 30, 220, 170) || in_win(250, 50, 440, 200) {
                    (r, g, b) = (200, 208, 220);
                }
                // Window title bars.
                if in_win(40, 30, 220, 48) || in_win(250, 50, 440, 68) {
                    (r, g, b) = (70, 110, 180);
                }
                // Taskbar strip.
                if y >= H - 24 {
                    (r, g, b) = (18, 20, 26);
                }
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
        }
        quill::calls::engine::VideoFrame {
            seq: 0,
            width: W as u16,
            height: H as u16,
            rgba,
            is_local: false,
            participant_user_id: None,
            is_screen: true,
        }
    }
}
