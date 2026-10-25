//! voice/video-note recording.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::demo::demo_file_json;
use super::message_media::waveform_row;
use super::recording_state::{CaptureFor, CaptureMedia};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::ComposerReplyTo;
use quill::diagnostics::DiagnosticSink;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::playback::PlaybackClock;
use quill::send_rights::SendKind;
use quill::telegram::client::copy_and_parse;
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
            RecordMode::Audio => tr(keys::Q_MISC_CLICK_TO_RECORD_AUDIO_RIGHT),
            RecordMode::Video => tr(keys::Q_MISC_CLICK_TO_RECORD_VIDEO_RIGHT),
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
            .is_some_and(|session| session.settings.media_prefs.prefer_video_mode)
        {
            RecordMode::Video
        } else {
            RecordMode::Audio
        }
    }

    pub(super) fn recording_active(&self) -> bool {
        self.recording.active()
    }

    /// A recording runs or its camera is being started: no other starts.
    pub(super) fn recording_busy(&self) -> bool {
        self.recording.busy()
    }

    /// MED2: right-click on the record button flips audio/video mode
    /// (TGX tap-to-switch, desktop-mapped). Ignored while recording.
    pub(super) fn toggle_record_mode(&mut self, cx: &mut Context<Self>) {
        if self.recording_active() {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MISC_FINISH_THE_RECORDING_FIRST,
            )));
            cx.notify();
            return;
        }
        let next = !matches!(self.record_mode(), RecordMode::Video);
        self.set_media_pref(|prefs| prefs.prefer_video_mode = next, cx);
        self.connection.set_note(StatusNote::silent(format!(
            "{} — {}",
            if next {
                tr(keys::Q_MISC_VIDEO_NOTE_MODE)
            } else {
                tr(keys::Q_MISC_VOICE_NOTE_MODE)
            },
            self.record_mode().hint()
        )));
        cx.notify();
    }

    pub(super) fn start_recording(&mut self, cx: &mut Context<Self>) {
        if quill::agent::ui_driver::guard::refuse_if_injecting("record a voice or video message") {
            return;
        }
        self.recording.lock_next = false;
        match self.record_mode() {
            RecordMode::Audio => self.start_voice_recording(cx),
            RecordMode::Video => self.start_video_note_recording(cx),
        }
    }

    pub(super) fn start_voice_recording(&mut self, cx: &mut Context<Self>) {
        if self.composer_ui.pending_edit.is_some() || self.recording_busy() {
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
        if self.recording_busy() {
            return;
        }
        match VoiceCapture::start() {
            Ok(capture) => {
                self.hold_capture(CaptureMedia::Voice(capture), CaptureFor::Message);
                self.sync_voice_action();
                self.spawn_voice_tick(cx);
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_MISC_RECORDING_VOICE_NOTE)));
            }
            Err(err) => {
                self.connection.set_note(StatusNote::error(err));
            }
        }
        cx.notify();
    }

    /// MED2: start a round video-note camera capture (ffmpeg V4L2, squared
    /// at finish; HQ size from media prefs). Needs a camera, honest error
    /// otherwise.
    pub(super) fn start_video_note_recording(&mut self, cx: &mut Context<Self>) {
        if self.composer_ui.pending_edit.is_some() || self.recording_busy() {
            return;
        }
        // Phase S1: round video notes need secret-chat layer ≥ 66 (TGX
        // `chatSupportsRoundVideos`).
        if let Some((name, layer)) = self.open_secret_chat_peer_layer()
            && layer < 66
        {
            self.connection.set_note(StatusNote::info(tr_with(
                keys::Q_MISC_S_TELEGRAM_CLIENT_DOESNT_SUPPORT,
                &[("name", &name.to_string())],
            )));
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
        let lock = std::mem::take(&mut self.recording.lock_next);
        self.start_video_capture(CaptureFor::Message, lock, cx);
    }

    /// A capture that stopped by itself: a video message that reached the
    /// 60 s limit is sent (as tdesktop does); a failed start (no camera or
    /// microphone access) ends the recording with the reason.
    pub(super) fn check_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(ended) = self.recording.video_mut().and_then(|c| c.ended()) {
            match ended {
                Ok(()) => self.send_recording_at_time_limit(cx),
                Err(reason) => self.fail_recording(reason, cx),
            }
            return;
        }
        if let Some(reason) = self.recording.voice_mut().and_then(|c| c.failure()) {
            self.fail_recording(reason, cx);
        }
    }

    fn fail_recording(&mut self, reason: String, cx: &mut Context<Self>) {
        self.cancel_recording(cx);
        self.connection.set_note(StatusNote::error(reason));
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
        self.connection.set_note(if self.recording.locked {
            StatusNote::silent(tr(keys::Q_MISC_RECORDING_LOCKED_ESC_WONT_CANCEL))
        } else {
            StatusNote::silent(tr(keys::Q_MISC_RECORDING_UNLOCKED))
        });
        cx.notify();
    }

    /// Pause the voice recording, or carry on after a pause. Pausing
    /// releases the microphone; resuming stops any preview first.
    pub(super) fn toggle_record_pause(&mut self, cx: &mut Context<Self>) {
        let Some(capture) = self.recording.voice_mut() else {
            return;
        };
        if capture.is_paused() {
            capture.resume();
            self.stop_record_preview();
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MISC_RECORDING_VOICE_NOTE)));
        } else {
            capture.pause();
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MISC_RECORDING_PAUSED)));
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
        let Some(capture) = self.recording.voice() else {
            return;
        };
        let Some(path) = capture.preview_path().map(std::path::Path::to_path_buf) else {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MISC_GETTING_THE_RECORDING_READY,
            )));
            cx.notify();
            return;
        };
        let seconds = f64::from(capture.elapsed_secs());
        let mut resumed_preview = false;
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
                    resumed_preview = true;
                }
            }
        }
        if resumed_preview {
            self.sound_starts(cx);
            cx.notify();
            return;
        }
        // Whatever else was playing gives way to the preview.
        self.sound_starts(cx);
        self.stop_voice_playback();
        self.stop_audio_playback();
        if self.start_player(&path, 0.0, seconds) {
            let mut clock = PlaybackClock::new(seconds);
            clock.resume();
            self.recording.preview = Some(clock);
            self.spawn_voice_tick(cx);
        } else {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MISC_COULDNT_PLAY_THE_RECORDING,
            )));
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
        self.connection.set_note(if self.recording.once {
            StatusNote::info(tr(keys::RECORD_ONCE_ACTIVE_TOOLTIP))
        } else {
            StatusNote::silent(tr(keys::Q_MISC_PLAY_ONCE_OFF))
        });
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
        let was_video = self.recording.video().is_some();
        match self.recording.take() {
            Some(CaptureMedia::Voice(capture)) => capture.discard(),
            Some(CaptureMedia::Video(capture)) => capture.discard(),
            None => {}
        }
        self.recording.locked = false;
        self.recording.discard_confirm = false;
        self.sync_voice_action();
        self.connection.set_note(if was_video {
            StatusNote::silent(tr(keys::Q_MISC_VIDEO_RECORDING_CANCELLED))
        } else {
            StatusNote::silent(tr(keys::Q_MISC_VOICE_RECORDING_CANCELLED))
        });
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
        let locked = self.session().is_some_and(|s| {
            quill::state::speech_trial_locked(
                s.sync.speech_trial,
                s.my_is_premium(),
                quill::local_time::now_unix(),
            )
        });
        if locked {
            // tdesktop `ShowPremiumPreviewBox(PremiumFeature::VoiceToText)`.
            self.open_premium_at(quill::premium_hub::PremiumFeature::VoiceRecognition, cx);
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.recognize_speech(chat_id, message_id) {
                Ok(_) => self
                    .connection
                    .set_note(StatusNote::silent(tr(keys::Q_MISC_TRANSCRIPTION_REQUESTED))),
                Err(_) => self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MISC_COULDNT_REQUEST_TRANSCRIPTION,
                ))),
            }
        } else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_MISC_DEMO_TRANSCRIPTION_NEEDS_A_LIVE,
            )));
        }
        cx.notify();
    }

    /// Phase 6: write the requested HQ value (was: flip), keeping the
    /// status-note feedback the toggle gave.
    pub(super) fn set_hq_round_videos(&mut self, on: bool, cx: &mut Context<Self>) {
        self.set_media_pref(|prefs| prefs.hq_round_videos = on, cx);
        self.connection.set_note(if on {
            StatusNote::silent(tr(keys::Q_MISC_HQ_ROUND_VIDEOS_ON_480PX))
        } else {
            StatusNote::silent(tr(keys::Q_MISC_HQ_ROUND_VIDEOS_OFF_280PX))
        });
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
        // Past the session's last update, or it is dropped as one already
        // applied (`apply_demo_outgoing` does the same).
        self.demo_ui
            .seq
            .fetch_max(session.last_seq, std::sync::atomic::Ordering::Relaxed);
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        // After the newest loaded message, so it lands at the bottom.
        let id = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.keys().next_back().copied())
            .map_or(1, |last| last.max(0) + 1);
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
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    /// One tick of the running recording (every 100 ms), with its window's
    /// state in place: a video message at its limit is queued to send, the
    /// preview and the waveform advance, the chat action is refreshed.
    /// `false` once nothing records any more.
    pub(super) fn recording_tick(&mut self, cx: &mut Context<Self>) -> bool {
        self.check_recording(cx);
        self.check_record_preview();
        let recording = self.recording_active();
        if let Some(capture) = self.recording.voice_mut() {
            capture.sample_bar();
        }
        if recording {
            self.sync_voice_action();
            cx.notify();
        }
        recording
    }

    /// While a recording bar is active, sync `chatActionRecordingVoiceNote`
    /// / `chatActionRecordingVideoNote` to the open chat (Unigram record
    /// actions). Cancels when recording stops.
    pub(super) fn sync_voice_action(&mut self) {
        let voice = self
            .recording
            .voice()
            .is_some_and(|capture| !capture.is_paused());
        let video = self.recording.video().is_some();
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
        let video = self.recording.video().is_some();
        let seconds = if video {
            self.recording
                .video()
                .map(|capture| capture.elapsed_secs())
                .unwrap_or(0)
        } else {
            self.recording
                .voice()
                .map(|capture| capture.elapsed_secs())
                .unwrap_or(0)
        };
        let bars = self
            .recording
            .voice()
            .map(|capture| capture.bars.clone())
            .unwrap_or_default();
        let paused = !video
            && self
                .recording
                .voice()
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
            if paused {
                tr(keys::Q_MISC_PAUSED_2)
            } else {
                tr(keys::Q_MISC_RECORDING)
            },
            if video {
                tr(keys::Q_MISC_VIDEO_MESSAGE)
            } else {
                tr(keys::Q_MISC_VOICE_MESSAGE)
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
                            .child(tr(keys::Q_MISC_VIDEO_MESSAGE_2))
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
                .child(div().text_sm().child(tr(keys::Q_MISC_DISCARD_RECORDING)))
                .child(
                    Button::new("discard-record-confirm")
                        .label(tr(keys::Q_MISC_DISCARD))
                        .danger()
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_discard_recording(cx);
                        })),
                )
                .child(
                    Button::new("keep-recording")
                        .label(tr(keys::Q_MISC_KEEP))
                        .ghost()
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.recording.discard_confirm = false;
                            cx.notify();
                        })),
                )
                .into_any_element();
        }
        let video = self.recording.video().is_some();
        let paused = !video
            && self
                .recording
                .voice()
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
                        tr(keys::Q_MISC_LOCKED_ESC_WONT_CANCEL_CLICK)
                    } else {
                        tr(keys::Q_MISC_LOCK_HANDS_FREE_RECORDING_ESC)
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
                    .label(tr(keys::Q_MISC_PLAY_ONCE))
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
