use super::*;
use serde_json::Value;

/// `linkPreview` card. Photo comes from `type` when that constructor carries a `photo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkPreview {
    pub url: String,
    pub display_url: String,
    pub site_name: String,
    pub title: String,
    pub description: String,
    pub show_large_media: bool,
    /// MED4b: `linkPreview.has_large_media` (schema:4570) — whether a
    /// large-media variant exists at all. TGX gates the large/small
    /// toggle on this (`LinkPreview.toggleLargeMedia` no-ops without
    /// it); the composer chip does the same.
    pub has_large_media: bool,
    pub show_media_above_description: bool,
    pub show_above_text: bool,
    pub instant_view_version: i32,
    pub photo: Option<PhotoContent>,
    /// MED4: the `linkPreviewType*` behind the card (embedded players /
    /// album strips need more than the plain card).
    pub kind: LinkPreviewKind,
}

/// MED4: `linkPreviewType*` (TDLib 1.8.67, `schema/td_api.tl:4392` album,
/// `:4434/:4443/:4452` embedded players). Plain article/photo/video types
/// render as the standard card; only the kinds needing distinct UI are
/// carried here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LinkPreviewKind {
    #[default]
    Plain,
    /// Embedded player (`linkPreviewTypeEmbedded{Video,Audio,Animation}Player`).
    /// The card shows a play badge; tap opens `url` in the browser (inline
    /// playback is out of slice — see DECISIONS.md MED4).
    EmbeddedPlayer {
        url: String,
        duration_secs: i32,
        audio: bool,
    },
    /// `linkPreviewTypeAlbum` — up to 4 thumbnails for the strip.
    Album { thumbnails: Vec<PhotoContent> },
}

impl LinkPreview {
    pub fn has_card(&self) -> bool {
        !self.url.is_empty()
            || !self.site_name.is_empty()
            || !self.title.is_empty()
            || !self.description.is_empty()
            || self.photo.is_some()
    }
}

pub(crate) fn parse_link_preview(value: Option<&Value>) -> (Option<LinkPreview>, Vec<ParsedFile>) {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return (None, Vec::new());
    };
    if value.get("@type").and_then(Value::as_str) != Some("linkPreview") {
        return (None, Vec::new());
    }
    let (photo, files) = value
        .get("type")
        .filter(|preview_type| !preview_type.is_null())
        .map(parse_link_preview_photo)
        .unwrap_or((None, Vec::new()));
    // MED4: embedded players / album strips ride the `type` object too.
    let (kind, kind_files) = parse_link_preview_kind(value.get("type"));
    let mut files = files;
    files.extend(kind_files);
    (
        Some(LinkPreview {
            url: json_field_str(value, "url"),
            display_url: json_field_str(value, "display_url"),
            site_name: json_field_str(value, "site_name"),
            title: json_field_str(value, "title"),
            description: parse_formatted_text(value.get("description")),
            show_large_media: json_bool(value.get("show_large_media"), false),
            has_large_media: json_bool(value.get("has_large_media"), false),
            show_media_above_description: json_bool(
                value.get("show_media_above_description"),
                false,
            ),
            show_above_text: json_bool(value.get("show_above_text"), false),
            // MED4: `linkPreview.instant_view_version` (schema:4570) — the
            // IV reader opens when this is > 0.
            instant_view_version: int53_or_zero(value.get("instant_view_version")).sat_i32(),
            photo,
            kind,
        }),
        files,
    )
}

