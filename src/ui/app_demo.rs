//! The `QuillApp` constructor; a screenshot demo (`ScreenshotDemo`) seeds
//! the session and runs its registered setup at the end.

use super::app::QuillApp;
use super::connect_ui::ConnectUiStatus;
use super::screenshot_demo::ScreenshotDemo;
use super::synthetic::SyntheticChat;
use super::*;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::should_send_on_enter;
use quill::credentials::TelegramCredentials;
use quill::diagnostics::MemorySink;
use quill::telegram::envelope::AuthorizationState;
use std::sync::Arc;
use std::time::Duration;
impl QuillApp {
    pub fn new_with_demo(
        window: &mut Window,
        cx: &mut Context<Self>,
        credentials: Option<TelegramCredentials>,
        demo: Option<ScreenshotDemo>,
    ) -> Self {
        #[cfg(test)]
        quill::wake::isolate_current_thread();
        super::tooltip_timing::install(cx);
        quill::media_tools::warm_up();
        let chat = cx.new(SyntheticChat::new);
        // Send-key mode drives kit's newline-vs-submit behavior: plain
        // Enter submits only in Enter mode; in CtrlEnter mode it inserts
        // a newline and Ctrl/Cmd+Enter sends.
        let chat_prefs = Self::load_chat_prefs();
        let submit_on_enter = chat_prefs.send_key_mode == quill::composer::SendKeyMode::Enter;
        let composer = Self::new_composer_field(chat_prefs.send_key_mode, window, cx);
        // Phase C2h: in-call group-chat composer for the voice-chat
        // overlay (sendGroupCallMessage).
        let group_call_composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Message the voice chat — Enter sends")
                .auto_grow(1, 3)
                .submit_on_enter(submit_on_enter)
        });
        let appearance_prefs = Self::load_appearance();
        let accent_picker = Self::new_accent_picker(appearance_prefs.accent_rgb, window, cx);
        let font_picker = Self::new_font_picker(&appearance_prefs.font_family, window, cx);
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
        let twofa = super::twofa_state::TwoStepUi::new(window, cx);
        let search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let chat_search_input = Self::new_chat_search_field(window, cx);
        let forward_search_input = Self::new_forward_search_field(window, cx);
        let stories = super::stories_state::StoryUi::new(window, cx);
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

        let demo_sink = Arc::new(MemorySink::new());
        let mut demo_session = None;
        let (connect_status, live, status_note, auth_demo) = match demo {
            Some(d) => {
                let (seed, status, note, auth) = d.start();
                if let Some(seed) = seed {
                    demo_session = Some(Box::new(seed(demo_sink.clone())));
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
        // Live: the wordlist answers at once and the platform engine
        // (OS spell server, COM, dictionaries) is built on a background task
        // below. Demo fixtures keep the synchronous engine so captures are
        // deterministic.
        let (spellchecker, spell_info, spell_chosen) = if demo.is_some() {
            let (checker, info) = Self::new_spellchecker(true);
            (checker, info, None)
        } else {
            let (checker, info, chosen) = Self::provisional_spellchecker();
            (checker, info, Some(chosen))
        };
        let mut app = Self {
            settings: super::settings_state::SettingsUi::new(cx, font_picker, accent_picker, demo),
            chat,
            composer,
            // kit Phase 7: in-window menu bar (menus installed by
            // `setup_app_menus` at startup).
            menu_bar: AppMenuBar::new(cx),
            chat_list: super::chat_list_state::ChatListUi::new(window, cx),
            stories,
            shared_media_grid: super::shared_media_grid_state::SharedMediaGridUi::new(),
            premium_preview: Default::default(),
            first_run: super::first_run_ui::initial_first_run(demo.is_some(), &credentials, cx),
            history: super::history_state::HistoryUi::new(cx),
            frame: super::frame_state::FrameUi::new(cx),
            group_call: super::group_call_state::GroupCallUi::new(group_call_composer),
            screen_share: Default::default(),
            composer_ui: super::composer_state::ComposerUi::new(),
            composer_sends: Default::default(),
            pickers: super::pickers_state::PickerUi::new(
                window,
                cx,
                emoji_search_input,
                reaction_search_input,
            ),
            sticker_box: super::sticker_set_box::state::StickerSetBoxUi::new(window, cx),
            payments: super::payments_state::PaymentUi::new(window, cx),
            earn: super::earn_state::EarnUi::new(window, cx),
            language: super::language_state::LanguageUi::new(demo.is_some(), cx),
            common_groups: Default::default(),
            auth_ui: super::auth_state::AuthUi::new(
                window,
                cx,
                email_input,
                phone_input,
                code_input,
                password_input,
                recovery_code_input,
                auth_demo,
                demo,
            ),
            twofa,
            account: super::account_state::AccountUi::new(window, cx),
            credentials,
            privacy: super::privacy_state::PrivacyState::new(window, cx),
            search_ui: super::search_state::SearchUi::new(search_input, chat_search_input),
            media_search: super::shared_media_search_state::SharedMediaSearchUi::new(window, cx),
            location_ui: Default::default(),
            share: super::share_state::ShareUi::new(window, cx, forward_search_input),
            start_files: super::start_files_state::StartFilesUi::new(window, cx),
            ai_compose: super::ai_compose_state::AiComposeUi::new(window, cx),
            update_pill: Default::default(),
            agent_ui: super::agent_state::AgentUi::new(window, cx),
            takeover: super::agent_takeover_state::TakeoverUi::new(window, cx),
            focus_sidebar: cx.focus_handle(),
            connection: super::connection_state::ConnectionUi::new(connect_status, status_note),
            live,
            demo_session,
            demo_ui: super::demo_state::DemoUi::new(demo_sink),
            calls: super::calls_state::CallUi::new(audio_output.clone()),
            notify: super::notify_state::NotifyUi::new(&audio_output),
            background: super::background_accounts::BackgroundAccounts::new(),
            message_ui: super::message_state::MessageUi::new(window, cx),
            dialogs: super::dialogs_state::DialogUi::new(window, cx),
            admin: super::admin_state::AdminUi::new(),
            inline_members: super::inline_members_state::InlineMembersUi::new(),
            appearance: appearance_prefs,
            chat_prefs,
            spell: super::spell_state::SpellUi::new(spellchecker, spell_info, dict_filter_input),
            playback: super::playback_state::PlaybackUi::new(&audio_output),
            slices: Default::default(),
            recording: super::recording_state::RecordingUi::new(),
            links: super::links_state::LinkUi::new(),
            giveaway: super::giveaway_state::GiveawayUi::new(),
            paid_messages: super::paid_messages_state::PaidMessagesUi::new(),
            suggested_posts: super::suggested_posts_state::SuggestedPostsUi::new(),
            gift_market: super::gift_market_state::GiftMarketUi::new(),
            paid_reaction: super::paid_reaction_state::PaidReactionUi::new(),
            pending_deep_link: None,
            viewer: super::viewer_state::ViewerUi::new(),
            mini_apps: Default::default(),
            folders: super::folders_state::FolderUi::new(),
            windows: super::chat_window::ChatWindows::new(cx),
            peer_qr: super::peer_qr::PeerQrUi::new(cx),
            video_avatar: Default::default(),
            profile_cover: Default::default(),
        };

        if let Some(chosen) = spell_chosen {
            app.start_spell_engine(chosen, cx);
        }
        if let Some(demo) = demo {
            demo.setup(&mut app, window, cx);
            app.seed_demo_attachments(pending_attachments);
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
                        this.context_menu_key(&event.keystroke, window, cx)
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
                            || this.dismiss_sticker_suggestions(window, cx)
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
                            || this.pick_suggest_selection(true, window, cx)
                            || this.insert_command_menu_selection(window, cx)
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
                .update(cx, |this, _| this.group_call.ptt_capture)
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
        // Backspace right after `**bold**` turned into formatting puts the
        // markers back (Telegram Desktop's reverse markdown replacement).
        let revert_app = cx.weak_entity();
        cx.intercept_keystrokes(move |event, window, cx| {
            let keystroke = &event.keystroke;
            if keystroke.key != "backspace" || keystroke.modifiers.modified() {
                return;
            }
            let handled = revert_app
                .update(cx, |this, cx| this.try_revert_markdown(window, cx))
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        })
        .detach();
        super::calls::stream_volume::intercept_live_story_volume_keys(window, cx);
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
        QuillApp::register_notification_responses(cx);
        if demo.is_none() && !app.account.passcode.deferred_connect {
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
            if active {
                quill::wake::poke_on_activation();
            }
            this.frame.window_active.set(active);
            super::glyph_sweep::window_activation(active);
            this.viewer_window_activation(window, cx);
            this.playback
                .inline_videos
                .as_ref()
                .set_window_active(active);
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
        // The main window's display, and displays added, removed or resized.
        super::display_events::install(&mut app, window, cx);
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
        if demo.is_none() {
            app.start_update_checks(cx);
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
        super::os_menus::install(demo.is_some(), window, cx);
        super::history_a11y::install(&app.message_ui.history_focus, window, cx);
        // The scroller notifies on every scroll: drives the floating date.
        cx.observe(&app.history.scroller, |this, _, cx| {
            this.note_history_scroll(cx);
        })
        .detach();
        app
    }
}
