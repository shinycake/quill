//! `messagePaidMedia` once the viewer has bought it: the photo or video
//! renders like an ordinary media message (tdesktop swaps the extended
//! preview for the real `Photo` / `Gif` view when the paid media arrives).
//! While it is locked the content stays a `ServiceAction::PaidMedia`
//! (`service_action.rs`) that draws the blurred preview and unlock button.

use super::message_audio_video::parse_message_video;
use super::message_media::parse_message_photo;
use super::*;
use serde_json::{Value, json};

/// A `messagePaidMedia`: the bought photo or video, else the locked
/// preview action.
pub(crate) fn parse_paid_media(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    if let Some(unlocked) = parse_unlocked_paid_media(value) {
        return unlocked;
    }
    let mut files = Vec::new();
    match ServiceAction::from_td("messagePaidMedia", value, &mut files) {
        Some(action) => (MessageContent::Action(Box::new(action)), files),
        None => (
            MessageContent::Unsupported {
                type_name: "messagePaidMedia".into(),
            },
            Vec::new(),
        ),
    }
}

/// The first unlocked item of a `messagePaidMedia` as a `messagePhoto` or
/// `messageVideo`, or `None` while the media is still locked (only
/// `paidMediaPreview` items) or unsupported.
pub(crate) fn parse_unlocked_paid_media(
    value: &Value,
) -> Option<(MessageContent, Vec<ParsedFile>)> {
    let items = value.get("media")?.as_array()?;
    let caption = value.get("caption").cloned().unwrap_or(Value::Null);
    let above = value
        .get("show_caption_above_media")
        .cloned()
        .unwrap_or(Value::Bool(false));
    for item in items {
        match item.get("@type").and_then(Value::as_str) {
            Some("paidMediaPhoto") => {
                return Some(parse_message_photo(&json!({
                    "@type": "messagePhoto",
                    "photo": item.get("photo"),
                    "caption": caption,
                    "show_caption_above_media": above,
                })));
            }
            Some("paidMediaVideo") => {
                return Some(parse_message_video(&json!({
                    "@type": "messageVideo",
                    "video": item.get("video"),
                    "caption": caption,
                    "show_caption_above_media": above,
                    "start_timestamp": item.get("start_timestamp"),
                })));
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo_json() -> Value {
        json!({"@type": "photo", "has_stickers": false, "minithumbnail": null, "sizes": [
            {"@type": "photoSize", "type": "x", "width": 800, "height": 600,
             "photo": {"@type": "file", "id": 9, "size": 10, "expected_size": 10,
                       "local": {"path": "", "can_be_downloaded": true}}}]})
    }

    #[test]
    fn locked_media_is_not_unlocked_content() {
        let value = json!({"@type": "messagePaidMedia", "star_count": 5, "media": [
            {"@type": "paidMediaPreview", "width": 4, "height": 3, "duration": 0,
             "minithumbnail": null}]});
        assert!(parse_unlocked_paid_media(&value).is_none());
        let none =
            json!({"@type": "messagePaidMedia", "media": [{"@type": "paidMediaUnsupported"}]});
        assert!(parse_unlocked_paid_media(&none).is_none());
    }

    #[test]
    fn a_bought_photo_becomes_a_photo_with_the_caption() {
        let value = json!({"@type": "messagePaidMedia", "star_count": 5,
            "caption": {"@type": "formattedText", "text": "Hello", "entities": []},
            "media": [{"@type": "paidMediaPhoto", "photo": photo_json()}]});
        let (content, files) = parse_unlocked_paid_media(&value).expect("unlocked");
        let MessageContent::Photo(photo) = content else {
            panic!("expected a photo");
        };
        assert_eq!(photo.caption, "Hello");
        assert_eq!(photo.sizes.len(), 1);
        assert_eq!(files.len(), 1);
    }
}
