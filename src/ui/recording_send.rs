//! Starting a round video and sending a finished recording.
//!
//! Finishing a recording takes time off the UI thread (ffmpeg ends the
//! file, the Opus encoder is joined), and the user can open another chat
//! meanwhile. The chat, reply target and caption are therefore fixed when
//! Send is tapped ([`SendTarget`]) and travel with the background job: the
//! recording goes to the chat it was sent from, never to whichever chat is
//! open when the file is ready.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::composer_state::DraftClear;
use super::recording_state::{CaptureFor, CaptureMedia, StartId};
use super::status_note::StatusNote;
use gpui_kit::*;
use quill::composer::{AttachmentKind, ComposerAttachment, ComposerReplyTo};
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::telegram::SendReply;
use quill::video::{VideoNoteCapture, VideoNoteDraft};
use quill::voice::{VoiceCapture, VoiceDraft};

/// Where a recording goes, fixed when Send is tapped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SendTarget {
    chat_id: ChatId,
    reply: Option<ComposerReplyTo>,
    caption: String,
}

impl SendTarget {
    pub(super) fn new(chat_id: ChatId, reply: Option<ComposerReplyTo>, caption: &str) -> Self {
        Self {
            chat_id,
            reply,
            caption: caption.trim().to_owned(),
        }
    }

    /// The reply of the request: the pending reply when it belongs to the
    /// target chat.
    fn reply_to(&self) -> Option<SendReply> {
        self.reply.as_ref()?.send_target(self.chat_id)
    }

    /// The composer on screen is the target chat's, so it holds its
    /// reply, draft and caption.
    fn is_open(&self, open: Option<ChatId>) -> bool {
        open == Some(self.chat_id)
    }

    /// What a failed send puts back into the composer: only into the
    /// target chat's own composer, and only into a part the user has not
    /// touched since Send (text typed or a reply picked meanwhile stays).
    fn restore_plan(
        &self,
        open: Option<ChatId>,
        composer_text: &str,
        reply: Option<&ComposerReplyTo>,
    ) -> Restore {
        if !self.is_open(open) {
            return Restore::default();
        }
        Restore {
            caption: !self.caption.is_empty() && composer_text.trim().is_empty(),
            reply: self.reply.is_some() && reply.is_none(),
        }
    }
}

/// Which parts of a failed send go back into the composer.
#[derive(Debug, Default, PartialEq, Eq)]
struct Restore {
    caption: bool,
    reply: bool,
}

impl QuillApp {
    /// Fix the target of a send from the open chat, or `None` when slow
    /// mode holds it back (the recording stays until the timer expires).
    fn begin_recording_send(
        &mut self,
        caption: &str,
        cx: &mut Context<Self>,
    ) -> Option<SendTarget> {
        let chat_id = self.open_chat_id()?;
        if self.slow_mode_blocked(chat_id, cx) {
            return None;
        }
        let reply = self.reply_for_send(cx);
        Some(SendTarget::new(chat_id, reply, caption))
    }

    /// The recording consumes the composer when Send is tapped, not when
    /// the file is ready: text typed or a reply picked while it is
    /// finished belongs to the next message, and Enter in that window must
    /// not send the caption again. A voice message takes the whole
    /// composer as its caption; a round video takes the reply only.
    fn consume_composer_for_send(
        &mut self,
        target: &SendTarget,
        take_text: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if take_text {
            self.composer_ui.set_reply(None);
            self.composer
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.forget_local_draft(target.chat_id);
        } else {
            self.consume_sent_reply(target.chat_id, cx);
        }
    }

    /// The send failed: put the caption and reply back unless the user
    /// started something new in the meantime.
    fn restore_composer_after_failed_send(
        &mut self,
        target: &SendTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.composer.read(cx).value().to_string();
        let plan = target.restore_plan(self.open_chat_id(), &text, self.composer_ui.reply());
        if plan.reply {
            self.composer_ui.set_reply(target.reply.clone());
        }
        if plan.caption {
            self.set_composer_markup(&target.caption, window, cx);
        }
        if plan.reply || plan.caption {
            self.note_open_draft(false, cx);
        }
    }

