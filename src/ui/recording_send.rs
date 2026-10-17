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
use super::recording_state::{CaptureFor, CaptureMedia};
use super::status_note::StatusNote;
use gpui_kit::*;
use quill::composer::{AttachmentKind, ComposerAttachment, ComposerReplyTo};
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::telegram::SendReply;
use quill::video::{VideoNoteCapture, VideoNoteDraft};
use quill::voice::VoiceDraft;

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

    /// Empty the composer once the recording went out, when it is still
    /// the target chat's.
    fn clear_composer_after_recording(
        &mut self,
        target: &SendTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !target.is_open(self.open_chat_id()) {
            return;
        }
        self.composer_ui.set_reply(None);
        self.composer
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.forget_local_draft(target.chat_id);
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
                if target.is_open(self.open_chat_id()) {
                    self.composer_ui.clear_draft_on_success = Some(target.chat_id);
                }
                self.clear_composer_after_recording(target, window, cx);
            } else {
                let _ = std::fs::remove_file(&draft.path);
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MISC_COULD_NOT_SEND_VOICE_NOTE,
                )));
            }
        } else if self.demo_session.is_some() {
            if target.is_open(self.open_chat_id()) {
                self.apply_demo_voice(&draft, &target.caption, target.reply.as_ref());
                self.clear_composer_after_recording(target, window, cx);
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_MISC_DEMO_VOICE_NOTE_APPLIED_LOCALLY,
                )));
            } else {
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_NOTES_VOICE_NOTE_NOT_SENT_WINDOW_CLOSED,
                )));
            }
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
                if target.is_open(self.open_chat_id()) {
                    self.composer_ui.clear_draft_on_success = Some(target.chat_id);
                }
                self.clear_composer_after_recording(target, window, cx);
            } else {
                let _ = std::fs::remove_file(&draft.path);
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MISC_COULD_NOT_SEND_VIDEO_NOTE,
                )));
            }
        } else if self.demo_session.is_some() {
            let attachment = ComposerAttachment::pick(&draft.path, AttachmentKind::VideoNote);
            match attachment.filter(|_| target.is_open(self.open_chat_id())) {
                Some(att) => {
                    self.apply_demo_outgoing("", Some(&att), target.reply.as_ref());
                    self.clear_composer_after_recording(target, window, cx);
                    self.connection.set_note(StatusNote::silent(tr(
                        keys::Q_MISC_DEMO_VIDEO_NOTE_APPLIED_LOCALLY,
                    )));
                }
                None => {
                    self.connection.set_note(StatusNote::silent(tr(
                        keys::Q_MISC_DEMO_RECORDED_CLIP_IS_OUTSIDE,
                    )));
                }
            }
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
        self.recording.starting = true;
        let starting = cx.background_spawn(async move { VideoNoteCapture::start(hq, &choice) });
        self.spawn_scoped(cx, async move |this, cx| {
            let result = starting.await;
            let held = this.update(cx, |this, cx| {
                this.recording.starting = false;
                this.hold_started_video(result, purpose, lock, cx);
            });
            if held.is_err() {
                let _ = this.main().update(cx, |this, _| {
                    this.recording.starting = false;
                });
            }
        })
        .detach();
    }

    fn hold_started_video(
        &mut self,
        result: Result<VideoNoteCapture, String>,
        purpose: CaptureFor,
        lock: bool,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(capture) if !self.recording_active() => {
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
            // Another recording took the place meanwhile.
            Ok(capture) => capture.discard(),
            Err(err) => self.connection.set_note(StatusNote::error(err)),
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::SendTarget;
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
    fn the_reply_belongs_to_the_target_chat_only() {
        let target = SendTarget::new(ChatId(1), Some(reply_in(1)), "");
        assert_eq!(target.reply_to().map(|r| r.message_id), Some(MessageId(40)));
        let elsewhere = SendTarget::new(ChatId(2), Some(reply_in(1)), "");
        assert!(elsewhere.reply_to().is_none());
    }
}
