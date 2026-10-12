//! Wallpaper and theme link previews (tdesktop `WebPageType::WallPaper` and
//! `WebPageType::Theme`, drawn by `HistoryView::ThemeDocument`).
//!
//! A `t.me/bg/...` link carries `linkPreviewTypeBackground` (the wallpaper's
//! document and its type) and a `t.me/addtheme/...` link carries
//! `linkPreviewTypeTheme` (the theme files for each platform). The card shows
//! the document's preview picture with its size, under the text, and a "View"
//! button when the page has a wallpaper or Desktop theme document.

use super::super::backgrounds::{Background, BackgroundType, parse_background_type};
use super::super::json_helpers::{int53_or_zero, json_field_str};
use super::super::message_media::{ParsedFile, parse_file};
use super::LinkPreviewKind;
use crate::ids::FileId;
use serde_json::Value;

/// The wallpaper behind a `bg/` link, for the card's picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewWallpaper {
    /// The wallpaper as the preview paints it: its type, the document's
    /// file (a pattern's PNG / TGV) and its small preview picture.
    pub background: Background,
    /// Size of the preview picture, for the card's proportions
    /// (0 when unknown).
    pub thumb_width: i32,
    pub thumb_height: i32,
    /// `document.document.size`; 0 when the link has no document.
    pub size: i64,
    /// The link has a wallpaper document, so the card gets its button.
    pub has_document: bool,
}

/// The Telegram Desktop theme file behind an `addtheme/` link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewTheme {
    /// The document's preview picture, when it has one.
    pub thumbnail: Option<ParsedFile>,
    /// `document.document.size`.
    pub size: i64,
}

/// tdesktop `DocumentData::isTheme`: a `.tdesktop-theme` or
/// `.tdesktop-palette` file, or an unnamed file of the Desktop theme type.
pub fn is_desktop_theme_document(file_name: &str, mime_type: &str) -> bool {
    let name = file_name.to_ascii_lowercase();
    name.ends_with(".tdesktop-theme")
        || name.ends_with(".tdesktop-palette")
        || (mime_type == "application/x-tgtheme-tdesktop"
            && (name.is_empty() || !name.contains('.')))
}