    /// The recording bar's Send for a voice message from the window whose
    /// state is in place.
    pub(super) fn send_voice_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.send_story_reply_voice_recording(cx) {
            return;
        }
        let caption = self.composer_markup(cx);
        let Some(target) = self.begin_recording_send(&caption, cx) else {
            return;
        };
        let Some(capture) = self.recording.take_voice() else {
            return;
        };
        // The send consumes the recording: locked state must not leak into
        // the next one.
        self.recording.locked = false;
        self.recording.discard_confirm = false;
        self.stop_record_preview();
        self.consume_composer_for_send(&target, true, window, cx);
        let play_once = std::mem::take(&mut self.recording.once);
        let finish = capture.close_microphone();
        self.sync_voice_action();
        cx.notify();
        // The encoder is joined off the UI thread.
        let finished = cx.background_spawn(async move { finish.complete() });
        self.spawn_scoped_in(window, cx, async move |this, cx| {
            let result = finished.await;
            let path = result.as_ref().ok().map(|draft| draft.path.clone());
            let sent = this.update_in(cx, |this, window, cx| {
                this.send_finished_voice(result, &target, play_once, window, cx);
            });
            if sent.is_err() {
                if let Some(path) = path {
                    let _ = std::fs::remove_file(path);
                }
                let _ = this.main().update(cx, |this, cx| {
                    this.connection.set_note(StatusNote::silent(tr(
                        keys::Q_NOTES_VOICE_NOTE_NOT_SENT_WINDOW_CLOSED,
                    )));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn send_finished_voice(
        &mut self,
        result: Result<VoiceDraft, String>,
        target: &SendTarget,
        play_once: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let draft = match result {
            Ok(draft) => draft,
            Err(err) => {
                self.restore_composer_after_failed_send(target, window, cx);
                self.connection.set_note(StatusNote::error(err));
                cx.notify();
                return;
            }
        };
        if let Some(live) = self.live.as_mut() {
            let sent = live.driver.send_voice_note_to(
                target.chat_id,
                &draft,
                &target.caption,
                target.reply_to(),
                play_once,
            );
            if sent.is_ok() {
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_MISC_SENDING_VOICE_NOTE)));
                self.composer_ui.clear_draft_on_success =
                    Some(DraftClear::whole_composer(target.chat_id));
            } else {
                let _ = std::fs::remove_file(&draft.path);
                self.restore_composer_after_failed_send(target, window, cx);
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MISC_COULD_NOT_SEND_VOICE_NOTE,
                )));
            }
        } else if self.demo_session.is_some() {
            if target.is_open(self.open_chat_id()) {
                self.apply_demo_voice(&draft, &target.caption, target.reply.as_ref());
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_MISC_DEMO_VOICE_NOTE_APPLIED_LOCALLY,
                )));
            } else {
                let _ = std::fs::remove_file(&draft.path);
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_NOTES_RECORDING_NOT_SENT_CHAT_CHANGED,
                )));
            }
        } else {
            let _ = std::fs::remove_file(&draft.path);
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_NOTES_RECORDING_NOT_SENT_NO_ACCOUNT,
            )));
        }
        cx.notify();
    }

    /// Send the finished recording, whichever mode is active, from the
    /// window whose state is in place (`send_recording` puts the
    /// recording's own window in place first).
    pub(super) fn send_recording_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.recording.video().is_some() {
            self.send_video_note_recording(window, cx);
        } else {
            self.send_voice_recording(window, cx);
        }
    }

    /// Finish the round video capture and send it (`inputMessageVideoNote`).
    /// Slow-mode gate first, the capture is only consumed when the send
    /// goes ahead, demo mode routes locally.
    pub(super) fn send_video_note_recording(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.story_reply_video_recording(window, cx) {
            return;
        }
        let Some(target) = self.begin_recording_send("", cx) else {
            return;
        };
        let Some(capture) = self.recording.take_video() else {
            return;
        };
        self.recording.once = false;
        self.recording.locked = false;
        self.recording.discard_confirm = false;
        self.consume_composer_for_send(&target, false, window, cx);
        self.sync_voice_action();
        cx.notify();
        // ffmpeg finishes the file after the stop signal: off the main thread.
        let finished = cx.background_spawn(async move { capture.finish() });
        self.spawn_scoped_in(window, cx, async move |this, cx| {
            let result = finished.await;
            let path = result.as_ref().ok().map(|draft| draft.path.clone());
            let sent = this.update_in(cx, |this, window, cx| {
                this.send_finished_video_note(result, &target, window, cx);
            });
            if sent.is_err() {
                // Its window closed while the file was finished: say so,
                // rather than send it to whichever chat is open now.
                if let Some(path) = path {
                    let _ = std::fs::remove_file(path);
                }
                let _ = this.main().update(cx, |this, cx| {
                    this.connection.set_note(StatusNote::silent(tr(
                        keys::Q_MISC_VIDEO_NOTE_NOT_SENT_ITS_WINDOW_CLOSED,
                    )));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn send_finished_video_note(
        &mut self,
        result: Result<VideoNoteDraft, String>,
        target: &SendTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let draft = match result {
            Ok(draft) => draft,
            Err(err) => {
                self.restore_composer_after_failed_send(target, window, cx);
                self.connection.set_note(StatusNote::error(err));
                cx.notify();
                return;
            }
        };
        if let Some(live) = self.live.as_mut() {
            let sent =
                live.driver
                    .send_recorded_video_note_to(target.chat_id, &draft, target.reply_to());
            if sent.is_ok() {
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_MISC_SENDING_VIDEO_NOTE)));
                self.composer_ui.clear_draft_on_success =
                    Some(DraftClear::whole_composer(target.chat_id));
            } else {
                let _ = std::fs::remove_file(&draft.path);
                self.restore_composer_after_failed_send(target, window, cx);
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MISC_COULD_NOT_SEND_VIDEO_NOTE,
                )));
            }
        } else if self.demo_session.is_some() {
            if !target.is_open(self.open_chat_id()) {
                let _ = std::fs::remove_file(&draft.path);
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_NOTES_RECORDING_NOT_SENT_CHAT_CHANGED,
                )));
            } else if let Some(att) =
                ComposerAttachment::pick(&draft.path, AttachmentKind::VideoNote)
            {
                self.apply_demo_outgoing("", Some(&att), target.reply.as_ref());
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_MISC_DEMO_VIDEO_NOTE_APPLIED_LOCALLY,
                )));
            } else {
                let _ = std::fs::remove_file(&draft.path);
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_MISC_DEMO_RECORDED_CLIP_IS_OUTSIDE,
                )));
            }
        } else {
            let _ = std::fs::remove_file(&draft.path);
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_NOTES_RECORDING_NOT_SENT_NO_ACCOUNT,
            )));
        }
        cx.notify();
    }

    /// Start a round video capture for `purpose`. Finding the encoder and
    /// launching ffmpeg wait on the system, so they run off the UI thread:
    /// tapping record never stalls drawing. The capture is held by the
    /// window that asked, or dropped (clip and log removed) once that
    /// window closed.
    pub(super) fn start_video_capture(
        &mut self,
        purpose: CaptureFor,
        lock: bool,
        cx: &mut Context<Self>,
    ) {
        if self.recording_busy() {
            return;
        }
        let hq = self
            .session()
            .is_some_and(|session| session.settings.media_prefs.hq_round_videos);
        let choice = self.capture_choice();
        let start = self.begin_capture_start(true, purpose);
        let starting = cx.background_spawn(async move { VideoNoteCapture::start(hq, &choice) });
        self.spawn_scoped(cx, async move |this, cx| {
            let result = starting.await;
            let held = this.update(cx, |this, cx| {
                this.hold_started_video(result, start, purpose, lock, cx);
            });
            if held.is_err() {
                let _ = this.main().update(cx, |this, _| {
                    this.recording.finish_start(start);
                });
            }
        })
        .detach();
    }

    fn hold_started_video(
        &mut self,
        result: Result<VideoNoteCapture, String>,
        start: StartId,
        purpose: CaptureFor,
        lock: bool,
        cx: &mut Context<Self>,
    ) {
        let wanted = self.recording.finish_start(start);
        match result {
            Ok(capture) if wanted && !self.recording_active() => {
                self.hold_capture(CaptureMedia::Video(capture), purpose);
                self.spawn_voice_tick(cx);
                let note = match purpose {
                    CaptureFor::Message => {
                        self.recording.locked = lock;
                        self.sync_voice_action();
                        keys::Q_MISC_RECORDING_VIDEO_NOTE
                    }
                    CaptureFor::StoryReply => keys::Q_MEDIA_RECORDING_VIDEO_REPLY,
                };
                self.connection.set_note(StatusNote::silent(tr(note)));
            }
            Ok(capture) => {
                cx.background_spawn(async move { capture.discard() })
                    .detach();
            }
            Err(err) if wanted => self.connection.set_note(StatusNote::error(err)),
            Err(_) => {}
        }
        cx.notify();
    }

    pub(super) fn start_voice_capture(&mut self, purpose: CaptureFor, cx: &mut Context<Self>) {
        if self.recording_busy() {
            return;
        }
        let start = self.begin_capture_start(false, purpose);
        let open = self.recording.open_voice.clone();
        let opening = cx.background_spawn(async move { open() });
        self.spawn_scoped(cx, async move |this, cx| {
            let mut result = Some(opening.await);
            let held = this.update(cx, |this, cx| {
                if let Some(result) = result.take() {
                    this.hold_started_voice(result, start, purpose, cx);
                }
            });
            if held.is_err() {
                let _ = this.main().update(cx, |this, _| {
                    this.recording.finish_start(start);
                });
                if let Some(Ok(capture)) = result {
                    cx.background_spawn(async move { capture.discard() })
                        .detach();
                }
            }
        })
        .detach();
    }

    fn hold_started_voice(
        &mut self,
        result: Result<VoiceCapture, String>,
        start: StartId,
        purpose: CaptureFor,
        cx: &mut Context<Self>,
    ) {
        let wanted = self.recording.finish_start(start);
        match result {
            Ok(capture) if wanted => {
                self.hold_capture(CaptureMedia::Voice(capture), purpose);
                self.spawn_voice_tick(cx);
                let note = match purpose {
                    CaptureFor::Message => {
                        self.sync_voice_action();
                        keys::Q_MISC_RECORDING_VOICE_NOTE
                    }
                    CaptureFor::StoryReply => keys::Q_MEDIA_RECORDING_VOICE_REPLY,
                };
                self.connection.set_note(StatusNote::silent(tr(note)));
            }
            Ok(capture) => {
                cx.background_spawn(async move { capture.discard() })
                    .detach();
            }
            Err(err) if wanted => self.connection.set_note(StatusNote::error(err)),
            Err(_) => {}
        }
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "recording_start_tests.rs"]
mod start_tests;

