//! voice/video-note recording.

use super::app::QuillApp;
use super::demo::demo_file_json;
use super::message_media::waveform_row;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::{AttachmentKind, ComposerAttachment, ComposerReplyTo};
use quill::diagnostics::DiagnosticSink;
use quill::ids::{ChatId, MessageId};
use quill::playback::PlaybackClock;
use quill::send_rights::SendKind;
use quill::telegram::client::copy_and_parse;
use quill::video::VideoNoteCapture;
use quill::voice::{
    RecordBarFacts, RecordControl, VoiceCapture, format_voice_duration, record_controls,
};
use std::sync::Arc;
/// MED2: the record button's mode (TGX `preferVideoMode`, persisted in
/// `MediaPrefs`). Desktop mapping of TGX's hold-to-record / tap-to-switch:
/// click records in the current mode, right-click switches mode (tdesktop's
/// mapping) — a touch hold gesture has no honest mouse equivalent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecordMode {
    Audio,
    Video,
}

impl RecordMode {
    pub(super) fn hint(self) -> &'static str {
        match self {
            // TGX strings, desktop-mapped.
            RecordMode::Audio => "Click to record audio · right-click for video",
            RecordMode::Video => "Click to record video · right-click for audio",
        }
    }
}

impl QuillApp {
    /// MED2: the record button's mode (TGX `preferVideoMode`, persisted in
    /// `MediaPrefs`). Desktop mapping of TGX's hold-to-record /
    /// tap-to-switch: click records in the current mode, right-click
    /// switches mode (tdesktop's mapping) — a touch hold gesture has no
    /// honest mouse equivalent.
    pub(super) fn record_mode(&self) -> RecordMode {
        if self
            .session()
            .is_some_and(|session| session.media_prefs.prefer_video_mode)
        {
            RecordMode::Video
        } else {
            RecordMode::Audio
        }
    }

    pub(super) fn recording_active(&self) -> bool {
        self.recording.voice_capture.is_some() || self.recording.video_note_capture.is_some()
    }

    /// MED2: right-click on the record button flips audio/video mode
    /// (TGX tap-to-switch, desktop-mapped). Ignored while recording.
    pub(super) fn toggle_record_mode(&mut self, cx: &mut Context<Self>) {
        if self.recording_active() {
            self.status_note = "finish the recording first".into();
            cx.notify();
            return;
        }
        let next = !matches!(self.record_mode(), RecordMode::Video);
        self.set_media_pref(|prefs| prefs.prefer_video_mode = next, cx);
        self.status_note = format!(
            "{} — {}",
            if next {
                "video note mode"
            } else {
                "voice note mode"
            },
            self.record_mode().hint()
        );
        cx.notify();
    }

    pub(super) fn start_recording(&mut self, cx: &mut Context<Self>) {
        match self.record_mode() {
            RecordMode::Audio => self.start_voice_recording(cx),
            RecordMode::Video => self.start_video_note_recording(cx),
        }
    }

    pub(super) fn start_voice_recording(&mut self, cx: &mut Context<Self>) {
        if self.composer_ui.pending_edit.is_some() || self.recording_active() {
            return;
        }
        if self.gif_panel_open() {
            self.close_gif_panel(cx);
        }
        if self.sticker_panel_open() {
            self.close_sticker_panel(cx);
        }
        if self.deny_send(SendKind::VoiceMessages, cx) {
            return;
        }
        self.with_capture_access(false, Self::begin_voice_capture, cx);
    }

    fn begin_voice_capture(&mut self, cx: &mut Context<Self>) {
        if self.recording_active() {
            return;
        }
        match VoiceCapture::start() {
            Ok(capture) => {
                self.recording.voice_capture = Some(capture);
                self.sync_voice_action();
                self.spawn_voice_tick(cx);
                self.status_note = "recording voice note".into();
            }
            Err(err) => {
                self.status_note = err;
            }
        }
        cx.notify();
    }