impl LinkPreviewKind {
    /// The files a wallpaper or theme card paints from: a photo's small
    /// preview (its file when it has none), a pattern's file, a theme's
    /// preview. Fills have none.
    pub fn picture_files(&self) -> Vec<FileId> {
        match self {
            LinkPreviewKind::Background(wallpaper) => {
                let background = &wallpaper.background;
                let file = match background.kind {
                    BackgroundType::Wallpaper { .. } => {
                        background.thumbnail.as_ref().or(background.file.as_ref())
                    }
                    BackgroundType::Pattern { .. } => background.file.as_ref(),
                    _ => None,
                };
                file.map(|file| file.id).into_iter().collect()
            }
            LinkPreviewKind::Theme(theme) => theme
                .thumbnail
                .as_ref()
                .map(|file| file.id)
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// tdesktop `Ui::FormatSizeText`: "512 B", "12.3 KB", "4.5 MB".
pub fn format_size_text(size: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = 1024 * 1024;
    if size >= MB {
        let tenths = size * 10 / MB;
        format!("{}.{} MB", tenths / 10, tenths % 10)
    } else if size >= KB {
        let tenths = size * 10 / KB;
        format!("{}.{} KB", tenths / 10, tenths % 10)
    } else {
        format!("{size} B")
    }
}

fn thumbnail_of(document: &Value) -> (Option<ParsedFile>, i32, i32) {
    let thumbnail = document.get("thumbnail").filter(|value| !value.is_null());
    let file = thumbnail.and_then(|thumb| parse_file(thumb.get("file")).ok());
    let dim = |key: &str| {
        thumbnail
            .map(|thumb| int53_or_zero(thumb.get(key)).clamp(0, i32::MAX as i64) as i32)
            .unwrap_or(0)
    };
    (file, dim("width"), dim("height"))
}

fn file_size(file: Option<&ParsedFile>) -> i64 {
    file.map_or(0, |file| file.size.max(file.expected_size))
}

/// `linkPreviewTypeBackground document:document background_type:BackgroundType`.
pub(crate) fn parse_background_preview(
    preview_type: &Value,
) -> Option<(LinkPreviewKind, Vec<ParsedFile>)> {
    let kind = parse_background_type(preview_type.get("background_type"))?;
    let document = preview_type
        .get("document")
        .filter(|value| !value.is_null());
    let file = document.and_then(|doc| parse_file(doc.get("document")).ok());
    let (thumbnail, thumb_width, thumb_height) = document.map(thumbnail_of).unwrap_or_default();
    let mut files = Vec::new();
    files.extend(file.clone());
    files.extend(thumbnail.clone());
    let wallpaper = PreviewWallpaper {
        thumb_width,
        thumb_height,
        size: file_size(file.as_ref()),
        has_document: document.is_some(),
        background: Background {
            id: 0,
            is_default: false,
            is_dark: false,
            name: String::new(),
            file,
            thumbnail,
            kind,
        },
    };
    Some((LinkPreviewKind::Background(Box::new(wallpaper)), files))
}

/// `linkPreviewTypeTheme documents:vector<document> settings:themeSettings`:
/// the first Desktop theme document, as tdesktop's `lookupThemeDocument`.
/// `None` when no document is one, so the card stays a plain one.
pub(crate) fn parse_theme_preview(
    preview_type: &Value,
) -> Option<(LinkPreviewKind, Vec<ParsedFile>)> {
    let documents = preview_type.get("documents").and_then(Value::as_array)?;
    let document = documents.iter().find(|document| {
        is_desktop_theme_document(
            &json_field_str(document, "file_name"),
            &json_field_str(document, "mime_type"),
        )
    })?;
    let file = parse_file(document.get("document")).ok();
    let (thumbnail, _, _) = thumbnail_of(document);
    let mut files = Vec::new();
    files.extend(file.clone());
    files.extend(thumbnail.clone());
    let theme = PreviewTheme {
        thumbnail,
        size: file_size(file.as_ref()),
    };
    Some((LinkPreviewKind::Theme(Box::new(theme)), files))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn document(name: &str, mime: &str, file: i64, thumb: Option<i64>) -> Value {
        json!({
            "@type": "document", "file_name": name, "mime_type": mime,
            "thumbnail": thumb.map(|id| json!({
                "@type": "thumbnail", "width": 320, "height": 480,
                "file": {"@type": "file", "id": id, "size": 9}
            })),
            "document": {"@type": "file", "id": file, "size": 4096}
        })
    }

    #[test]
    fn cards_name_the_files_they_paint_from() {
        let photo = LinkPreviewKind::Background(Box::new(PreviewWallpaper {
            background: Background {
                id: 0,
                is_default: false,
                is_dark: false,
                name: String::new(),
                file: parse_file(Some(&json!({"@type": "file", "id": 7}))).ok(),
                thumbnail: parse_file(Some(&json!({"@type": "file", "id": 8}))).ok(),
                kind: BackgroundType::Wallpaper {
                    blurred: false,
                    moving: false,
                },
            },
            thumb_width: 0,
            thumb_height: 0,
            size: 0,
            has_document: true,
        }));
        assert_eq!(photo.picture_files(), vec![FileId(8)]);
        assert!(LinkPreviewKind::Plain.picture_files().is_empty());
    }

    #[test]
    fn sizes_read_like_tdesktop() {
        assert_eq!(format_size_text(0), "0 B");
        assert_eq!(format_size_text(1023), "1023 B");
        assert_eq!(format_size_text(1024), "1.0 KB");
        assert_eq!(format_size_text(12_595), "12.2 KB");
        assert_eq!(format_size_text(1024 * 1024), "1.0 MB");
        assert_eq!(format_size_text(4_718_592), "4.5 MB");
    }

    #[test]
    fn desktop_theme_files_are_recognised_like_tdesktop() {
        assert!(is_desktop_theme_document("night.tdesktop-theme", ""));
        assert!(is_desktop_theme_document(
            "A.TDESKTOP-PALETTE",
            "text/plain"
        ));
        assert!(is_desktop_theme_document(
            "",
            "application/x-tgtheme-tdesktop"
        ));
        assert!(is_desktop_theme_document(
            "theme",
            "application/x-tgtheme-tdesktop"
        ));
        assert!(!is_desktop_theme_document(
            "a.attheme",
            "application/x-tgtheme-android"
        ));
        assert!(!is_desktop_theme_document(
            "x.zip",
            "application/x-tgtheme-tdesktop"
        ));
    }

    #[test]
    fn background_card_keeps_the_document_and_thumbnail() {
        let (kind, files) = parse_background_preview(&json!({
            "@type": "linkPreviewTypeBackground",
            "document": document("sky.jpg", "image/jpeg", 7, Some(8)),
            "background_type": {"@type": "backgroundTypeWallpaper", "is_blurred": true, "is_moving": false}
        }))
        .unwrap();
        let LinkPreviewKind::Background(wallpaper) = kind else {
            panic!("not a background card");
        };
        assert!(wallpaper.has_document);
        assert_eq!((wallpaper.thumb_width, wallpaper.thumb_height), (320, 480));
        assert_eq!(wallpaper.size, 4096);
        assert_eq!(
            wallpaper.background.thumbnail.as_ref().map(|f| f.id.0),
            Some(8)
        );
        assert_eq!(
            wallpaper.background.kind,
            BackgroundType::Wallpaper {
                blurred: true,
                moving: false
            }
        );
        assert_eq!(files.iter().map(|f| f.id.0).collect::<Vec<_>>(), vec![7, 8]);
    }

    #[test]
    fn a_fill_link_without_a_document_has_no_button_document() {
        let (kind, files) = parse_background_preview(&json!({
            "@type": "linkPreviewTypeBackground",
            "document": null,
            "background_type": {"@type": "backgroundTypeFill",
                "fill": {"@type": "backgroundFillSolid", "color": 255}}
        }))
        .unwrap();
        let LinkPreviewKind::Background(wallpaper) = kind else {
            panic!("not a background card");
        };
        assert!(!wallpaper.has_document);
        assert!(files.is_empty());
        assert_eq!(wallpaper.size, 0);
    }

    #[test]
    fn an_unknown_background_type_is_a_plain_card() {
        assert!(
            parse_background_preview(&json!({
                "@type": "linkPreviewTypeBackground",
                "background_type": {"@type": "backgroundTypeNew"}
            }))
            .is_none()
        );
    }

    #[test]
    fn theme_card_takes_the_first_desktop_theme_document() {
        let (kind, files) = parse_theme_preview(&json!({
            "@type": "linkPreviewTypeTheme",
            "documents": [
                document("a.attheme", "application/x-tgtheme-android", 1, Some(2)),
                document("b.tdesktop-theme", "application/x-tgtheme-tdesktop", 3, Some(4)),
                document("c.tdesktop-theme", "application/x-tgtheme-tdesktop", 5, Some(6)),
            ]
        }))
        .unwrap();
        let LinkPreviewKind::Theme(theme) = kind else {
            panic!("not a theme card");
        };
        assert_eq!(theme.thumbnail.as_ref().map(|f| f.id.0), Some(4));
        assert_eq!(theme.size, 4096);
        assert_eq!(files.iter().map(|f| f.id.0).collect::<Vec<_>>(), vec![3, 4]);
    }

    #[test]
    fn theme_without_a_desktop_file_stays_plain() {
        assert!(
            parse_theme_preview(&json!({
                "@type": "linkPreviewTypeTheme",
                "documents": [document("a.attheme", "application/x-tgtheme-android", 1, None)]
            }))
            .is_none()
        );
        assert!(parse_theme_preview(&json!({"@type": "linkPreviewTypeTheme"})).is_none());
    }
}
