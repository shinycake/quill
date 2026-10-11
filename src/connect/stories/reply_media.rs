//! Connect driver: story replies that carry a sticker, GIF or voice note
//! (`inputMessageReplyToStory`; tdesktop `media_stories_reply.cpp` uses the
//! full compose control for the story reply field).
use super::*;
use crate::telegram::requests::{
    AnimationSend, StickerSend, StoryReplyTarget, VideoNoteSend, VideoNoteThumbnailSend,
    VoiceNoteSend, send_animation_story_reply, send_sticker_story_reply,
    send_video_note_story_reply, send_voice_note_story_reply,
};
use crate::voice::VoiceDraft;

impl<S: JsonSender> ConnectDriver<S> {
    /// The reply target when the story can take a reply and the poster chat
    /// accepts sends; `None` otherwise.
    fn story_reply_media_target(&self, chat_id: ChatId, story_id: i32) -> Option<StoryReplyTarget> {
        if !self.chats_path_active() {
            return None;
        }
        let repliable = self
            .session
            .stories
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_be_replied);
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        (repliable && supported).then_some(StoryReplyTarget {
            poster_chat_id: chat_id,
            story_id,
        })
    }

    fn send_story_reply_json(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        let extra =
            self.session
                .request_for_story(RequestPurpose::SendStoryReply, chat_id, story_id);
        let json = build(extra);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Reply to a story with a sticker.
    pub fn send_story_reply_sticker(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        sticker: StickerSend<'_>,
    ) -> Result<RequestId, ConnectSendError> {
        let target = self
            .story_reply_media_target(chat_id, story_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        if sticker.file_id.0 <= 0
            || (self.session.sticker_requires_premium(sticker.file_id)
                && !self.session.my_is_premium())
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_story_reply_json(chat_id, story_id, |extra| {
            send_sticker_story_reply(extra, chat_id, target, sticker)
        })
    }

    /// Reply to a story with a saved GIF.
    pub fn send_story_reply_animation(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        animation: AnimationSend,
    ) -> Result<RequestId, ConnectSendError> {
        let target = self
            .story_reply_media_target(chat_id, story_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        if animation.file_id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_story_reply_json(chat_id, story_id, |extra| {
            send_animation_story_reply(extra, chat_id, target, animation)
        })
    }

    /// Reply to a story with a finished voice recording.
    pub fn send_story_reply_voice(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        draft: &VoiceDraft,
    ) -> Result<RequestId, ConnectSendError> {
        let target = self
            .story_reply_media_target(chat_id, story_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let path = crate::local_path::pick_send_path(&draft.path)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let path = path.to_string_lossy().into_owned();
        let waveform = draft.waveform_b64();
        self.send_story_reply_json(chat_id, story_id, |extra| {
            send_voice_note_story_reply(
                extra,
                chat_id,
                target,
                VoiceNoteSend {
                    path: &path,
                    duration: draft.duration_secs,
                    waveform_b64: &waveform,
                    caption: "",
                    reply_to: None,
                    topic_id: None,
                    self_destruct: None,
                },
            )
        })
    }

    /// Reply to a story with a finished round video recording.
    pub fn send_story_reply_video_note(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        draft: &crate::video::VideoNoteDraft,
    ) -> Result<RequestId, ConnectSendError> {
        let target = self
            .story_reply_media_target(chat_id, story_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let path = crate::local_path::pick_send_path(&draft.path)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let path = path.to_string_lossy().into_owned();
        let thumbnail = crate::video::write_video_note_thumbnail(&draft.path).map(|thumb| {
            VideoNoteThumbnailSend {
                path: thumb.path.to_string_lossy().into_owned(),
                width: thumb.width,
                height: thumb.height,
            }
        });
        let note = VideoNoteSend {
            duration: draft.duration_secs,
            length: draft.length,
            thumbnail,
        };
        self.send_story_reply_json(chat_id, story_id, |extra| {
            send_video_note_story_reply(extra, chat_id, target, &path, &note)
        })
    }
}
