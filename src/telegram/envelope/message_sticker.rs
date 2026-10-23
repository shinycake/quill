use super::*;
use crate::ids::FileId;
use serde_json::Value;

/// Still-image, vector-animation, and video sticker formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickerFormat {
    Webp,
    Tgs,
    Webm,
    Unknown,
}

/// `messageSticker` (TDLib 1.8.67). Display uses `thumbnail` (WEBP/JPEG) or a WEBP `sticker` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StickerContent {
    pub emoji: String,
    pub width: i32,
    pub height: i32,
    pub format: StickerFormat,
    pub file_id: FileId,
    pub thumb_file_id: Option<FileId>,
    pub thumb_width: i32,
    pub thumb_height: i32,
    pub is_premium: bool,
    pub requires_premium: bool,
    /// `full_type.premium_animation`: the TGS effect played around a Premium
    /// sticker (tdesktop `Sticker::hasPremiumEffect`).
    pub premium_animation: Option<FileId>,
    /// `sticker.set_id` (0 when the sticker belongs to no set): feeds the
    /// message menu's "View Sticker Set" / "Add Stickers".
    pub set_id: i64,
}

impl StickerContent {
    /// Display the full static sticker; animated formats use their image thumbnail until decoded.
    pub fn display_file_id(&self) -> Option<FileId> {
        if self.format == StickerFormat::Webp && self.file_id.0 != 0 {
            return Some(self.file_id);
        }
        self.thumb_file_id.filter(|id| id.0 != 0)
    }
}

/// One sticker inside `stickerSet.stickers.stickers` (picker). Same file ids as `sticker`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StickerItem {
    pub custom_emoji_id: Option<i64>,
    pub id: i64,
    pub set_id: i64,
    pub emoji: String,
    pub width: i32,
    pub height: i32,
    pub format: StickerFormat,
    pub file_id: FileId,
    pub thumb_file_id: Option<FileId>,
    pub thumb_width: i32,
    pub thumb_height: i32,
    pub requires_premium: bool,
    pub premium_animation: Option<FileId>,
}

impl StickerItem {
    /// Display the full static sticker; animated formats use their image thumbnail until decoded.
    pub fn display_file_id(&self) -> Option<FileId> {
        if self.format == StickerFormat::Webp && self.file_id.0 != 0 {
            return Some(self.file_id);
        }
        self.thumb_file_id.filter(|id| id.0 != 0)
    }
}

/// `stickerSetInfo` row from `getInstalledStickerSets` (regular sets only are requested).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StickerSetInfo {
    pub id: i64,
    pub title: String,
    pub name: String,
    pub size: i32,
    pub is_installed: bool,
    pub is_official: bool,
    /// `is_owned`: the current user created the set (tdesktop `AmCreator`),
    /// so the set box offers editing.
    pub is_owned: bool,
    /// `stickerSetInfo.covers`: the first stickers, for a row preview.
    pub covers: Vec<StickerItem>,
    /// The files of `covers`, remembered when the row arrives.
    pub cover_files: Vec<ParsedFile>,
    /// `is_viewed`: false while a trending set is new to the user (the
    /// stickers box shows an unread dot beside its title).
    pub is_viewed: bool,
}

