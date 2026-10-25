//! MED1: viewer seek, keyboard, full screen, speed and volume, media
//! preferences, the media-session tick, and zoom/pan.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with};
use std::time::Instant;

impl QuillApp {
    /// MED1: `SliderEvent` sink for the viewer video seek slider. Drag
    /// previews the position; release seeks the clock and restarts the sound
    /// at the new offset (the history-row `on_seek_event` pattern).
    pub(in crate::ui) fn on_viewer_seek_event(
        &mut self,
        event: &SliderEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SliderEvent::Change(value) => {
                self.viewer.controls.seek_preview = Some(f64::from(value.end()));
                self.notify_viewer_panel(cx);
            }
            SliderEvent::Release(value) => {
                self.viewer.controls.seek_preview = None;
                self.seek_viewer_to(f64::from(value.end()), cx);
            }
        }
    }

    /// MED1: apply a finished viewer seek (paused or playing; the player
    /// keeps its sound in step).
    pub(in crate::ui) fn seek_viewer_to(&mut self, secs: f64, cx: &mut Context<Self>) {
        let Some(player) = &self.viewer.player else {
            return;
        };
        player.seek(secs);
        cx.notify();
    }

    /// `J` / `L` and the full-screen arrows: seek `delta_secs` from the
    /// playhead, inside the clip.
    pub(in crate::ui) fn seek_viewer_by(&mut self, delta_secs: f64, cx: &mut Context<Self>) {
        let Some(transport) = self.viewer_transport() else {
            return;
        };
        let target = seek_target_secs(transport.position, transport.total(), delta_secs);
        self.seek_viewer_to(target, cx);
    }

    /// Full-screen digits: jump to `fraction` of the clip.
    fn seek_viewer_to_fraction(&mut self, fraction: f64, cx: &mut Context<Self>) {
        if let Some(transport) = self.viewer_transport() {
            let target = transport.total() * fraction.clamp(0.0, 1.0);
            self.seek_viewer_to(target, cx);
        }
    }

    /// A viewer playback key (see `quill::media_viewer::viewer_key_action`).
    /// `true` when the key was consumed.
    pub(in crate::ui) fn handle_viewer_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(kind) = self.viewer.state.current().map(|item| item.kind) else {
            return false;
        };
        let modifiers = &keystroke.modifiers;
        let mods = ViewerKeyMods {
            primary: if cfg!(target_os = "macos") {
                modifiers.platform
            } else {
                modifiers.control
            },
            alt: modifiers.alt,
            shift: modifiers.shift,
        };
        let paused = self
            .viewer_transport()
            .is_some_and(|transport| !transport.is_playing());
        let Some(action) = viewer_key_action(
            &keystroke.key,
            mods,
            kind,
            self.viewer.extra.video_fullscreen.is_on(),
            paused,
        ) else {
            return false;
        };
        self.viewer_note_activity(cx);
        match action {
            ViewerKeyAction::TogglePlayback => self.toggle_viewer_video(cx),
            ViewerKeyAction::HoldSpace => self.viewer_space_down(window, cx),
            ViewerKeyAction::StepFrame(forward) => {
                if let Some(player) = &self.viewer.player {
                    player.step_frame(forward);
                }
                cx.notify();
            }
            ViewerKeyAction::SeekBy(secs) => self.seek_viewer_by(secs, cx),
            ViewerKeyAction::SeekToFraction(fraction) => self.seek_viewer_to_fraction(fraction, cx),
            ViewerKeyAction::ToggleFullscreen => self.viewer_toggle_fullscreen(window, cx),
            ViewerKeyAction::ResetZoom => self.viewer_toggle_natural_zoom(cx),
        }
        true
    }

    /// Video full screen (tdesktop `playbackToggleFullScreen`): the window
    /// goes full screen, the arrows seek instead of paging, Escape leaves.
    pub(in crate::ui) fn viewer_toggle_fullscreen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.viewer.extra.video_fullscreen.is_on() {
            self.viewer_leave_video_fullscreen();
        } else {
            let (mode, toggle) = self
                .viewer
                .extra
                .video_fullscreen
                .enter(window.is_fullscreen());
            if toggle {
                window.toggle_fullscreen();
            }
            self.viewer.extra.video_fullscreen = mode;
        }
        cx.notify();
    }

    /// Leave video full screen; the window returns to windowed on the next
    /// frame unless it was full screen before.
    pub(in crate::ui) fn viewer_leave_video_fullscreen(&mut self) {
        self.viewer.extra.video_fullscreen =
            self.viewer.extra.video_fullscreen.leave(Instant::now());
    }

    /// Keep video full screen and the window in step, once a frame: give
    /// the window back after the mode ended, and end the mode when the
    /// window left full screen some other way.
    pub(in crate::ui) fn viewer_follow_window_fullscreen(&mut self, window: &mut Window) {
        let (mode, toggle) = self
            .viewer
            .extra
            .video_fullscreen
            .follow_window(window.is_fullscreen(), Instant::now());
        self.viewer.extra.video_fullscreen = mode;
        if toggle {
            window.toggle_fullscreen();
        }
    }

    /// Looping clips play only while the window is active (tdesktop pauses
    /// them in the background): pause on deactivation, resume on return.
    pub(super) fn sync_viewer_window_activity(&mut self, active: bool, cx: &mut Context<Self>) {
        if !self.viewer_loops() || self.viewer.player.is_none() {
            return;
        }
        let playing = self.viewer_video_playing();
        if !active && playing {
            self.viewer.extra.inactive_paused = true;
            self.pause_viewer_video(cx);
        } else if active && self.viewer.extra.inactive_paused {
            self.viewer.extra.inactive_paused = false;
            self.resume_viewer_video(cx);
        }
    }

    /// Open the current clip with the system player (the fallback when
    /// in-viewer playback fails, e.g. ffmpeg is missing).
    pub(in crate::ui) fn open_viewer_clip_externally(&mut self, cx: &mut Context<Self>) {
        let path = self
            .viewer
            .state
            .current()
            .and_then(|item| self.viewer_clip_path(item));
        match path {
            Some(path) => self.open_file_guarded(path, cx),
            None => {
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_AV_DOWNLOAD_THE_MEDIA_FIRST_TO_2,
                )));
                cx.notify();
            }
        }
    }

    /// MED1: push the viewer clock into the seek slider so the thumb
    /// follows elapsed time. Called from the overlay render (the tick has
    /// no `&mut Window`, which `SliderState::set_value` needs). Skipped
    /// while scrubbing so the drag is never fought.
    pub(in crate::ui) fn sync_viewer_seek_slider(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.viewer.controls.seek_preview.is_some() {
            return;
        }
        let Some(transport) = self.viewer_transport() else {
            return;
        };
        // The length became known (or differs from the message's): the
        // slider's range has no setter, so it is made again.
        let total = transport.total();
        let stale = self.viewer.controls.seek.as_ref().is_some_and(|slider| {
            crate::ui::video_player::controls::seek_range_stale(slider.read(cx).max_value(), total)
        });
        if stale {
            self.create_viewer_seek_slider(total, cx);
        }
        if let Some(slider) = self.viewer.controls.seek.as_ref() {
            let value = transport.position.clamp(0.0, total.max(0.0)) as f32;
            let changed = slider.read(cx).value() != SliderValue::Single(value);
            if changed {
                slider.update(cx, |state, cx| {
                    state.set_value(value, window, cx);
                });
            }
        }
    }

    /// MED1: `SliderEvent` sink for the viewer volume slider. The volume
    /// applies on release so a drag doesn't restart the sound per tick.
    pub(in crate::ui) fn on_viewer_volume_event(
        &mut self,
        event: &SliderEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SliderEvent::Change(_) => {
                self.viewer.controls.volume_dragging = true;
                self.notify_viewer_panel(cx);
            }
            SliderEvent::Release(value) => {
                self.viewer.controls.volume_dragging = false;
                self.set_playback_volume(f64::from(value.end()) as f32 / 100.0, cx);
            }
        }
    }

    /// MED1: push `playback_volume` into the volume slider (e.g. after
    /// the mute toggle moved it). Skipped while dragging.
    pub(in crate::ui) fn sync_viewer_volume_slider(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.viewer.controls.volume_dragging {
            return;
        }
        if let Some(slider) = self.viewer.controls.volume.as_ref() {
            let value = (self.playback.volume * 100.0).round();
            let changed = slider.read(cx).value() != SliderValue::Single(value);
            if changed {
                slider.update(cx, |state, cx| {
                    state.set_value(value, window, cx);
                });
            }
        }
    }

    /// MED1: cycle playback speed through the TGX `PlaybackSpeedLayout`
    /// set (0.5x, 0.7x, 1x, 1.2x, 1.5x, 2x). Applies to the active
    /// voice/audio track and the viewer video: the clock rate moves the
    /// playhead and the sound restarts at the new tempo so audio stays in
    /// sync.
    pub(in crate::ui) fn cycle_playback_speed(&mut self, cx: &mut Context<Self>) {
        const SPEEDS: [f64; 6] = [0.5, 0.7, 1.0, 1.2, 1.5, 2.0];
        let next = SPEEDS
            .iter()
            .position(|s| (*s - self.playback.speed).abs() < 0.01)
            .and_then(|i| SPEEDS.get((i + 1) % SPEEDS.len()).copied())
            .unwrap_or(1.0);
        self.set_playback_speed(next, cx);
    }

    /// Apply a playback speed from the speed dial (slider or preset) to the
    /// active track and the viewer video.
    pub(in crate::ui) fn set_playback_speed(&mut self, speed: f64, cx: &mut Context<Self>) {
        let next = quill::viewer_extras::clamp_speed(speed);
        self.playback.speed = next;
        self.viewer.controls.speed_preview = None;
        // A chosen speed ends a Space hold (the rate is set below).
        self.viewer.controls.boost_timer = None;
        let _ = self.viewer.controls.boost.cancel();
        let mut restarted = false;
        if self.playback.track_duration().is_some() {
            let was_playing = self.playback.is_playing();
            // The tempo stretcher follows the new speed on the fly.
            self.playback.audio.set_speed(next);
            restarted = was_playing;
        }
        if let Some(player) = self.viewer.player.as_ref().filter(|_| !self.viewer_loops()) {
            player.set_rate(next);
            restarted = true;
        }
        if !restarted {
            // Nothing playing: the speed applies to the next play.
            self.connection.set_note(StatusNote::silent(tr_with(
                keys::Q_AV_PLAYBACK_SPEED,
                &[("self_speed_label", &(Self::speed_label(next)).to_string())],
            )));
        }
        cx.notify();
    }

    /// The speed slider of the speed dial, created on first use.
    fn ensure_speed_slider(&mut self, cx: &mut Context<Self>) -> Entity<SliderState> {
        if let Some(slider) = self.viewer.controls.speed.clone() {
            return slider;
        }
        let slider = cx.new(|_| {
            SliderState::new()
                .min(quill::viewer_extras::SPEED_MIN as f32)
                .max(quill::viewer_extras::SPEED_MAX as f32)
                .step(0.1)
                .default_value(self.playback.speed as f32)
        });
        // The slider is this viewer's: its events act on this window's.
        let scope = self.windows.scope;
        cx.subscribe(&slider, move |this, _, event, cx| {
            this.in_window_scope(scope, cx, |this, cx| match event {
                SliderEvent::Change(value) => {
                    this.viewer.controls.speed_preview =
                        Some(quill::viewer_extras::snap_speed(f64::from(value.end())));
                    cx.notify();
                }
                SliderEvent::Release(value) => {
                    let speed = quill::viewer_extras::snap_speed(f64::from(value.end()));
                    this.set_playback_speed(speed, cx);
                }
            });
        })
        .detach();
        self.viewer.controls.speed = Some(slider.clone());
        slider
    }

    /// Move the speed dial's thumb to the current speed.
    fn sync_speed_slider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let speed = self.playback.speed as f32;
        if let Some(slider) = self.viewer.controls.speed.as_ref() {
            slider.update(cx, |state, cx| state.set_value(speed, window, cx));
        }
    }

    /// Telegram Desktop's speed dial: `trigger` opens a slider from 0.5x
    /// to 2.5x (sticking to the usual speeds) above the named presets
    /// (`video_player::controls::SpeedMenu`). Shared by the viewer's
    /// controller and the audio player bar.
    pub(in crate::ui) fn speed_dial(
        &mut self,
        id: &'static str,
        trigger: Button,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::ui::video_player::controls::SpeedMenu;
        let slider = self.ensure_speed_slider(cx);
        let dragging = self.viewer.controls.speed_preview.is_some();
        let current = self.playback.speed;
        let shown = self.viewer.controls.speed_preview.unwrap_or(current);
        let view = cx.entity().downgrade();
        Popover::new((id, 1usize))
            .anchor(Anchor::BottomRight)
            .default_open(self.viewer.controls.demo_speed_open)
            .trigger(trigger)
            .content(move |_state, window, cx| {
                // Another control (a row's speed link) may have moved the
                // speed since the thumb was last set.
                if !dragging {
                    let want = current as f32;
                    if slider.read(cx).value() != SliderValue::Single(want) {
                        slider.update(cx, |state, cx| state.set_value(want, window, cx));
                    }
                }
                let view = view.clone();
                let popover = cx.entity();
                SpeedMenu::new(
                    id,
                    slider.clone(),
                    shown,
                    std::rc::Rc::new(move |speed, window, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.set_playback_speed(speed, cx);
                            this.sync_speed_slider(window, cx);
                        });
                        popover.update(cx, |state, cx| state.dismiss(window, cx));
                    }),
                )
            })
            .into_any_element()
    }

    /// The player bar's speed button: the speed as text.
    pub(in crate::ui) fn speed_label_trigger(&self, id: &'static str) -> Button {
        let shown = self
            .viewer
            .controls
            .speed_preview
            .unwrap_or(self.playback.speed);
        Button::new((id, 0usize))
            .label(Self::speed_label(shown))
            .ghost()
            .text_color(gpui_kit::white())
            .tooltip(tr(keys::Q_AV_PLAYBACK_SPEED_2))
            .accessibility_label(tr(keys::Q_AV_PLAYBACK_SPEED_2))
    }

    /// MED1: short "1.5×" style label for the speed button.
    pub(in crate::ui) fn speed_label(speed: f64) -> String {
        quill::viewer_extras::speed_label(speed)
    }

    /// MED1: mute toggle — 0 volume remembers the previous level and
    /// restores it on unmute; the sound restarts at the new volume.
    pub(in crate::ui) fn toggle_playback_mute(&mut self, cx: &mut Context<Self>) {
        if self.playback.volume > 0.01 {
            self.playback.unmuted_volume = self.playback.volume;
            self.set_playback_volume(0.0, cx);
        } else {
            let restore = self.playback.unmuted_volume.max(0.01);
            self.set_playback_volume(restore, cx);
        }
    }

    /// MED1: set playback volume 0.0–1.0 and restart any active player so
    /// the sound picks up the new volume.
    pub(in crate::ui) fn set_playback_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.playback.volume = volume.clamp(0.0, 1.0);
        self.playback.audio.set_volume(self.playback.volume);
        if let Some(live) = self.live.as_mut() {
            live.driver.set_stream_volume(self.playback.volume);
        }
        if let Some(player) = self.viewer.player.as_ref().filter(|_| !self.viewer_loops()) {
            player.set_volume(self.playback.volume);
        }
        cx.notify();
    }

    /// MED1: update one media pref in the session and persist it
    /// (the `set_call_pref` pattern).
    pub(in crate::ui) fn set_media_pref(
        &mut self,
        update: impl FnOnce(&mut MediaPrefs),
        cx: &mut Context<Self>,
    ) {
        let mut prefs = self
            .session()
            .map(|session| session.settings.media_prefs.clone())
            .unwrap_or_default();
        update(&mut prefs);
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.media_prefs = prefs;
            if let Err(err) = live.driver.save_media_prefs() {
                self.connection.set_note(StatusNote::error(tr_with(
                    keys::Q_AV_COULDNT_SAVE_MEDIA_SETTINGS,
                    &[("err", &err.to_string())],
                )));
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.media_prefs = prefs;
        }
        cx.notify();
    }

    /// MED1: effective composer grouping — the user's toggle when set,
    /// else the remembered pref (grouped when remembering is off).
    pub(in crate::ui) fn composer_group_media_effective(&self) -> bool {
        self.composer_ui.group_media.unwrap_or_else(|| {
            self.session()
                .map(|s| s.settings.media_prefs.default_grouping())
                .unwrap_or(true)
        })
    }

    /// MED1: flip the composer "group media" choice for 2+ attachments;
    /// persisted when "remember grouping" is on (TGX `RememberAlbumSetting`).
    pub(in crate::ui) fn toggle_composer_group_media(&mut self, cx: &mut Context<Self>) {
        let next = !self.composer_group_media_effective();
        self.composer_ui.group_media = Some(next);
        if self
            .session()
            .is_some_and(|s| s.settings.media_prefs.remember_media_grouping)
        {
            self.set_media_pref(|prefs| prefs.group_media = next, cx);
        } else {
            cx.notify();
        }
    }

    /// MED1: flip the "remember media grouping" setting itself.
    pub(in crate::ui) fn toggle_remember_media_grouping(&mut self, cx: &mut Context<Self>) {
        let next = !self
            .session()
            .map(|s| s.settings.media_prefs.remember_media_grouping)
            .unwrap_or(false);
        // Turning it on snapshots the current grouping choice.
        let current = self.composer_group_media_effective();
        self.set_media_pref(
            |prefs| {
                prefs.remember_media_grouping = next;
                if next {
                    prefs.group_media = current;
                }
            },
            cx,
        );
    }

    /// While a viewer clip is open: media keys and the OS Now Playing
    /// widget, and the player's error once it fails. The picture and the
    /// controls redraw themselves (`video_player`), so this never redraws
    /// the window on its own.
    pub(in crate::ui) fn spawn_viewer_session_tick(&mut self, cx: &mut Context<Self>) {
        if self.viewer.session_tick {
            return;
        }
        self.viewer.session_tick = true;
        self.spawn_scoped(cx, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        if this.viewer.player.is_none() {
                            if this.active_playback_id().is_none() {
                                quill::media_session::publish(None);
                            }
                            return false;
                        }
                        this.drive_viewer_media_session(cx);
                        let failed = this
                            .viewer
                            .player
                            .as_ref()
                            .and_then(|player| player.take_error());
                        if let Some(error) = failed
                            && let Some(item) = this.viewer.state.current()
                        {
                            this.playback.error = Some(video::viewer_error_message(item, &error));
                            cx.notify();
                        }
                        this.viewer.player.is_some()
                    })
                    .unwrap_or_else(|_| {
                        // The clip's window closed: the OS widget must not
                        // keep showing it, unless something else plays.
                        let _ = this.main().update(cx, |this, _| {
                            if this.viewer.player.is_none() && this.active_playback_id().is_none() {
                                quill::media_session::publish(None);
                            }
                        });
                        false
                    });
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.viewer.session_tick = false;
            });
        })
        .detach();
    }

    /// Scroll-zoom the viewer visual around the pointer (`anchor`, px from
    /// the visual's top-left): the point under it stays put and the step
    /// scales with the scroll distance (positive y zooms in).
    pub(in crate::ui) fn viewer_zoom_scroll(
        &mut self,
        delta_y: f32,
        anchor: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        if delta_y == 0.0 {
            return;
        }
        self.viewer
            .zoom
            .zoom_at(wheel_zoom_factor(delta_y), anchor, self.viewer.frame);
        cx.notify();
    }

    /// Parity slice 5: drag-pan the zoomed visual by a mouse delta in px.
    pub(in crate::ui) fn viewer_pan_drag(&mut self, dx: f32, dy: f32, cx: &mut Context<Self>) {
        if !self.viewer.zoom.is_zoomed() {
            return;
        }
        self.viewer.zoom.pan_by(dx, dy, self.viewer.frame);
        cx.notify();
    }

    /// Parity slice 5: step the viewer zoom in or out one notch.
    pub(in crate::ui) fn viewer_zoom_step(&mut self, zoom_in: bool, cx: &mut Context<Self>) {
        self.viewer.zoom.step(zoom_in, self.viewer.frame);
        cx.notify();
    }

    /// `0` and Ctrl+0 (tdesktop `zoomReset`): natural size, or back to fit
    /// from it. Video full screen keeps its zoom, as in tdesktop.
    pub(in crate::ui) fn viewer_toggle_natural_zoom(&mut self, cx: &mut Context<Self>) {
        if self.viewer.extra.video_fullscreen.is_on() {
            return;
        }
        self.viewer.zoom.toggle_natural(self.viewer.frame);
        self.viewer.drag = None;
        cx.notify();
    }

    /// Parity slice 5: reset zoom/pan to fit (double-click, Reset button).
    pub(in crate::ui) fn viewer_reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.viewer.zoom.reset();
        self.viewer.drag = None;
        cx.notify();
    }
}
