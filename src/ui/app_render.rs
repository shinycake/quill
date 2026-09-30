//! impl Render for QuillApp (root view composition).

use super::actions::{
    CancelSearch, ChatSearchNewer, ChatSearchOlder, CloseWindow, FocusComposer, FocusSidebar,
    FormatBold, FormatItalic, FormatUnderline, LoadOlder, MinimizeWindow, OpenChatSearch,
    OpenGiftPurchase, OpenHelp, OpenSearch, QuitApp, SubmitCode, SubmitPassword, SubmitPhone,
    ToggleFullscreen, ToggleTheme, ViewerNext, ViewerPrev, ViewerZoomIn, ViewerZoomOut,
    ViewerZoomReset, ZoomWindow,
};
use super::app::QuillApp;
use super::shell::title_bar;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::auth::{AuthAction, view_for};
use quill::composer::FormatAction;
use quill::ids::ChatId;
use quill::settings::ThemeChoice;
use quill::state::{ConnectionIndicator, StoryPostOutcome, connection_indicator};
impl Render for QuillApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Phase 8.1: feed OS window focus into the notification decision, then
        // dispatch any notifications the reducer queued since the last frame.
        if let Some(live) = self.live.as_mut() {
            live.driver.session.app_active = window.is_window_active();
        }
        self.flush_notifications(window, cx);
        // kit Phase 2 (redo): hand-rolled toast pill replaced by kit
        // notifications; new status notes are consumed here.
        self.push_status_note(window, cx);
        // Slice P1 fix-up: the checkout dialog opens on Buy press before
        // the form arrives — prefill the saved order info once, on the
        // first frame after the form answer lands. (This can't live in
        // `poll_live`: prefill needs a `&mut Window` for the inputs.)
        let payment_form = self.session().and_then(|s| s.payment_form.clone());
        if let (Some(dialog), Some(form)) = (self.payment_dialog.as_mut(), payment_form)
            && !dialog.prefilled
        {
            dialog.prefill_from_form(&form, window, cx);
        }
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
        // MED4b: debounced `getLinkPreview` prefetch for the
        // detected-URL chip (spawns at most one timer per new URL).
        self.maybe_prefetch_link_preview(cx);
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
        // Phase 9.3: the composer sent `canPostStory` — once the answer
        // lands, either post (eligible) or surface the reason in the
        // composer. Eligibility is re-checked on every Post press.
        if self.story_composer.check_sent {
            let answered = self.session().is_some_and(|session| {
                session.story_post.eligibility.is_some() || session.story_post.check_error.is_some()
            });
            if answered {
                self.story_composer.check_sent = false;
                self.story_composer_after_check(cx);
            }
        }
        // Phase 9.3: `postStory` was sent (`post_sent`) — once its
        // answer moves `story_post.outcome` out of `None` the outcome
        // owns the busy state again and the flag clears.
        if self.story_composer.post_sent
            && self.session().is_some_and(|session| {
                !matches!(session.story_post.outcome, StoryPostOutcome::None)
            })
        {
            self.story_composer.post_sent = false;
        }
        // Phase 9.5: `editStory` was sent (`save_sent`) — once
        // `story_manage.pending` clears, success closes the composer
        // (the edited story arrives via `updateStory`); failure
        // surfaces `story_manage.error` in the composer.
        if self.story_composer.save_sent
            && self
                .session()
                .is_some_and(|session| !session.story_manage.pending)
        {
            self.story_composer.save_sent = false;
            let failed = self.session().and_then(|s| s.story_manage.error.clone());
            match failed {
                Some(error) => self.story_composer.local_error = Some(error),
                None => self.close_story_composer(cx),
            }
        }
        // Phase 9.5: the viewer cover editor / privacy editor sent a
        // management call — once `story_manage.pending` clears, close
        // the panel on success or leave it open showing the error.
        if self.story_cover_sent && self.session().is_some_and(|s| !s.story_manage.pending) {
            self.story_cover_sent = false;
            if self
                .session()
                .is_some_and(|s| s.story_manage.error.is_none())
            {
                self.story_cover_target = None;
            }
        }
        if self.story_privacy_sent && self.session().is_some_and(|s| !s.story_manage.pending) {
            self.story_privacy_sent = false;
            if self
                .session()
                .is_some_and(|s| s.story_manage.error.is_none())
            {
                self.story_privacy_edit = None;
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
        let show_qr = inputs_live && matches!(auth.action, AuthAction::WaitOtherDevice);
        // Slice parity:platform-offline-indicator — re-read every frame
        // (the 40ms `poll_live` loop applies `updateConnectionState` and
        // re-renders), so the indicator follows TDLib live.
        let connection = self
            .session()
            .map(|session| session.connection)
            .and_then(connection_indicator);
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
            // kit Phase 7: window-chrome actions behind the File / Window /
            // View / Help menus (same dispatch path as the key bindings).
            .on_action(cx.listener(|this, _: &CloseWindow, window, cx| {
                let _ = this;
                window.remove_window();
                // macOS keeps a windowless app alive for its menu bar;
                // elsewhere closing the only window quits.
                #[cfg(not(target_os = "macos"))]
                cx.quit();
            }))
            .on_action(cx.listener(|this, _: &MinimizeWindow, window, _| {
                let _ = this;
                window.minimize_window();
            }))
            .on_action(cx.listener(|this, _: &ZoomWindow, window, _| {
                let _ = this;
                window.zoom_window();
            }))
            .on_action(cx.listener(|this, _: &ToggleFullscreen, window, _| {
                let _ = this;
                window.toggle_fullscreen();
            }))
            .on_action(cx.listener(|this, _: &ToggleTheme, _, cx| {
                // Write through the appearance funnel (persist + re-apply)
                // so the 60s auto-night tick can't silently revert the
                // flip. Auto-night, when enabled, still overrides the
                // manual choice while active — same as the dialog.
                let next = if this.appearance.theme == ThemeChoice::Dark {
                    ThemeChoice::Light
                } else {
                    ThemeChoice::Dark
                };
                this.set_appearance(cx, |a| a.theme = next);
            }))
            .on_action(cx.listener(|this, _: &OpenHelp, _, cx| {
                let _ = this;
                cx.open_url("https://github.com/shinycake/quill");
            }))
            // Slice parity:gifts-signed-comment: open the "Buy collectible
            // gift" dialog (`sendResoldGift` with the personal comment).
            .on_action(cx.listener(|this, _: &OpenGiftPurchase, window, cx| {
                this.open_gift_purchase_dialog(window, cx);
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
            // M1: formatting shortcuts only apply when the composer has
            // focus (otherwise the keystroke belongs to whatever is
            // focused).
            .on_action(cx.listener(|this, _: &FormatBold, window, cx| {
                if this.composer.read(cx).focus_handle(cx).is_focused(window) {
                    this.apply_composer_format(FormatAction::Bold, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FormatItalic, window, cx| {
                if this.composer.read(cx).focus_handle(cx).is_focused(window) {
                    this.apply_composer_format(FormatAction::Italic, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FormatUnderline, window, cx| {
                if this.composer.read(cx).focus_handle(cx).is_focused(window) {
                    this.apply_composer_format(FormatAction::Underline, window, cx);
                }
            }))
            // kit Phase 7: in-window menu bar on Linux/Windows (macOS uses
            // the native menu bar installed by `setup_app_menus`).
            .when(cfg!(not(target_os = "macos")), |this| {
                this.child(
                    div()
                        .h(px(30.))
                        .w_full()
                        .flex_none()
                        .bg(cx.theme().title_bar)
                        .border_b_1()
                        .border_color(cx.theme().title_bar_border)
                        .child(self.menu_bar.clone()),
                )
            })
            .child(title_bar(
                self.pane_mode(),
                self.live.is_some(),
                self.search_is_open(),
                self.chat_search_is_open(),
                cx,
            ))
            // Slice parity:platform-offline-indicator — slim connection
            // strip below the title bar. Offline gets the kit warning
            // banner with the "Waiting for network…" label; transitional
            // states get a presence dot only (per-state labels are the
            // `platform-reconnect-states` slice).
            .when(connection == Some(ConnectionIndicator::Offline), |this| {
                this.child(Alert::warning("connection-indicator", "Waiting for network…").banner())
            })
            .when(
                connection == Some(ConnectionIndicator::Transitioning),
                |this| {
                    this.child(
                        div()
                            .w_full()
                            .flex_none()
                            .flex()
                            .justify_center()
                            .py(px(4.))
                            .child(div().size(px(8.)).rounded_full().bg(cx.theme().warning)),
                    )
                },
            )
            .child(
                div()
                    .id("quill-shell")
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(&auth, show_phone, show_code, show_password, show_qr, cx))
                    .child(self.conversation(cx))
                    // Phase 6: user / group info panel beside the conversation.
                    .when_some(self.info_panel(cx), |this, panel| this.child(panel))
                    // MED3: downloads manager panel beside the conversation.
                    .when_some(self.downloads_panel(cx), |this, panel| this.child(panel))
                    // Slice media-shared-gallery: shared-media gallery panel.
                    .when_some(self.shared_media_panel(cx), |this, panel| this.child(panel)),
            )
            .when(self.media_viewer.is_open(), |this| {
                this.child(self.media_viewer_overlay(window, cx))
            })
            // Phase 9.1: story viewer overlay above the media viewer.
            .when(self.story_viewer.is_open(), |this| {
                this.child(self.story_viewer_overlay(cx))
            })
            // Phase 9.3: story composer overlay above the story viewer.
            .when(self.story_composer.open, |this| {
                this.child(self.story_composer_overlay(cx))
            })
            // Phase 9.7: chat story page overlay (albums / chat page /
            // archive) above the story composer.
            .when(self.story_page.is_some(), |this| {
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
            // kit Phase 2 (redo): notification defaults now hosted in a kit
            // Dialog via the shell sync — render wiring deleted.
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
            // Slice S3: privacy overlay (Settings → Privacy) plus the
            // per-rule editor and the always/never exception list.
            .when(self.privacy_open, |this| {
                this.child(self.privacy_overlay(cx))
            })
            .when_some(self.privacy_editor_overlay(cx), |this, overlay| {
                this.child(overlay)
            })
            .when_some(self.privacy_exceptions_overlay(cx), |this, overlay| {
                this.child(overlay)
            })
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
            .when_some(self.message_menu, |this, menu| {
                this.child(self.message_menu_overlay(menu, cx))
            })
            // Slice CL1: right-click chat-row context menu.
            .when_some(self.chat_menu, |this, menu| {
                this.child(self.chat_menu_overlay(menu, cx))
            })
            // Slice CL: floating peek preview — read-only recent
            // messages beside the pressed chat-list row. Rendered above
            // the row menu; any click or the long-press release closes
            // it.
            .when_some(self.chat_preview, |this, preview| {
                this.child(self.chat_preview_overlay(preview, cx))
            })
            // MED4: Instant View reader overlay (above the menu).
            .when_some(self.instant_view_overlay(cx), |this, overlay| {
                this.child(overlay)
            })
    }
}
