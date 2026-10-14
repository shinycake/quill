//! `editMessageMedia` bodies for "Edit Cover" (TDLib `inputVideo.cover`,
//! schema: "pass null to skip cover uploading; not supported in secret
//! chats and for self-destructing messages").

use super::*;
use crate::composer::ExistingVideo;
use serde_json::{Value, json};

/// Put a local image on a video `inputMessageVideo` as its cover.
pub fn with_video_cover(mut content: Value, cover_path: &str) -> Value {
    crate::json_access::set_path(
        &mut content,
        &["video", "cover"],
        json!({
            "@type": "inputFileLocal",
            "path": cover_path
        }),
    );
    content
}

/// `inputMessageVideo` that resends the video already on the message by
/// file id (`inputFileId`) with a new cover, keeping its metadata, start
/// timestamp and spoiler.
pub fn existing_video_cover_content(
    video: &ExistingVideo,
    cover_path: &str,
    caption: &str,
    caption_above: bool,
    strip_blockquote: bool,
) -> Value {
    let mut content = input_message_video(
        "",
        &VideoSend {
            duration: video.duration,
            width: video.width,
            height: video.height,
            supports_streaming: video.supports_streaming,
            self_destruct: None,
        },
        caption,
        caption_above,
        strip_blockquote,
    );
    crate::json_access::set_path(
        &mut content,
        &["video", "video"],
        json!({
            "@type": "inputFileId",
            "id": video.file_id.0
        }),
    );
    crate::json_access::set_path(
        &mut content,
        &["video", "start_timestamp"],
        json!(video.start_timestamp),
    );
    crate::json_access::set(&mut content, "has_spoiler", Value::Bool(video.has_spoiler));
    with_video_cover(content, cover_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ChatId, FileId, MessageId, RequestId};

    fn video() -> ExistingVideo {
        ExistingVideo {
            file_id: FileId(77),
            duration: 12,
            width: 1280,
            height: 720,
            supports_streaming: true,
            start_timestamp: 4,
            has_spoiler: true,
        }
    }

    #[test]
    fn existing_video_cover_resends_by_file_id() {
        let content = existing_video_cover_content(&video(), "/tmp/cover.jpg", "cap", true, false);
        let json = edit_message_media(RequestId(3), ChatId(-100), MessageId(9), content);
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "editMessageMedia");
        assert_eq!(v["chat_id"], -100);
        assert_eq!(v["message_id"], 9);
        let content = &v["input_message_content"];
        assert_eq!(content["@type"], "inputMessageVideo");
        assert_eq!(content["video"]["video"]["@type"], "inputFileId");
        assert_eq!(content["video"]["video"]["id"], 77);
        assert_eq!(content["video"]["cover"]["@type"], "inputFileLocal");
        assert_eq!(content["video"]["cover"]["path"], "/tmp/cover.jpg");
        assert_eq!(content["video"]["start_timestamp"], 4);
        assert_eq!(content["video"]["duration"], 12);
        assert_eq!(content["video"]["width"], 1280);
        assert_eq!(content["caption"]["text"], "cap");
        assert_eq!(content["show_caption_above_media"], true);
        assert_eq!(content["has_spoiler"], true);
        assert_eq!(content["self_destruct_type"], Value::Null);
    }

    #[test]
    fn replacement_video_gets_the_cover() {
        let base = input_message_video(
            "/tmp/new.mp4",
            &VideoSend {
                duration: 1,
                width: 2,
                height: 3,
                supports_streaming: false,
                self_destruct: None,
            },
            "",
            false,
            false,
        );
        assert_eq!(base["video"]["cover"], Value::Null);
        let content = with_video_cover(base, "/tmp/c.png");
        assert_eq!(content["video"]["video"]["path"], "/tmp/new.mp4");
        assert_eq!(content["video"]["cover"]["path"], "/tmp/c.png");
    }
}
