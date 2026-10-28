//! shared audio/voice player.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::demo::{demo_file_json, demo_media_allowlist, demo_thumb_png_path};
use super::play_intent::{PlayKind, PlayTarget};
use super::playback_state::{SeekHover, SeekSite};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, FileId, MessageId};
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
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
        self.playback.audio.stop();
    }

    pub(super) fn stop_voice_playback(&mut self) {
        if self.playback.playing_voice.is_some() {
            self.kill_shared_player();
        }
        // Save the position before clearing the id: clear_playback_state
        // reads active_playback_id().
        self.clear_playback_state();
        self.playback.playing_voice = None;
        self.drop_play_intent(PlayKind::Voice);
    }

    pub(super) fn stop_audio_playback(&mut self) {
        if self.playback.playing_audio.is_some() {
            self.kill_shared_player();
        }
        self.clear_playback_state();
        self.playback.playing_audio = None;
        self.drop_play_intent(PlayKind::Audio);
    }

    /// Drop the seek-bar state for the active track (slider entity, scrub
    /// preview). Idempotent: both stop functions call it. The
    /// remembered per-message position in `playback_positions` is kept so a
    /// stopped row still shows where it got to.
    pub(super) fn clear_playback_state(&mut self) {
        if let (Some(message_id), Some(position)) = (
            self.active_playback_id(),
            self.playback.active_position_secs(),
        ) {
            self.playback.positions.insert(message_id, position);
        }
        self.playback.track_length = None;
        quill::wake::CHROME.poke();
        self.playback.seek_slider = None;
        self.playback.player.slider = None;
        self.playback.seek_scrubbing = false;
        self.playback.seek_preview_secs = None;
        self.playback.seek_hover = None;
    }

    /// Message id of the active (playing or paused) track, if any.
    pub(super) fn active_playback_id(&self) -> Option<MessageId> {
        self.playback.playing_voice.or(self.playback.playing_audio)
    }

    /// Kind of the active track, for status notes.
    pub(super) fn active_playback_kind(&self) -> PlaybackKind {
        if self.playback.playing_voice.is_some() {
            PlaybackKind::Voice
        } else {
            PlaybackKind::Audio
        }
    }

    /// Make the given row the active track and start its sound at
    /// `offset_secs`: builds the seek slider entity and starts the progress
    /// tick. The row becomes active even when the sound can't start; the
    /// engine then reports `Failed` and the UI shows exactly that. Returns
    /// whether the sound is playing.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn begin_track_playback(
        &mut self,
        kind: PlaybackKind,
        chat_id: ChatId,
        message_id: MessageId,
        duration_secs: f64,
        offset_secs: f64,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) -> bool {
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.stop_viewer_video();
        self.sound_starts(cx);
        // The sound was replaced, so a recording preview is over.
        self.recording.preview = None;
        self.playback.error = None;
        let playing = self.start_player(path, offset_secs, duration_secs);
        match kind {
            PlaybackKind::Voice => self.playback.playing_voice = Some(message_id),
            PlaybackKind::Audio => self.playback.playing_audio = Some(message_id),
        }
        self.playback.track_length = Some(duration_secs.max(0.0));
        quill::wake::CHROME.poke();
        let slider = cx.new(|_| {
            SliderState::new()
                .label(tr(keys::Q_AV_PLAYBACK_POSITION))
                .min(0.0)
                .max(duration_secs.max(0.1) as f32)
                .step(0.1)
                .default_value(offset_secs.clamp(0.0, duration_secs.max(0.0)) as f32)
        });
        cx.subscribe(&slider, |this, _entity, event: &SliderEvent, cx| {
            this.on_seek_event(event, cx);
        })
        .detach();
        self.playback.seek_slider = Some(slider);
        self.playback.seek_scrubbing = false;
        self.playback.seek_preview_secs = None;
        self.note_track_started(kind, chat_id, message_id, cx);
        self.spawn_playback_tick(cx);
        playing
    }

    /// Start the in-process player on `path` at `offset_secs` with the
    /// current volume and speed. Returns true when it is playing; on false
    /// the engine is `Failed` and `audio.error()` says why. The screenshot
    /// demos have no audio device and use the engine's explicit simulation.
    pub(super) fn start_player(
        &mut self,
        path: &std::path::Path,
        offset_secs: f64,
        duration_secs: f64,
    ) -> bool {
        let simulation = self
            .demo_session
            .is_some()
            .then(|| super::audio::Simulation {
                fail: std::env::var_os("QUILL_DEMO_AUDIO_FAIL").is_some(),
                stall: std::env::var_os("QUILL_DEMO_AUDIO_STALL").is_some(),
            });
        self.playback.audio.set_simulation(simulation);
        match self.playback.audio.start(
            path,
            offset_secs,
            self.playback.volume,
            self.playback.speed,
            duration_secs,
        ) {
            Ok(()) => true,
            // The engine keeps the reason (`audio.error()`).
            Err(_) => false,
        }
    }

    /// Pause the active track: hold the sound, keep the row active so the
    /// seek bar stays interactive and Play resumes from here.
    pub(super) fn pause_active_playback(&mut self) {
        self.playback.audio.pause();
        if let (Some(id), Some(position)) = (
            self.active_playback_id(),
            self.playback.active_position_secs(),
        ) {
            self.playback.positions.insert(id, position);
        }
        quill::wake::CHROME.poke();
    }

    /// Resume the active track from where the sound is. A failure leaves the
    /// engine in the state it reports (still paused, or failed), with the
    /// reason in `audio.error()`.
    pub(super) fn resume_active_playback(&mut self, cx: &mut Context<Self>) {
        let Some(position) = self.playback.active_position_secs() else {
            self.stop_voice_playback();
            self.stop_audio_playback();
            return;
        };
        self.sound_starts(cx);
        // A failure is kept by the engine (`audio.error()`).
        let _ = self.playback.audio.resume(position);
        quill::wake::CHROME.poke();
    }

    /// `SliderEvent` sink for the active row's seek slider.
    pub(super) fn on_seek_event(&mut self, event: &SliderEvent, cx: &mut Context<Self>) {
        match event {
            SliderEvent::Change(value) => {
                // Drag (or track click) in progress: show the preview in the
                // time label, but don't touch the player until Release.
                self.playback.seek_scrubbing = true;
                self.playback.seek_preview_secs = Some(f64::from(value.end()));
                cx.notify();
            }
            SliderEvent::Release(value) => {
                self.playback.seek_scrubbing = false;
                self.playback.seek_preview_secs = None;
                // A drag released off the scrubber gets no leave event;
                // the next move over it sets the hover again.
                self.playback.seek_hover = None;
                self.seek_active_to(f64::from(value.end()), cx);
            }
        }
    }

    /// Apply a finished seek: clamp it and move the sound. Seeking while
    /// not playing just moves the held position; the sound catches up on
    /// resume.
    pub(super) fn seek_active_to(&mut self, secs: f64, cx: &mut Context<Self>) {
        let Some(duration) = self.playback.track_duration() else {
            return;
        };
        let offset = secs.clamp(0.0, duration.max(0.0));
        let was_playing = self.playback.is_playing();
        if let Some(id) = self.active_playback_id() {
            self.playback.positions.insert(id, offset);
        }
        // A failure is kept by the engine (`audio.error()`).
        let _ = self.playback.audio.seek(offset, was_playing);
        if was_playing {
            self.connection.set_note(StatusNote::silent(format!(
                "{} — seek {}",
                match self.active_playback_kind() {
                    PlaybackKind::Voice => tr(keys::Q_AV_PLAYING_VOICE_NOTE),
                    PlaybackKind::Audio => tr(keys::Q_AV_PLAYING_AUDIO),
                },
                format_voice_duration(offset as i32)
            )));
        }
        cx.notify();
    }

    /// 250 ms progress tick while a track is active: re-renders so the seek
    /// bar follows the sound, and ends the track when the sound ran out.
    pub(super) fn spawn_playback_tick(&mut self, cx: &mut Context<Self>) {
        if self.playback.tick {
            return;
        }
        self.playback.tick = true;
        Self::spawn_main(cx, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        let active = this.active_playback_id().is_some();
                        if !active {
                            quill::media_session::publish(None);
                            return false;
                        }
                        // Media keys and the OS Now Playing widget.
                        this.drain_media_commands(cx);
                        if this.active_playback_id().is_none() {
                            quill::media_session::publish(None);
                            return false;
                        }
                        // The sound is the authority on where the track
                        // ends (its real length can differ from TDLib's
                        // rounded duration).
                        this.playback.audio.settle();
                        let finished = this.playback.audio.is_ended();
                        if finished && !this.playback.seek_scrubbing {
                            // Capture the id first: the stops below save the
                            // (now end-of-track) position via clear_playback_state,
                            // then reset to 0.0 so replay-after-finish starts at the top.
                            let finished_id = this.active_playback_id();
                            // What the playlist continues with (voice note
                            // chain, next song, repeat) is decided while
                            // the finished track is still the active one.
                            let next = this.next_after_finish();
                            this.stop_voice_playback();
                            this.stop_audio_playback();
                            if let Some(id) = finished_id {
                                this.playback.positions.insert(id, 0.0);
                            }
                            this.connection
                                .set_note(StatusNote::silent(tr(keys::Q_AV_PLAYBACK_FINISHED)));
                            if let Some((chat, id)) = next {
                                this.play_message(chat, id, cx);
                            }
                        }
                        this.publish_now_playing();
                        if finished {
                            cx.notify();
                        } else {
                            this.notify_playback_position(cx);
                        }
                        true
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.playback.tick = false;
            });
        })
        .detach();
    }

    /// The position moved: redraw the seek bars, the player bar and the
    /// Touch Bar scrubber, not the whole app.
    fn notify_playback_position(&self, cx: &mut Context<Self>) {
        self.notify_conversations(cx);
        #[cfg(target_os = "macos")]
        super::touch_bar::request_sync(cx);
    }

    /// Push the sound's position into the seek slider entity so the thumb
    /// follows it. Called at the top of `render` (the tick has no
    /// `&mut Window`, which `SliderState::set_value` needs). Skipped while
    /// scrubbing so the user's drag is never fought.
    pub(super) fn sync_seek_slider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.playback.seek_scrubbing {
            return;
        }
        if let (Some(slider), Some(position)) = (
            self.playback.seek_slider.as_ref(),
            self.playback.active_position_secs(),
        ) {
            let value = position as f32;
            // `set_value` calls `cx.notify()` unconditionally — only push when
            // the value actually changed, otherwise this render-triggered sync
            // loops at frame rate instead of the tick cadence.
            let changed = slider.read(cx).value() != SliderValue::Single(value);
            if changed {
                slider.update(cx, |state, cx| {
                    state.set_value(value, window, cx);
                });
            }
            // The player bar's own slider follows the same position.
            if let Some(bar) = self.playback.player.slider.as_ref()
                && bar.read(cx).value() != SliderValue::Single(value)
            {
                bar.update(cx, |state, cx| {
                    state.set_value(value, window, cx);
                });
            }
        }
    }

    /// Where the pointer is over `site`'s scrubber (`None`: it left).
    /// Redraws only when that changes.
    pub(super) fn set_seek_hover(
        &mut self,
        site: SeekSite,
        at: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        let next = match at {
            Some(at) => Some(SeekHover { site, at }),
            // A late leave from one control never clears the other's hover.
            None => self.playback.seek_hover.filter(|hover| hover.site != site),
        };
        if next != self.playback.seek_hover {
            self.playback.seek_hover = next;
            cx.notify();
        }
    }

    /// The scrubber hover handler for `site`.
    pub(super) fn seek_hover_handler(
        site: SeekSite,
        cx: &Context<Self>,
    ) -> super::message_media::HoverHandler {
        let app = cx.weak_entity();
        std::rc::Rc::new(move |at, _, cx| {
            let _ = app.update(cx, |this, cx| this.set_seek_hover(site, at, cx));
        })
    }

    /// View model for one audio/voice row's seek bar.
    pub(super) fn seek_bar_view(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        duration_secs: f64,
    ) -> SeekBarView {
        // Playback continues across chats, and message ids repeat between
        // them: a row is the active one only in the track's own chat.
        let active = self.active_playback_id() == Some(message_id)
            && self.playback.player.chat.is_none_or(|chat| chat == chat_id);
        if active {
            let display = self
                .playback
                .seek_preview_secs
                .or_else(|| self.playback.active_position_secs());
            SeekBarView {
                slider: self.playback.seek_slider.clone(),
                display_secs: display.unwrap_or(0.0),
                duration_secs,
                is_playing: self.playback.is_playing(),
                // Speed, mute and the engine's error show on the active row.
                speed: self.playback.speed,
                muted: self.playback.volume < 0.01,
                error: self.playback.audio.error().map(audio_error_text),
                hover: self
                    .playback
                    .seek_hover
                    .filter(|hover| hover.site == SeekSite::Row)
                    .map(|hover| hover.at),
                dragging: self.playback.seek_preview_secs.is_some(),
            }
        } else {
            SeekBarView {
                slider: None,
                display_secs: self
                    .playback
                    .positions
                    .get(&message_id)
                    .copied()
                    .unwrap_or(0.0),
                duration_secs,
                is_playing: false,
                speed: self.playback.speed,
                muted: false,
                error: None,
                hover: None,
                dragging: false,
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
        if self.playback.playing_voice == Some(message_id)
            && self.playback.player.chat.is_none_or(|chat| chat == chat_id)
        {
            // Active row: pause ↔ resume (the row stays active so the seek
            // bar keeps working and Play resumes from the frozen position).
            if self.playback.is_playing() {
                self.pause_active_playback();
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_AV_VOICE_NOTE_PAUSED)));
            } else {
                self.resume_active_playback(cx);
                self.connection.set_note(if self.playback.is_playing() {
                    StatusNote::silent(tr(keys::Q_AV_PLAYING_VOICE_NOTE))
                } else {
                    StatusNote::error(tr(keys::Q_AV_VOICE_NOTE_CANT_BE_PLAYED))
                });
            }
            cx.notify();
            return;
        }
        let path = self.session().and_then(|session| {
            session
                .media
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .map(str::to_string)
        });
        let Some(path) = path else {
            let target = PlayTarget::Voice {
                chat: chat_id,
                message: message_id,
                listened,
                duration: duration_secs,
            };
            self.play_when_ready(file_id, target, cx);
            return;
        };
        self.playback.intent = None;
        let roots = self.media_display_roots();
        let Some(safe) = sandboxed_display_path(&path, &roots) else {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_AV_VOICE_FILE_IS_OUTSIDE_THE)));
            cx.notify();
            return;
        };
        let offset = self
            .playback
            .positions
            .get(&message_id)
            .copied()
            .unwrap_or(0.0);
        let playing = self.begin_track_playback(
            PlaybackKind::Voice,
            chat_id,
            message_id,
            duration_secs,
            offset,
            &safe,
            cx,
        );
        if !listened {
            self.mark_voice_opened(chat_id, message_id);
        }
        self.connection.set_note(if playing {
            StatusNote::silent(tr(keys::Q_AV_PLAYING_VOICE_NOTE))
        } else {
            StatusNote::error(tr(keys::Q_AV_VOICE_NOTE_CANT_BE_PLAYED))
        });
        cx.notify();
    }

    pub(super) fn toggle_audio_playback(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        file_id: FileId,
        duration_secs: f64,
        cx: &mut Context<Self>,
    ) {
        if self.playback.playing_audio == Some(message_id)
            && self.playback.player.chat.is_none_or(|chat| chat == chat_id)
        {
            // Active row: pause ↔ resume (see voice toggle).
            if self.playback.is_playing() {
                self.pause_active_playback();
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_AV_AUDIO_PAUSED)));
            } else {
                self.resume_active_playback(cx);
                self.connection.set_note(if self.playback.is_playing() {
                    StatusNote::silent(tr(keys::Q_AV_PLAYING_AUDIO))
                } else {
                    StatusNote::error(tr(keys::Q_AV_AUDIO_CANT_BE_PLAYED))
                });
            }
            cx.notify();
            return;
        }
        let path = self.session().and_then(|session| {
            session
                .media
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .map(str::to_string)
        });
        let Some(path) = path else {
            let target = PlayTarget::Audio {
                chat: chat_id,
                message: message_id,
                duration: duration_secs,
            };
            self.play_when_ready(file_id, target, cx);
            return;
        };
        self.playback.intent = None;
        let roots = self.media_display_roots();
        let Some(safe) = sandboxed_display_path(&path, &roots) else {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_AV_AUDIO_FILE_IS_OUTSIDE_THE)));
            cx.notify();
            return;
        };
        self.stop_animation_playback();
        let offset = self
            .playback
            .positions
            .get(&message_id)
            .copied()
            .unwrap_or(0.0);
        let playing = self.begin_track_playback(
            PlaybackKind::Audio,
            chat_id,
            message_id,
            duration_secs,
            offset,
            &safe,
            cx,
        );
        self.connection.set_note(if playing {
            StatusNote::silent(tr(keys::Q_AV_PLAYING_AUDIO))
        } else {
            StatusNote::error(tr(keys::Q_AV_AUDIO_CANT_BE_PLAYED))
        });
        cx.notify();
    }

    pub(super) fn mark_voice_opened(&mut self, chat_id: ChatId, message_id: MessageId) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.open_voice_content(chat_id, message_id);
            return;
        }
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let json = format!(
            r#"{{"@type":"updateMessageContentOpened","chat_id":{},"message_id":{}}}"#,
            chat_id.0, message_id.0
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// What the row says for an engine error (sentence case, localized).
fn audio_error_text(error: super::audio::AudioError) -> String {
    use super::audio::AudioError;
    match error {
        AudioError::Open => tr(keys::Q_AV_COULDNT_OPEN_THE_AUDIO_FILE),
        AudioError::Unsupported => tr(keys::Q_AV_THIS_AUDIO_FORMAT_CANT_BE_PLAYED),
        AudioError::NoOutput => tr(keys::Q_AV_NO_AUDIO_OUTPUT_DEVICE),
        AudioError::DeviceStalled => tr(keys::Q_AV_THE_AUDIO_DEVICE_ISNT_RESPONDING),
    }
    .to_string()
}
