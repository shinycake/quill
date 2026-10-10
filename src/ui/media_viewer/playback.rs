//! MED1: viewer seek, keyboard, full screen, speed and volume, media
//! preferences, the viewer tick, and zoom/pan.

use super::*;

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
                self.viewer_seek_scrubbing = true;
                self.viewer_seek_preview_secs = Some(f64::from(value.end()));
                cx.notify();
            }
            SliderEvent::Release(value) => {
                self.viewer_seek_scrubbing = false;
                self.viewer_seek_preview_secs = None;
                self.seek_viewer_to(f64::from(value.end()), cx);
            }
        }
    }

    /// MED1: apply a finished viewer seek. Seeking while paused just moves
    /// the frozen clock; while playing, the sound restarts at the offset.
    pub(in crate::ui) fn seek_viewer_to(&mut self, secs: f64, cx: &mut Context<Self>) {
        let Some(clock) = self.viewer_clock.as_mut() else {
            return;
        };
        clock.seek(secs);
        let offset = clock.elapsed_secs();
        if let Some(video) = self.viewer_native.as_mut() {
            video.seek(offset);
            cx.notify();
            return;
        }
        if clock.is_playing()
            && !self.viewer_loops()
            && let Some(path) = self.viewer_video_path.clone()
        {
            self.start_viewer_audio(&path, offset);
        }
        cx.notify();
    }

    /// `J` / `L` and the full-screen arrows: seek `delta_secs` from the
    /// playhead, inside the clip.
    pub(in crate::ui) fn seek_viewer_by(&mut self, delta_secs: f64, cx: &mut Context<Self>) {
        let Some(clock) = self.viewer_clock.as_ref() else {
            return;
        };
        let position = self
            .viewer_native
            .as_ref()
            .map(|video| video.position_secs())
            .unwrap_or_else(|| clock.elapsed_secs());
        let target = seek_target_secs(position, clock.duration_secs(), delta_secs);
        self.seek_viewer_to(target, cx);
    }

    /// Full-screen digits: jump to `fraction` of the clip.
    fn seek_viewer_to_fraction(&mut self, fraction: f64, cx: &mut Context<Self>) {
        if let Some(clock) = self.viewer_clock.as_ref() {
            let target = clock.duration_secs() * fraction.clamp(0.0, 1.0);
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
        let Some(kind) = self.media_viewer.current().map(|item| item.kind) else {
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
        let Some(action) = viewer_key_action(
            &keystroke.key,
            mods,
            kind,
            self.viewer_extra.video_fullscreen,
        ) else {
            return false;
        };
        self.viewer_note_activity(false, cx);
        match action {
            ViewerKeyAction::TogglePlayback => self.toggle_viewer_video(cx),
            ViewerKeyAction::SeekBy(secs) => self.seek_viewer_by(secs, cx),
            ViewerKeyAction::SeekToFraction(fraction) => self.seek_viewer_to_fraction(fraction, cx),
            ViewerKeyAction::ToggleFullscreen => self.viewer_toggle_fullscreen(window, cx),
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
        if self.viewer_extra.video_fullscreen {
            self.viewer_leave_video_fullscreen();
        } else {
            self.viewer_extra.window_was_fullscreen = window.is_fullscreen();
            if !self.viewer_extra.window_was_fullscreen {
                window.toggle_fullscreen();
            }
            self.viewer_extra.video_fullscreen = true;
        }
        cx.notify();
    }

    /// Leave video full screen; the window returns to windowed on the next
    /// frame unless it was full screen before.
    pub(in crate::ui) fn viewer_leave_video_fullscreen(&mut self) {
        if !self.viewer_extra.video_fullscreen {
            return;
        }
        self.viewer_extra.video_fullscreen = false;
        self.viewer_extra.restore_fullscreen = !self.viewer_extra.window_was_fullscreen;
    }

    /// Looping clips play only while the window is active (tdesktop pauses
    /// them in the background): pause on deactivation, resume on return.
    pub(super) fn sync_viewer_window_activity(&mut self, active: bool, cx: &mut Context<Self>) {
        if !self.viewer_loops() || self.viewer_video.is_none() {
            return;
        }
        let playing = self
            .viewer_clock
            .as_ref()
            .is_some_and(|clock| clock.is_playing());
        if !active && playing {
            self.viewer_extra.inactive_paused = true;
            self.pause_viewer_video(cx);
        } else if active && self.viewer_extra.inactive_paused {
            self.viewer_extra.inactive_paused = false;
            self.resume_viewer_video(cx);
        }
    }

    /// Open the current clip with the system player (the fallback when
    /// in-viewer playback fails, e.g. ffmpeg is missing).
    pub(in crate::ui) fn open_viewer_clip_externally(&mut self, cx: &mut Context<Self>) {
        let path = self
            .media_viewer
            .current()
            .and_then(|item| self.viewer_clip_path(item));
        match path {
            Some(path) => self.open_file_guarded(path, cx),
            None => {
                self.status_note = "download the media first to open it".into();
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
        if self.viewer_seek_scrubbing {
            return;
        }
        if let (Some(slider), Some(clock)) =
            (self.viewer_seek_slider.as_ref(), self.viewer_clock.as_ref())
        {
            let value = clock.elapsed_secs().clamp(0.0, clock.duration_secs()) as f32;
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
                self.viewer_volume_scrubbing = true;
                cx.notify();
            }
            SliderEvent::Release(value) => {
                self.viewer_volume_scrubbing = false;
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
        if self.viewer_volume_scrubbing {
            return;
        }
        if let Some(slider) = self.viewer_volume_slider.as_ref() {
            let value = (self.playback_volume * 100.0).round();
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
            .position(|s| (*s - self.playback_speed).abs() < 0.01)
            .map(|i| SPEEDS[(i + 1) % SPEEDS.len()])
            .unwrap_or(1.0);
        self.set_playback_speed(next, cx);
    }

    /// Apply a playback speed from the speed dial (slider or preset) to the
    /// active track and the viewer video.
    pub(in crate::ui) fn set_playback_speed(&mut self, speed: f64, cx: &mut Context<Self>) {
        let next = quill::viewer_extras::clamp_speed(speed);
        self.playback_speed = next;
        self.viewer_extra.speed_preview = None;
        let mut restarted = false;
        if let Some(clock) = self.playback_clock.as_mut() {
            let was_playing = clock.is_playing();
            clock.set_rate(next);
            // The tempo stretcher follows the new speed on the fly.
            self.audio.set_speed(next);
            restarted = was_playing;
        }
        if let Some(video) = self.viewer_native.as_mut() {
            video.set_rate(next as f32);
            if let Some(clock) = self.viewer_clock.as_mut() {
                clock.set_rate(next);
            }
            restarted = true;
        } else if let Some(clock) = self.viewer_clock.as_mut() {
            let offset = clock.elapsed_secs();
            let was_playing = clock.is_playing();
            clock.set_rate(next);
            if was_playing && let Some(path) = self.viewer_video_path.clone() {
                self.start_viewer_audio(&path, offset);
                restarted = true;
            }
        }
        if !restarted {
            // Nothing playing: the speed applies to the next play.
            self.status_note = format!("playback speed {}×", Self::speed_label(next));
        }
        cx.notify();
    }

    /// The speed slider of the speed dial, created on first use.
    fn ensure_speed_slider(&mut self, cx: &mut Context<Self>) -> Entity<SliderState> {
        if let Some(slider) = self.viewer_extra.speed_slider.clone() {
            return slider;
        }
        let slider = cx.new(|_| {
            SliderState::new()
                .min(quill::viewer_extras::SPEED_MIN as f32)
                .max(quill::viewer_extras::SPEED_MAX as f32)
                .step(0.1)
                .default_value(self.playback_speed as f32)
        });
        cx.subscribe(&slider, |this, _, event, cx| match event {
            SliderEvent::Change(value) => {
                this.viewer_extra.speed_preview =
                    Some(quill::viewer_extras::snap_speed(f64::from(value.end())));
                cx.notify();
            }
            SliderEvent::Release(value) => {
                let speed = quill::viewer_extras::snap_speed(f64::from(value.end()));
                this.set_playback_speed(speed, cx);
            }
        })
        .detach();
        self.viewer_extra.speed_slider = Some(slider.clone());
        slider
    }

    /// Move the speed dial's thumb to the current speed.
    fn sync_speed_slider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let speed = self.playback_speed as f32;
        if let Some(slider) = self.viewer_extra.speed_slider.as_ref() {
            slider.update(cx, |state, cx| state.set_value(speed, window, cx));
        }
    }

    /// Telegram Desktop's speed dial: the current speed on a button that
    /// opens a slider from 0.5x to 2.5x (sticking to the usual speeds)
    /// above the named presets.
    pub(in crate::ui) fn speed_dial(
        &mut self,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use quill::viewer_extras::{SPEED_PRESETS, equal_speeds, speed_label};
        let slider = self.ensure_speed_slider(cx);
        let dragging = self.viewer_extra.speed_preview.is_some();
        let current = self.playback_speed;
        let shown = self.viewer_extra.speed_preview.unwrap_or(current);
        let view = cx.entity().downgrade();
        Popover::new((id, 1usize))
            .anchor(Anchor::BottomRight)
            .default_open(self.viewer_extra.demo_speed_dial_open)
            .trigger(
                Button::new((id, 0usize))
                    .label(speed_label(shown))
                    .ghost()
                    .text_color(gpui_kit::white())
                    .tooltip("Playback speed")
                    .accessibility_label("Playback speed"),
            )
            .content(move |_state, window, cx| {
                // Another control (a row's speed link) may have moved the
                // speed since the thumb was last set.
                if !dragging {
                    let want = current as f32;
                    if slider.read(cx).value() != SliderValue::Single(want) {
                        slider.update(cx, |state, cx| state.set_value(want, window, cx));
                    }
                }
                let rows = SPEED_PRESETS.iter().map(|(speed, name)| {
                    let (speed, name) = (*speed, *name);
                    let chosen = equal_speeds(speed, current);
                    let view = view.clone();
                    let popover = cx.entity();
                    Button::new((id, 100 + (speed * 10.0) as usize))
                        .label(format!("{name}  {}", speed_label(speed)))
                        .ghost()
                        .small()
                        .w_full()
                        .when(chosen, |button| {
                            button.icon(gpui_kit::assets::IconName::Check)
                        })
                        .on_click(move |_, window, cx| {
                            let _ = view.update(cx, |this, cx| {
                                this.set_playback_speed(speed, cx);
                                this.sync_speed_slider(window, cx);
                            });
                            popover.update(cx, |state, cx| state.dismiss(window, cx));
                        })
                });
                div()
                    .w(px(220.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().px_2().py_1().child(Slider::new(&slider)))
                    .children(rows)
            })
            .into_any_element()
    }

    /// MED1: short "1.5×" style label for the speed button.
    pub(in crate::ui) fn speed_label(speed: f64) -> String {
        quill::viewer_extras::speed_label(speed)
    }

    /// MED1: mute toggle — 0 volume remembers the previous level and
    /// restores it on unmute; the sound restarts at the new volume.
    pub(in crate::ui) fn toggle_playback_mute(&mut self, cx: &mut Context<Self>) {
        if self.playback_volume > 0.01 {
            self.playback_unmuted_volume = self.playback_volume;
            self.set_playback_volume(0.0, cx);
        } else {
            let restore = self.playback_unmuted_volume.max(0.01);
            self.set_playback_volume(restore, cx);
        }
    }

    /// MED1: set playback volume 0.0–1.0 and restart any active player so
    /// the sound picks up the new volume.
    pub(in crate::ui) fn set_playback_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.playback_volume = volume.clamp(0.0, 1.0);
        self.audio.set_volume(self.playback_volume);
        if let Some(video) = self.viewer_native.as_mut() {
            video.set_volume(self.playback_volume);
        } else if self.viewer_clock.as_ref().is_some_and(|c| c.is_playing())
            && let Some(path) = self.viewer_video_path.clone()
        {
            let offset = self
                .viewer_clock
                .as_ref()
                .map(|c| c.elapsed_secs())
                .unwrap_or(0.0);
            self.start_viewer_audio(&path, offset);
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
            .map(|session| session.media_prefs.clone())
            .unwrap_or_default();
        update(&mut prefs);
        if let Some(live) = self.live.as_mut() {
            live.driver.session.media_prefs = prefs;
            if let Err(err) = live.driver.save_media_prefs() {
                self.status_note = format!("couldn't save media settings: {err}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.media_prefs = prefs;
        }
        cx.notify();
    }

    /// MED1: effective composer grouping — the user's toggle when set,
    /// else the remembered pref (grouped when remembering is off).
    pub(in crate::ui) fn composer_group_media_effective(&self) -> bool {
        self.composer_group_media.unwrap_or_else(|| {
            self.session()
                .map(|s| s.media_prefs.default_grouping())
                .unwrap_or(true)
        })
    }

    /// MED1: flip the composer "group media" choice for 2+ attachments;
    /// persisted when "remember grouping" is on (TGX `RememberAlbumSetting`).
    pub(in crate::ui) fn toggle_composer_group_media(&mut self, cx: &mut Context<Self>) {
        let next = !self.composer_group_media_effective();
        self.composer_group_media = Some(next);
        if self
            .session()
            .is_some_and(|s| s.media_prefs.remember_media_grouping)
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
            .map(|s| s.media_prefs.remember_media_grouping)
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

    /// 125 ms tick while a viewer clip is active: re-renders so the
    /// elapsed/total label advances and the in-viewer frame animates
    /// (8 fps frames need a sub-250 ms refresh); auto-stops when the clock
    /// reaches the duration (the sound ends on its own).
    pub(in crate::ui) fn spawn_viewer_tick(&mut self, cx: &mut Context<Self>) {
        if self.viewer_tick {
            return;
        }
        self.viewer_tick = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(125))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        let active = this.viewer_video.is_some();
                        if !active {
                            if this.active_playback_id().is_none() {
                                quill::media_session::publish(None);
                            }
                            return false;
                        }
                        this.drive_viewer_media_session(cx);
                        if this.viewer_video.is_none() {
                            return false;
                        }
                        // The native player owns time: the clock mirrors
                        // its position, and its end pauses (the last frame
                        // stays; Play starts over).
                        if let Some(video) = this.viewer_native.as_mut() {
                            let playing = video.is_playing();
                            let position = video.position_secs();
                            if let Some(error) = video.error() {
                                this.playback_error = Some(error);
                            }
                            let looping = this.viewer_loops();
                            if let Some(clock) = this.viewer_clock.as_mut() {
                                clock.seek(position);
                                if !playing && clock.is_playing() {
                                    if looping {
                                        // The clip ended: start over.
                                        clock.seek(0.0);
                                    } else {
                                        clock.pause();
                                    }
                                }
                            }
                            if looping
                                && !playing
                                && this.viewer_clock.as_ref().is_some_and(|c| c.is_playing())
                                && let Some(video) = this.viewer_native.as_mut()
                            {
                                video.seek(0.0);
                                video.play();
                            }
                            cx.notify();
                            return true;
                        }
                        let finished = this
                            .viewer_clock
                            .as_ref()
                            .is_some_and(|clock| clock.is_playing() && clock.finished());
                        if finished && this.viewer_loops() {
                            if let Some(clock) = this.viewer_clock.as_mut() {
                                clock.seek(0.0);
                            }
                            cx.notify();
                            return true;
                        }
                        if finished {
                            this.stop_viewer_video();
                            cx.notify();
                            return false;
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.viewer_tick = false;
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
        self.viewer_zoom
            .zoom_at(wheel_zoom_factor(delta_y), anchor, self.viewer_frame);
        cx.notify();
    }

    /// Parity slice 5: drag-pan the zoomed visual by a mouse delta in px.
    pub(in crate::ui) fn viewer_pan_drag(&mut self, dx: f32, dy: f32, cx: &mut Context<Self>) {
        if !self.viewer_zoom.is_zoomed() {
            return;
        }
        self.viewer_zoom.pan_by(dx, dy, self.viewer_frame);
        cx.notify();
    }

    /// Parity slice 5: step the viewer zoom in or out one notch.
    pub(in crate::ui) fn viewer_zoom_step(&mut self, zoom_in: bool, cx: &mut Context<Self>) {
        self.viewer_zoom.step(zoom_in, self.viewer_frame);
        cx.notify();
    }

    /// Parity slice 5: reset zoom/pan to fit (double-click / `0`).
    pub(in crate::ui) fn viewer_reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.viewer_zoom.reset();
        self.viewer_drag = None;
        cx.notify();
    }
}
