//! `messagePaidMedia` once the viewer has bought it: the photo or video
//! renders like an ordinary media message (tdesktop swaps the extended
//! preview for the real `Photo` / `Gif` view when the paid media arrives).
//! While it is locked the content stays a `ServiceAction::PaidMedia`
//! (`service_action.rs`) that draws the blurred preview and unlock button.

use super::message_audio_video::parse_message_video;
use super::message_media::parse_message_photo;
use super::*;
use serde_json::{Value, json};

/// A `messagePaidMedia`: while locked a `ServiceAction::PaidMedia`; once
/// bought, a single item is a photo or video message and an album is a
/// `ServiceAction::PaidMedia` carrying every bought item.
pub(crate) fn parse_paid_media(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let caption = value.get("caption").cloned().unwrap_or(Value::Null);
    let above = value
        .get("show_caption_above_media")
        .cloned()
        .unwrap_or(Value::Bool(false));
    let items = value
        .get("media")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let count = items.iter().filter(|item| is_unlocked(item)).count();
    if count == 1 {
        let item = items.iter().find(|item| is_unlocked(item));
        if let Some(parsed) = item.and_then(|item| parse_item(item, &caption, &above)) {
            return parsed;
        }
    } else if count > 1 {
        // The caption belongs to the album, not to each item.
        let no_caption = Value::Null;
        let mut files = Vec::new();
        let unlocked: Vec<MessageContent> = items
            .iter()
            .filter(|item| is_unlocked(item))
            .filter_map(|item| parse_item(item, &no_caption, &Value::Bool(false)))
            .map(|(content, mut item_files)| {
                files.append(&mut item_files);
                content
            })
            .collect();
        let caption_text = caption
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        return (
            MessageContent::Action(Box::new(ServiceAction::PaidMedia {
                stars: value.get("star_count").and_then(Value::as_i64).unwrap_or(0),
                locked: Vec::new(),
                unlocked,
                caption: caption_text,
            })),
            files,
        );
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

fn is_unlocked(item: &Value) -> bool {
    matches!(
        item.get("@type").and_then(Value::as_str),
        Some("paidMediaPhoto" | "paidMediaVideo")
    )
}

/// One bought item as a `messagePhoto` or `messageVideo`.
fn parse_item(
    item: &Value,
    caption: &Value,
    above: &Value,
) -> Option<(MessageContent, Vec<ParsedFile>)> {
    match item.get("@type").and_then(Value::as_str) {
        Some("paidMediaPhoto") => Some(parse_message_photo(&json!({
            "@type": "messagePhoto",
            "photo": item.get("photo"),
            "caption": caption,
            "show_caption_above_media": above,
        }))),
        Some("paidMediaVideo") => Some(parse_message_video(&json!({
            "@type": "messageVideo",
            "video": item.get("video"),
            "caption": caption,
            "show_caption_above_media": above,
            "start_timestamp": item.get("start_timestamp"),
        }))),
        _ => None,
    }
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
        assert!(matches!(
            parse_paid_media(&value).0,
            MessageContent::Action(_)
        ));
        let none =
            json!({"@type": "messagePaidMedia", "media": [{"@type": "paidMediaUnsupported"}]});
        assert!(matches!(
            parse_paid_media(&none).0,
            MessageContent::Action(_)
        ));
    }

    #[test]
    fn a_bought_album_keeps_every_item() {
        let value = json!({"@type": "messagePaidMedia", "star_count": 5,
            "caption": {"@type": "formattedText", "text": "Pair", "entities": []},
            "media": [{"@type": "paidMediaPhoto", "photo": photo_json()},
                      {"@type": "paidMediaPhoto", "photo": photo_json()}]});
        let (content, files) = parse_paid_media(&value);
        let MessageContent::Action(action) = content else {
            panic!("expected an action");
        };
        let ServiceAction::PaidMedia {
            unlocked,
            locked,
            caption,
            ..
        } = *action
        else {
            panic!("expected paid media");
        };
        assert!(locked.is_empty());
        assert_eq!(unlocked.len(), 2);
        assert!(
            unlocked
                .iter()
                .all(|c| matches!(c, MessageContent::Photo(p) if p.caption.is_empty()))
        );
        assert_eq!(caption, "Pair");
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn a_bought_photo_becomes_a_photo_with_the_caption() {
        let value = json!({"@type": "messagePaidMedia", "star_count": 5,
            "caption": {"@type": "formattedText", "text": "Hello", "entities": []},
            "media": [{"@type": "paidMediaPhoto", "photo": photo_json()}]});
        let (content, files) = parse_paid_media(&value);
        let MessageContent::Photo(photo) = content else {
            panic!("expected a photo");
        };
        assert_eq!(photo.caption, "Hello");
        assert_eq!(photo.sizes.len(), 1);
        assert_eq!(files.len(), 1);
    }
}
