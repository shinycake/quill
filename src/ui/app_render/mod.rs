//! impl Render for QuillApp (root view composition).
#![deny(clippy::indexing_slicing)]

use super::actions::{
    CancelSearch, CloseWindow, FirstChat, FocusSidebar, LastChat, LockApp, MarkChatRead,
    MinimizeWindow, NextChat, NextFolder, OpenArchive, OpenContacts, OpenHelp, OpenPinnedChat,
    OpenSavedMessages, OpenSearch, OpenSettings, OpenShortcuts, PrevChat, PrevFolder, QuitApp,
    ShowChatMenu, ShowChatPreview, StoryTogglePause, SubmitCode, SubmitPassword, SubmitPhone,
    ToggleFullscreen, ToggleSidebar, ToggleTheme, ZoomWindow,
};
use super::app::QuillApp;
use super::command_shortcuts::CommandActions;
use super::shell::title_bar;
use super::window_actions::WindowActions;
use crate::ui::dismiss_layer::DismissLayer;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::agent::ui_driver::guard::{self, HardSurface};
use quill::auth::{AuthAction, view_for};
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::state::{ConnectionIndicator, connection_indicator};
impl QuillApp {
    /// What each frame of the main window starts with, whether it renders
    /// the app or only redraws the animation layers. The first call of a
    /// frame counts: `QuillApp` renders ahead of the layers.
    pub(super) fn main_frame_starts(
        &mut self,
        window: &mut Window,
        draw: super::image_budget::WindowDraw,
        cx: &mut Context<Self>,
    ) {
        let frame = window.frame_number();
        if self.frame.frame_started.replace(frame) == frame {
            return;
        }
        self.sticker_frame_starts(cx);
        // Bounded image memory: retired images no window can show any
        // more leave the atlas, then this frame counts (`image_budget`).
        super::image_budget::begin_main_frame(window, draw, cx);
        self.note_frame_timing(window, cx);
        let display = self.frame.clock_timing.get().map(|timing| timing.display);
        super::sticker_player::note_display(None, display, window, cx);
        super::glyph_sweep::start_frame(window, cx);
        super::spoiler_fx::release_media_tile(Some(window), cx);
        if let Some(cache) = self.slices.image_cache() {
            cache.update(cx, |cache, cx| cache.trim(window, cx));
        }
        self.schedule_idle_image_trim(cx);
    }
}

