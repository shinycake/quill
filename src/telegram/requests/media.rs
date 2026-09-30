use super::{
    SelfDestructSend, SendReply, formatted_caption, message_topic_value, self_destruct_type_value,
    send_reply_value,
};
use crate::ids::{ChatId, FileId, RequestId};
use serde_json::{Value, json};

/// `downloadFile` (TDLib 1.8.67). `synchronous: false` returns the current
/// `file` immediately; progress continues on `updateFile`.
pub fn download_file(extra: RequestId, file_id: FileId, priority: i32) -> String {
    json!({
        "@type": "downloadFile",
        "@extra": extra.as_extra(),
        "file_id": file_id.0,
        "priority": priority,
        "offset": 0,
        "limit": 0,
        "synchronous": false,
    })
    .to_string()
}

/// `cancelDownloadFile` (TDLib 1.8.67, schema :13990-13991): "Stops the
/// downloading of a file. If a file has already been downloaded, does
/// nothing." `only_if_pending: false` cancels an in-flight download (TGX
/// `cancelDownloadOrUploadFile`); `true` only stops one that hasn't started.
pub fn cancel_download_file(extra: RequestId, file_id: FileId, only_if_pending: bool) -> String {
    json!({
        "@type": "cancelDownloadFile",
        "@extra": extra.as_extra(),
        "file_id": file_id.0,
        "only_if_pending": only_if_pending,
    })
    .to_string()
}

/// `inputMessagePhoto` body (TDLib 1.8.67). Shared by `sendMessage` and `sendMessageAlbum`.
pub fn input_message_photo(
    path: &str,
    caption: &str,
    caption_above: bool,
    self_destruct: Option<SelfDestructSend>,
    strip_blockquote: bool,
) -> Value {
    json!({
        "@type": "inputMessagePhoto",
        "photo": {
            "@type": "inputPhoto",
            "photo": {
                "@type": "inputFileLocal",
                "path": path
            },
            "thumbnail": Value::Null,
            "video": Value::Null,
            "added_sticker_file_ids": [],
            "width": 0,
            "height": 0
        },
        "caption": formatted_caption(caption, strip_blockquote),
        "show_caption_above_media": caption_above,
        "self_destruct_type": self_destruct_type_value(self_destruct),
        "has_spoiler": false
    })
}

/// `sendMessage` + `inputMessagePhoto` / `inputPhoto` / `inputFileLocal` (1.8.67).
/// `path` must already be an explicitly picked local file — never a JSON `local.path`.
#[allow(clippy::too_many_arguments)] // `strip_blockquote` is pure pass-through to `input_message_photo`
pub fn send_photo(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    path: &str,
    caption: &str,
    caption_above: bool,
    reply_to: Option<SendReply>,
    self_destruct: Option<SelfDestructSend>,
    strip_blockquote: bool,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": input_message_photo(path, caption, caption_above, self_destruct, strip_blockquote)
    })
    .to_string()
}

/// Fields for `inputVideo` (TDLib 1.8.67). Thumbnail stays null: the schema says
/// pass null to skip thumbnail uploading, and TDLib fills one for small files.
pub struct VideoSend {
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub supports_streaming: bool,
    /// Phase B3: `inputMessageVideo.self_destruct_type` (schema 1.8.67
    /// line 6128 — private chats only).
    pub self_destruct: Option<SelfDestructSend>,
}

/// `inputMessageVideo` body (TDLib 1.8.67). Shared by `sendMessage` and `sendMessageAlbum`.
pub fn input_message_video(
    path: &str,
    video: &VideoSend,
    caption: &str,
    caption_above: bool,
    strip_blockquote: bool,
) -> Value {
    json!({
        "@type": "inputMessageVideo",
        "video": {
            "@type": "inputVideo",
            "video": {
                "@type": "inputFileLocal",
                "path": path
            },
            "thumbnail": Value::Null,
            "cover": Value::Null,
            "start_timestamp": 0,
            "added_sticker_file_ids": [],
            "duration": video.duration,
            "width": video.width,
            "height": video.height,
            "supports_streaming": video.supports_streaming
        },
        "caption": formatted_caption(caption, strip_blockquote),
        "show_caption_above_media": caption_above,
        "self_destruct_type": self_destruct_type_value(video.self_destruct),
        "has_spoiler": false
    })
}

/// `inputVideoNote.thumbnail` when a JPEG was written locally. `None` is JSON null
/// (schema: pass null to skip thumbnail uploading).
pub struct VideoNoteThumbnailSend {
    pub path: String,
    pub width: i32,
    pub height: i32,
}

/// Fields for `inputVideoNote` (TDLib 1.8.67). `duration` is 0–60. `length` is
/// the square side, positive and at most 640.
pub struct VideoNoteSend {
    pub duration: i32,
    pub length: i32,
    pub thumbnail: Option<VideoNoteThumbnailSend>,
}

