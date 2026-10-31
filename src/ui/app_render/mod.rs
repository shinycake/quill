//! impl Render for QuillApp (root view composition).
#![deny(clippy::indexing_slicing)]

use super::actions::{
    AttachFile, CancelSearch, ChatSearchNewer, ChatSearchOlder, CloseWindow,
    ComposerEditCodeLanguage, ComposerEditLink, ComposerPastePlain, DeleteSelection,
    EmojiWordBlock, EmojiWordSuggest, EmojiWordsOff, FirstChat, FocusComposer, FocusSidebar,
    FormatBlockQuote, FormatBold, FormatClear, FormatItalic, FormatMonospace, FormatSpoiler,
    FormatStrikethrough, FormatUnderline, HistoryPageDown, HistoryPageUp, HistoryToBottom,
    HistoryToTop, LastChat, LoadOlder, LockApp, MarkChatRead, MinimizeWindow, NextChat, NextFolder,
    OpenArchive, OpenChatSearch, OpenContacts, OpenHelp, OpenPinnedChat, OpenSavedMessages,
    OpenSearch, OpenSettings, OpenShortcuts, PrevChat, PrevFolder, QuitApp, ReplyToNext,
    ReplyToPrevious, SelectionExtendNewer, SelectionExtendOlder, SelectionFocusNewer,
    SelectionFocusOlder, ShowChatMenu, ShowChatPreview, SpellingIgnore, SpellingLearn,
    SpellingReplace, SpellingUnlearn, StoryTogglePause, SubmitCode, SubmitPassword, SubmitPhone,
    ToggleFullscreen, ToggleMessageSelection, ToggleSidebar, ToggleTheme, ViewerCopy,
    ViewerFlipHorizontal, ViewerFlipVertical, ViewerNext, ViewerPrev, ViewerSave, ViewerZoomIn,
    ViewerZoomOut, ViewerZoomReset, ZoomWindow,
};
use super::app::QuillApp;
use super::command_shortcuts::CommandActions;
use super::inline_players::Panel;
use super::shell::title_bar;
use super::shortcut_pack::HistoryKey;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::input::{Copy as CopyAction, Paste as PasteAction};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::agent::ui_driver::guard::{self, HardSurface};
use quill::auth::{AuthAction, view_for};
use quill::composer::ComposerShortcut;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::settings::ThemeChoice;
use quill::state::{ConnectionIndicator, connection_indicator};
impl Render for QuillApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The main window draws with its own view state in place; what was
        // there (a chat window's, when this draw interrupts its input) is
        // restored once the root is built, and around its layout and paint
        // (`WindowScope`).
        quill::startup_trace::mark("first_render");
        let outer_scope = self.windows.scope;
        self.set_window_scope(None);
        self.windows.main = Some(window.window_handle());
        self.slices.a11y_active.set(self.a11y_tree_wanted(window));
        self.release_viewer_key_probe(window, cx);
        self.sync_update_pill();
        // Before this frame counts: what showed stayed until now.
        self.sticker_frame_starts(None, cx);
        // Bounded image memory: retired images no window can show any
        // more leave the atlas, then this frame counts (`image_budget`).
        super::image_budget::begin_main_frame(window, cx);
        self.note_frame_timing(window, cx);
        super::glyph_sweep::start_frame(window, cx);
        self.sync_emoji_style(cx);
        super::spoiler_fx::release_media_tile(Some(window), cx);
        let image_cache = self.slices.image_cache();
        if let Some(cache) = &image_cache {
            cache.update(cx, |cache, cx| cache.trim(window, cx));
        }
        self.schedule_idle_image_trim(cx);
        self.sync_capture_block(window);
        self.sync_poll_dialog_height(window);
        self.sync_viewer_hdr(window);
        self.sync_speech_trial_hint(window, cx);
        self.sync_window_title(window);
        super::window_frame::sync(window, self.appearance.native_window_frame);
        // Rows the history list painted last frame are what the user saw.
        self.passcode_frame(window, cx);
        self.report_visible_history(
            window.is_window_active() && !self.account.passcode.locked,
            cx,
        );
        self.report_visible_sponsored(window.is_window_active() && !self.account.passcode.locked);
        // A conversation replayed from its cache (`app_slice`) still shows
        // its clips: only a conversation that rendered (or a frame without
        // one) and swept no player orphans them.
        let history_drawn = self.slices.conversation_rendered.replace(false)
            || !self.slices.conversation_shown.replace(false);
        self.playback
            .inline_videos
            .frame_start(history_drawn, window.scale_factor());
        // A viewer covers the chat: its clips stop even if a cached
        // conversation replays its animation layer; a closed panel stops
        // its own.
        // Picture-in-Picture outlives neither its message, the account
        // nor an unlocked app.
        self.check_video_pip(cx);
        if self.viewer.state.is_open() || self.stories.viewer.is_open() {
            self.playback.inline_videos.clear();
        } else {
            if !self.composer_ui.inline_results_open {
                self.playback
                    .inline_videos
                    .clear_panel(Panel::InlineResults);
            }
            if !self.gif_panel_open() {
                self.playback.inline_videos.clear_panel(Panel::Gifs);
            }
        }
        // Animations run while any Quill window is in front, a chat
        // window's included.
        let active = window.is_window_active()
            || self.windows.any_active()
            || super::frame_clock::assume_active();
        if self.frame.window_active.replace(active) != active {
            self.playback.inline_videos.set_window_active(active);
        }
        self.sync_video_avatar(active, window.scale_factor(), cx);
        self.sync_userpic_videos(active, window.scale_factor(), history_drawn);
        self.viewer_follow_window_fullscreen(window);
        self.frame.media_roots_frame.borrow_mut().take();
        // Spoiler specks painted last frame keep drifting.
        if super::spoiler_fx::take_text_painted() || super::spoiler_fx::revealing() {
            self.request_animation_tick(30, cx);
        }
        let status_toast = self.status_toast_visible(cx);
        self.frame.context_menu_items.begin();
        let menu_open = self.context_menu_open();
        if menu_open && !self.frame.context_menu_was_open {
            self.frame.context_menu_previous_focus = window.focused(cx);
            window.focus(&self.frame.context_menu_focus, cx);
        } else if !menu_open && self.frame.context_menu_was_open {
            // A menu action may already have focused an editor or a dialog.
            if self.frame.context_menu_focus.contains_focused(window, cx)
                && let Some(previous) = self.frame.context_menu_previous_focus.as_ref()
            {
                window.focus(previous, cx);
            }
            self.frame.context_menu_previous_focus = None;
        }
        self.frame.context_menu_was_open = menu_open;
        self.track_layer_focus(window, cx);
        self.frame_upkeep(window, cx);
        let auth_state = self.current_auth();
        let auth = view_for(&auth_state);
        let inputs_live = self.live.is_some() || self.auth_ui.demo_inputs;
        let show_phone = inputs_live && matches!(auth.action, AuthAction::EnterPhone);
        let show_code = inputs_live && matches!(auth.action, AuthAction::EnterCode);
        let show_password = inputs_live && matches!(auth.action, AuthAction::EnterPassword);
        let show_qr = inputs_live && matches!(auth.action, AuthAction::WaitOtherDevice);
        // Slice parity:platform-offline-indicator — re-read every frame
        // (the 40ms `poll_live` loop applies `updateConnectionState` and
        // re-renders), so the indicator follows TDLib live.
        let connection = self
            .session()
            .map(|session| session.connection)
            .and_then(connection_indicator);
        let root = div()
            .flex()
            .flex_col()
            .size_full()
            .relative()
            .bg(cx.theme().background)
            // Window-wide text selection: message text can be selected and
            // copied within a message (Telegram Desktop).
            .child(gpui_kit::base::TextSelectionLayer)
            // Input in the main window puts its own state in place.
            .child(super::chat_window::scope_probe(cx.weak_entity(), None))
            // Batch 4: input clock for the online/idle presence.
            .child(super::presence::input_probe())
            // The viewer's Space hold ends on the key-up (key-downs reach it
            // through the keystroke interceptor).
            .capture_key_up(cx.listener(|this, event: &KeyUpEvent, _, cx| {
                if this.viewer_key_up(&event.keystroke.key, cx) {
                    cx.stop_propagation();
                }
            }))
            // Capture phase: with message text selected, ⌘C copies it even
            // while the composer has focus.
            .capture_action(cx.listener(|_this, _: &CopyAction, window, cx| {
                if let Some(((chat_id, _), text)) =
                    super::selectable_text::selected_message_text(window, cx)
                {
                    if !_this.refuse_protected_copy(ChatId(chat_id), cx) {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                    cx.stop_propagation();
                } else if let Some((chat_id, text)) = _this.selected_messages_text() {
                    // Selection mode: copy the selected messages.
                    if !_this.refuse_protected_copy(chat_id, cx) {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &LockApp, _, cx| this.lock_by_passcode(cx)))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                this.navigate(super::navigation::NavigationAction::Settings, window, cx);
            }))
            .on_action(cx.listener(|_, _: &QuitApp, window, cx| {
                if guard::refuse_hard_if_injecting(HardSurface::Window) {
                    return;
                }
                window.remove_window();
                cx.quit();
            }))
            // kit Phase 7: window-chrome actions behind the File / Window /
            // View / Help menus (same dispatch path as the key bindings).
            .on_action(cx.listener(|this, _: &CloseWindow, window, cx| {
                use quill::tray::{CloseOutcome, close_outcome};
                if guard::refuse_hard_if_injecting(HardSurface::Window) {
                    return;
                }
                match close_outcome(
                    this.appearance.minimize_to_tray,
                    quill::tray::tray_available(),
                    cfg!(target_os = "macos"),
                    super::window_control::hide_supported(window),
                ) {
                    CloseOutcome::HideApp => cx.hide(),
                    CloseOutcome::Hide => {
                        if !super::window_control::set_visible(window, false) {
                            window.minimize_window();
                        }
                    }
                    CloseOutcome::Minimize => window.minimize_window(),
                    CloseOutcome::Quit => {
                        window.remove_window();
                        // macOS keeps a windowless app alive for its menu
                        // bar; elsewhere closing the only window quits.
                        #[cfg(not(target_os = "macos"))]
                        cx.quit();
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &MinimizeWindow, window, cx| {
                if guard::refuse_hard_if_injecting(HardSurface::Window) {
                    return;
                }
                #[cfg(target_os = "macos")]
                if this.appearance.minimize_to_tray && quill::tray::tray_available() {
                    cx.hide();
                    return;
                }
                let _ = (this, cx);
                window.minimize_window();
            }))
            .on_action(cx.listener(|this, _: &ZoomWindow, window, _| {
                let _ = this;
                if guard::refuse_hard_if_injecting(HardSurface::Window) {
                    return;
                }
                window.zoom_window();
            }))
            .on_action(cx.listener(|this, _: &ToggleFullscreen, window, _| {
                let _ = this;
                if guard::refuse_hard_if_injecting(HardSurface::Window) {
                    return;
                }
                window.toggle_fullscreen();
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, window, cx| {
                this.toggle_sidebar(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleTheme, _, cx| {
                // A settings change, written to disk.
                if guard::refuse_if_injecting("change the theme") {
                    return;
                }
                // Write through the appearance funnel (persist + re-apply)
                // so the 60s auto-night tick can't silently revert the
                // flip. Auto-night, when enabled, still overrides the
                // manual choice while active — same as the dialog.
                // stories-high-contrast: the toggle cycles all three
                // modes (Light → Dark → High contrast).
                let next = match this.appearance.theme {
                    ThemeChoice::Light => ThemeChoice::Dark,
                    ThemeChoice::Dark => ThemeChoice::HighContrast,
                    ThemeChoice::HighContrast => ThemeChoice::Light,
                };
                this.set_appearance(cx, |a| a.theme = next);
            }))
            .on_action(cx.listener(|this, _: &OpenHelp, _, cx| {
                let _ = this;
                cx.open_url("https://github.com/shinycake/quill");
            }))
            .on_action(cx.listener(|this, _: &OpenShortcuts, _, cx| {
                this.open_settings(Some(quill::settings_catalog::Page::Shortcuts));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NextChat, window, cx| {
                this.step_open_chat(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PrevChat, window, cx| {
                this.step_open_chat(-1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusComposer, window, cx| {
                this.composer
                    .update(cx, |input, cx| input.focus(window, cx));
            }))
            // Parity slice (platform-paste-image): the kit Textarea's paste is
            // text-only, and GPUI stops an action at the first handler, so a
            // bubbling handler never ran. This one captures (runs before the
            // focused textarea): clipboard images and copied files become
            // attachments; anything else falls through to the text paste.
            .capture_action(cx.listener(|this, _: &PasteAction, window, cx| {
                if this.paste_image_from_clipboard(window, cx) {
                    cx.stop_propagation();
                } else {
                    this.offer_rich_paste(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FocusSidebar, window, cx| {
                window.focus(&this.focus_sidebar, cx);
            }))
            // Shortcut pack (see `shortcut_pack.rs`): a handler that has no
            // use for the key propagates it to the focused input.
            .on_action(cx.listener(|this, _: &ReplyToPrevious, window, cx| {
                if !this.reply_by_key(false, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ReplyToNext, window, cx| {
                if !this.reply_by_key(true, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &AttachFile, window, cx| {
                if !this.attach_by_key(window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &HistoryPageUp, window, cx| {
                if !this.scroll_history_by_key(HistoryKey::PageUp, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &HistoryPageDown, window, cx| {
                if !this.scroll_history_by_key(HistoryKey::PageDown, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &HistoryToTop, window, cx| {
                if !this.scroll_history_by_key(HistoryKey::Top, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &HistoryToBottom, window, cx| {
                if !this.scroll_history_by_key(HistoryKey::Bottom, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &DeleteSelection, window, cx| {
                if !this.delete_selection_by_key(window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleMessageSelection, window, cx| {
                if !this.toggle_focused_selection_by_key(window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &SelectionFocusOlder, window, cx| {
                if !this.move_selection_focus_by_key(true, false, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &SelectionFocusNewer, window, cx| {
                if !this.move_selection_focus_by_key(false, false, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &SelectionExtendOlder, window, cx| {
                if !this.move_selection_focus_by_key(true, true, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &SelectionExtendNewer, window, cx| {
                if !this.move_selection_focus_by_key(false, true, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, action: &OpenPinnedChat, window, cx| {
                if !this.open_pinned_by_key(action.index, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &OpenSavedMessages, window, cx| {
                if !this.open_saved_by_key(window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &OpenArchive, window, cx| {
                if !this.open_archive_by_key(window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &OpenContacts, window, cx| {
                if !this.open_contacts_by_key(window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &FirstChat, window, cx| {
                if !this.open_edge_chat_by_key(false, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &LastChat, window, cx| {
                if !this.open_edge_chat_by_key(true, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &PrevFolder, window, cx| {
                if !this.step_folder_by_key(false, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &NextFolder, window, cx| {
                if !this.step_folder_by_key(true, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &MarkChatRead, window, cx| {
                if !this.mark_read_by_key(window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ShowChatMenu, window, cx| {
                if !this.chat_menu_by_key(window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ShowChatPreview, window, cx| {
                if !this.chat_preview_by_key(window, cx) {
                    cx.propagate();
                }
            }))
            .command_actions(cx)
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
            // zoom. Only while the viewer is open: otherwise the handler
            // propagates, so the keystroke reaches the focused input (an
            // action handler stops propagation unless told to propagate,
            // which used to eat the composer's arrow keys and "0").
            .on_action(cx.listener(|this, _: &StoryTogglePause, window, cx| {
                if this.stories.viewer.is_open() && !this.story_text_input_open(window, cx) {
                    this.toggle_story_pause(cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ViewerPrev, window, cx| {
                if this.stories.viewer.is_open() && !this.story_text_input_open(window, cx) {
                    this.step_story_viewer(-1, cx);
                } else if this.viewer.state.is_open() {
                    this.step_media_viewer(-1, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ViewerNext, window, cx| {
                if this.stories.viewer.is_open() && !this.story_text_input_open(window, cx) {
                    this.step_story_viewer(1, cx);
                } else if this.viewer.state.is_open() {
                    this.step_media_viewer(1, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ViewerZoomReset, _, cx| {
                if this.viewer.state.is_open() {
                    this.viewer_toggle_natural_zoom(cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ViewerZoomIn, _, cx| {
                if this.viewer.state.is_open() {
                    this.viewer_zoom_step(true, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ViewerZoomOut, _, cx| {
                if this.viewer.state.is_open() {
                    this.viewer_zoom_step(false, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ViewerFlipHorizontal, _, cx| {
                if this.viewer.state.is_open() {
                    this.flip_viewer_horizontal(cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ViewerFlipVertical, _, cx| {
                if this.viewer.state.is_open() {
                    this.flip_viewer_vertical(cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ViewerCopy, _, cx| {
                if this.viewer.state.is_open() {
                    this.copy_viewer_photo(cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ViewerSave, _, cx| {
                if this.viewer.state.is_open() {
                    this.save_viewer_media(cx);
                } else {
                    cx.propagate();
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
            // M1: formatting shortcuts only apply when the composer has
            // focus (otherwise the keystroke belongs to whatever is
            // focused).
            // codex:spellcheck-native: composer context-menu spelling items.
            .on_action(cx.listener(|this, action: &SpellingReplace, window, cx| {
                this.on_spelling_replace(action, window, cx);
            }))
            .on_action(cx.listener(|this, action: &SpellingLearn, _, cx| {
                this.on_spelling_learn(action, cx);
            }))
            .on_action(cx.listener(|this, action: &SpellingUnlearn, _, cx| {
                this.on_spelling_unlearn(action, cx);
            }))
            .on_action(cx.listener(|this, action: &SpellingIgnore, _, cx| {
                this.on_spelling_ignore(action, cx);
            }))
            .on_action(cx.listener(|this, action: &EmojiWordSuggest, _, cx| {
                this.on_emoji_word_suggest(action, cx);
            }))
            .on_action(cx.listener(|this, action: &EmojiWordBlock, _, cx| {
                this.on_emoji_word_block(action, cx);
            }))
            .on_action(cx.listener(|this, _: &EmojiWordsOff, _, cx| {
                this.on_emoji_words_off(cx);
            }))
            .on_action(cx.listener(|this, _: &FormatBold, window, cx| {
                this.run_composer_shortcut(ComposerShortcut::Bold, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FormatItalic, window, cx| {
                this.run_composer_shortcut(ComposerShortcut::Italic, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FormatUnderline, window, cx| {
                this.run_composer_shortcut(ComposerShortcut::Underline, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FormatStrikethrough, window, cx| {
                this.run_composer_shortcut(ComposerShortcut::Strikethrough, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FormatMonospace, window, cx| {
                this.run_composer_shortcut(ComposerShortcut::Monospace, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FormatBlockQuote, window, cx| {
                this.run_composer_shortcut(ComposerShortcut::BlockQuote, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FormatSpoiler, window, cx| {
                this.run_composer_shortcut(ComposerShortcut::Spoiler, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FormatClear, window, cx| {
                this.run_composer_shortcut(ComposerShortcut::ClearFormatting, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ComposerEditLink, window, cx| {
                this.composer_link_chord(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &super::actions::ComposerEditDate, window, cx| {
                    this.composer_date_chord(window, cx);
                }),
            )
            .on_action(cx.listener(
                |this, action: &super::actions::ComposerInsertControlChar, window, cx| {
                    this.insert_composer_control_char(action.character, window, cx);
                },
            ))
            .on_action(
                cx.listener(|this, _: &ComposerEditCodeLanguage, window, cx| {
                    this.open_code_language_dialog(window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &ComposerPastePlain, window, cx| {
                this.paste_plain_text(window, cx);
            }))
            .child(title_bar(
                self.pane_mode(),
                self.live.is_some(),
                self.search_is_open(),
                self.title_bar_update_button(cx),
                // kit Phase 7: the in-window menu bar on Linux/Windows leads
                // the title row (macOS uses the native menu bar installed by
                // `setup_app_menus`).
                cfg!(not(target_os = "macos")).then(|| self.menu_bar.clone()),
                cx,
            ))
            .children(self.frozen_banner(cx))
            .children(self.live_share_strip(cx))
            .children(self.unconfirmed_login_banner(cx))
            .when(
                matches!(
                    self.settings.update_state,
                    quill::updater::UpdateState::Installed(_)
                ) && !self.settings.update_banner_dismissed,
                |this| this.child(self.update_banner(cx)),
            )
            // Slice parity:platform-offline-indicator — slim connection
            // strip below the title bar. Offline gets the kit warning
            // banner with the "Waiting for network…" label; transitional
            // states get a presence dot plus their per-state label
            // (slice parity:platform-reconnect-states).
            .when(connection == Some(ConnectionIndicator::Offline), |this| {
                this.child(
                    Alert::warning("connection-indicator", tr(keys::Q_MISC_WAITING_FOR_NETWORK))
                        .banner(),
                )
            })
            .when_some(
                connection.and_then(|c| match c {
                    ConnectionIndicator::Transitioning(label) => Some(label),
                    ConnectionIndicator::Offline => None,
                }),
                |this, label| {
                    this.child(
                        div()
                            .w_full()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(6.))
                            .py(px(4.))
                            .child(div().size(px(8.)).rounded_full().bg(cx.theme().warning))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(label),
                            )
                            .child(self.proxy_strip_link(cx)),
                    )
                },
            )
            .when(
                self.pane_mode() == super::app::PaneMode::Connecting,
                |this| {
                    // Sign-in replaces the chat layout until the account is
                    // ready; the first-run onboarding comes before it.
                    let page = match self.first_run_view(cx) {
                        Some(first_run) => first_run,
                        None => self.onboarding(
                            &auth,
                            show_phone,
                            show_code,
                            show_password,
                            show_qr,
                            cx,
                        ),
                    };
                    this.child(div().flex_1().min_h_0().child(page))
                },
            )
            .when(
                self.pane_mode() != super::app::PaneMode::Connecting,
                |this| {
                    // The onboarding replayed from Settings takes the
                    // signed-in layout's place until it is finished.
                    match self.first_run_view(cx) {
                        Some(first_run) => this.child(div().flex_1().min_h_0().child(first_run)),
                        // Settings covers the chat list and the chat.
                        None if self.settings.open => this.child(self.settings_window(window, cx)),
                        None => this.child(self.shell_columns(
                            &auth,
                            show_phone,
                            show_code,
                            show_password,
                            show_qr,
                            window,
                            cx,
                        )),
                    }
                },
            )
            // Screenshot demos keep their caption as a fixed footer line
            // (`QUILL_DEMO_HIDE_STATUS=1` hides it, for recordings).
            .when(
                self.live.is_none()
                    && !self.connection.status_note().is_empty()
                    && !self.demo_pins_toast()
                    && std::env::var_os("QUILL_DEMO_HIDE_STATUS").is_none(),
                |this| {
                    this.child(
                        div()
                            .id("status-line")
                            .flex_none()
                            .min_h(px(24.))
                            .px_3()
                            .role(Role::Label)
                            .aria_label(self.connection.status_note().to_string())
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.connection.status_note().to_string()),
                    )
                },
            )
            .when(self.viewer.state.is_open(), |this| {
                this.child(self.media_viewer_overlay(window, cx))
            })
            .children(self.first_run_window_cover(cx))
            // Live: the latest status note is a transient toast floating
            // above the composer — no layout shift, no click capture, gone
            // after a few seconds. It paints above the media viewer too
            // ("Saved to Downloads", "Frame copied"), clear of its controls.
            .when_some(status_toast, |this, note| {
                let insets = self.toast_insets(window, cx);
                this.child(self.status_toast_layer(&note, insets, cx))
            })
            .children(self.custom_emoji_card(cx))
            .children(self.photo_editor_overlay(cx))
            // Phase 9.1: story viewer overlay above the media viewer.
            .when(self.stories.viewer.is_open(), |this| {
                match self.story_slot() {
                    Some(slot) => this.child(slot),
                    None => this.child(
                        self.story_viewer_overlay(f32::from(window.viewport_size().width), cx),
                    ),
                }
            })
            // Phase 9.3: story composer overlay above the story viewer.
            .when(self.stories.composer.open, |this| {
                this.child(self.story_composer_overlay(cx))
            })
            // Phase 9.7: chat story page overlay (albums / chat page /
            // archive) above the story composer.
            .when(self.stories.page.is_some(), |this| {
                this.child(self.story_page_overlay(cx))
            })
            // kit Phase 2 (redo): add-contact now hosted in a kit Dialog
            // via the shell sync — render wiring deleted.
            // kit Phase 2 (redo): edit-profile now hosted in a kit Dialog
            // via the shell sync — render wiring deleted.
            // kit Phase 2 (redo): vCard import now hosted in a kit Dialog
            // via the shell sync — render wiring deleted.
            // kit Phase 2 (redo): G1 dialogs now hosted in kit Dialogs
            // via the shell sync — render wiring deleted.
            // kit Phase 2 (redo): B1 dialogs now hosted in kit Dialogs via
            // the shell sync — render wiring deleted.
            // kit Phase 2 (redo): folder dialogs now hosted in kit Dialogs
            // via the shell sync — render wiring deleted.
            // kit Phase 2 (redo): call confirm now hosted in a kit Dialog
            // via the shell sync — render wiring deleted.
            // kit Phase 2 (redo): storage usage now hosted in a kit Dialog
            // via the shell sync — render wiring deleted.
            // kit Phase 2 (redo): two-step verification now hosted in a kit
            // Dialog via the shell sync — render wiring deleted.
            // kit Phase 2 (redo): sessions now hosted in a kit Dialog via
            // the shell sync — render wiring deleted.
            // kit Phase 2 (redo): websites now hosted in a kit Dialog via
            // the shell sync — render wiring deleted.
            // kit Phase 2 (redo): archive settings now hosted in a kit
            // Dialog via the shell sync — render wiring deleted.
            // kit Phase 2 (redo): appearance now hosted in a kit Dialog via
            // the shell sync — render wiring deleted.
            // Settings → Privacy: the per-rule editor and the always/never
            // exception list, over the settings page.
            .when_some(self.privacy_editor_overlay(cx), |this, overlay| {
                this.child(overlay)
            })
            .when_some(self.privacy_exceptions_overlay(cx), |this, overlay| {
                this.child(overlay)
            })
            // Avatar-click profile layer (tdesktop `Info::LayerWidget`).
            .when_some(self.profile_modal_overlay(window, cx), |this, overlay| {
                this.child(overlay)
            })
            // Revenue and affiliate pages (tdesktop `Info::LayerWidget`).
            .when_some(self.earn_layer(window, cx), |this, layer| this.child(layer))
            // Phase C1: call overlay above everything else.
            .when_some(self.call_overlay(cx), |this, overlay| this.child(overlay))
            // Phase C3a: group-call (voice chat) overlay above the call
            // overlay.
            .when_some(self.group_call_overlay(cx), |this, overlay| {
                this.child(overlay)
            })
            // kit Phase 2 (redo): group-call start now hosted in a kit
            // Dialog via the shell sync — render wiring deleted.
            // kit Phase 2 (redo): scheduled messages now hosted in a kit
            // Dialog via the shell sync — render wiring deleted.
            // M1: right-click message context menu.
            .when_some(self.message_ui.menu, |this, menu| {
                this.child(self.message_menu_overlay(menu, cx))
            })
            // The expanded reaction selector, where the menu was, or the
            // emoji status selector under the main menu.
            .when_some(
                self.pickers.media_panel.selector_anchor(),
                |this, position| {
                    let panel = self.media_panel(cx);
                    this.child(
                        div()
                            .id("reaction-selector-layer")
                            .occlude()
                            .absolute()
                            .inset_0()
                            .child(
                                div()
                                    .id("reaction-selector-backdrop")
                                    .absolute()
                                    .inset_0()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_media_panel(cx);
                                    })),
                            )
                            .child(
                                anchored()
                                    .position(position)
                                    .snap_to_window_with_margin(px(8.))
                                    .child(panel),
                            ),
                    )
                },
            )
            // Slice CL1: right-click chat-row context menu.
            .when_some(self.chat_list.menu, |this, menu| {
                this.child(self.chat_menu_overlay(menu, cx))
            })
            .when_some(
                self.chat_list.global.story_menu,
                |this, (chat_id, position)| {
                    this.child(self.story_menu_overlay(chat_id, position, cx))
                },
            )
            .when_some(self.playback.playlist_menu, |this, menu| {
                this.child(self.playlist_menu_overlay(menu, cx))
            })
            .when_some(self.chat_list.archive_menu, |this, position| {
                this.child(self.archive_menu_overlay(position, cx))
            })
            .when_some(self.folders.tab_menu, |this, menu| {
                this.child(self.folder_tab_menu_overlay(menu, cx))
            })
            // Slice CL: floating peek preview — read-only recent
            // messages beside the pressed chat-list row. Rendered above
            // the row menu; any click or the long-press release closes
            // it.
            .when_some(self.chat_list.preview, |this, preview| {
                this.child(self.chat_preview_overlay(preview, cx))
            })
            // MED4: Instant View reader overlay (above the menu).
            .when_some(self.instant_view_overlay(cx), |this, overlay| {
                this.child(overlay)
            })
            // The copy menu of a phone number, card number, date or TON
            // address, and the tooltip of a text link; above the Instant
            // View reader, whose links open them too.
            .when_some(self.link_popup_overlay(cx), |this, overlay| {
                this.child(overlay)
            })
            .when_some(self.link_tooltip_overlay(), |this, overlay| {
                this.child(overlay)
            })
            // Middle-click autoscroll: the anchor mark, and any other press
            // ends the mode (`ListWidget::mousePressEvent`).
            .when_some(self.autoscroll_mark(cx), |this, mark| this.child(mark))
            .when(self.autoscroll_active(), |this| {
                this.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.autoscroll_stop(cx)),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _, _, cx| this.autoscroll_stop(cx)),
                )
            });
        // The lock screen covers the whole window, above every overlay.
        let root = root.when(self.account.passcode.locked, |this| {
            this.child(self.lock_overlay(cx))
        });
        // An agent driving the window: the glow, the ghost cursor and the
        // pill, deferred above everything (`agent_takeover`).
        let root = root.children(self.takeover_layer(window, cx));
        let root = match image_cache {
            Some(cache) => super::image_budget::CacheScope::new(cache, root).into_any_element(),
            None => root.into_any_element(),
        };
        self.frame
            .context_menu_items
            .apply_pending_focus(window, cx);
        // Back to the scope this draw interrupted; the layout and paint of
        // the root run as the main window's again.
        self.set_window_scope(outer_scope);
        super::chat_window::WindowScope::new(cx.weak_entity(), None, root)
    }
}

mod frame_upkeep;
mod status_toast;
#[allow(unused_imports)]
use status_toast::*;
