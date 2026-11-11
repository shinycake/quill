use super::*;
use crate::ids::FileId;
use serde_json::Value;

/// Bots slice: one `inlineQueryResults` page (TDLib 1.8.68,
/// `schema/td_api.tl:8051`) — the `getInlineQueryResults` answer (schema
/// line 13413).
#[derive(Debug, Clone, PartialEq)]
pub struct InlineQueryResultsPage {
    pub inline_query_id: i64,
    pub button: Option<InlineQueryResultsButton>,
    pub results: Vec<InlineQueryResultSummary>,
    /// Slice S9: the nested `animation` objects of `inlineQueryResultAnimation`
    /// entries, parsed to `AnimationItem`s for the GIF panel search. The
    /// per-result summaries above carry the same entries' grid image.
    pub animations: Vec<AnimationItem>,
    /// `inlineQueryResultVenue` entries, for Send Location's venue list.
    pub venues: Vec<crate::location_picker::Venue>,
    /// Files referenced by the results (grid images) and by `animations`
    /// (same `remember_files` shape as S8's `Stickers` payload).
    pub files: Vec<ParsedFile>,
    pub next_offset: String,
}

/// Image shown for one inline result: the file to display and the
/// intrinsic size the justified grid lays out by (tdesktop reads the
/// photo / animation size, not the thumbnail's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineMedia {
    pub file_id: FileId,
    pub width: i32,
    pub height: i32,
}

/// Which `inlineQueryResult*` variant a result is; a future variant is
/// `Unknown`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InlineResultKind {
    Article,
    Contact,
    Location,
    Venue,
    Game,
    Animation,
    Audio,
    Document,
    Photo,
    Sticker,
    Video,
    VoiceNote,
    #[default]
    Unknown,
}

impl InlineResultKind {
    /// The snake-case name (`voice_note`), which a row without a title
    /// shows as its label.
    pub fn name(self) -> &'static str {
        match self {
            Self::Article => "article",
            Self::Contact => "contact",
            Self::Location => "location",
            Self::Venue => "venue",
            Self::Game => "game",
            Self::Animation => "animation",
            Self::Audio => "audio",
            Self::Document => "document",
            Self::Photo => "photo",
            Self::Sticker => "sticker",
            Self::Video => "video",
            Self::VoiceNote => "voice_note",
            Self::Unknown => "unknown",
        }
    }
}

impl From<&str> for InlineResultKind {
    fn from(name: &str) -> Self {
        match name {
            "article" => Self::Article,
            "contact" => Self::Contact,
            "location" => Self::Location,
            "venue" => Self::Venue,
            "game" => Self::Game,
            "animation" => Self::Animation,
            "audio" => Self::Audio,
            "document" => Self::Document,
            "photo" => Self::Photo,
            "sticker" => Self::Sticker,
            "video" => Self::Video,
            "voice_note" => Self::VoiceNote,
            _ => Self::Unknown,
        }
    }
}

/// Bots slice: one inline query result, reduced to what the results panel
/// shows: text for list rows plus the image for grid cells and list
/// thumbnails.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InlineQueryResultSummary {
    pub id: String,
    pub kind: InlineResultKind,
    pub title: String,
    pub description: String,
    /// `inline_query_id` of the page this result came from. Every page of
    /// a query has its own id, and sending needs the matching one.
    pub query_id: i64,
    /// The image to show, when the result has one. `file_id` 0 means the
    /// result is a media cell whose image has no file yet.
    pub media: Option<InlineMedia>,
    /// The clip an animation result plays in the grid: Telegram's MP4 GIF
    /// file (true `image/gif` files stay still, as in the history).
    pub clip: Option<FileId>,
    /// An animation's `thumbnailFormatMpeg4` thumbnail: a small MP4 loop
    /// (tens of KB) the grid plays until the full clip is downloaded. Such
    /// a thumbnail is no still image, so `media.file_id` is 0 for it.
    pub preview_clip: Option<FileId>,
}

/// Bots slice: `inlineQueryResultsButton` (TDLib 1.8.68,
/// `schema/td_api.tl:8043`) — the button shown above the results.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineQueryResultsButton {
    pub text: String,
    pub kind: InlineQueryResultsButtonKind,
}

/// What the button does (schema `td_api.tl:8036`, `:8039`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum InlineQueryResultsButtonKind {
    /// Start the bot with `parameter`.
    StartBot { parameter: String },
    /// Open the web app at `url`.
    WebApp { url: String },
    #[default]
    Unknown,
}