    /// MED2: start a round video-note camera capture (ffmpeg V4L2, squared
    /// at finish; HQ size from media prefs). Needs a camera, honest error
    /// otherwise.
    pub(super) fn start_video_note_recording(&mut self, cx: &mut Context<Self>) {
        if self.composer_ui.pending_edit.is_some() || self.recording_active() {
            return;
        }
        // Phase S1: round video notes need secret-chat layer ≥ 66 (TGX
        // `chatSupportsRoundVideos`).
        if let Some((name, layer)) = self.open_secret_chat_peer_layer()
            && layer < 66
        {
            self.status_note = format!(
                "{name}'s Telegram client doesn't support this feature. \
                 They need to install an update first."
            );
            cx.notify();
            return;
        }
        if self.gif_panel_open() {
            self.close_gif_panel(cx);
        }
        if self.sticker_panel_open() {
            self.close_sticker_panel(cx);
        }
        if self.deny_send(SendKind::VideoMessages, cx) {
            return;
        }
        self.with_capture_access(true, Self::begin_video_note_capture, cx);
    }

    fn begin_video_note_capture(&mut self, cx: &mut Context<Self>) {
        if self.recording_active() {
            return;
        }
        let hq = self
            .session()
            .is_some_and(|session| session.media_prefs.hq_round_videos);
        match VideoNoteCapture::start(hq) {
            Ok(capture) => {
                self.recording.video_note_capture = Some(capture);
                self.sync_voice_action();
                self.spawn_voice_tick(cx);
                self.status_note = "recording video note".into();
            }
            Err(err) => {
                self.status_note = err;
            }
        }
        cx.notify();
    }

