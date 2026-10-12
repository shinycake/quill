//! The media of a poll option (`pollOption.media`, TDLib 1.8.68 `PollMedia`):
//! a small picture, clip, sticker, link or place next to the option's text.
//! TDLib limits an option to `pollMediaAnimation`, `pollMediaLink`,
//! `pollMediaLocation`, `pollMediaPhoto`, `pollMediaSticker`,
//! `pollMediaVenue` and `pollMediaVideo`; anything else is dropped, as the
//! option is then shown without a picture.

use super::message_audio_video::parse_animation_value;
use super::message_link_preview::parse_link_preview;
use super::message_location::geo_location;
use super::message_sticker::parse_sticker_value;
use super::*;
use crate::ids::FileId;
use serde_json::Value;

/// `video` of a `pollMediaVideo`: what the viewer and the thumbnail need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollOptionVideo {
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub mime_type: String,
    /// The clip itself (`video.video`).
    pub file_id: FileId,
    /// `video.thumbnail`, else the `cover` photo's smallest size.
    pub thumb_file_id: Option<FileId>,
}

/// One option's media.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOptionMedia {
    /// `pollMediaPhoto`; a live photo's `video` is not kept, the still shows.
    Photo(PhotoContent),
    Video(PollOptionVideo),
    Animation(AnimationItem),
    Sticker(StickerItem),
    /// `pollMediaLink`: the address and, once TDLib has it, the preview's
    /// picture (tdesktop shows the webpage photo, else a link glyph).
    Link {
        url: String,
        photo: Option<PhotoContent>,
    },
    Location(GeoLocation),
    Venue {
        location: GeoLocation,
        title: String,
    },
}

impl PollOptionMedia {
    /// The place to pin on a map tile.
    pub fn place(&self) -> Option<GeoLocation> {
        match self {
            Self::Location(location) | Self::Venue { location, .. } => Some(*location),
            _ => None,
        }
    }

    /// Files worth showing as the option's small picture, best first. A
    /// place has none (its map tile is asked for separately).
    pub fn thumb_candidates(&self) -> Vec<FileId> {
        let mut ids = Vec::new();
        match self {
            Self::Photo(photo)
            | Self::Link {
                photo: Some(photo), ..
            } => {
                ids.extend(photo.thumb_size().map(|size| size.file_id));
                ids.extend(photo.largest_size().map(|size| size.file_id));
            }
            Self::Video(video) => ids.extend(video.thumb_file_id),
            Self::Animation(animation) => ids.extend(animation.thumb_file_id),
            Self::Sticker(sticker) => ids.extend(sticker.display_file_id()),
            Self::Link { photo: None, .. } | Self::Location(_) | Self::Venue { .. } => {}
        }
        ids.retain(|id| id.0 != 0);
        ids.dedup();
        ids
    }
}