fn str_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn join_nonempty(parts: &[String], separator: &str) -> String {
    parts
        .iter()
        .filter(|part| !part.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(separator)
}

/// A `thumbnail` object as display media (its own size).
fn thumbnail_media(thumb: Option<&Value>, files: &mut Vec<ParsedFile>) -> Option<InlineMedia> {
    let thumb = thumb.filter(|thumb| !thumb.is_null())?;
    let file = parse_file(thumb.get("file"))
        .ok()
        .filter(|file| file.id.0 != 0)?;
    let media = InlineMedia {
        file_id: file.id,
        width: int53_or_zero(thumb.get("width")).sat_i32(),
        height: int53_or_zero(thumb.get("height")).sat_i32(),
    };
    files.push(file);
    Some(media)
}

/// The photo size shown in the grid (the `m` box, else the largest at or
/// under 320px, else the smallest) with the largest size's dimensions.
fn photo_media(photo: Option<&Value>, files: &mut Vec<ParsedFile>) -> Option<InlineMedia> {
    let (sizes, size_files) = parse_photo_sizes(photo.filter(|photo| !photo.is_null())?);
    let shown = sizes
        .iter()
        .find(|size| size.type_name == "m")
        .or_else(|| {
            sizes
                .iter()
                .filter(|size| size.width > 0 && size.width <= 320)
                .max_by_key(|size| size.width)
        })
        .or_else(|| sizes.iter().min_by_key(|size| (size.width, size.height)))?;
    let largest = sizes
        .iter()
        .max_by_key(|size| i64::from(size.width) * i64::from(size.height))?;
    let media = InlineMedia {
        file_id: shown.file_id,
        width: largest.width,
        height: largest.height,
    };
    files.extend(size_files);
    Some(media)
}

/// Bots slice: one `InlineQueryResult` (schema lines 7963-8030), reduced
/// to what the results panel shows. Variants without a title or
/// description get one built from their fields (contact name and phone,
/// venue title and address, audio title and performer); an unknown future
/// variant degrades to `kind: "unknown"`. The files of the shown image
/// come back with it.
pub(crate) fn parse_inline_query_result(
    value: &Value,
) -> (InlineQueryResultSummary, Vec<ParsedFile>) {
    let type_name = value
        .get("@type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let kind = match type_name {
        "inlineQueryResultArticle" => InlineResultKind::Article,
        "inlineQueryResultContact" => InlineResultKind::Contact,
        "inlineQueryResultLocation" => InlineResultKind::Location,
        "inlineQueryResultVenue" => InlineResultKind::Venue,
        "inlineQueryResultGame" => InlineResultKind::Game,
        "inlineQueryResultAnimation" => InlineResultKind::Animation,
        "inlineQueryResultAudio" => InlineResultKind::Audio,
        "inlineQueryResultDocument" => InlineResultKind::Document,
        "inlineQueryResultPhoto" => InlineResultKind::Photo,
        "inlineQueryResultSticker" => InlineResultKind::Sticker,
        "inlineQueryResultVideo" => InlineResultKind::Video,
        "inlineQueryResultVoiceNote" => InlineResultKind::VoiceNote,
        _ => InlineResultKind::Unknown,
    };
    let mut files = Vec::new();
    let mut title = str_field(value, "title");
    let mut description = str_field(value, "description");
    let null = Value::Null;
    let mut clip = None;
    let mut preview_clip = None;
    let media = match kind {
        InlineResultKind::Article | InlineResultKind::Location => {
            thumbnail_media(value.get("thumbnail"), &mut files)
        }
        InlineResultKind::Contact => {
            let contact = value.get("contact").unwrap_or(&null);
            title = join_nonempty(
                &[
                    str_field(contact, "first_name"),
                    str_field(contact, "last_name"),
                ],
                " ",
            );
            description = str_field(contact, "phone_number");
            thumbnail_media(value.get("thumbnail"), &mut files)
        }
        InlineResultKind::Venue => {
            let venue = value.get("venue").unwrap_or(&null);
            title = str_field(venue, "title");
            description = str_field(venue, "address");
            thumbnail_media(value.get("thumbnail"), &mut files)
        }
        InlineResultKind::Game => {
            let game = value.get("game").unwrap_or(&null);
            title = str_field(game, "title");
            description = str_field(game, "description");
            None
        }
        InlineResultKind::Audio => {
            let audio = value.get("audio").unwrap_or(&null);
            title = str_field(audio, "title");
            if title.is_empty() {
                title = str_field(audio, "file_name");
            }
            description = str_field(audio, "performer");
            None
        }
        InlineResultKind::Document => {
            let document = value.get("document").unwrap_or(&null);
            if title.is_empty() {
                title = str_field(document, "file_name");
            }
            thumbnail_media(document.get("thumbnail"), &mut files)
        }
        InlineResultKind::Photo => photo_media(value.get("photo"), &mut files),
        InlineResultKind::Animation => {
            let (item, item_files) = parse_animation_value(value.get("animation"));
            // No thumbnail yet still gets a cell: the animation's size
            // decides the layout (file 0 draws a placeholder).
            item.map(|item| {
                files.extend(item_files);
                clip = Some(item.file_id).filter(|id| id.0 != 0 && item.mime_type != "image/gif");
                // An MP4 thumbnail cannot be drawn as an image: it plays
                // as the preview clip instead.
                preview_clip = item.preview_clip();
                let still = item.still_thumb();
                InlineMedia {
                    file_id: still.unwrap_or(FileId(0)),
                    width: item.width,
                    height: item.height,
                }
            })
        }
        InlineResultKind::Sticker => {
            let (item, item_files) = parse_sticker_value(value.get("sticker"));
            item.map(|item| {
                let file_id = item.display_file_id().unwrap_or(FileId(0));
                files.extend(item_files);
                InlineMedia {
                    file_id,
                    width: item.width,
                    height: item.height,
                }
            })
        }
        InlineResultKind::Video => {
            let video = value.get("video").unwrap_or(&null);
            thumbnail_media(video.get("thumbnail"), &mut files)
        }
        _ => None,
    };
    files.retain(|file| file.id.0 != 0);
    (
        InlineQueryResultSummary {
            id: str_field(value, "id"),
            kind,
            title,
            description,
            query_id: 0,
            media,
            clip,
            preview_clip,
        },
        files,
    )
}

/// Bots slice: `inlineQueryResultsButton` (schema 1.8.68, line 8043).
pub(crate) fn parse_inline_query_results_button(value: &Value) -> InlineQueryResultsButton {
    let button_type = value.get("type");
    let text_of = |key: &str| {
        button_type
            .and_then(|type_value| type_value.get(key))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let kind = match button_type
        .and_then(|type_value| type_value.get("@type"))
        .and_then(Value::as_str)
    {
        Some("inlineQueryResultsButtonTypeStartBot") => InlineQueryResultsButtonKind::StartBot {
            parameter: text_of("parameter"),
        },
        Some("inlineQueryResultsButtonTypeWebApp") => InlineQueryResultsButtonKind::WebApp {
            url: text_of("url"),
        },
        _ => InlineQueryResultsButtonKind::Unknown,
    };
    InlineQueryResultsButton {
        text: value
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file(id: i64) -> Value {
        json!({"@type": "file", "id": id, "size": 10, "expected_size": 10, "local": {}})
    }

    fn thumb(id: i64, w: i64, h: i64) -> Value {
        json!({"@type": "thumbnail", "width": w, "height": h, "file": file(id)})
    }

    fn page(results: Value, button: Value) -> InlineQueryResultsPage {
        let value = json!({
            "@type": "inlineQueryResults",
            "inline_query_id": "9007199254740993",
            "button": button,
            "results": results,
            "next_offset": "25",
        });
        match crate::telegram::envelope::parse_envelope(&value.to_string())
            .unwrap()
            .payload
        {
            EnvelopePayload::Bots(BotsPayload::InlineQueryResults(page)) => page,
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn photo_result_keeps_the_m_size_and_the_largest_dimensions() {
        let p = page(
            json!([{
                "@type": "inlineQueryResultPhoto", "id": "p1", "title": "T", "description": "D",
                "photo": {"@type": "photo", "sizes": [
                    {"type": "s", "width": 90, "height": 60, "photo": file(1)},
                    {"type": "m", "width": 320, "height": 213, "photo": file(2)},
                    {"type": "x", "width": 1280, "height": 853, "photo": file(3)},
                ]},
            }]),
            Value::Null,
        );
        let media = p.results[0].media.unwrap();
        assert_eq!(media.file_id, FileId(2));
        assert_eq!((media.width, media.height), (1280, 853));
        assert_eq!(p.results[0].title, "T");
        assert_eq!(p.files.len(), 3);
        // int64 arrives as a string and must not pass through f64.
        assert_eq!(p.inline_query_id, 9007199254740993);
        assert_eq!(p.results[0].query_id, 9007199254740993);
    }

    #[test]
    fn animation_sticker_and_video_results_carry_their_images() {
        let p = page(
            json!([
                {"@type": "inlineQueryResultAnimation", "id": "g", "title": "",
                 "animation": {"@type": "animation", "width": 480, "height": 270,
                   "animation": file(10), "thumbnail": thumb(11, 90, 50)}},
                {"@type": "inlineQueryResultAnimation", "id": "g2", "title": "",
                 "animation": {"@type": "animation", "width": 480, "height": 270,
                   "animation": file(12)}},
                {"@type": "inlineQueryResultSticker", "id": "s",
                 "sticker": {"@type": "sticker", "width": 512, "height": 512,
                   "format": {"@type": "stickerFormatTgs"},
                   "sticker": file(20), "thumbnail": thumb(21, 128, 128)}},
                {"@type": "inlineQueryResultVideo", "id": "v", "title": "Clip", "description": "",
                 "video": {"@type": "video", "width": 640, "height": 360,
                   "video": file(30), "thumbnail": thumb(31, 90, 50)}},
            ]),
            Value::Null,
        );
        let ids: Vec<_> = p
            .results
            .iter()
            .map(|r| r.media.map(|m| m.file_id.0))
            .collect();
        // An animation without a thumbnail keeps its size and shows a placeholder.
        assert_eq!(ids, vec![Some(11), Some(0), Some(21), Some(31)]);
        assert_eq!(p.results[1].media.unwrap().width, 480);
        assert_eq!(p.animations.len(), 2);
        // MP4 GIFs play in the grid; the clip is the animation file.
        assert_eq!(p.results[0].clip, Some(FileId(10)));
        assert_eq!(p.results[2].clip, None);
    }

    #[test]
    fn list_results_build_their_text() {
        let p = page(
            json!([
                {"@type": "inlineQueryResultContact", "id": "c",
                 "contact": {"first_name": "Ada", "last_name": "Lovelace", "phone_number": "+100"},
                 "thumbnail": null},
                {"@type": "inlineQueryResultVenue", "id": "v",
                 "venue": {"title": "Cafe", "address": "1 Main St"}, "thumbnail": thumb(5, 64, 64)},
                {"@type": "inlineQueryResultAudio", "id": "a",
                 "audio": {"title": "", "file_name": "song.mp3", "performer": "Band"}},
                {"@type": "inlineQueryResultGame", "id": "g",
                 "game": {"title": "Chess", "description": "Play"}},
                {"@type": "inlineQueryResultDocument", "id": "d", "title": "",
                 "description": "", "document": {"file_name": "a.pdf", "thumbnail": null}},
                {"@type": "inlineQueryResultVoiceNote", "id": "n", "title": "Note",
                 "voice_note": {}},
                {"@type": "inlineQueryResultFuture", "id": "u"},
            ]),
            Value::Null,
        );
        let text: Vec<_> = p
            .results
            .iter()
            .map(|r| (r.kind.name(), r.title.as_str(), r.description.as_str()))
            .collect();
        assert_eq!(
            text,
            vec![
                ("contact", "Ada Lovelace", "+100"),
                ("venue", "Cafe", "1 Main St"),
                ("audio", "song.mp3", "Band"),
                ("game", "Chess", "Play"),
                ("document", "a.pdf", ""),
                ("voice_note", "Note", ""),
                ("unknown", "", ""),
            ]
        );
        assert_eq!(p.results[1].media.unwrap().file_id, FileId(5));
        assert!(p.results[0].media.is_none());
    }

    #[test]
    fn button_variants_parse() {
        let start = page(
            json!([]),
            json!({"@type": "inlineQueryResultsButton", "text": "Go",
                "type": {"@type": "inlineQueryResultsButtonTypeStartBot", "parameter": "p1"}}),
        );
        let button = start.button.unwrap();
        assert_eq!(
            button.kind,
            InlineQueryResultsButtonKind::StartBot {
                parameter: "p1".into()
            }
        );
        let app = page(
            json!([]),
            json!({"@type": "inlineQueryResultsButton", "text": "App",
                "type": {"@type": "inlineQueryResultsButtonTypeWebApp", "url": "https://x.test"}}),
        );
        assert_eq!(
            app.button.unwrap().kind,
            InlineQueryResultsButtonKind::WebApp {
                url: "https://x.test".into()
            }
        );
        assert!(page(json!([]), Value::Null).button.is_none());
    }
}
