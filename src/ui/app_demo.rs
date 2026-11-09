//! The `QuillApp` constructor; a screenshot demo (`ScreenshotDemo`) seeds
//! the session and runs its registered setup at the end.

use super::app::QuillApp;
use super::connect_ui::ConnectUiStatus;
use super::screenshot_demo::ScreenshotDemo;
use super::synthetic::SyntheticChat;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::*;
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
        {
            quill::wake::isolate_current_thread();
            quill::power_saving::isolate_for_tests();
        }
        super::tooltip_timing::install(cx);
        super::image_budget::start_sweeper(cx);
        quill::media_tools::warm_up();
        let chat = cx.new(SyntheticChat::new);
        // Send-key mode drives kit's newline-vs-submit behavior: plain
        // Enter submits only in Enter mode; in CtrlEnter mode it inserts
        // a newline and Ctrl/Cmd+Enter sends.
        let chat_prefs = Self::load_chat_prefs();
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
        let screen = super::on_screen::OnScreen::new(
            Default::default(),
            &chat_prefs,
            connect_status,
            status_note,
            window,
            cx,
        );
        let appearance_prefs = Self::load_appearance();
        let accent_picker = Self::new_accent_picker(appearance_prefs.accent_rgb, window, cx);
        let font_picker = Self::new_font_picker(&appearance_prefs.font_family, window, cx);
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
            screen,
            players: super::players_state::PlayersUi::new(&audio_output),
            delivery: super::notify_delivery_state::NotifyDelivery::new(&audio_output),
            preferences: super::settings_state::PreferencesUi::new(
                cx,
                font_picker,
                accent_picker,
                demo,
            ),
            chat,
            // kit Phase 7: in-window menu bar (menus installed by
            // `setup_app_menus` at startup).
            menu_bar: AppMenuBar::new(cx),
            first_run: super::first_run_ui::initial_first_run(demo.is_some(), &credentials, cx),
            frame: super::frame_state::FrameUi::new(cx),
            group_call: super::group_call_state::GroupCallUi::new(),
            screen_share: Default::default(),
            composer_sends: Default::default(),
            language: super::language_state::LanguageUi::new(demo.is_some(), cx),
            auth_demo: super::auth_state::AuthDemo::new(auth_demo, demo),
            account: super::account_state::AccountUi::new(window, cx),
            credentials,
            start_files: super::start_files_state::StartFilesUi::new(window, cx),
            update_pill: Default::default(),
            agent_bridge: super::agent_state::AgentBridge::new(window, cx),
            takeover: super::agent_takeover_state::TakeoverUi::new(window, cx),
            focus_sidebar: cx.focus_handle(),
            connect: super::connection_state::ConnectGeneration::new(),
            disk: super::disk_state::DiskUi::start(window, cx),
            live,
            demo_session,
            demo_ui: super::demo_state::DemoUi::new(demo_sink),
            calls: super::calls_state::CallUi::new(audio_output.clone()),
            background: super::background_accounts::BackgroundAccounts::new(),
            appearance: appearance_prefs,
            chat_prefs,
            spell: super::spell_state::SpellUi::new(spellchecker, spell_info, dict_filter_input),
            slices: Default::default(),
            pending_deep_link: None,
            main_window: super::chat_window::MainWindow::new(cx),
        };

        if let Some(chosen) = spell_chosen {
            app.start_spell_engine(chosen, cx);
        }
        if let Some(demo) = demo {
            demo.setup(&mut app, window, cx);
            app.seed_demo_attachments(pending_attachments);
        }

        super::window_keys::intercept_window_keys(cx);
        // GPUI matches keybindings before `on_key_down`. While a shortcut
        // row is capturing, consume the key here so Escape cannot dismiss
        // Appearance and quit/close/other chords cannot fire underneath.
        // The element `on_key_down` also stops propagation for keys that
        // reach the bubble phase.
        let capture_app = cx.weak_entity();
        super::passcode::intercept_keys_unlocked(cx, move |event, _window, cx| {
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
        super::passcode::intercept_keys_unlocked(cx, move |event, _window, cx| {
            let armed = ptt_app
                .update(cx, |this, _| this.group_call.ptt_capture)
                .unwrap_or(false);
            if !armed || event.keystroke.is_modifier_key() {
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
        super::passcode::intercept_keys_unlocked(cx, move |event, window, cx| {
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
        super::passcode::intercept_keys_unlocked(cx, move |event, window, cx| {
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
        super::calls::stream_volume::intercept_live_story_volume_keys(cx);
        // Fast buttons mode: keys 1 to 9 in an empty composer press the
        // last message's inline buttons (tdesktop `setupFastButtonMode`).
        let fast_app = cx.weak_entity();
        super::passcode::intercept_keys_unlocked(cx, move |event, window, cx| {
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
        if demo.is_none() && app.account.passcode.connect_at_unlock.is_none() {
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
            // The gate counts the chat windows too: one may be in front.
            this.frame.main_active.set(active);
            this.follow_windows(cx);
            super::glyph_sweep::window_activation(window.window_handle().window_id(), active);
            this.viewer_window_activation(window, cx);
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
            this.main_window_placed(window, cx);
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
        // screen-scope: the minute timer re-applies the appearance for whichever account is on screen.
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
        app
    }
}
