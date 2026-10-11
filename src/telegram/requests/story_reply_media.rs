//! Story replies that carry a sticker, GIF or voice note
//! (`inputMessageReplyToStory`, TDLib 1.8.68 `schema/td_api.tl:3099`).
//!
//! Telegram Desktop's story reply field (`media_stories_reply.cpp`) is a
//! full compose control, so a sticker, a saved GIF or a voice message sent
//! from it goes to the poster chat quoting the story. The ordinary send
//! builders already produce the right `sendMessage` for each content type;
//! these wrappers only swap the `reply_to` for the story target.

use super::stories::input_message_reply_to_story;
use super::{
    AnimationSend, StickerSend, VideoNoteSend, VoiceNoteSend, send_animation, send_sticker,
    send_video_note, send_voice_note,
};
use crate::ids::{ChatId, RequestId};
use serde_json::Value;

/// Which story a reply points at: the poster chat and the story id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoryReplyTarget {
    pub poster_chat_id: ChatId,
    pub story_id: i32,
}

/// Replace `reply_to` of a built `sendMessage` with the story target.
/// A forum topic and a message reply make no sense on a story reply, so the
/// result carries neither.
fn retarget(json: String, target: StoryReplyTarget) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(&json) else {
        return json;
    };
    value["reply_to"] = input_message_reply_to_story(target.poster_chat_id, target.story_id);
    value["topic_id"] = Value::Null;
    value.to_string()
}

/// `sendMessage` + `inputMessageSticker`, replying to a story.
pub fn send_sticker_story_reply(
    extra: RequestId,
    chat_id: ChatId,
    target: StoryReplyTarget,
    sticker: StickerSend<'_>,
) -> String {
    let sticker = StickerSend {
        reply_to: None,
        topic_id: None,
        ..sticker
    };
    retarget(send_sticker(extra, chat_id, sticker), target)
}

/// `sendMessage` + `inputMessageAnimation` (a saved GIF), replying to a story.
pub fn send_animation_story_reply(
    extra: RequestId,
    chat_id: ChatId,
    target: StoryReplyTarget,
    animation: AnimationSend,
) -> String {
    let animation = AnimationSend {
        reply_to: None,
        topic_id: None,
        ..animation
    };
    retarget(send_animation(extra, chat_id, animation), target)
}

/// `sendMessage` + `inputMessageVoiceNote`, replying to a story. "Play once"
/// is not offered on a story reply, so the self-destruct type is dropped.
pub fn send_voice_note_story_reply(
    extra: RequestId,
    chat_id: ChatId,
    target: StoryReplyTarget,
    voice: VoiceNoteSend<'_>,
) -> String {
    let voice = VoiceNoteSend {
        reply_to: None,
        topic_id: None,
        self_destruct: None,
        ..voice
    };
    retarget(send_voice_note(extra, chat_id, voice), target)
}

/// `sendMessage` + `inputMessageVideoNote` (a round video), replying to a story.
pub fn send_video_note_story_reply(
    extra: RequestId,
    chat_id: ChatId,
    target: StoryReplyTarget,
    path: &str,
    note: &VideoNoteSend,
) -> String {
    retarget(
        send_video_note(extra, chat_id, None, path, note, None),
        target,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FileId, MessageId};
    use crate::telegram::SendReply;

    fn target() -> StoryReplyTarget {
        StoryReplyTarget {
            poster_chat_id: ChatId(7),
            story_id: 6,
        }
    }

    fn parse(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn sticker_reply_targets_the_story() {
        let json = send_sticker_story_reply(
            RequestId(5),
            ChatId(7),
            target(),
            StickerSend {
                file_id: FileId(40),
                emoji: "x",
                width: 512,
                height: 512,
                thumb: None,
                // A stale message reply must not survive.
                reply_to: Some(SendReply {
                    message_id: MessageId(9),
                    quote: None,
                    source_chat: None,
                }),
                topic_id: Some(3),
            },
        );
        let v = parse(&json);
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToStory");
        assert_eq!(v["reply_to"]["story_poster_chat_id"], 7);
        assert_eq!(v["reply_to"]["story_id"], 6);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageSticker");
    }

    #[test]
    fn animation_reply_targets_the_story() {
        let json = send_animation_story_reply(
            RequestId(6),
            ChatId(7),
            target(),
            AnimationSend {
                file_id: FileId(41),
                duration: 3,
                width: 320,
                height: 240,
                reply_to: None,
                topic_id: None,
            },
        );
        let v = parse(&json);
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToStory");
        assert_eq!(v["reply_to"]["story_id"], 6);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageAnimation");
    }

    #[test]
    fn video_note_reply_targets_the_story() {
        let json = send_video_note_story_reply(
            RequestId(8),
            ChatId(7),
            target(),
            "/tmp/n.mp4",
            &VideoNoteSend {
                duration: 3,
                length: 384,
                thumbnail: None,
            },
        );
        let v = parse(&json);
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToStory");
        assert_eq!(v["reply_to"]["story_id"], 6);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageVideoNote");
    }

    #[test]
    fn voice_reply_targets_the_story_and_drops_play_once() {
        let json = send_voice_note_story_reply(
            RequestId(7),
            ChatId(7),
            target(),
            VoiceNoteSend {
                path: "/tmp/v.ogg",
                duration: 4,
                waveform_b64: "AAA",
                caption: "",
                reply_to: None,
                topic_id: None,
                self_destruct: Some(super::super::SelfDestructSend::Immediately),
            },
        );
        let v = parse(&json);
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToStory");
        assert_eq!(v["reply_to"]["story_poster_chat_id"], 7);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageVoiceNote");
        assert_eq!(
            v["input_message_content"]["self_destruct_type"],
            Value::Null
        );
    }
}