/// MED4: classify the `linkPreview.type` object (schema:4392 album,
/// :4434/:4443/:4452 embedded players). Anything else is `Plain` — the
/// standard card already covers it.
pub(crate) fn parse_link_preview_kind(
    preview_type: Option<&Value>,
) -> (LinkPreviewKind, Vec<ParsedFile>) {
    let Some(preview_type) = preview_type.filter(|value| !value.is_null()) else {
        return (LinkPreviewKind::Plain, Vec::new());
    };
    match preview_type
        .get("@type")
        .and_then(Value::as_str)
        .unwrap_or("")
    {
        "linkPreviewTypeEmbeddedVideoPlayer" | "linkPreviewTypeEmbeddedAnimationPlayer" => (
            LinkPreviewKind::EmbeddedPlayer {
                url: json_field_str(preview_type, "url"),
                duration_secs: int53_or_zero(preview_type.get("duration")).sat_i32(),
                audio: false,
            },
            Vec::new(),
        ),
        "linkPreviewTypeEmbeddedAudioPlayer" => (
            LinkPreviewKind::EmbeddedPlayer {
                url: json_field_str(preview_type, "url"),
                duration_secs: int53_or_zero(preview_type.get("duration")).sat_i32(),
                audio: true,
            },
            Vec::new(),
        ),
        "linkPreviewTypeAlbum" => {
            let mut thumbnails = Vec::new();
            let mut files = Vec::new();
            let media = preview_type
                .get("media")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            // ponytail: 4 thumbs is the strip's ceiling; the card never
            // needs the whole album.
            for item in media.iter().take(4) {
                match item.get("@type").and_then(Value::as_str).unwrap_or("") {
                    "linkPreviewAlbumMediaPhoto" => {
                        let (sizes, item_files) =
                            parse_photo_sizes(item.get("photo").unwrap_or(&Value::Null));
                        files.extend(item_files);
                        if !sizes.is_empty() {
                            thumbnails.push(PhotoContent {
                                has_stickers: false,
                                caption: String::new(),
                                caption_entities: Vec::new(),
                                show_caption_above_media: false,
                                sizes,
                                is_secret: false,
                                has_spoiler: false,
                                minithumbnail: None,
                            });
                        }
                    }
                    "linkPreviewAlbumMediaVideo" => {
                        let thumb = item.get("video").and_then(|video| video.get("thumbnail"));
                        if let Some(thumb) = thumb.filter(|thumb| !thumb.is_null())
                            && let Ok(file) = parse_file(thumb.get("file"))
                        {
                            let id = file.id;
                            files.push(file);
                            thumbnails.push(PhotoContent {
                                has_stickers: false,
                                caption: String::new(),
                                caption_entities: Vec::new(),
                                show_caption_above_media: false,
                                sizes: vec![PhotoSizeView {
                                    type_name: "t".to_string(),
                                    width: int53_or_zero(thumb.get("width")).sat_i32(),
                                    height: int53_or_zero(thumb.get("height")).sat_i32(),
                                    file_id: id,
                                }],
                                is_secret: false,
                                has_spoiler: false,
                                minithumbnail: None,
                            });
                        }
                    }
                    _ => {}
                }
            }
            (LinkPreviewKind::Album { thumbnails }, files)
        }
        _ => (LinkPreviewKind::Plain, Vec::new()),
    }
}

/// Photo on `linkPreviewTypeArticle` / `Photo` (`photo`) and embedded players (`thumbnail` / `cover`).
pub(crate) fn parse_link_preview_photo(
    preview_type: &Value,
) -> (Option<PhotoContent>, Vec<ParsedFile>) {
    for key in ["photo", "thumbnail", "cover"] {
        let Some(candidate) = preview_type.get(key).filter(|value| !value.is_null()) else {
            continue;
        };
        let typed_photo = candidate.get("@type").and_then(Value::as_str) == Some("photo");
        if !typed_photo && candidate.get("sizes").and_then(Value::as_array).is_none() {
            continue;
        }
        let (sizes, files) = parse_photo_sizes(candidate);
        if sizes.is_empty() {
            continue;
        }
        return (
            Some(PhotoContent {
                has_stickers: false,
                caption: String::new(),
                caption_entities: Vec::new(),
                show_caption_above_media: false,
                sizes,
                is_secret: false,
                has_spoiler: false,
                minithumbnail: None,
            }),
            files,
        );
    }
    (None, Vec::new())
}
