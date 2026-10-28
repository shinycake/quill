//! `messageAnimatedEmoji`: a message that is one emoji TDLib has a sticker
//! for. It stays a text message (copy, search, replies and the chat list
//! read its text) that also carries the sticker, its skin tone and its
//! sound (`TextContent::animated`). A custom emoji gets its entity back.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ids::FileId;
use crate::sticker_playback::FitzType;
use crate::text::{TextEntity, TextEntityKind};
use serde_json::Value;

/// `animatedEmoji` with a known sticker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimatedEmoji {
    /// The emoji's sticker; for a custom emoji, the custom emoji's own.
    pub sticker: StickerItem,
    /// `sticker_width` / `sticker_height`: the expected size.
    pub width: i32,
    pub height: i32,
    /// `fitzpatrick_type`: the skin tone the sticker is drawn in.
    pub fitz: FitzType,
    /// `sound`: played on a tap by some apps (tdesktop plays none).
    pub sound: Option<FileId>,
    /// `is_regular_emoji`: false for a custom emoji.
    pub is_regular_emoji: bool,
}

/// `messageAnimatedEmoji animated_emoji:animatedEmoji emoji:string`: the text
/// of the emoji, with the sticker when TDLib knows it (it may not yet for a
/// custom emoji, and then the message shows as plain text until an
/// `updateMessageContent` brings it).
pub(crate) fn parse_message_animated_emoji(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let emoji = value
        .get("emoji")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let animated = value.get("animated_emoji").filter(|v| !v.is_null());
    let (sticker, mut files) = parse_sticker_value(animated.and_then(|a| a.get("sticker")));
    let sound = animated
        .and_then(|a| a.get("sound"))
        .filter(|v| !v.is_null())
        .and_then(|v| parse_file(Some(v)).ok())
        .filter(|file| file.id.0 != 0);
    let sound_id = sound.as_ref().map(|file| file.id);
    files.extend(sound);
    let animated = animated
        .zip(sticker.filter(|s| s.file_id.0 != 0))
        .map(|(value, sticker)| AnimatedEmoji {
            width: int53_or_zero(value.get("sticker_width")).sat_i32(),
            height: int53_or_zero(value.get("sticker_height")).sat_i32(),
            fitz: FitzType::from_tdlib(int53_or_zero(value.get("fitzpatrick_type")).sat_i32()),
            sound: sound_id,
            is_regular_emoji: value
                .get("is_regular_emoji")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            sticker,
        });
    let entities = animated
        .as_ref()
        .and_then(|animated| animated.sticker.custom_emoji_id)
        .filter(|_| !emoji.is_empty())
        .map(|custom_emoji_id| {
            vec![TextEntity {
                utf8_start: 0,
                utf8_end: emoji.len(),
                kind: TextEntityKind::CustomEmoji { custom_emoji_id },
            }]
        })
        .unwrap_or_default();
    (
        MessageContent::Text(TextContent {
            text: emoji,
            entities,
            link_preview: None,
            animated,
        }),
        files,
    )
}

#[cfg(test)]
#[path = "message_animated_emoji_tests.rs"]
mod tests;