fn input_video_note_thumbnail(thumb: Option<&VideoNoteThumbnailSend>) -> Value {
    match thumb {
        Some(thumb) => json!({
            "@type": "inputThumbnail",
            "thumbnail": {
                "@type": "inputFileLocal",
                "path": thumb.path
            },
            "width": thumb.width,
            "height": thumb.height
        }),
        None => Value::Null,
    }
}

/// `sendMessage` + `inputMessageVideoNote` / `inputVideoNote` / `inputFileLocal` (1.8.67).
/// No caption: the constructor is `video_note` and `self_destruct_type` only.
/// `path` must already be an explicitly picked local file.
pub fn send_video_note(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    path: &str,
    note: &VideoNoteSend,
    reply_to: Option<SendReply>,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageVideoNote",
            "video_note": {
                "@type": "inputVideoNote",
                "video_note": {
                    "@type": "inputFileLocal",
                    "path": path
                },
                "thumbnail": input_video_note_thumbnail(note.thumbnail.as_ref()),
                "duration": note.duration,
                "length": note.length
            },
            "self_destruct_type": Value::Null
        }
    })
    .to_string()
}

/// `sendMessage` + `inputMessageVideo` / `inputVideo` / `inputFileLocal` (1.8.67).
/// `path` must already be an explicitly picked local file — never a JSON `local.path`.
#[allow(clippy::too_many_arguments)] // `strip_blockquote` is pure pass-through to `input_message_video`
pub fn send_video(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    path: &str,
    video: &VideoSend,
    caption: &str,
    caption_above: bool,
    reply_to: Option<SendReply>,
    strip_blockquote: bool,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": input_message_video(path, video, caption, caption_above, strip_blockquote)
    })
    .to_string()
}

/// `sendMessageAlbum` (TDLib 1.8.67). 2–10 contents, same `show_caption_above_media`.
/// Caption sits on the last item (`show_caption_above_media` is false).
pub fn send_message_album(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    reply_to: Option<SendReply>,
    input_message_contents: Vec<Value>,
) -> String {
    json!({
        "@type": "sendMessageAlbum",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": Value::Null,
        "input_message_contents": input_message_contents
    })
    .to_string()
}

/// `sendMessage` + `inputMessageDocument` / `inputDocument` / `inputFileLocal` (1.8.67).
/// `path` must already be an explicitly picked local file — never a JSON `local.path`.
pub fn send_document(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    path: &str,
    caption: &str,
    reply_to: Option<SendReply>,
    strip_blockquote: bool,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageDocument",
            "document": {
                "@type": "inputDocument",
                "document": {
                    "@type": "inputFileLocal",
                    "path": path
                },
                "thumbnail": Value::Null,
                "disable_content_type_detection": false
            },
            "caption": formatted_caption(caption, strip_blockquote)
        }
    })
    .to_string()
}

/// `sendMessage` + `inputMessageVoiceNote` / `inputVoiceNote` / `inputFileLocal`.
/// `path` must already be an explicitly recorded or picked file.
/// `waveform_b64` is the 5-bit waveform as TDLib `bytes` (base64); empty if unknown.
/// Voice-note send parameters. Bundled into a struct so the send constructor
/// stays under clippy's argument limit as topic/reply support grows.
pub struct VoiceNoteSend<'a> {
    pub path: &'a str,
    pub duration: i32,
    pub waveform_b64: &'a str,
    pub caption: &'a str,
    pub reply_to: Option<SendReply>,
    /// Parity slice 4: forum topic the send is addressed to (`None` = no topic).
    pub topic_id: Option<i32>,
}

/// `sendMessage` + `inputMessageVoiceNote` / `inputVoiceNote` / `inputFileLocal`.
/// `path` must already be an explicitly recorded or picked file.
/// `waveform_b64` is the 5-bit waveform as TDLib `bytes` (base64); empty if unknown.
pub fn send_voice_note(extra: RequestId, chat_id: ChatId, voice: VoiceNoteSend<'_>) -> String {
    let caption_json = if voice.caption.is_empty() {
        Value::Null
    } else {
        json!({
            "@type": "formattedText",
            "text": voice.caption,
            "entities": []
        })
    };
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(voice.topic_id),
        "reply_to": send_reply_value(voice.reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageVoiceNote",
            "voice_note": {
                "@type": "inputVoiceNote",
                "voice_note": {
                    "@type": "inputFileLocal",
                    "path": voice.path
                },
                "duration": voice.duration,
                "waveform": voice.waveform_b64
            },
            "caption": caption_json,
            "self_destruct_type": Value::Null
        }
    })
    .to_string()
}
