//! The viewer's side of the video controller (`video_player::controls`):
//! building the panel and the states over the picture from `QuillApp`'s
//! state, and the Space-hold speed boost (tdesktop
//! `OverlayWidget::handleKeyPress` / `handleKeyRelease`).
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::play_intent::PlayTarget;
use crate::ui::video_player::controls::{
    ControllerActions, PictureOverlay, PictureState, VideoController, WaitingCheck, speed_trigger,
};
use crate::ui::video_player::{PlayState, Transport};
use quill::media_viewer::{BoostPress, BoostRelease, SPEED_BOOST_HOLD_MS, SPEED_BOOST_RATE};
use std::rc::Rc;

impl QuillApp {
    /// The controller panel for a video whose clip is local; `None` while
    /// it downloads or when it failed (the picture overlay says so).
    pub(super) fn viewer_video_controls(
        &mut self,
        item: &MediaViewerItem,
        row_id: u64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.viewer_clip_path(item)?;
        if self.playback.error.is_some() && self.viewer.player.is_none() {
            return None;
        }
        let transport = self
            .viewer_transport()
            .filter(|_| self.viewer.video == Some(item.message_id))
            .unwrap_or_else(|| Transport {
                state: PlayState::Ready,
                position: 0.0,
                duration: item.duration_secs.map(|secs| f64::from(secs.max(0))),
                buffered: Vec::new(),
                rate: self.playback.speed,
                volume: self.playback.volume,
                has_sound: true,
                live: false,
            });
        if transport.state == PlayState::Failed {
            return None;
        }
        let boosted = self.viewer.controls.boost.is_active();
        let trigger = speed_trigger(
            ("media-viewer-speed", row_id),
            self.playback.speed,
            boosted,
            cx,
        );
        let speed_dial = self.speed_dial("media-viewer-speed", trigger, cx);
        let quality = self.viewer_quality_picker(item, row_id, cx);
        let view = cx.entity().downgrade();
        let handler = |run: fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>)| {
            let view = view.clone();
            Rc::new(move |window: &mut Window, cx: &mut App| {
                let _ = view.update(cx, |this, cx| run(this, window, cx));
            }) as crate::ui::video_player::controls::Handler
        };
        let hover_view = view.clone();
        let actions = ControllerActions {
            toggle_play: handler(|this, _, cx| this.toggle_viewer_video(cx)),
            toggle_mute: handler(|this, _, cx| this.toggle_playback_mute(cx)),
            fullscreen: handler(|this, window, cx| this.viewer_toggle_fullscreen(window, cx)),
            pip: self
                .viewer
                .player
                .is_some()
                .then(|| handler(|this, _, cx| this.open_video_pip(cx))),
            hover: Rc::new(move |at, _, cx| {
                let _ = hover_view.update(cx, |this, cx| {
                    if this.viewer.controls.hover != at {
                        this.viewer.controls.hover = at;
                        this.notify_viewer_panel(cx);
                    }
                });
            }),
        };
        Some(
            VideoController::new(row_id, &transport, &self.viewer.controls, actions)
                .level(self.playback.volume)
                .fullscreen(self.viewer.extra.video_fullscreen.is_on())
                .shown(!self.viewer.controls_hidden)
                .speed_dial(speed_dial)
                .quality(quality)
                .into_any_element(),
        )
    }

    /// What shows over a video's picture: the download ring or disc while
    /// the clip is not local, the "can't be played" plate once it failed,
    /// and (in chat windows, which draw the picture inline) the waiting
    /// spinner. The main window's video layer draws its own spinner.
    pub(super) fn viewer_picture_overlay(
        &mut self,
        item: &MediaViewerItem,
        row_id: u64,
        files: &quill::state::FileMap,
        downloading: &quill::state::DownloadIntents,
        layered: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if item.kind != MediaViewerKind::Video {
            return None;
        }
        let local = self.viewer_clip_path(item).is_some();
        let failed = self
            .viewer
            .player
            .as_ref()
            .is_some_and(|player| player.state() == PlayState::Failed);
        let view = cx.entity().downgrade();
        let state = if let Some(error) = self.playback.error.clone().filter(|_| local) {
            let open_view = view.clone();
            PictureState::Failed {
                detail: error.into(),
                open: Some(Rc::new(move |_, cx| {
                    let _ = open_view.update(cx, |this, cx| this.open_viewer_clip_externally(cx));
                })),
            }
        } else if failed {
            // Failed, the reason still on its way (the session tick).
            let open_view = view.clone();
            PictureState::Failed {
                detail: "couldn't play this video".into(),
                open: Some(Rc::new(move |_, cx| {
                    let _ = open_view.update(cx, |this, cx| this.open_viewer_clip_externally(cx));
                })),
            }
        } else if !local {
            let play_id = item.play_file_id?;
            let message_id = item.message_id;
            if file_is_downloading(play_id, files, downloading) {
                let progress = files
                    .get(&play_id.0)
                    .and_then(|file| file.download_progress());
                PictureState::Downloading {
                    progress,
                    on_cancel: Rc::new(move |_, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.cancel_media_download(play_id, cx);
                        });
                    }),
                }
            } else {
                PictureState::Download {
                    on_start: Rc::new(move |_, cx| {
                        let _ = view.update(cx, |this, cx| {
                            let target = PlayTarget::Viewer {
                                message: message_id,
                            };
                            this.play_when_ready(play_id, target, cx);
                        });
                    }),
                }
            }
        } else if !layered && self.viewer_waiting_shown(cx) {
            PictureState::Waiting
        } else {
            return None;
        };
        Some(PictureOverlay::new(row_id, state).into_any_element())
    }

    /// Chat windows: whether the clip has waited long enough for the
    /// spinner, arming a redraw for when it will have.
    fn viewer_waiting_shown(&mut self, cx: &mut Context<Self>) -> bool {
        let waiting = self.viewer.player.as_ref().is_some_and(|player| {
            matches!(player.state(), PlayState::Opening | PlayState::Buffering)
        });
        let now = cx.background_executor().now();
        match self.viewer.controls.waiting.observe(waiting, now) {
            WaitingCheck::Settled { shown } => shown,
            WaitingCheck::Pending(wait) => {
                let timer = cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(wait).await;
                    let _ = this.update(cx, |_, cx| cx.notify());
                });
                self.viewer.controls.waiting.arm(timer);
                false
            }
        }
    }

    /// Redraw only the view the controller is in (the cached viewer slice,
    /// or a chat window), not every slice.
    pub(in crate::ui) fn notify_viewer_panel(&mut self, cx: &mut Context<Self>) {
        match self.viewer.controls.panel_view {
            Some(view) => {
                let app: &mut App = cx;
                app.notify(view);
            }
            None => cx.notify(),
        }
    }

    /// A zero-size element holding the viewer's key focus, so a Space
    /// key-up reaches the window's listeners when nothing else has focus.
    pub(super) fn viewer_key_focus_probe(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let focus = self
            .viewer
            .controls
            .focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        div()
            .id("media-viewer-key-focus")
            .absolute()
            .size_0()
            .track_focus(&focus)
            .into_any_element()
    }

    /// The window's activation changed: once it is in the background a
    /// Space hold's key-up goes to another window, so the hold ends here,
    /// not at the next render or press.
    pub(in crate::ui) fn viewer_window_activation(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !window.is_window_active() {
            // The viewer of the window that went to the background.
            let scope = self.window_scope_of(window);
            self.in_window_scope(scope, cx, Self::cancel_viewer_boost);
        }
    }

    /// The viewer closed: let go of the key probe (Space focuses it when
    /// nothing else has focus), so the window's keys go back to the app.
    pub(in crate::ui) fn release_viewer_key_probe(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.viewer.state.is_open() {
            return;
        }
        if let Some(focus) = self.viewer.controls.focus.take()
            && focus.is_focused(window)
        {
            window.blur(cx);
        }
    }

    /// Space went down (key repeats included): arm the hold check.
    pub(super) fn viewer_space_down(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // GIFs have no transport to speed up: Space just toggles.
        if self.viewer.player.is_none() || self.viewer_loops() {
            self.toggle_viewer_video(cx);
            return;
        }
        let was_boosted = self.viewer.controls.boost.is_active();
        let press = self
            .viewer
            .controls
            .boost
            .press(cx.background_executor().now());
        if was_boosted && !self.viewer.controls.boost.is_active() {
            // The last hold's release never came.
            self.restore_viewer_rate(cx);
        }
        if press != BoostPress::Armed {
            return;
        }
        if window.focused(cx).is_none()
            && let Some(focus) = self.viewer.controls.focus.clone()
        {
            window.focus(&focus, cx);
        }
        let timer = self.spawn_scoped(cx, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(SPEED_BOOST_HOLD_MS))
                .await;
            let _ = this.update(cx, |this, cx| this.viewer_boost_tick(cx));
        });
        self.viewer.controls.boost_timer = Some(timer);
    }

    /// The hold check: still held after 200 ms plays at 2×.
    fn viewer_boost_tick(&mut self, cx: &mut Context<Self>) {
        self.viewer.controls.boost_timer = None;
        if !self
            .viewer
            .controls
            .boost
            .tick(cx.background_executor().now())
        {
            return;
        }
        if let Some(player) = &self.viewer.player {
            player.set_rate(SPEED_BOOST_RATE);
        }
        cx.notify();
    }

    /// A key went up in the window: Space ends a hold, or toggles pause
    /// when it was a tap. `true` when it was the viewer's Space.
    pub(in crate::ui) fn viewer_key_up(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        if key != "space" || !self.viewer.state.is_open() {
            return false;
        }
        self.viewer.controls.boost_timer = None;
        match self.viewer.controls.boost.release() {
            BoostRelease::TogglePause => {
                self.viewer_note_activity(cx);
                self.toggle_viewer_video(cx);
                true
            }
            BoostRelease::EndBoost => {
                self.restore_viewer_rate(cx);
                true
            }
            BoostRelease::Nothing => false,
        }
    }

    /// Forget a Space press whose key-up will not come (the window went to
    /// the background); a running boost ends.
    pub(in crate::ui) fn cancel_viewer_boost(&mut self, cx: &mut Context<Self>) {
        self.viewer.controls.boost_timer = None;
        if self.viewer.controls.boost.cancel() {
            self.restore_viewer_rate(cx);
        }
    }

    /// Back to the chosen speed after a boost.
    fn restore_viewer_rate(&mut self, cx: &mut Context<Self>) {
        if let Some(player) = self.viewer.player.as_ref().filter(|_| !self.viewer_loops()) {
            player.set_rate(self.playback.speed);
        }
        cx.notify();
    }
}
