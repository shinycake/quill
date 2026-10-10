use super::*;
use crate::ids::FileId;
use crate::text::TextEntity;
use serde_json::Value;

/// `audio` inside `messageAudio` (TDLib 1.8.67). Music files, not voice notes.
///
/// Schema: `duration`, `title`, `performer`, `file_name`, `mime_type`,
/// `album_cover_minithumbnail`, `album_cover_thumbnail`,
/// `external_album_covers`, `audio:file`, plus `messageAudio.caption`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioContent {
    pub duration: i32,
    pub title: String,
    pub performer: String,
    pub file_name: String,
    pub mime_type: String,
    pub caption: String,
    /// Phase 4.1: entities for `caption` (same list as message text).
    pub caption_entities: Vec<TextEntity>,
    pub album_cover_minithumbnail: Option<MiniThumbnail>,
    pub album_cover_thumbnail: Option<AlbumCoverThumb>,
    pub external_album_covers: Vec<AlbumCoverThumb>,
    pub file_id: FileId,
}

impl AudioContent {
    /// The track itself (`audio.audio`).
    pub fn play_file_id(&self) -> Option<FileId> {
        (self.file_id.0 != 0).then_some(self.file_id)
    }

    /// Cover to show and auto-download. The sender thumbnail wins; otherwise
    /// the largest `external_album_covers` entry (schema: fallback when the
    /// file has no embedded cover).
    pub fn cover_file_id(&self) -> Option<FileId> {
        if let Some(cover) = &self.album_cover_thumbnail
            && cover.file_id.0 != 0
        {
            return Some(cover.file_id);
        }
        self.external_album_covers
            .iter()
            .filter(|cover| cover.file_id.0 != 0)
            .max_by_key(|cover| i64::from(cover.width) * i64::from(cover.height))
            .map(|cover| cover.file_id)
    }
}

/// `messageVoiceNote` / `voiceNote` (TDLib 1.8.67).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceNoteContent {
    pub duration: i32,
    /// Raw `waveform` bytes (5-bit packed), not the decoded bars.
    pub waveform: Vec<u8>,
    pub mime_type: String,
    pub caption: String,
    /// Phase 4.1: entities for `caption` (same list as message text).
    pub caption_entities: Vec<TextEntity>,
    pub is_listened: bool,
    pub file_id: FileId,
    /// MED2: `speech_recognition_result` (`SpeechRecognitionResult`;
    /// `None` when TDLib sent null / the field is absent).
    pub transcription: Option<SpeechRecognition>,
}

/// `speechRecognitionResult` (TDLib 1.8.67, schema lines 7390-7399):
/// the outcome of `recognizeSpeech` on a voice or video note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeechRecognition {
    /// `speechRecognitionResultPending` — TDLib is still recognizing.
    Pending { partial_text: String },
    /// `speechRecognitionResultText` — final transcript.
    Text { text: String },
    /// `speechRecognitionResultError` — recognition failed server-side.
    Error { message: String },
}

