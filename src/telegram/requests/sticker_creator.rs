//! Requests that edit an owned sticker or custom emoji set (TDLib
//! `addStickerToSet`, `removeStickerFromSet`, `setStickerPositionInSet`,
//! `setStickerSetTitle`, `deleteStickerSet`). The set is named by its short
//! name, as TDLib requires; a sticker by its file id.
use crate::ids::{FileId, RequestId};
use crate::telegram::envelope::StickerFormat;
use serde_json::{Value, json};

/// The file of a sticker to add: a freshly rendered image on disk, or a
/// sticker that already exists on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewStickerFile {
    Local(String),
    Existing(FileId),
}

fn input_file(file: &NewStickerFile) -> Value {
    match file {
        NewStickerFile::Local(path) => json!({ "@type": "inputFileLocal", "path": path }),
        NewStickerFile::Existing(id) => json!({ "@type": "inputFileId", "id": id.0 }),
    }
}

fn sticker_format(format: StickerFormat) -> Value {
    let name = match format {
        StickerFormat::Tgs => "stickerFormatTgs",
        StickerFormat::Webm => "stickerFormatWebm",
        StickerFormat::Webp | StickerFormat::Unknown => "stickerFormatWebp",
    };
    json!({ "@type": name })
}

/// `addStickerToSet`. `user_id` is ignored for regular users (TDLib uses
/// the current user), so it is 0.
pub fn add_sticker_to_set(
    extra: RequestId,
    name: &str,
    file: &NewStickerFile,
    format: StickerFormat,
    emojis: &str,
) -> String {
    json!({
        "@type": "addStickerToSet",
        "@extra": extra.as_extra(),
        "user_id": 0,
        "name": name,
        "sticker": {
            "@type": "newSticker",
            "sticker": input_file(file),
            "format": sticker_format(format),
            "emojis": emojis,
            "mask_position": null,
            "keywords": [],
        },
    })
    .to_string()
}

/// `removeStickerFromSet`.
pub fn remove_sticker_from_set(extra: RequestId, file_id: FileId) -> String {
    json!({
        "@type": "removeStickerFromSet",
        "@extra": extra.as_extra(),
        "sticker": { "@type": "inputFileId", "id": file_id.0 },
    })
    .to_string()
}

/// `setStickerPositionInSet`: `position` is 0-based.
pub fn set_sticker_position_in_set(extra: RequestId, file_id: FileId, position: i32) -> String {
    json!({
        "@type": "setStickerPositionInSet",
        "@extra": extra.as_extra(),
        "sticker": { "@type": "inputFileId", "id": file_id.0 },
        "position": position,
    })
    .to_string()
}

/// `setStickerSetTitle`.
pub fn set_sticker_set_title(extra: RequestId, name: &str, title: &str) -> String {
    json!({
        "@type": "setStickerSetTitle",
        "@extra": extra.as_extra(),
        "name": name,
        "title": title,
    })
    .to_string()
}

/// `deleteStickerSet`.
pub fn delete_sticker_set(extra: RequestId, name: &str) -> String {
    json!({
        "@type": "deleteStickerSet",
        "@extra": extra.as_extra(),
        "name": name,
    })
    .to_string()
}