pub(crate) fn parse_message_sticker(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let sticker = value.get("sticker");
    let (item, mut files) = parse_sticker_value(sticker);
    let Some(item) = item else {
        return (
            MessageContent::Unsupported {
                type_name: "messageSticker".into(),
            },
            files,
        );
    };
    (
        MessageContent::Sticker(StickerContent {
            emoji: item.emoji,
            width: item.width,
            height: item.height,
            format: item.format,
            file_id: item.file_id,
            thumb_file_id: item.thumb_file_id,
            thumb_width: item.thumb_width,
            thumb_height: item.thumb_height,
            requires_premium: item.requires_premium,
            premium_animation: item.premium_animation,
            set_id: item.set_id,
            is_premium: value
                .get("is_premium")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        {
            files.retain(|file| file.id.0 != 0);
            files
        },
    )
}

pub(crate) fn parse_sticker_format(value: Option<&Value>) -> StickerFormat {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("stickerFormatWebp") => StickerFormat::Webp,
        Some("stickerFormatTgs") => StickerFormat::Tgs,
        Some("stickerFormatWebm") => StickerFormat::Webm,
        _ => StickerFormat::Unknown,
    }
}

/// The `premium_animation` file of a regular sticker's full type, when set.
fn premium_animation_file(sticker: &Value) -> Option<ParsedFile> {
    let full_type = sticker.get("full_type")?;
    if full_type.get("@type").and_then(Value::as_str) != Some("stickerFullTypeRegular") {
        return None;
    }
    parse_file(full_type.get("premium_animation").filter(|v| !v.is_null()))
        .ok()
        .filter(|file| file.id.0 != 0)
}

pub(crate) fn parse_sticker_value(value: Option<&Value>) -> (Option<StickerItem>, Vec<ParsedFile>) {
    let Some(value) = value else {
        return (None, Vec::new());
    };
    if value.get("@type").and_then(Value::as_str) != Some("sticker") {
        return (None, Vec::new());
    }
    let mut files = Vec::new();
    let file_id = match parse_file(value.get("sticker")) {
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
            id,
            int53_or_zero(thumb.get("width")).sat_i32(),
            int53_or_zero(thumb.get("height")).sat_i32(),
        )
    } else {
        (None, 0, 0)
    };
    let premium_animation = premium_animation_file(value);
    let premium_animation_id = premium_animation.as_ref().map(|file| file.id);
    files.extend(premium_animation);
    (
        Some(StickerItem {
            custom_emoji_id: value
                .get("full_type")
                .filter(|v| {
                    v.get("@type").and_then(Value::as_str) == Some("stickerFullTypeCustomEmoji")
                })
                .and_then(|v| int64(v.get("custom_emoji_id")))
                .filter(|id| *id > 0),
            id: int64(value.get("id")).unwrap_or(0),
            set_id: int64(value.get("set_id")).unwrap_or(0),
            emoji: value
                .get("emoji")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            width: int53_or_zero(value.get("width")).sat_i32(),
            height: int53_or_zero(value.get("height")).sat_i32(),
            format: parse_sticker_format(value.get("format")),
            file_id,
            thumb_file_id,
            thumb_width,
            thumb_height,
            requires_premium: value.get("full_type").is_some_and(|full_type| {
                full_type.get("@type").and_then(Value::as_str) == Some("stickerFullTypeRegular")
                    && full_type
                        .get("premium_animation")
                        .is_some_and(|animation| !animation.is_null())
            }),
            premium_animation: premium_animation_id,
        }),
        files,
    )
}