/// Parse `speech_recognition_result` (`SpeechRecognitionResult`, may be
/// null). Unknown `@type` values map to `None` — never a fake result.
pub(crate) fn parse_speech_recognition(value: Option<&Value>) -> Option<SpeechRecognition> {
    let result = value?;
    if result.is_null() {
        return None;
    }
    let kind = result.get("@type").and_then(Value::as_str)?;
    match kind {
        "speechRecognitionResultPending" => Some(SpeechRecognition::Pending {
            partial_text: result
                .get("partial_text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        "speechRecognitionResultText" => Some(SpeechRecognition::Text {
            text: result
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        "speechRecognitionResultError" => Some(SpeechRecognition::Error {
            message: result
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("transcription failed")
                .to_string(),
        }),
        _ => None,
    }
}

/// `animation` inside `messageAnimation` or `getSavedAnimations` (TDLib 1.8.67).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationContent {
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub file_name: String,
    pub mime_type: String,
    pub caption: String,
    /// Phase 4.1: entities for `caption` (same list as message text).
    pub caption_entities: Vec<TextEntity>,
    pub show_caption_above_media: bool,
    pub has_spoiler: bool,
    pub is_secret: bool,
    pub file_id: FileId,
    pub thumb_file_id: Option<FileId>,
    pub thumb_width: i32,
    pub thumb_height: i32,
}

impl AnimationContent {
    /// JPEG/MPEG4 thumbnail file, when the sender attached one.
    pub fn thumb_file_id(&self) -> Option<FileId> {
        self.thumb_file_id.filter(|id| id.0 != 0)
    }

    /// The clip itself (`animation.animation`).
    pub fn play_file_id(&self) -> Option<FileId> {
        (self.file_id.0 != 0).then_some(self.file_id)
    }
}

/// `video` inside `messageVideo` (TDLib 1.8.67).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoContent {
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub file_name: String,
    pub mime_type: String,
    pub caption: String,
    /// Phase 4.1: entities for `caption` (same list as message text).
    pub caption_entities: Vec<TextEntity>,
    pub show_caption_above_media: bool,
    pub has_spoiler: bool,
    pub is_secret: bool,
    /// `messageVideo.start_timestamp` — seconds to seek before playback.
    pub start_timestamp: i32,
    pub supports_streaming: bool,
    pub has_stickers: bool,
    pub file_id: FileId,
    pub thumb_file_id: Option<FileId>,
    pub thumb_width: i32,
    pub thumb_height: i32,
}

impl VideoContent {
    /// JPEG/MPEG4 thumbnail file, when the sender attached one.
    pub fn thumb_file_id(&self) -> Option<FileId> {
        self.thumb_file_id.filter(|id| id.0 != 0)
    }

    /// The clip itself (`video.video`).
    pub fn play_file_id(&self) -> Option<FileId> {
        (self.file_id.0 != 0).then_some(self.file_id)
    }
}

/// `videoNote` inside `messageVideoNote` (TDLib 1.8.67).
///
/// Schema: square MPEG4 cropped to a circle. Fields stored are `duration`,
/// `waveform`, `length` (width and height), `thumbnail`, `video`,
/// `speech_recognition_result`, plus `is_viewed` and `is_secret` on the
/// message. `minithumbnail` is left unused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoNoteContent {
    pub duration: i32,
    /// MED2: `speech_recognition_result` (`SpeechRecognitionResult`;
    /// `None` when TDLib sent null / the field is absent).
    pub transcription: Option<SpeechRecognition>,
    /// Raw `waveform` bytes (5-bit packed). Empty when unknown.
    pub waveform: Vec<u8>,
    /// Video width and height, as defined by the sender.
    pub length: i32,
    pub is_viewed: bool,
    pub is_secret: bool,
    pub file_id: FileId,
    pub thumb_file_id: Option<FileId>,
    pub thumb_width: i32,
    pub thumb_height: i32,
}

impl VideoNoteContent {
    /// JPEG thumbnail file, when the sender attached one.
    pub fn thumb_file_id(&self) -> Option<FileId> {
        self.thumb_file_id.filter(|id| id.0 != 0)
    }

    /// The clip itself (`videoNote.video`). Schema: MPEG4.
    pub fn play_file_id(&self) -> Option<FileId> {
        (self.file_id.0 != 0).then_some(self.file_id)
    }
}

/// One saved GIF from `animations.animations` (picker). Same file ids as `animation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationItem {
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub file_name: String,
    pub mime_type: String,
    pub file_id: FileId,
    pub thumb_file_id: Option<FileId>,
    pub thumb_width: i32,
    pub thumb_height: i32,
}

pub(crate) fn parse_message_animation(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let animation = value.get("animation");
    let (item, mut files) = parse_animation_value(animation);
    let Some(item) = item else {
        return (
            MessageContent::Unsupported {
                type_name: "messageAnimation".into(),
            },
            files,
        );
    };
    files.retain(|file| file.id.0 != 0);
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    (
        MessageContent::Animation(AnimationContent {
            duration: item.duration,
            width: item.width,
            height: item.height,
            file_name: item.file_name,
            mime_type: item.mime_type,
            caption,
            caption_entities,
            show_caption_above_media: value
                .get("show_caption_above_media")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            has_spoiler: value
                .get("has_spoiler")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_secret: value
                .get("is_secret")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            file_id: item.file_id,
            thumb_file_id: item.thumb_file_id,
            thumb_width: item.thumb_width,
            thumb_height: item.thumb_height,
        }),
        files,
    )
}