impl Render for QuillApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The main window draws with its own view state in place; what was
        // there (a chat window's, when this draw interrupts its input) is
        // restored once the root is built, and around its layout and paint
        // (`WindowScope`).
        quill::startup_trace::mark("first_render");
        super::frame_clock::trace_slice_render("app");
        let outer_scope = self.windows.scope;
        self.set_window_scope(None);
        self.main_window.handle = Some(window.window_handle());
        self.release_viewer_key_probe(window, cx);
        self.sync_update_pill();
        self.main_frame_starts(window, super::image_budget::WindowDraw::Whole, cx);
        self.sync_emoji_style(cx);
        let image_cache = self.slices.image_cache();
        self.sync_capture_block(window);
        self.sync_viewer_hdr(window);
        self.sync_window_title(window);
        self.sync_proxy_list_box();
        self.sync_premium_preview_files();
        super::window_frame::sync(window, self.appearance.native_window_frame);
        // Rows the history list painted last frame are what the user saw.
        self.passcode_frame(window, cx);
        self.report_visible_history(
            window.is_window_active() && !self.account.passcode.is_locked(),
            cx,
        );
        self.report_visible_sponsored(
            window.is_window_active() && !self.account.passcode.is_locked(),
        );
        // A main window that drew no history (no chat open, a gated pane)
        // shows none of its widgets.
        if !self.slices.window.conversation_shown() {
            self.widgets_left_window(None);
        }
        let history_drawn = self.slices.window.history_drawn();
        self.players.inline_videos.set_scale(window.scale_factor());
        self.widgets.scale = window.scale_factor();
        self.widgets_frame_start(None, cx);
        // Picture-in-Picture outlives neither its message, the account
        // nor an unlocked app.
        self.check_video_pip(cx);
        self.inline_videos_frame_start(history_drawn, window, cx);
        // Animations run while any Quill window is in front, a chat
        // window's included; the main window's own hold still while the
        // active chat window hides it whole (`chat_window::cover`).
        let main_active = window.is_window_active() || super::frame_clock::assume_active();
        self.note_main_place(window, main_active);
        self.frame.main_active.set(main_active);
        let active = self.follow_windows(cx);
        self.sync_video_avatar(active, window.scale_factor(), cx);
        self.sync_userpic_videos(active, window.scale_factor(), history_drawn);
        self.viewer_follow_window_fullscreen(window);
        self.frame.media_roots_frame.borrow_mut().take();
        let opening = self.viewer.state.is_open().then_some(self.viewer.open_gen);
        let cover = self.frame.viewer_cover.get();
        self.frame
            .viewer_cover
            .set(cover.follow(opening, cx.background_executor().now()));
        // Spoiler specks painted last frame keep drifting.
        if super::spoiler_fx::take_text_painted(window) || super::spoiler_fx::revealing() {
            self.request_animation_tick(30, cx);
        }
        let status_toast = self.status_toast_in(None, cx);
        // The menus' and layers' focus hand-off runs first in
        // `per_window_upkeep`, so a chat window's frame runs its own.
        // The rest is the app's: no window asked for what it opens.
        self.unasked(|this| this.frame_upkeep(window, cx));
        // Locked: the lock screen is all this window builds.
        if self.account.passcode.is_locked() {
            return self.locked_root(outer_scope, cx);
        }
        let auth_state = self.current_auth();
        let auth = view_for(&auth_state);
        let inputs_live =
            (self.live.is_some() || self.auth_demo.demo_inputs) && !self.disk.shows_screen();
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
            // The composer, history, selection, in-chat search and viewer
            // actions, the same list every chat window binds.
            .window_actions(cx.weak_entity(), None)
            .on_action(cx.listener(|this, _: &LockApp, _, cx| this.lock_by_passcode(cx)))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                this.navigate(super::navigation::NavigationAction::Settings, window, cx);
            }))
            .on_action(cx.listener(|this, _: &QuitApp, window, cx| {
                if guard::refuse_hard_if_injecting(HardSurface::Window) {
                    return;
                }
                this.exit_after_exports(super::export_quit::quit_app, window, cx);
            }))
            // kit Phase 7: window-chrome actions behind the File / Window /
            // View / Help menus (same dispatch path as the key bindings).
            .on_action(cx.listener(|this, _: &CloseWindow, window, cx| {
                if guard::refuse_hard_if_injecting(HardSurface::Window) {
                    return;
                }
                if this.close_main_window(window, cx) {
                    window.remove_window();
                }
            }))
            .on_action(cx.listener(|this, _: &MinimizeWindow, window, cx| {
                if guard::refuse_hard_if_injecting(HardSurface::Window) {
                    return;
                }
                if quill::tray::minimize_hides_main_window(
                    this.minimize_to_tray(),
                    quill::tray::tray_available(),
                    cfg!(target_os = "macos"),
                ) {
                    crate::ui::link_inbox::hide_main_window(window, cx);
                } else {
                    window.minimize_window();
                }
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
                this.cycle_theme_by_key(cx);
            }))
            .on_action(cx.listener(|this, _: &OpenHelp, _, cx| {
                this.open_link("https://github.com/shinycake/quill", cx);
            }))
            .on_action(cx.listener(|this, _: &OpenShortcuts, _, cx| {
                this.open_settings(Some(quill::settings_catalog::Page::Shortcuts), cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NextChat, window, cx| {
                this.step_open_chat(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PrevChat, window, cx| {
                this.step_open_chat(-1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusSidebar, window, cx| {
                window.focus(&this.focus_sidebar, cx);
            }))
            // Shortcut pack (see `shortcut_pack.rs`): a handler that has no
            // use for the key propagates it to the focused input. The chat
            // list's keys are the main window's.
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
            .on_action(cx.listener(|this, _: &OpenSearch, window, cx| {
                this.open_search_ui(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CancelSearch, window, cx| {
                this.cancel_search(window, cx);
            }))
            // The story viewer is the main window's. Only while it is open:
            // otherwise the key reaches the focused input.
            .on_action(cx.listener(|this, _: &StoryTogglePause, window, cx| {
                if this.stories.viewer.is_open() && !this.story_text_input_open(window, cx) {
                    this.toggle_story_pause(cx);
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
            .children(self.disk_low_banner(cx))
            .children(self.live_share_strip(cx))
            .children(self.unconfirmed_login_banner(cx))
            .when(
                matches!(
                    self.preferences.update_state,
                    quill::updater::UpdateState::Installed(_)
                ) && !self.preferences.update_banner_dismissed,
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
                            // tdesktop's connecting widget spins
                            // (`Window::ConnectionState`).
                            .child(
                                gpui_kit::component::spinner::Spinner::new()
                                    .xsmall()
                                    .color(cx.theme().muted_foreground),
                            )
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
                self.pane_mode() == super::app::PaneMode::Connecting && !self.disk.shows_screen(),
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
                    let panel = self.selector_panel(panel);
                    this.child(
                        DismissLayer::new(
                            "reaction-selector-layer",
                            cx.listener(|this, _, _, cx| {
                                this.close_media_panel(cx);
                            }),
                        )
                        .anchored(position, panel)
                        .decorations(self.panel_painter()),
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
            .when_some(self.admin.community.flow.row_menu, |this, menu| {
                this.child(self.community_row_menu_overlay(menu, cx))
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
            .when_some(self.link_tooltip_overlay(cx), |this, overlay| {
                this.child(overlay)
            });
        // Middle-click autoscroll: the anchor mark, and any other press
        // ends the mode (`ListWidget::mousePressEvent`).
        let root = self.autoscroll_layer(cx).on(root, cx.weak_entity());
        let root = root.children(self.disk_full_cover(&auth, cx));
        // An agent driving the window: the glow, the ghost cursor and the
        // pill, deferred above everything (`agent_takeover`).
        let root = root
            .children(self.takeover_layer(window, cx))
            .children(self.slices.app_only());
        let root = match image_cache {
            Some(cache) => super::image_budget::CacheScope::new(cache, root).into_any_element(),
            None => root.into_any_element(),
        };
        self.frame.menu.items.apply_pending_focus(window, cx);
        // Back to the scope this draw interrupted; the layout and paint of
        // the root run as the main window's again.
        self.set_window_scope(outer_scope);
        super::chat_window::WindowScope::new(cx.weak_entity(), None, root)
    }
}

mod frame_upkeep;
mod locked_root;
mod status_toast;
mod window_upkeep;
#[cfg(test)]
pub(in crate::ui) use status_toast::pin_toasts_for_tests;
#[allow(unused_imports)]
use status_toast::*;