    /// A capture that stopped by itself: a video message that reached the
    /// 60 s limit is sent (as tdesktop does); a failed start (no camera or
    /// microphone access) ends the recording with the reason.
    pub(super) fn check_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(ended) = self
            .recording
            .video_note_capture
            .as_mut()
            .and_then(|c| c.ended())
        {
            match ended {
                Ok(()) => {
                    self.recording.auto_send = true;
                    cx.notify();
                }
                Err(reason) => self.fail_recording(reason, cx),
            }
            return;
        }
        if let Some(reason) = self
            .recording
            .voice_capture
            .as_mut()
            .and_then(|c| c.failure())
        {
            self.fail_recording(reason, cx);
        }
    }

    fn fail_recording(&mut self, reason: String, cx: &mut Context<Self>) {
        self.cancel_recording(cx);
        self.status_note = reason;
        cx.notify();
    }

    /// MED2: Cancel / Esc on an unlocked recording opens the confirm row
    /// instead of discarding silently.
    pub(super) fn request_discard_recording(&mut self, cx: &mut Context<Self>) {
        // MED2 fix-up: locked recordings CAN be cancelled — the confirm
        // row still guards against accidental discards. Esc stays locked.
        if !self.recording_active() {
            return;
        }
        self.recording.discard_confirm = true;
        cx.notify();
    }

    /// MED2: the confirm row's Discard button — performs the discard.
    pub(super) fn confirm_discard_recording(&mut self, cx: &mut Context<Self>) {
        self.cancel_recording(cx);
    }

    /// MED2: Lock ↔ unlock the in-progress recording (TGX `RecordLockView`,
    /// desktop-mapped to a Lock button). A locked recording ignores Esc.
    pub(super) fn toggle_record_lock(&mut self, cx: &mut Context<Self>) {
        if !self.recording_active() {
            return;
        }
        self.recording.locked = !self.recording.locked;
        self.status_note = if self.recording.locked {
            "recording locked — Esc won't cancel it".into()
        } else {
            "recording unlocked".into()
        };
        cx.notify();
    }

    /// Pause the voice recording, or carry on after a pause. Pausing
    /// releases the microphone; resuming stops any preview first.
    pub(super) fn toggle_record_pause(&mut self, cx: &mut Context<Self>) {
        let Some(capture) = self.recording.voice_capture.as_mut() else {
            return;
        };
        if capture.is_paused() {
            capture.resume();
            self.stop_record_preview();
            self.status_note = "recording voice note".into();
        } else {
            capture.pause();
            self.status_note = "recording paused".into();
        }
        self.sync_voice_action();
        cx.notify();
    }

    /// Stop the preview of a paused recording.
    pub(super) fn stop_record_preview(&mut self) {
        if self.recording.preview.take().is_some() {
            self.kill_shared_player();
        }
    }

    /// Play or pause what has been recorded so far. A preview that reached
    /// its end starts again from the top.
    pub(super) fn toggle_record_preview(&mut self, cx: &mut Context<Self>) {
        let Some(capture) = self.recording.voice_capture.as_ref() else {
            return;
        };
        let Some(path) = capture.preview_path().map(std::path::Path::to_path_buf) else {
            self.status_note = "getting the recording ready…".into();
            cx.notify();
            return;
        };
        let seconds = f64::from(capture.elapsed_secs());
        if let Some(clock) = self.recording.preview.as_mut() {
            if clock.is_playing() {
                clock.pause();
                self.playback.audio.pause();
                cx.notify();
                return;
            }
            if !clock.finished()
                && self.playback.audio.is_loaded()
                && !self.playback.audio.is_ended()
            {
                let at = clock.elapsed_secs();
                if self.playback.audio.resume(at).is_ok() {
                    clock.resume();
                    cx.notify();
                    return;
                }
            }
        }
        // Whatever else was playing gives way to the preview.
        self.stop_voice_playback();
        self.stop_audio_playback();
        if self.start_player(&path, 0.0) {
            let mut clock = PlaybackClock::new(seconds);
            clock.resume();
            self.recording.preview = Some(clock);
            self.spawn_voice_tick(cx);
        } else {
            self.status_note = "couldn't play the recording".into();
        }
        cx.notify();
    }

    /// The preview ran to its end: show it stopped, ready to play again.
    pub(super) fn check_record_preview(&mut self) {
        let ended = self.recording.preview.as_ref().is_some_and(|clock| {
            clock.is_playing() && (clock.finished() || self.playback.audio.is_ended())
        });
        if ended {
            self.recording.preview = None;
            self.kill_shared_player();
        }
    }

    /// Switch "Play once" for the voice message being recorded.
    pub(super) fn toggle_record_once(&mut self, cx: &mut Context<Self>) {
        self.recording.once = !self.recording.once;
        self.status_note = if self.recording.once {
            "The recipient will be able to listen only once.".into()
        } else {
            "play once off".into()
        };
        cx.notify();
    }

    /// Self-destructing messages exist only in private chats.
    fn once_allowed(&self) -> bool {
        self.session()
            .and_then(|session| session.chats.get(&session.open_chat?.0))
            .is_some_and(|chat| {
                matches!(
                    chat.kind,
                    quill::telegram::envelope::ChatKind::Private { .. }
                )
            })
    }

    pub(super) fn cancel_recording(&mut self, cx: &mut Context<Self>) {
        self.stop_record_preview();
        self.recording.once = false;
        let was_video = self.recording.video_note_capture.is_some();
        if let Some(capture) = self.recording.voice_capture.take() {
            capture.discard();
        }
        if let Some(capture) = self.recording.video_note_capture.take() {
            capture.discard();
        }
        self.recording.locked = false;
        self.recording.discard_confirm = false;
        self.sync_voice_action();
        self.status_note = if was_video {
            "video recording cancelled".into()
        } else {
            "voice recording cancelled".into()
        };
        cx.notify();
    }

    pub(super) fn send_voice_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Phase A1: slow-mode gate applies to voice notes too — checked
        // before consuming the capture so a blocked recording survives
        // until the timer expires.
        if self
            .open_chat_id()
            .is_some_and(|chat_id| self.slow_mode_blocked(chat_id, cx))
        {
            return;
        }
        let Some(capture) = self.recording.voice_capture.take() else {
            return;
        };
        // MED2 fix-up: the send consumes the recording — locked state must
        // not leak into the next recording.
        self.recording.locked = false;
        self.recording.discard_confirm = false;
        self.stop_record_preview();
        let play_once = std::mem::take(&mut self.recording.once);
        let caption = self.composer_markup(cx);
        let draft = match capture.finish() {
            Ok(draft) => draft,
            Err(err) => {
                self.sync_voice_action();
                self.status_note = err;
                cx.notify();
                return;
            }
        };
        let reply = self.composer_ui.pending_reply.clone();
        if self.live.is_some() {
            let reply_to = self.recording_send_reply();
            let result = self.live.as_mut().expect("live").driver.send_voice_note(
                &draft,
                caption.trim(),
                reply_to,
                play_once,
            );
            self.status_note = match result {
                Ok(_) => "sending voice note".into(),
                Err(_) => "could not send voice note".into(),
            };
            if self.status_note == "sending voice note" {
                self.composer_ui.pending_reply = None;
                self.composer_ui.clear_draft_on_success = Some(
                    self.live
                        .as_ref()
                        .and_then(|live| live.driver.session.open_chat)
                        .unwrap_or(ChatId(0)),
                );
                self.composer
                    .update(cx, |input, cx| input.set_value("", window, cx));
                if let Some(open) = self.open_chat_id() {
                    self.forget_local_draft(open);
                }
            }
        } else if self.demo_session.is_some() {
            self.apply_demo_voice(&draft, caption.trim(), reply.as_ref());
            self.composer_ui.pending_reply = None;
            self.composer
                .update(cx, |input, cx| input.set_value("", window, cx));
            if let Some(open) = self.demo_session.as_ref().and_then(|s| s.open_chat) {
                self.forget_local_draft(open);
            }
            self.status_note = "demo voice note applied locally (no live Telegram)".into();
        }
        self.sync_voice_action();
        cx.notify();
    }

    /// Reply target for a recording send: the pending reply when it belongs
    /// to the open chat.
    pub(super) fn recording_send_reply(&self) -> Option<quill::telegram::SendReply> {
        let reply = self.composer_ui.pending_reply.as_ref()?;
        let open = self.live.as_ref()?.driver.session.open_chat?;
        reply.send_target(open)
    }

    /// MED2: send the finished recording, whichever mode is active.
    pub(super) fn send_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.recording.video_note_capture.is_some() {
            self.send_video_note_recording(window, cx);
        } else {
            self.send_voice_recording(window, cx);
        }
    }

    /// MED2: finish the round video-note capture and send it
    /// (`inputMessageVideoNote`). Same lifecycle as `send_voice_recording`:
    /// slow-mode gate first, the capture is only consumed when the send
    /// goes ahead, demo mode routes locally.
    pub(super) fn send_video_note_recording(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .open_chat_id()
            .is_some_and(|chat_id| self.slow_mode_blocked(chat_id, cx))
        {
            return;
        }
        let Some(capture) = self.recording.video_note_capture.take() else {
            return;
        };
        self.recording.once = false;
        // MED2 fix-up: the send consumes the recording — locked state must
        // not leak into the next recording.
        self.recording.locked = false;
        self.recording.discard_confirm = false;
        self.sync_voice_action();
        cx.notify();
        // ffmpeg finishes the file after the stop signal: off the main thread.
        let finished = cx.background_spawn(async move { capture.finish() });
        cx.spawn_in(window, async move |this, cx| {
            let result = finished.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.send_finished_video_note(result, window, cx);
            });
        })
        .detach();
    }

    fn send_finished_video_note(
        &mut self,
        result: Result<quill::video::VideoNoteDraft, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let draft = match result {
            Ok(draft) => draft,
            Err(err) => {
                self.status_note = err;
                cx.notify();
                return;
            }
        };
        let reply = self.composer_ui.pending_reply.clone();
        if self.live.is_some() {
            let reply_to = self.recording_send_reply();
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .send_recorded_video_note(&draft, reply_to);
            self.status_note = match result {
                Ok(_) => "sending video note".into(),
                Err(_) => "could not send video note".into(),
            };
            if self.status_note == "sending video note" {
                self.composer_ui.pending_reply = None;
                self.composer_ui.clear_draft_on_success = Some(
                    self.live
                        .as_ref()
                        .and_then(|live| live.driver.session.open_chat)
                        .unwrap_or(ChatId(0)),
                );
                self.composer
                    .update(cx, |input, cx| input.set_value("", window, cx));
                if let Some(open) = self.open_chat_id() {
                    self.forget_local_draft(open);
                }
            }
        } else if self.demo_session.is_some() {
            if let Some(att) = ComposerAttachment::pick(&draft.path, AttachmentKind::VideoNote) {
                self.apply_demo_outgoing("", Some(&att), reply.as_ref());
                self.composer_ui.pending_reply = None;
                self.composer
                    .update(cx, |input, cx| input.set_value("", window, cx));
                if let Some(open) = self.demo_session.as_ref().and_then(|s| s.open_chat) {
                    self.forget_local_draft(open);
                }
                self.status_note = "demo video note applied locally (no live Telegram)".into();
            } else {
                self.status_note = "demo: recorded clip is outside the sendable paths".into();
            }
        }
        self.sync_voice_action();
        cx.notify();
    }

    /// MED2: ask TDLib to recognize speech in a voice/video note
    /// (`recognizeSpeech`, 1.8.67). The transcript arrives later via
    /// `updateMessageContent`; a refusal is surfaced, never faked.
    pub(super) fn request_transcription(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.recognize_speech(chat_id, message_id) {
                Ok(_) => self.status_note = "transcription requested".into(),
                Err(_) => self.status_note = "couldn't request transcription".into(),
            }
        } else {
            self.status_note = "demo: transcription needs a live connection".into();
        }
        cx.notify();
    }

    /// Phase 6: write the requested HQ value (was: flip), keeping the
    /// status-note feedback the toggle gave.
    pub(super) fn set_hq_round_videos(&mut self, on: bool, cx: &mut Context<Self>) {
        self.set_media_pref(|prefs| prefs.hq_round_videos = on, cx);
        self.status_note = if on {
            "HQ round videos on — 480px captures".into()
        } else {
            "HQ round videos off — 280px captures".into()
        };
        cx.notify();
    }

    pub(super) fn apply_demo_voice(
        &mut self,
        draft: &quill::voice::VoiceDraft,
        caption: &str,
        reply: Option<&ComposerReplyTo>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let Some(chat_id) = session.open_chat else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let id = -(session.view_generation.0 as i64);
        let path = draft.path.to_string_lossy();
        let file = demo_file_json(910, &path, true);
        let waveform = draft.waveform_b64();
        let reply_json = reply
            .filter(|r| r.chat_id == chat_id)
            .map(|r| {
                format!(
                    r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":{},"quote":null,"checklist_task_id":0,"poll_option_id":""}}"#,
                    r.chat_id.0, r.message_id.0
                )
            })
            .unwrap_or_default();
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":{},"waveform":{},"mime_type":"audio/ogg","speech_recognition_result":null,"voice":{file}}},"caption":{{"@type":"formattedText","text":{},"entities":[]}},"is_listened":true}}{reply_json}}}}}"#,
            chat_id.0,
            draft.duration_secs,
            serde_json::to_string(&waveform).unwrap_or_else(|_| "\"\"".into()),
            serde_json::to_string(caption).unwrap_or_else(|_| "\"\"".into()),
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    /// While a recording bar is active, sync `chatActionRecordingVoiceNote`
    /// / `chatActionRecordingVideoNote` to the open chat (Unigram record
    /// actions). Cancels when recording stops.
    pub(super) fn sync_voice_action(&mut self) {
        let voice = self
            .recording
            .voice_capture
            .as_ref()
            .is_some_and(|capture| !capture.is_paused());
        let video = self.recording.video_note_capture.is_some();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if let Some(live) = self.live.as_mut() {
            if video {
                let _ = live.driver.sync_video_note_recording(true, now_ms);
            } else {
                let _ = live.driver.sync_voice_recording(voice, now_ms);
            }
        }
    }

    /// MED2: the record bar for voice and round video-note captures:
    /// elapsed time + waveform (voice only), Lock/Unlock (TGX `RecordLockView`,
    /// desktop-mapped), Cancel (asks for confirmation), Send. A locked
    /// recording ignores Esc.
    pub(super) fn record_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let video = self.recording.video_note_capture.is_some();
        let seconds = if video {
            self.recording
                .video_note_capture
                .as_ref()
                .map(|capture| capture.elapsed_secs())
                .unwrap_or(0)
        } else {
            self.recording
                .voice_capture
                .as_ref()
                .map(|capture| capture.elapsed_secs())
                .unwrap_or(0)
        };
        let bars = self
            .recording
            .voice_capture
            .as_ref()
            .map(|capture| capture.bars.clone())
            .unwrap_or_default();
        let paused = !video
            && self
                .recording
                .voice_capture
                .as_ref()
                .is_some_and(|capture| capture.is_paused());
        let previewing = self
            .recording
            .preview
            .as_ref()
            .is_some_and(|clock| clock.is_playing());
        // While a paused recording plays, the time and the lit part of the
        // waveform follow the playhead.
        let (shown_seconds, lit) = match &self.recording.preview {
            Some(clock) if paused => (clock.elapsed_secs().floor() as i32, clock.fraction() as f32),
            _ => (seconds, 1.0),
        };
        let label = format!(
            "{} {} {}",
            if paused { "Paused" } else { "Recording" },
            if video {
                "video message"
            } else {
                "voice message"
            },
            format_voice_duration(seconds),
        );
        // A pulsing red dot while recording; on a paused recording a play
        // button takes its place (tdesktop's listen state).
        let leading = if paused {
            let control = RecordControl::Preview;
            Button::new("record-preview")
                .icon(if previewing {
                    gpui_kit::assets::IconName::Pause
                } else {
                    gpui_kit::assets::IconName::Play
                })
                .primary()
                .rounded_full()
                .small()
                .tooltip(control.label(previewing))
                .accessibility_label(control.label(previewing))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_record_preview(cx);
                }))
                .into_any_element()
        } else {
            div()
                .size(px(10.))
                .flex_none()
                .rounded_full()
                .bg(danger())
                .with_animation(
                    "record-pulse",
                    Animation::new(std::time::Duration::from_millis(1200))
                        .repeat()
                        .with_easing(pulsating_between(0.35, 1.0)),
                    |dot, delta| dot.opacity(delta),
                )
                .into_any_element()
        };
        div()
            .id("voice-record-bar")
            .role(gpui_kit::Role::Group)
            .aria_label(label)
            .flex()
            .items_center()
            .gap_3()
            .child(leading)
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .flex_none()
                    .child(format_voice_duration(shown_seconds)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .when(video, |this| {
                        this.text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Video message")
                    })
                    .when(!video, |this| {
                        this.child(waveform_row(0, &bars, accent().into(), lit))
                    }),
            )
            .child(self.record_bar_actions(cx))
    }

    /// MED2: the record bar's right-side row — either the normal
    /// Lock/Cancel/Send buttons or the discard-confirmation row.
    pub(super) fn record_bar_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.recording.discard_confirm {
            return div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().text_sm().child("Discard recording?"))
                .child(
                    Button::new("discard-record-confirm")
                        .label("Discard")
                        .danger()
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_discard_recording(cx);
                        })),
                )
                .child(
                    Button::new("keep-recording")
                        .label("Keep")
                        .ghost()
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.recording.discard_confirm = false;
                            cx.notify();
                        })),
                )
                .into_any_element();
        }
        let video = self.recording.video_note_capture.is_some();
        let paused = !video
            && self
                .recording
                .voice_capture
                .as_ref()
                .is_some_and(|capture| capture.is_paused());
        let controls = record_controls(RecordBarFacts {
            video,
            paused,
            once_allowed: !video && self.once_allowed(),
        });
        let mut row = div().flex().items_center().gap_1();
        for control in controls {
            row = row.child(match control {
                RecordControl::Lock => Button::new("lock-record")
                    .icon(gpui_kit::assets::IconName::Lock)
                    .ghost()
                    .selected(self.recording.locked)
                    .tooltip(if self.recording.locked {
                        "Locked — Esc won't cancel. Click to unlock"
                    } else {
                        "Lock: hands-free recording — Esc won't cancel"
                    })
                    .accessibility_label(control.label(self.recording.locked))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_record_lock(cx);
                    }))
                    .into_any_element(),
                RecordControl::Pause | RecordControl::Resume => {
                    let resume = control == RecordControl::Resume;
                    Button::new("pause-record")
                        .icon(if resume {
                            gpui_kit::assets::IconName::Mic
                        } else {
                            gpui_kit::assets::IconName::Pause
                        })
                        .ghost()
                        .tooltip(control.label(false))
                        .accessibility_label(control.label(false))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_record_pause(cx);
                        }))
                        .into_any_element()
                }
                // The play button leads the bar while paused.
                RecordControl::Preview => div().into_any_element(),
                RecordControl::PlayOnce => Button::new("once-record")
                    .label("Play once")
                    .ghost()
                    .small()
                    .selected(self.recording.once)
                    .tooltip(control.label(self.recording.once))
                    .accessibility_label(control.label(self.recording.once))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_record_once(cx);
                    }))
                    .into_any_element(),
                RecordControl::Discard => Button::new("cancel-record")
                    .icon(gpui_kit::assets::IconName::Trash)
                    .ghost()
                    .tooltip(control.label(false))
                    .accessibility_label(control.label(false))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.request_discard_recording(cx);
                    }))
                    .into_any_element(),
                RecordControl::Send => Button::new("send-record")
                    .icon(gpui_kit::assets::IconName::Send)
                    .primary()
                    .rounded_full()
                    .tooltip(control.label(false))
                    .accessibility_label(control.label(false))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.send_recording(window, cx);
                    }))
                    .into_any_element(),
            });
        }
        row.into_any_element()
    }
}
