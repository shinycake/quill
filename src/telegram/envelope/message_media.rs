use super::*;
use crate::ids::FileId;
use crate::text::TextEntity;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;

/// `photo` + caption flags from `messagePhoto` (TDLib 1.8.67).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoContent {
    pub caption: String,
    /// Phase 4.1: entities for `caption` (same list as message text).
    pub caption_entities: Vec<TextEntity>,
    /// MED4: `messagePhoto.show_caption_above_media` (schema:6117).
    pub show_caption_above_media: bool,
    pub sizes: Vec<PhotoSizeView>,
    pub is_secret: bool,
    pub has_spoiler: bool,
    /// `photo.minithumbnail`: a tiny inline JPEG shown (scaled up, so
    /// soft) until a real size downloads. Never shown for secret or
    /// spoiler photos.
    pub minithumbnail: Option<MiniThumbnail>,
}

impl PhotoContent {
    /// Prefer `photoSize.type == "m"` (box 320), else the largest size ≤ 320px wide.
    pub fn thumb_size(&self) -> Option<&PhotoSizeView> {
        self.sizes
            .iter()
            .find(|size| size.type_name == "m")
            .or_else(|| {
                self.sizes
                    .iter()
                    .filter(|size| size.width > 0 && size.width <= 320)
                    .max_by_key(|size| size.width)
            })
            .or_else(|| {
                self.sizes
                    .iter()
                    .min_by_key(|size| (size.width, size.height))
            })
    }

    pub fn largest_size(&self) -> Option<&PhotoSizeView> {
        self.sizes
            .iter()
            .max_by_key(|size| i64::from(size.width) * i64::from(size.height))
    }

    pub fn open_file_id(&self) -> Option<FileId> {
        self.largest_size()
            .or_else(|| self.thumb_size())
            .map(|size| size.file_id)
    }

    /// Secret photos must not download on placeholder click (schema: show only while tapped).
    pub fn click_requests_download(&self) -> bool {
        !self.is_secret
    }

    /// Placeholder copy follows file state for secret and spoiler photos.
    pub fn placeholder_label(&self, downloading: bool, ready: bool) -> String {
        let kind = if self.is_secret {
            "Secret photo"
        } else if self.has_spoiler {
            "Photo (spoiler)"
        } else {
            "Photo"
        };
        let state = if ready {
            "ready"
        } else if downloading {
            "downloading…"
        } else {
            "not downloaded"
        };
        format!("{kind} — {state}")
    }
}

/// `photoSize` fields used for display / download (schema: type, photo, width, height).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoSizeView {
    pub type_name: String,
    pub width: i32,
    pub height: i32,
    pub file_id: FileId,
}

/// `document` + caption from `messageDocument`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentContent {
    pub file_name: String,
    pub mime_type: String,
    pub caption: String,
    /// Phase 4.1: entities for `caption` (same list as message text).
    pub caption_entities: Vec<TextEntity>,
    pub file_id: FileId,
}

/// `minithumbnail` (TDLib 1.8.67): JPEG bytes, usually ≤ 40px.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniThumbnail {
    pub width: i32,
    pub height: i32,
    /// Raw JPEG from `minithumbnail.data` (`bytes`).
    pub data: Vec<u8>,
}

/// One `thumbnail` used as an album cover (`album_cover_thumbnail` or
/// `external_album_covers`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumCoverThumb {
    pub width: i32,
    pub height: i32,
    pub file_id: FileId,
}

/// Typed `file` + `localFile` (no `remoteFile.id` — that can be an HTTP URL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFile {
    pub id: FileId,
    pub size: i64,
    pub expected_size: i64,
    pub local: LocalFileState,
}

impl ParsedFile {
    pub fn usable_path(&self) -> Option<&str> {
        if self.local.is_downloading_completed && !self.local.path.is_empty() {
            Some(self.local.path.as_str())
        } else {
            None
        }
    }

    pub fn needs_download(&self) -> bool {
        self.usable_path().is_none() && self.local.can_be_downloaded
    }

    /// Download progress fraction from `local.downloaded_size` over the
    /// known total (TGX `TD.getFileProgress` semantics: `downloadedSize /
    /// expectedSize`). `None` when the total is unknown — no percent to show.
    pub fn download_progress(&self) -> Option<f32> {
        let total = self.display_size();
        if total <= 0 {
            return None;
        }
        Some((self.local.downloaded_size as f32 / total as f32).clamp(0.0, 1.0))
    }