pub(crate) fn parse_sticker_set_info(value: &Value) -> Option<StickerSetInfo> {
    if value.get("@type").and_then(Value::as_str) != Some("stickerSetInfo") {
        return None;
    }
    let mut covers = Vec::new();
    let mut cover_files = Vec::new();
    for entry in value
        .get("covers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (item, files) = parse_sticker_value(Some(entry));
        covers.extend(item);
        cover_files.extend(files);
    }
    Some(StickerSetInfo {
        id: int64(value.get("id")).unwrap_or(0),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        size: int53_or_zero(value.get("size")).sat_i32(),
        is_installed: value
            .get("is_installed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_official: value
            .get("is_official")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_owned: value
            .get("is_owned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        covers,
        cover_files,
        is_viewed: value
            .get("is_viewed")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    })
}

pub(crate) fn parse_sticker_sets(value: &Value) -> EnvelopePayload {
    let sets = value
        .get("sets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(parse_sticker_set_info)
        .collect();
    EnvelopePayload::Stickers(StickersPayload::StickerSets {
        total_count: int53_or_zero(value.get("total_count")).sat_i32(),
        sets,
    })
}

pub(crate) fn parse_sticker_set(value: &Value) -> EnvelopePayload {
    let mut stickers = Vec::new();
    let mut files = Vec::new();
    if let Some(entries) = value.get("stickers").and_then(Value::as_array) {
        for entry in entries {
            let (item, item_files) = parse_sticker_value(Some(entry));
            if let Some(item) = item {
                stickers.push(item);
            }
            files.extend(item_files);
        }
    }
    EnvelopePayload::Stickers(StickersPayload::StickerSet {
        id: int64(value.get("id")).unwrap_or(0),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        is_installed: value
            .get("is_installed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_owned: value
            .get("is_owned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_custom_emoji: value.pointer("/sticker_type/@type").and_then(Value::as_str)
            == Some("stickerTypeCustomEmoji"),
        stickers,
        files,
    })
}

/// Slice S8: `trendingStickerSets` — same `stickerSetInfo` rows as
/// `stickerSets`, plus the premium-row flag.
pub(crate) fn parse_trending_sticker_sets(value: &Value) -> EnvelopePayload {
    let sets = value
        .get("sets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(parse_sticker_set_info)
        .collect();
    EnvelopePayload::Stickers(StickersPayload::TrendingStickerSets {
        total_count: int53_or_zero(value.get("total_count")).sat_i32(),
        sets,
        is_premium: value
            .get("is_premium")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Slice S8: `stickers` — bare `vector<sticker>` (search / favorites /
/// recent). Same per-entry parse as `stickerSet.stickers.stickers`.
pub(crate) fn parse_stickers(value: &Value) -> EnvelopePayload {
    let mut stickers = Vec::new();
    let mut files = Vec::new();
    if let Some(entries) = value.get("stickers").and_then(Value::as_array) {
        for entry in entries {
            let (item, item_files) = parse_sticker_value(Some(entry));
            if let Some(item) = item {
                stickers.push(item);
            }
            files.extend(item_files);
        }
    }
    EnvelopePayload::Stickers(StickersPayload::Stickers { stickers, files })
}

#[cfg(test)]
mod premium_animation_tests {
    use super::parse_sticker_value;
    use crate::ids::FileId;
    use serde_json::{Value, json};

    fn file(id: i64) -> Value {
        json!({
            "@type": "file", "id": id, "size": 10, "expected_size": 10,
            "local": {"path": "", "can_be_downloaded": true, "is_downloading_completed": false},
            "remote": {"id": "r", "unique_id": "u"}
        })
    }

    fn sticker(full_type: Value) -> Value {
        json!({
            "@type": "sticker", "id": "1", "set_id": "7", "width": 512, "height": 512,
            "emoji": "x", "format": {"@type": "stickerFormatTgs"},
            "full_type": full_type, "thumbnail": null, "sticker": file(42)
        })
    }

    #[test]
    fn premium_animation_is_parsed_and_listed_for_download() {
        let value = sticker(json!({
            "@type": "stickerFullTypeRegular", "premium_animation": file(43)
        }));
        let (item, files) = parse_sticker_value(Some(&value));
        let item = item.unwrap();
        assert_eq!(item.premium_animation, Some(FileId(43)));
        assert!(item.requires_premium);
        assert!(files.iter().any(|f| f.id == FileId(43)));
    }

    #[test]
    fn regular_and_emoji_stickers_have_no_premium_animation() {
        let regular = sticker(json!({
            "@type": "stickerFullTypeRegular", "premium_animation": null
        }));
        let (item, files) = parse_sticker_value(Some(&regular));
        assert_eq!(item.unwrap().premium_animation, None);
        assert_eq!(files.len(), 1);
        let emoji = sticker(json!({
            "@type": "stickerFullTypeCustomEmoji", "custom_emoji_id": "9",
            "needs_repainting": false
        }));
        assert_eq!(
            parse_sticker_value(Some(&emoji))
                .0
                .unwrap()
                .premium_animation,
            None
        );
    }
}
