//! Media spoilers (tdesktop `MediaSpoiler`, `history_view_media_spoiler.h`):
//! a photo, video or GIF sent with a spoiler stays covered until clicked.
//! Once revealed it renders, opens and plays exactly like plain media, so
//! the history draws the unveiled content instead of special-casing each
//! surface.

use crate::telegram::envelope::MessageContent;

/// `content` is a photo, video or GIF sent behind a spoiler.
pub fn has_media_spoiler(content: &MessageContent) -> bool {
    match content {
        MessageContent::Photo(photo) => photo.has_spoiler,
        MessageContent::Video(video) => video.has_spoiler,
        MessageContent::Animation(animation) => animation.has_spoiler,
        _ => false,
    }
}

/// `content` as it renders once its spoiler is revealed: the same media
/// without the spoiler flag. `None` when it has no spoiler to lift.
pub fn unveiled(content: &MessageContent) -> Option<MessageContent> {
    let mut plain = content.clone();
    match &mut plain {
        MessageContent::Photo(photo) if photo.has_spoiler => photo.has_spoiler = false,
        MessageContent::Video(video) if video.has_spoiler => video.has_spoiler = false,
        MessageContent::Animation(animation) if animation.has_spoiler => {
            animation.has_spoiler = false;
        }
        _ => return None,
    }
    Some(plain)
}

#[cfg(test)]
mod tests {
    use super::{has_media_spoiler, unveiled};
    use crate::ids::FileId;
    use crate::telegram::envelope::{MessageContent, TextContent, VideoContent};

    fn video(spoiler: bool, secret: bool) -> MessageContent {
        MessageContent::Video(VideoContent {
            duration: 5,
            width: 640,
            height: 360,
            file_name: "clip.mp4".to_string(),
            mime_type: "video/mp4".to_string(),
            caption: String::new(),
            caption_entities: Vec::new(),
            show_caption_above_media: false,
            has_spoiler: spoiler,
            is_secret: secret,
            start_timestamp: 0,
            supports_streaming: true,
            has_stickers: false,
            file_id: FileId(7),
            thumb_file_id: None,
            thumb_width: 0,
            thumb_height: 0,
            minithumbnail: None,
            thumb_still: true,
            alternatives: Vec::new(),
        })
    }

    #[test]
    fn a_revealed_spoiler_video_is_plain_video() {
        let covered = video(true, false);
        assert!(has_media_spoiler(&covered));
        let plain = unveiled(&covered).expect("a spoiler video unveils");
        assert!(!has_media_spoiler(&plain));
        assert!(matches!(plain, MessageContent::Video(ref v) if !v.is_secret && v.duration == 5));
    }

    #[test]
    fn unveiling_keeps_a_secret_video_secret() {
        let plain = unveiled(&video(true, true)).expect("a spoiler video unveils");
        assert!(matches!(plain, MessageContent::Video(v) if v.is_secret));
    }

    #[test]
    fn content_without_a_spoiler_has_nothing_to_unveil() {
        assert!(unveiled(&video(false, false)).is_none());
        assert!(unveiled(&MessageContent::Text(TextContent::plain("hi"))).is_none());
        assert!(!has_media_spoiler(&MessageContent::Text(
            TextContent::plain("hi")
        )));
    }
}
