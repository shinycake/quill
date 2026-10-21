//! The files a saved page links to: which TDLib files the blocks show and
//! the `media/…` names they get, after tdesktop's `addPhotoJob`,
//! `addDocumentJob` and `reserveMediaName`.
//!
//! The page is rendered twice through the same code: first with a
//! [`Collector`], which records every file the blocks ask for, then, once
//! the files are copied, with the [`Saved`] names. So the files the page
//! links and the files the worker copies can never disagree.
use crate::export::names::sanitize;
use crate::json_access::JsonExt;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// tdesktop `kMaxFolderNameLength`: longest folder and file name part.
pub(super) const MAX_NAME_CHARS: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MediaKind {
    Photo,
    Video,
    Audio,
    /// A small image inside the text (`richTextIcon`).
    Image,
}

/// Where the blocks' files come from while rendering.
pub(crate) trait MediaSource {
    /// The `media/…` path for `file` (a TDLib `file`), described by `meta`
    /// (the `photo`, `video`, `audio`, … object holding it), or `None`
    /// when the page shows "Media unavailable" instead.
    fn path(&mut self, kind: MediaKind, meta: &Value, file: &Value) -> Option<String>;
}

/// A file the worker copies into `media/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Job {
    pub file_id: i32,
    pub relative: String,
}

/// The first render: hands out names and records the jobs.
#[derive(Debug, Default)]
pub(crate) struct Collector {
    pub jobs: Vec<Job>,
    names: HashMap<i32, String>,
    used: HashSet<String>,
}

impl MediaSource for Collector {
    fn path(&mut self, kind: MediaKind, meta: &Value, file: &Value) -> Option<String> {
        let file_id = file
            .at("id")
            .as_i64()
            .and_then(|id| i32::try_from(id).ok())?;
        if file_id <= 0 || self.names.contains_key(&file_id) {
            return None;
        }
        let relative = self.reserve(file_id, &file_name(kind, meta, file_id));
        self.names.insert(file_id, relative.clone());
        self.jobs.push(Job { file_id, relative });
        None
    }
}

impl Collector {
    /// tdesktop `reserveMediaName`: a name already taken (case-insensitive)
    /// gets `_<id>` before its extension.
    fn reserve(&mut self, file_id: i32, name: &str) -> String {
        let mut name = name.to_string();
        if self.used.contains(&name.to_lowercase()) {
            name = match name.rfind('.').filter(|dot| *dot > 0) {
                Some(dot) => {
                    let (stem, ext) = name.split_at(dot);
                    format!("{stem}_{file_id}{ext}")
                }
                None => format!("{name}_{file_id}"),
            };
        }
        self.used.insert(name.to_lowercase());
        format!("media/{name}")
    }
}

/// The second render: the files that were copied, by TDLib file id.
#[derive(Debug, Default)]
pub(crate) struct Saved(pub HashMap<i32, String>);

impl MediaSource for Saved {
    fn path(&mut self, _kind: MediaKind, _meta: &Value, file: &Value) -> Option<String> {
        let file_id = file
            .at("id")
            .as_i64()
            .and_then(|id| i32::try_from(id).ok())?;
        self.0.get(&file_id).cloned()
    }
}

/// The largest size of a TDLib `photo`: its file, width and height
/// (tdesktop saves `PhotoSize::Large`).
pub(super) fn best_photo_size(photo: &Value) -> Option<(&Value, i64, i64)> {
    photo
        .at("sizes")
        .as_array()?
        .iter()
        .map(|size| {
            let width = size.at("width").as_i64().unwrap_or(0).max(0);
            let height = size.at("height").as_i64().unwrap_or(0).max(0);
            (size.at("photo"), width, height)
        })
        .filter(|(file, _, _)| file.is_object())
        .max_by_key(|(_, width, height)| width.saturating_mul(*height))
}

/// `name` cut to [`MAX_NAME_CHARS`], keeping `keep_extension` at the end
/// (tdesktop `SanitizeFileNamePart`).
pub(super) fn name_part(name: &str, keep_extension: &str) -> String {
    let clean = sanitize(&name.split_whitespace().collect::<Vec<_>>().join(" "));
    if clean.chars().count() <= MAX_NAME_CHARS {
        return clean;
    }
    // Not `sanitize`: it would drop the extension's leading dot.
    let tail: String = keep_extension
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let room = MAX_NAME_CHARS.saturating_sub(tail.chars().count());
    let head: String = clean.chars().take(room).collect();
    let joined = format!("{}{tail}", head.trim_end_matches(['.', ' ']));
    joined.trim_end_matches(['.', ' ']).to_string()
}

/// tdesktop `DocumentFileExtension`: the file name's own extension, else
/// one for the kind, else one for the MIME type.
fn extension(kind: MediaKind, meta: &Value) -> String {
    let name = meta.at("file_name").as_str().unwrap_or("");
    if let Some(dot) = name.rfind('.')
        && dot > 0
        && dot + 1 < name.len()
        && name.len() - dot <= 8
        && let Some(ext) = name.get(dot..)
    {
        return ext.to_string();
    }
    let mime = meta.at("mime_type").as_str().unwrap_or("");
    let by_mime = match mime.to_ascii_lowercase().as_str() {
        "image/jpeg" => Some(".jpg"),
        "image/png" => Some(".png"),
        "image/gif" => Some(".gif"),
        "image/webp" => Some(".webp"),
        "video/mp4" => Some(".mp4"),
        "video/webm" => Some(".webm"),
        "video/quicktime" => Some(".mov"),
        "audio/ogg" => Some(".ogg"),
        "audio/mpeg" | "audio/mp3" => Some(".mp3"),
        "audio/mp4" | "audio/m4a" => Some(".m4a"),
        _ => None,
    };
    match (kind, by_mime) {
        (_, Some(ext)) => ext.to_string(),
        (MediaKind::Video, None) => ".mp4".into(),
        (MediaKind::Audio, None) if mime.contains("ogg") => ".ogg".into(),
        (MediaKind::Audio, None) => ".mp3".into(),
        (MediaKind::Photo, None) => ".jpg".into(),
        (MediaKind::Image, None) => ".bin".into(),
    }
}

/// The name a file gets in `media/`: `photo_<id>.jpg` for photos, else
/// the document's own file name, else `<kind>_<id><ext>`.
fn file_name(kind: MediaKind, meta: &Value, file_id: i32) -> String {
    if kind == MediaKind::Photo {
        return format!("photo_{file_id}.jpg");
    }
    let ext = extension(kind, meta);
    let own = name_part(meta.at("file_name").as_str().unwrap_or(""), &ext);
    if !own.is_empty() {
        return own;
    }
    let prefix = match kind {
        MediaKind::Video => "video",
        MediaKind::Audio => "audio",
        MediaKind::Image | MediaKind::Photo => "image",
    };
    format!("{prefix}_{file_id}{ext}")
}
