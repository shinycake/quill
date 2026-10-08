//! shared audio/voice player.

use super::app::QuillApp;
use super::demo::{demo_file_json, demo_media_allowlist, demo_thumb_png_path};
use super::*;
use gpui_kit::component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::playback::PlaybackClock;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::voice::{self, format_voice_duration};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
pub(super) fn apply_ready_audio(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let cover_path = demo_thumb_png_path();
    let track_path = demo_media_allowlist()
        .join("demo-voice.ogg")
        .to_string_lossy()
        .into_owned();
    let cover = demo_file_json(101, &cover_path, true);
    let track = demo_file_json(102, &track_path, true);
    let pending = demo_file_json(103, "", false);
    let playing = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":801,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageAudio","audio":{{"@type":"audio","duration":214,"title":"Night Drive","performer":"Ada Lovelace","file_name":"night.mp3","mime_type":"audio/mpeg","album_cover_minithumbnail":null,"album_cover_thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":90,"height":90,"file":{cover}}},"external_album_covers":[],"audio":{track}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
    );
    let waiting = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":802,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageAudio","audio":{{"@type":"audio","duration":45,"title":"Untitled","performer":"","file_name":"pending.mp3","mime_type":"audio/mpeg","album_cover_minithumbnail":null,"album_cover_thumbnail":null,"external_album_covers":[],"audio":{pending}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [playing, waiting, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

pub(super) fn apply_ready_voice(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let path = demo_media_allowlist()
        .join("demo-voice.ogg")
        .to_string_lossy()
        .into_owned();
    let wave = voice::waveform_base64(&[4, 16, 28, 12, 8, 20, 6, 18, 10, 24, 8, 14]);
    let incoming_file = demo_file_json(81, &path, true);
    let outgoing_file = demo_file_json(82, &path, true);
    let wave_json = serde_json::to_string(&wave).unwrap_or_else(|_| "\"\"".into());
    // MED2: the incoming note carries a real transcript so the demo
    // shows the transcription row.
    let incoming = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":90,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":12,"waveform":{wave_json},"mime_type":"audio/ogg","speech_recognition_result":{{"@type":"speechRecognitionResultText","text":"don't forget the milk"}},"voice":{incoming_file}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"is_listened":false}}}}}}"#
    );
    let outgoing = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":91,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":3,"waveform":{wave_json},"mime_type":"audio/ogg","speech_recognition_result":null,"voice":{outgoing_file}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"is_listened":true}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [incoming, outgoing, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

impl QuillApp {
    pub(super) fn kill_shared_player(&mut self) {
        self.audio.stop();
    }

    pub(super) fn stop_voice_playback(&mut self) {
        if self.playing_voice.is_some() {
            self.kill_shared_player();
        }
        // Save the position before clearing the id: clear_playback_state
        // reads active_playback_id().
        self.clear_playback_state();
        self.playing_voice = None;
        self.pending_voice_play = None;
    }

    pub(super) fn stop_audio_playback(&mut self) {
        if self.playing_audio.is_some() {
            self.kill_shared_player();
        }
        self.clear_playback_state();
        self.playing_audio = None;
        self.pending_audio_play = None;
    }

    /// Drop the seek-bar state for the active track (clock, slider entity,
    /// scrub preview). Idempotent: both stop functions call it. The
    /// remembered per-message position in `playback_positions` is kept so a
    /// stopped row still shows where it got to.
    pub(super) fn clear_playback_state(&mut self) {
        if let (Some(message_id), Some(clock)) =
            (self.active_playback_id(), self.playback_clock.as_ref())
        {
            self.playback_positions
                .insert(message_id, clock.elapsed_secs());
        }
        self.playback_clock = None;
        self.playback_path = None;
        self.seek_slider = None;
        self.seek_scrubbing = false;
        self.seek_preview_secs = None;
    }

    /// Message id of the active (playing or paused) track, if any.
    pub(super) fn active_playback_id(&self) -> Option<MessageId> {
        self.playing_voice.or(self.playing_audio)
    }

    /// Kind of the active track, for status notes.
    pub(super) fn active_playback_kind(&self) -> PlaybackKind {
        if self.playing_voice.is_some() {
            PlaybackKind::Voice
        } else {
            PlaybackKind::Audio
        }
    }

    /// Mark the given row as the active track: sets `playing_voice` /
    /// `playing_audio`, starts the playback clock at `offset_secs`, builds
    /// the seek slider entity, and starts the progress tick. Does not spawn
    /// the caller starts the sound (the screenshot demo fakes playback
    /// without a subprocess).
    pub(super) fn begin_track_playback(
        &mut self,
        kind: PlaybackKind,
        message_id: MessageId,
        duration_secs: f64,
        offset_secs: f64,
        cx: &mut Context<Self>,
    ) {
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.stop_viewer_video();
        self.playback_error = None;
        match kind {
            PlaybackKind::Voice => self.playing_voice = Some(message_id),
            PlaybackKind::Audio => self.playing_audio = Some(message_id),
        }
        let mut clock = PlaybackClock::new(duration_secs);
        clock.set_rate(self.playback_speed);
        clock.seek(offset_secs);
        clock.resume();
        self.playback_clock = Some(clock);
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(duration_secs.max(0.1) as f32)
                .step(0.1)
                .default_value(offset_secs.clamp(0.0, duration_secs.max(0.0)) as f32)
        });
        cx.subscribe(&slider, |this, _entity, event: &SliderEvent, cx| {
            this.on_seek_event(event, cx);
        })
        .detach();
        self.seek_slider = Some(slider);
        self.seek_scrubbing = false;
        self.seek_preview_secs = None;
        self.spawn_playback_tick(cx);
    }

    /// Start the in-process player on `path` at `offset_secs` with the
    /// current volume and speed. Returns true when it is playing.
    pub(super) fn start_player(&mut self, path: &std::path::Path, offset_secs: f64) -> bool {
        match self
            .audio
            .start(path, offset_secs, self.playback_volume, self.playback_speed)
        {
            Ok(()) => {
                self.playback_error = None;
                true
            }
            // MED1: honest error instead of a silent failure.
            Err(err) => {
                self.playback_error = Some(err.to_string());
                false
            }
        }
    }

    /// Move the sound to `offset_secs` (seek while playing).
    pub(super) fn restart_player_at(&mut self, offset_secs: f64) {
        if let Err(err) = self.audio.seek(offset_secs, true) {
            self.playback_error = Some(err.to_string());
        }
    }

    /// Pause the active track: freeze the clock and the sound, keep the row
    /// active so the seek bar stays interactive and Play resumes from here.
    pub(super) fn pause_active_playback(&mut self) {
        let id = self.active_playback_id();
        if let Some(clock) = self.playback_clock.as_mut() {
            clock.pause();
            if let Some(id) = id {
                self.playback_positions.insert(id, clock.elapsed_secs());
            }
        }
        self.audio.pause();
    }

    /// Resume the active track from the frozen clock position.
    pub(super) fn resume_active_playback(&mut self) {
        let offset = self.playback_clock.as_ref().map(|c| c.elapsed_secs());
        match offset {
            Some(offset) => {
                if let Err(err) = self.audio.resume(offset) {
                    self.playback_error = Some(err.to_string());
                }
                if let Some(clock) = self.playback_clock.as_mut() {
                    clock.resume();
                }
            }
            _ => {
                self.stop_voice_playback();
                self.stop_audio_playback();
            }
        }
    }

    /// `SliderEvent` sink for the active row's seek slider.
    pub(super) fn on_seek_event(&mut self, event: &SliderEvent, cx: &mut Context<Self>) {
        match event {
            SliderEvent::Change(value) => {
                // Drag (or track click) in progress: show the preview in the
                // time label, but don't touch the player until Release.
                self.seek_scrubbing = true;
                self.seek_preview_secs = Some(f64::from(value.end()));
                cx.notify();
            }
            SliderEvent::Release(value) => {
                self.seek_scrubbing = false;
                self.seek_preview_secs = None;
                self.seek_active_to(f64::from(value.end()), cx);
            }
        }
    }

    /// Apply a finished seek: clamp, move the clock, and seek the sound.
    /// Seeking while paused just moves the frozen position; the sound
    /// catches up on resume.
    pub(super) fn seek_active_to(&mut self, secs: f64, cx: &mut Context<Self>) {
        let Some(clock) = self.playback_clock.as_mut() else {
            return;
        };
        clock.seek(secs);
        let offset = clock.elapsed_secs();
        let was_playing = clock.is_playing();
        if let Some(id) = self.active_playback_id() {
            self.playback_positions.insert(id, offset);
        }
        if !was_playing {
            let _ = self.audio.seek(offset, false);
        }
        if was_playing {
            self.restart_player_at(offset);
            self.status_note = format!(
                "{} — seek {}",
                match self.active_playback_kind() {
                    PlaybackKind::Voice => "playing voice note",
                    PlaybackKind::Audio => "playing audio",
                },
                format_voice_duration(offset as i32)
            );
        }
        cx.notify();
    }

    /// 250 ms progress tick while a track is active: re-renders so the seek
    /// bar advances, and auto-stops when the clock reaches the duration
    /// (the sound ending ends the track too).
    pub(super) fn spawn_playback_tick(&mut self, cx: &mut Context<Self>) {
        if self.playback_tick {
            return;
        }
        self.playback_tick = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        let active = this.active_playback_id().is_some();
                        if !active {
                            return false;
                        }
                        // The sound is the authority on where the track
                        // ends (its real length can differ from TDLib's
                        // rounded duration); without one (demo) the clock is.
                        let sound_ended = this.audio.is_loaded() && this.audio.is_ended();
                        let finished = this.playback_clock.as_ref().is_some_and(|clock| {
                            clock.is_playing()
                                && if this.audio.is_loaded() {
                                    sound_ended
                                } else {
                                    clock.finished()
                                }
                        });
                        if finished && !this.seek_scrubbing {
                            // Capture the id first: the stops below save the
                            // (now end-of-track) position via clear_playback_state,
                            // then reset to 0.0 so replay-after-finish starts at the top.
                            let finished_id = this.active_playback_id();
                            this.stop_voice_playback();
                            this.stop_audio_playback();
                            if let Some(id) = finished_id {
                                this.playback_positions.insert(id, 0.0);
                            }
                            this.status_note = "playback finished".into();
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
                this.playback_tick = false;
            });
        })
        .detach();
    }

    /// Push the playback clock into the seek slider entity so the thumb
    /// follows elapsed time. Called at the top of `render` (the tick has no
    /// `&mut Window`, which `SliderState::set_value` needs). Skipped while
    /// scrubbing so the user's drag is never fought.
    pub(super) fn sync_seek_slider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.seek_scrubbing {
            return;
        }
        if let (Some(slider), Some(clock)) =
            (self.seek_slider.as_ref(), self.playback_clock.as_ref())
        {
            let value = clock.elapsed_secs().clamp(0.0, clock.duration_secs()) as f32;
            // `set_value` calls `cx.notify()` unconditionally — only push when
            // the value actually changed, otherwise this render-triggered sync
            // loops at frame rate instead of the tick cadence.
            let changed = slider.read(cx).value() != SliderValue::Single(value);
            if changed {
                slider.update(cx, |state, cx| {
                    state.set_value(value, window, cx);
                });
            }
        }
    }

    /// View model for one audio/voice row's seek bar.
    pub(super) fn seek_bar_view(&self, message_id: MessageId, duration_secs: f64) -> SeekBarView {
        let active = self.active_playback_id() == Some(message_id);
        if active {
            let display = self.seek_preview_secs.or_else(|| {
                self.playback_clock
                    .as_ref()
                    .map(|clock| clock.elapsed_secs())
            });
            SeekBarView {
                slider: self.seek_slider.clone(),
                display_secs: display.unwrap_or(0.0),
                duration_secs,
                is_playing: self
                    .playback_clock
                    .as_ref()
                    .is_some_and(PlaybackClock::is_playing),
                // MED1: speed/mute/error shown on the active row.
                speed: self.playback_speed,
                muted: self.playback_volume < 0.01,
                error: self.playback_error.clone(),
            }
        } else {
            SeekBarView {
                slider: None,
                display_secs: self
                    .playback_positions
                    .get(&message_id)
                    .copied()
                    .unwrap_or(0.0),
                duration_secs,
                is_playing: false,
                speed: self.playback_speed,
                muted: false,
                error: None,
            }
        }
    }

    pub(super) fn toggle_voice_playback(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        file_id: FileId,
        listened: bool,
        duration_secs: f64,
        cx: &mut Context<Self>,
    ) {
        if self.playing_voice == Some(message_id) {
            // Active row: pause ↔ resume (the row stays active so the seek
            // bar keeps working and Play resumes from the frozen position).
            let playing = self
                .playback_clock
                .as_ref()
                .is_some_and(PlaybackClock::is_playing);
            if playing {
                self.pause_active_playback();
                self.status_note = "voice note paused".into();
            } else {
                self.resume_active_playback();
                self.status_note = "playing voice note".into();
            }
            cx.notify();
            return;
        }
        let path = self.session().and_then(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .map(str::to_string)
        });
        let Some(path) = path else {
            self.pending_audio_play = None;
            self.pending_voice_play = Some((chat_id, message_id, file_id, listened, duration_secs));
            self.request_media_download(file_id, None, cx);
            return;
        };
        self.pending_voice_play = None;
        let roots = self.media_display_roots();
        let Some(safe) = sandboxed_display_path(&path, &roots) else {
            self.status_note = "voice file is outside the account files".into();
            cx.notify();
            return;
        };
        self.stop_video_playback();
        let offset = self
            .playback_positions
            .get(&message_id)
            .copied()
            .unwrap_or(0.0);
        self.begin_track_playback(PlaybackKind::Voice, message_id, duration_secs, offset, cx);
        self.playback_path = Some(safe.clone().into());
        if !listened {
            self.mark_voice_opened(chat_id, message_id);
        }
        let playing = self.start_player(&safe, offset);
        self.status_note = if playing {
            "playing voice note".into()
        } else {
            "voice note can't be played".into()
        };
        cx.notify();
    }

    pub(super) fn toggle_audio_playback(
        &mut self,
        message_id: MessageId,
        file_id: FileId,
        duration_secs: f64,
        cx: &mut Context<Self>,
    ) {
        if self.playing_audio == Some(message_id) {
            // Active row: pause ↔ resume (see voice toggle).
            let playing = self
                .playback_clock
                .as_ref()
                .is_some_and(PlaybackClock::is_playing);
            if playing {
                self.pause_active_playback();
                self.status_note = "audio paused".into();
            } else {
                self.resume_active_playback();
                self.status_note = "playing audio".into();
            }
            cx.notify();
            return;
        }
        let path = self.session().and_then(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .map(str::to_string)
        });
        let Some(path) = path else {
            self.pending_voice_play = None;
            self.pending_audio_play = Some((message_id, file_id, duration_secs));
            self.request_media_download(file_id, None, cx);
            return;
        };
        self.pending_audio_play = None;
        let roots = self.media_display_roots();
        let Some(safe) = sandboxed_display_path(&path, &roots) else {
            self.status_note = "audio file is outside the account files".into();
            cx.notify();
            return;
        };
        self.stop_video_playback();
        self.stop_animation_playback();
        let offset = self
            .playback_positions
            .get(&message_id)
            .copied()
            .unwrap_or(0.0);
        self.begin_track_playback(PlaybackKind::Audio, message_id, duration_secs, offset, cx);
        self.playback_path = Some(safe.clone().into());
        let playing = self.start_player(&safe, offset);
        self.status_note = if playing {
            "playing audio".into()
        } else {
            "audio can't be played".into()
        };
        cx.notify();
    }

    pub(super) fn resume_pending_voice(&mut self, cx: &mut Context<Self>) {
        let Some((chat_id, message_id, file_id, listened, duration)) = self.pending_voice_play
        else {
            return;
        };
        if self
            .session()
            .and_then(|s| s.file(file_id))
            .and_then(|f| f.usable_path())
            .is_some()
        {
            self.toggle_voice_playback(chat_id, message_id, file_id, listened, duration, cx);
        }
    }

    pub(super) fn resume_pending_audio(&mut self, cx: &mut Context<Self>) {
        let Some((message_id, file_id, duration_secs)) = self.pending_audio_play else {
            return;
        };
        let ready = self.session().is_some_and(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .is_some()
        });
        if ready {
            self.toggle_audio_playback(message_id, file_id, duration_secs, cx);
        }
    }

    pub(super) fn mark_voice_opened(&mut self, chat_id: ChatId, message_id: MessageId) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.open_voice_content(chat_id, message_id);
            return;
        }
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let json = format!(
            r#"{{"@type":"updateMessageContentOpened","chat_id":{},"message_id":{}}}"#,
            chat_id.0, message_id.0
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}