/// `pollOption.media`: `None` when the option has none, or when it is of a
/// kind a poll option may not carry.
pub(crate) fn parse_poll_option_media(
    value: Option<&Value>,
) -> (Option<PollOptionMedia>, Vec<ParsedFile>) {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return (None, Vec::new());
    };
    match value.get("@type").and_then(Value::as_str) {
        Some("pollMediaPhoto") => {
            let Some(photo) = value.get("photo").filter(|photo| !photo.is_null()) else {
                return (None, Vec::new());
            };
            let (sizes, files) = parse_photo_sizes(photo);
            if sizes.is_empty() {
                return (None, files);
            }
            (Some(PollOptionMedia::Photo(photo_of(sizes))), files)
        }
        Some("pollMediaVideo") => parse_video(value),
        Some("pollMediaAnimation") => {
            let (item, mut files) = parse_animation_value(value.get("animation"));
            files.retain(|file| file.id.0 != 0);
            (item.map(PollOptionMedia::Animation), files)
        }
        Some("pollMediaSticker") => {
            let (item, mut files) = parse_sticker_value(value.get("sticker"));
            files.retain(|file| file.id.0 != 0);
            (item.map(PollOptionMedia::Sticker), files)
        }
        Some("pollMediaLink") => {
            let url = value
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if url.is_empty() {
                return (None, Vec::new());
            }
            let (preview, files) = parse_link_preview(value.get("link_preview"));
            let photo = preview.and_then(|preview| preview.photo);
            (Some(PollOptionMedia::Link { url, photo }), files)
        }
        Some("pollMediaLocation") => (
            geo_location(value.get("location")).map(PollOptionMedia::Location),
            Vec::new(),
        ),
        Some("pollMediaVenue") => {
            let venue = value.get("venue");
            let location = geo_location(venue.and_then(|venue| venue.get("location")));
            let title = venue
                .and_then(|venue| venue.get("title"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            (
                location.map(|location| PollOptionMedia::Venue { location, title }),
                Vec::new(),
            )
        }
        _ => (None, Vec::new()),
    }
}

fn photo_of(sizes: Vec<PhotoSizeView>) -> PhotoContent {
    PhotoContent {
        caption: String::new(),
        caption_entities: Vec::new(),
        show_caption_above_media: false,
        sizes,
        is_secret: false,
        has_spoiler: false,
        has_stickers: false,
        minithumbnail: None,
    }
}

fn parse_video(value: &Value) -> (Option<PollOptionMedia>, Vec<ParsedFile>) {
    let Some(video) = value
        .get("video")
        .filter(|video| video.get("@type").and_then(Value::as_str) == Some("video"))
    else {
        return (None, Vec::new());
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
    let mut thumb_file_id = video
        .get("thumbnail")
        .filter(|thumb| !thumb.is_null())
        .and_then(|thumb| parse_file(thumb.get("file")).ok())
        .map(|file| {
            let id = file.id;
            files.push(file);
            id
        })
        .filter(|id| id.0 != 0);
    if thumb_file_id.is_none()
        && let Some(cover) = value.get("cover").filter(|cover| !cover.is_null())
    {
        let (sizes, cover_files) = parse_photo_sizes(cover);
        thumb_file_id = photo_of(sizes).thumb_size().map(|size| size.file_id);
        files.extend(cover_files);
    }
    files.retain(|file| file.id.0 != 0);
    (
        Some(PollOptionMedia::Video(PollOptionVideo {
            duration: int53_or_zero(video.get("duration")).sat_i32(),
            width: int53_or_zero(video.get("width")).sat_i32(),
            height: int53_or_zero(video.get("height")).sat_i32(),
            mime_type: video
                .get("mime_type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            file_id,
            thumb_file_id,
        })),
        files,
    )
}

#[cfg(test)]
mod tests {
    use super::{PollOptionMedia, parse_poll_option_media};
    use serde_json::json;

    fn file(id: i32) -> serde_json::Value {
        json!({"@type":"file","id":id,"size":10,"expected_size":10,
            "local":{"@type":"localFile","path":"","can_be_downloaded":true,
                "can_be_deleted":false,"is_downloading_active":false,
                "is_downloading_completed":false,"download_offset":0,
                "downloaded_prefix_size":0,"downloaded_size":0},
            "remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,
                "is_uploading_completed":true,"uploaded_size":10}})
    }

    #[test]
    fn a_photo_keeps_its_sizes_and_files() {
        let media = json!({"@type":"pollMediaPhoto","video":null,"photo":{
            "@type":"photo","has_stickers":false,"sizes":[
                {"@type":"photoSize","type":"s","photo":file(11),"width":90,"height":90,"progressive_sizes":[]},
                {"@type":"photoSize","type":"x","photo":file(12),"width":800,"height":800,"progressive_sizes":[]}]}});
        let (parsed, files) = parse_poll_option_media(Some(&media));
        let Some(PollOptionMedia::Photo(photo)) = parsed else {
            panic!("a photo");
        };
        assert_eq!(photo.sizes.len(), 2);
        assert_eq!(files.len(), 2);
        assert_eq!(photo.largest_size().unwrap().file_id.0, 12);
    }

    #[test]
    fn a_link_keeps_its_address_and_the_previews_picture() {
        let media = json!({"@type":"pollMediaLink","url":" https://example.com/a ",
            "link_preview":{"@type":"linkPreview","url":"https://example.com/a",
                "display_url":"example.com/a","site_name":"Example","title":"A","description":null,
                "author":"","type":{"@type":"linkPreviewTypePhoto","photo":{"@type":"photo",
                    "has_stickers":false,"sizes":[{"@type":"photoSize","type":"m","photo":file(21),
                    "width":320,"height":200,"progressive_sizes":[]}]},"author":""},
                "has_large_media":false,"show_large_media":false,"show_media_above_description":false,
                "skip_confirmation":false,"show_above_text":false,"instant_view_version":0}});
        let (parsed, files) = parse_poll_option_media(Some(&media));
        let Some(PollOptionMedia::Link { url, photo }) = parsed else {
            panic!("a link");
        };
        assert_eq!(url, "https://example.com/a");
        assert!(photo.is_some());
        assert_eq!(files.len(), 1);
        // A link without a preview still parses.
        let bare = json!({"@type":"pollMediaLink","url":"https://example.com","link_preview":null});
        assert!(matches!(
            parse_poll_option_media(Some(&bare)).0,
            Some(PollOptionMedia::Link { photo: None, .. })
        ));
        let empty = json!({"@type":"pollMediaLink","url":"  "});
        assert_eq!(parse_poll_option_media(Some(&empty)).0, None);
    }

    #[test]
    fn places_parse_and_bad_coordinates_are_dropped() {
        let location = json!({"@type":"pollMediaLocation","location":
            {"@type":"location","latitude":48.8566,"longitude":2.3522,"horizontal_accuracy":0.0}});
        let (parsed, _) = parse_poll_option_media(Some(&location));
        assert_eq!(parsed.unwrap().place().unwrap().lat_e6, 48_856_600);
        let venue = json!({"@type":"pollMediaVenue","venue":{"@type":"venue","title":"Louvre",
            "address":"","provider":"","id":"","type":"",
            "location":{"@type":"location","latitude":48.86,"longitude":2.33}}});
        let (parsed, _) = parse_poll_option_media(Some(&venue));
        assert!(matches!(
            parsed,
            Some(PollOptionMedia::Venue { title, .. }) if title == "Louvre"
        ));
        let bad = json!({"@type":"pollMediaLocation","location":
            {"@type":"location","latitude":123.0,"longitude":2.0}});
        assert_eq!(parse_poll_option_media(Some(&bad)).0, None);
    }

    #[test]
    fn a_video_names_its_clip_and_a_thumbnail() {
        let media = json!({"@type":"pollMediaVideo","video":{"@type":"video","duration":7,
            "width":640,"height":360,"file_name":"a.mp4","mime_type":"video/mp4",
            "has_stickers":false,"supports_streaming":true,"minithumbnail":null,
            "thumbnail":{"@type":"thumbnail","format":{"@type":"thumbnailFormatJpeg"},
                "width":320,"height":180,"file":file(32)},"video":file(31)},
            "alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0});
        let (parsed, files) = parse_poll_option_media(Some(&media));
        let Some(PollOptionMedia::Video(video)) = parsed else {
            panic!("a video");
        };
        assert_eq!(
            (video.file_id.0, video.thumb_file_id.map(|id| id.0)),
            (31, Some(32))
        );
        assert_eq!(video.duration, 7);
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn missing_null_and_unsupported_media_give_none() {
        assert_eq!(parse_poll_option_media(None).0, None);
        assert_eq!(parse_poll_option_media(Some(&json!(null))).0, None);
        let audio = json!({"@type":"pollMediaAudio","audio":{}});
        assert_eq!(parse_poll_option_media(Some(&audio)).0, None);
    }
}