    pub fn display_size(&self) -> i64 {
        if self.size > 0 {
            self.size
        } else {
            self.expected_size
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFileState {
    pub path: String,
    pub can_be_downloaded: bool,
    pub is_downloading_active: bool,
    pub is_downloading_completed: bool,
    /// Total downloaded bytes so far (schema: "can be used only for
    /// calculating download progress"). `downloaded_prefix_size` is the
    /// contiguous readable prefix from `download_offset`; progress uses
    /// this total instead.
    pub downloaded_size: i64,
}

impl LocalFileState {
    /// Download is neither in flight nor finished (`file` / `updateFile` idle).
    pub fn is_idle_incomplete(&self) -> bool {
        !self.is_downloading_active && !self.is_downloading_completed
    }
}

pub(crate) fn parse_message_photo(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let photo = value.get("photo");
    let (sizes, files) = photo.map(parse_photo_sizes).unwrap_or_default();
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    (
        MessageContent::Photo(PhotoContent {
            caption,
            caption_entities,
            show_caption_above_media: value
                .get("show_caption_above_media")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            sizes,
            is_secret: value
                .get("is_secret")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            has_spoiler: value
                .get("has_spoiler")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            minithumbnail: photo.and_then(|photo| parse_minithumbnail(photo.get("minithumbnail"))),
        }),
        files,
    )
}

pub(crate) fn parse_message_document(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let document = value.get("document");
    let mut files = Vec::new();
    let file_id = match document.and_then(|d| parse_file(d.get("document")).ok()) {
        Some(file) => {
            let id = file.id;
            files.push(file);
            id
        }
        None => FileId(0),
    };
    if let Some(thumb) = document.and_then(|d| d.get("thumbnail"))
        && let Ok(thumb_file) = parse_file(thumb.get("file"))
    {
        files.push(thumb_file);
    }
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    (
        MessageContent::Document(DocumentContent {
            file_name: document
                .and_then(|d| d.get("file_name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            mime_type: document
                .and_then(|d| d.get("mime_type"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            caption,
            caption_entities,
            file_id,
        }),
        files,
    )
}

pub(crate) fn parse_minithumbnail(value: Option<&Value>) -> Option<MiniThumbnail> {
    let value = value.filter(|value| !value.is_null())?;
    if value.get("@type").and_then(Value::as_str) != Some("minithumbnail") {
        return None;
    }
    let data = parse_tdlib_bytes(value.get("data"));
    if data.is_empty() {
        return None;
    }
    Some(MiniThumbnail {
        width: int53_or_zero(value.get("width")) as i32,
        height: int53_or_zero(value.get("height")) as i32,
        data,
    })
}

pub(crate) fn parse_album_cover_thumb(
    value: &Value,
    files: &mut Vec<ParsedFile>,
) -> Option<AlbumCoverThumb> {
    if value.get("@type").and_then(Value::as_str) != Some("thumbnail") {
        return None;
    }
    let file = parse_file(value.get("file")).ok()?;
    if file.id.0 == 0 {
        return None;
    }
    let cover = AlbumCoverThumb {
        width: int53_or_zero(value.get("width")) as i32,
        height: int53_or_zero(value.get("height")) as i32,
        file_id: file.id,
    };
    files.push(file);
    Some(cover)
}

/// TDLib JSON `bytes` is a base64 string (empty when the waveform is unknown).
pub(crate) fn parse_tdlib_bytes(value: Option<&Value>) -> Vec<u8> {
    let Some(text) = value.and_then(Value::as_str) else {
        return Vec::new();
    };
    if text.is_empty() {
        return Vec::new();
    }
    STANDARD.decode(text).unwrap_or_default()
}

/// Parity slice: the `small` file from a `chatPhotoInfo` (`chat.photo` /
/// `updateChatPhoto.photo`, schema 1.8.67, lines 762 and 10488). The small
/// variant is the cheap thumbnail the chat list renders; `big` is not
/// kept. Null/absent/malformed → `None`.
pub(crate) fn parse_chat_photo_small(value: Option<&Value>) -> Option<ParsedFile> {
    let photo = value.filter(|v| !v.is_null())?;
    parse_file(photo.get("small")).ok()
}

pub(crate) fn parse_photo_sizes(photo: &Value) -> (Vec<PhotoSizeView>, Vec<ParsedFile>) {
    let mut files = Vec::new();
    let mut sizes = Vec::new();
    if let Some(entries) = photo.get("sizes").and_then(Value::as_array) {
        for entry in entries {
            let Ok(file) = parse_file(entry.get("photo")) else {
                continue;
            };
            sizes.push(PhotoSizeView {
                type_name: entry
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                width: int53_or_zero(entry.get("width")) as i32,
                height: int53_or_zero(entry.get("height")) as i32,
                file_id: file.id,
            });
            files.push(file);
        }
    }
    (sizes, files)
}

pub(crate) fn parse_file(value: Option<&Value>) -> Result<ParsedFile, ParseError> {
    let value = value.ok_or(ParseError::MissingField)?;
    let id = i32::try_from(int53(value.get("id"))?).map_err(|_| ParseError::BadInt)?;
    let local = value.get("local");
    Ok(ParsedFile {
        id: FileId(id),
        size: int53_or_zero(value.get("size")),
        expected_size: int53_or_zero(value.get("expected_size")),
        local: LocalFileState {
            path: local
                .and_then(|l| l.get("path"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            can_be_downloaded: local
                .and_then(|l| l.get("can_be_downloaded"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_downloading_active: local
                .and_then(|l| l.get("is_downloading_active"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_downloading_completed: local
                .and_then(|l| l.get("is_downloading_completed"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            downloaded_size: int53_or_zero(local.and_then(|l| l.get("downloaded_size"))),
        },
    })
}
