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

/// Bots slice: one inline query result, reduced to what the results panel
/// shows: text for list rows plus the image for grid cells and list
/// thumbnails.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InlineQueryResultSummary {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub description: String,
    /// `inline_query_id` of the page this result came from. Every page of
    /// a query has its own id, and sending needs the matching one.
    pub query_id: i64,
    /// The image to show, when the result has one. `file_id` 0 means the
    /// result is a media cell whose image has no file yet.
    pub media: Option<InlineMedia>,
}

/// Bots slice: `inlineQueryResultsButton` (TDLib 1.8.68,
/// `schema/td_api.tl:8043`) — the button shown above the results.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineQueryResultsButton {
    pub text: String,
    pub kind: String,
    /// StartBot parameter, empty unless `kind == "start_bot"`
    /// (schema `td_api.tl:8036`).
    pub parameter: String,
    /// WebApp url, empty unless `kind == "web_app"` (schema `td_api.tl:8039`).
    pub url: String,
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
        "inlineQueryResultArticle" => "article",
        "inlineQueryResultContact" => "contact",
        "inlineQueryResultLocation" => "location",
        "inlineQueryResultVenue" => "venue",
        "inlineQueryResultGame" => "game",
        "inlineQueryResultAnimation" => "animation",
        "inlineQueryResultAudio" => "audio",
        "inlineQueryResultDocument" => "document",
        "inlineQueryResultPhoto" => "photo",
        "inlineQueryResultSticker" => "sticker",
        "inlineQueryResultVideo" => "video",
        "inlineQueryResultVoiceNote" => "voice_note",
        _ => "unknown",
    };
    let mut files = Vec::new();
    let mut title = str_field(value, "title");
    let mut description = str_field(value, "description");
    let null = Value::Null;
    let media = match kind {
        "article" | "location" => thumbnail_media(value.get("thumbnail"), &mut files),
        "contact" => {
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
        "venue" => {
            let venue = value.get("venue").unwrap_or(&null);
            title = str_field(venue, "title");
            description = str_field(venue, "address");
            thumbnail_media(value.get("thumbnail"), &mut files)
        }
        "game" => {
            let game = value.get("game").unwrap_or(&null);
            title = str_field(game, "title");
            description = str_field(game, "description");
            None
        }
        "audio" => {
            let audio = value.get("audio").unwrap_or(&null);
            title = str_field(audio, "title");
            if title.is_empty() {
                title = str_field(audio, "file_name");
            }
            description = str_field(audio, "performer");
            None
        }
        "document" => {
            let document = value.get("document").unwrap_or(&null);
            if title.is_empty() {
                title = str_field(document, "file_name");
            }
            thumbnail_media(document.get("thumbnail"), &mut files)
        }
        "photo" => photo_media(value.get("photo"), &mut files),
        "animation" => {
            let (item, item_files) = parse_animation_value(value.get("animation"));
            // No thumbnail yet still gets a cell: the animation's size
            // decides the layout (file 0 draws a placeholder).
            item.map(|item| {
                files.extend(item_files);
                InlineMedia {
                    file_id: item.thumb_file_id.unwrap_or(FileId(0)),
                    width: item.width,
                    height: item.height,
                }
            })
        }
        "sticker" => {
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
        "video" => {
            let video = value.get("video").unwrap_or(&null);
            thumbnail_media(video.get("thumbnail"), &mut files)
        }
        _ => None,
    };
    files.retain(|file| file.id.0 != 0);
    (
        InlineQueryResultSummary {
            id: str_field(value, "id"),
            kind: kind.to_string(),
            title,
            description,
            query_id: 0,
            media,
        },
        files,
    )
}

/// Bots slice: `inlineQueryResultsButton` (schema 1.8.68, line 8043).
pub(crate) fn parse_inline_query_results_button(value: &Value) -> InlineQueryResultsButton {
    let button_type = value.get("type");
    let kind = match button_type
        .and_then(|type_value| type_value.get("@type"))
        .and_then(Value::as_str)
    {
        Some("inlineQueryResultsButtonTypeStartBot") => "start_bot",
        Some("inlineQueryResultsButtonTypeWebApp") => "web_app",
        _ => "unknown",
    };
    InlineQueryResultsButton {
        text: value
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        kind: kind.to_string(),
        parameter: button_type
            .and_then(|type_value| type_value.get("parameter"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        url: button_type
            .and_then(|type_value| type_value.get("url"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
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
            .map(|r| (r.kind.as_str(), r.title.as_str(), r.description.as_str()))
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
            (button.kind.as_str(), button.parameter.as_str()),
            ("start_bot", "p1")
        );
        let app = page(
            json!([]),
            json!({"@type": "inlineQueryResultsButton", "text": "App",
                "type": {"@type": "inlineQueryResultsButtonTypeWebApp", "url": "https://x.test"}}),
        );
        assert_eq!(app.button.unwrap().url, "https://x.test");
        assert!(page(json!([]), Value::Null).button.is_none());
    }
}