pub(crate) fn parse_message_video(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    let video = value.get("video");
    let Some(video) =
        video.filter(|video| video.get("@type").and_then(Value::as_str) == Some("video"))
    else {
        return (
            MessageContent::Unsupported {
                type_name: "messageVideo".into(),
            },
            Vec::new(),
        );
    };
    let mut files = Vec::new();
    let file_id = match parse_file(video.get("video")) {
        Ok(file) => {
            let id = file.id;
            files.push(file);
            id
        }
        Err(_) => FileId(0),
    };
    let thumb = video.get("thumbnail").filter(|thumb| !thumb.is_null());
    let (thumb_file_id, thumb_width, thumb_height) = if let Some(thumb) = thumb {
        let id = match parse_file(thumb.get("file")) {
            Ok(file) => {
                let id = file.id;
                files.push(file);
                Some(id)
            }
            Err(_) => None,
        };
        (
            id.filter(|id| id.0 != 0),
            int53_or_zero(thumb.get("width")).sat_i32(),
            int53_or_zero(thumb.get("height")).sat_i32(),
        )
    } else {
        (None, 0, 0)
    };
    files.retain(|file| file.id.0 != 0);
    (
        MessageContent::Video(VideoContent {
            duration: int53_or_zero(video.get("duration")).sat_i32(),
            width: int53_or_zero(video.get("width")).sat_i32(),
            height: int53_or_zero(video.get("height")).sat_i32(),
            file_name: video
                .get("file_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            mime_type: video
                .get("mime_type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            caption,
            caption_entities,
            show_caption_above_media: value
                .get("show_caption_above_media")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            has_spoiler: value
                .get("has_spoiler")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_secret: value
                .get("is_secret")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            start_timestamp: int53_or_zero(value.get("start_timestamp")).sat_i32(),
            supports_streaming: video
                .get("supports_streaming")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            has_stickers: video
                .get("has_stickers")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            file_id,
            thumb_file_id,
            thumb_width,
            thumb_height,
        }),
        files,
    )
}

pub(crate) fn parse_message_video_note(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let note = value.get("video_note");
    let Some(note) =
        note.filter(|note| note.get("@type").and_then(Value::as_str) == Some("videoNote"))
    else {
        return (
            MessageContent::Unsupported {
                type_name: "messageVideoNote".into(),
            },
            Vec::new(),
        );
    };
    let mut files = Vec::new();
    let file_id = match parse_file(note.get("video")) {
        Ok(file) => {
            let id = file.id;
            files.push(file);
            id
        }
        Err(_) => FileId(0),
    };
    let thumb = note.get("thumbnail").filter(|thumb| !thumb.is_null());
    let (thumb_file_id, thumb_width, thumb_height) = if let Some(thumb) = thumb {
        let id = match parse_file(thumb.get("file")) {
            Ok(file) => {
                let id = file.id;
                files.push(file);
                Some(id)
            }
            Err(_) => None,
        };
        (
            id.filter(|id| id.0 != 0),
            int53_or_zero(thumb.get("width")).sat_i32(),
            int53_or_zero(thumb.get("height")).sat_i32(),
        )
    } else {
        (None, 0, 0)
    };
    files.retain(|file| file.id.0 != 0);
    (
        MessageContent::VideoNote(VideoNoteContent {
            duration: int53_or_zero(note.get("duration")).sat_i32(),
            waveform: parse_tdlib_bytes(note.get("waveform")),
            length: int53_or_zero(note.get("length")).sat_i32(),
            is_viewed: value
                .get("is_viewed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_secret: value
                .get("is_secret")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            file_id,
            thumb_file_id,
            thumb_width,
            thumb_height,
            transcription: parse_speech_recognition(note.get("speech_recognition_result")),
        }),
        files,
    )
}

pub(crate) fn parse_animation_value(
    value: Option<&Value>,
) -> (Option<AnimationItem>, Vec<ParsedFile>) {
    let Some(value) = value else {
        return (None, Vec::new());
    };
    if value.get("@type").and_then(Value::as_str) != Some("animation") {
        return (None, Vec::new());
    }
    let mut files = Vec::new();
    let file_id = match parse_file(value.get("animation")) {
        Ok(file) => {
            let id = file.id;
            files.push(file);
            id
        }
        Err(_) => FileId(0),
    };
    let thumb = value.get("thumbnail").filter(|thumb| !thumb.is_null());
    let (thumb_file_id, thumb_width, thumb_height) = if let Some(thumb) = thumb {
        let id = match parse_file(thumb.get("file")) {
            Ok(file) => {
                let id = file.id;
                files.push(file);
                Some(id)
            }
            Err(_) => None,
        };
        (
            id.filter(|id| id.0 != 0),
            int53_or_zero(thumb.get("width")).sat_i32(),
            int53_or_zero(thumb.get("height")).sat_i32(),
        )
    } else {
        (None, 0, 0)
    };
    (
        Some(AnimationItem {
            duration: int53_or_zero(value.get("duration")).sat_i32(),
            width: int53_or_zero(value.get("width")).sat_i32(),
            height: int53_or_zero(value.get("height")).sat_i32(),
            file_name: value
                .get("file_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            mime_type: value
                .get("mime_type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            file_id,
            thumb_file_id,
            thumb_width,
            thumb_height,
        }),
        files,
    )
}

pub(crate) fn parse_animations(value: &Value) -> EnvelopePayload {
    let mut animations = Vec::new();
    let mut files = Vec::new();
    if let Some(entries) = value.get("animations").and_then(Value::as_array) {
        for entry in entries {
            let (item, item_files) = parse_animation_value(Some(entry));
            files.extend(item_files);
            if let Some(item) = item {
                animations.push(item);
            }
        }
    }
    files.retain(|file| file.id.0 != 0);
    EnvelopePayload::Stickers(StickersPayload::Animations { animations, files })
}

pub(crate) fn parse_message_audio(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    let audio = value.get("audio");
    let Some(audio) =
        audio.filter(|audio| audio.get("@type").and_then(Value::as_str) == Some("audio"))
    else {
        return (
            MessageContent::Unsupported {
                type_name: "messageAudio".into(),
            },
            Vec::new(),
        );
    };
    let mut files = Vec::new();
    let file_id = match parse_file(audio.get("audio")) {
        Ok(file) => {
            let id = file.id;
            files.push(file);
            id
        }
        Err(_) => FileId(0),
    };
    let album_cover_thumbnail = audio
        .get("album_cover_thumbnail")
        .filter(|thumb| !thumb.is_null())
        .and_then(|thumb| parse_album_cover_thumb(thumb, &mut files));
    let mut external_album_covers = Vec::new();
    if let Some(entries) = audio.get("external_album_covers").and_then(Value::as_array) {
        for entry in entries {
            if let Some(cover) = parse_album_cover_thumb(entry, &mut files) {
                external_album_covers.push(cover);
            }
        }
    }
    files.retain(|file| file.id.0 != 0);
    (
        MessageContent::Audio(AudioContent {
            duration: int53_or_zero(audio.get("duration")).sat_i32(),
            title: json_field_str(audio, "title"),
            performer: json_field_str(audio, "performer"),
            file_name: json_field_str(audio, "file_name"),
            mime_type: json_field_str(audio, "mime_type"),
            caption,
            caption_entities,
            album_cover_minithumbnail: parse_minithumbnail(audio.get("album_cover_minithumbnail")),
            album_cover_thumbnail,
            external_album_covers,
            file_id,
        }),
        files,
    )
}

pub(crate) fn parse_message_voice_note(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    let voice_note = value.get("voice_note");
    let mut files = Vec::new();
    let file_id = match voice_note.and_then(|note| parse_file(note.get("voice")).ok()) {
        Some(file) => {
            let id = file.id;
            files.push(file);
            id
        }
        None => FileId(0),
    };
    let duration = voice_note
        .and_then(|note| note.get("duration"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .clamp(0, i64::from(i32::MAX)) as i32;
    (
        MessageContent::VoiceNote(VoiceNoteContent {
            duration,
            waveform: parse_tdlib_bytes(voice_note.and_then(|note| note.get("waveform"))),
            mime_type: voice_note
                .and_then(|note| note.get("mime_type"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            caption,
            caption_entities,
            is_listened: value
                .get("is_listened")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            file_id,
            transcription: parse_speech_recognition(
                voice_note.and_then(|note| note.get("speech_recognition_result")),
            ),
        }),
        files,
    )
}