#[cfg(test)]
mod tests {
    use super::{Restore, SendTarget};
    use quill::composer::ComposerReplyTo;
    use quill::ids::{ChatId, MessageId};

    fn reply_in(chat: i64) -> ComposerReplyTo {
        ComposerReplyTo::new(ChatId(chat), MessageId(40))
    }

    #[test]
    fn the_target_keeps_the_chat_it_was_fixed_for() {
        let target = SendTarget::new(ChatId(1), None, "  hi ");
        assert!(target.is_open(Some(ChatId(1))));
        // The user opened another chat while the file was finished.
        assert!(!target.is_open(Some(ChatId(2))));
        assert!(!target.is_open(None));
        assert_eq!(target.chat_id, ChatId(1));
        assert_eq!(target.caption, "hi");
    }

    #[test]
    fn a_failed_send_puts_back_what_the_user_has_not_replaced() {
        let target = SendTarget::new(ChatId(1), Some(reply_in(1)), " caption ");
        let open = Some(ChatId(1));
        // Composer still empty: both come back.
        assert_eq!(
            target.restore_plan(open, "", None),
            Restore {
                caption: true,
                reply: true
            }
        );
        // Text typed and another reply picked after Send stay.
        let newer = reply_in(1);
        assert_eq!(
            target.restore_plan(open, "next", Some(&newer)),
            Restore::default()
        );
        // Only the part left empty comes back.
        assert_eq!(
            target.restore_plan(open, "  ", Some(&newer)),
            Restore {
                caption: true,
                reply: false
            }
        );
    }

    #[test]
    fn a_failed_send_restores_nothing_into_another_chat() {
        let target = SendTarget::new(ChatId(1), Some(reply_in(1)), "caption");
        assert_eq!(
            target.restore_plan(Some(ChatId(2)), "", None),
            Restore::default()
        );
        assert_eq!(target.restore_plan(None, "", None), Restore::default());
    }

    #[test]
    fn a_video_without_caption_restores_only_the_reply() {
        let target = SendTarget::new(ChatId(1), Some(reply_in(1)), "");
        assert_eq!(
            target.restore_plan(Some(ChatId(1)), "", None),
            Restore {
                caption: false,
                reply: true
            }
        );
    }

    #[test]
    fn the_reply_belongs_to_the_target_chat_only() {
        let target = SendTarget::new(ChatId(1), Some(reply_in(1)), "");
        assert_eq!(target.reply_to().map(|r| r.message_id), Some(MessageId(40)));
        let elsewhere = SendTarget::new(ChatId(2), Some(reply_in(1)), "");
        assert!(elsewhere.reply_to().is_none());
    }
}
