//! The MPEG4 variant of a chat photo (`chatPhoto.animation` and
//! `small_animation`, `animatedChatPhoto`): a video profile photo.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ids::FileId;
use serde_json::Value;

/// The clip of a video profile photo. tdesktop streams the large variant
/// both on the profile cover and in the viewer (`PhotoData::
/// createStreamingLoader` with `PhotoSize::Large`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatPhotoAnimation {
    pub file_id: FileId,
    /// `main_frame_timestamp`: where playback starts, in milliseconds
    /// (tdesktop `videoStartPosition`).
    pub start_ms: u32,
    /// `small_animation` (160 px), when the photo has one: what a userpic
    /// beside a message or in the chat list plays.
    pub small_file_id: Option<FileId>,
}

impl ChatPhotoAnimation {
    pub fn start_secs(self) -> f64 {
        f64::from(self.start_ms) / 1000.0
    }

    /// The clip a small userpic plays: tdesktop's `VideoUserpic` asks for
    /// `PhotoSize::Small` and falls back to the large one.
    pub fn userpic_file_id(self) -> FileId {
        self.small_file_id.unwrap_or(self.file_id)
    }
}

/// The clips of a `chatPhoto` with their files: `animation` (up to
/// 1280 px), else `small_animation` (160 px), and the small one beside it.
/// `None` for a still photo. A photo made from a sticker
/// (`chatPhoto.sticker`) plays only through these, as in tdesktop, which
/// ignores the emoji markup of a photo.
pub(crate) fn parse_chat_photo_animation(
    photo: &Value,
) -> Option<(ChatPhotoAnimation, Vec<ParsedFile>)> {
    let clip = |key: &str| {
        let animated = photo.get(key).filter(|value| !value.is_null())?;
        let file = parse_file(animated.get("file")).ok()?;
        let start = animated
            .get("main_frame_timestamp")
            .and_then(Value::as_f64)
            .filter(|secs| secs.is_finite() && *secs > 0.0)
            .map_or(0, |secs| (secs * 1000.0).min(f64::from(u32::MAX)) as u32);
        Some((file, start))
    };
    let small = clip("small_animation");
    let small_file_id = small.as_ref().map(|(file, _)| file.id);
    match (clip("animation"), small) {
        (Some((large, start_ms)), small) => Some((
            ChatPhotoAnimation {
                file_id: large.id,
                start_ms,
                small_file_id,
            },
            std::iter::once(large)
                .chain(small.map(|(file, _)| file))
                .collect(),
        )),
        (None, Some((small, start_ms))) => Some((
            ChatPhotoAnimation {
                file_id: small.id,
                start_ms,
                small_file_id,
            },
            vec![small],
        )),
        (None, None) => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::super::users::parse_profile_photo;
    use super::*;

    fn file(id: i32) -> String {
        format!(
            r#"{{"@type":"file","id":{id},"size":10,"expected_size":10,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_active":false,"is_downloading_completed":false,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}}}"#
        )
    }

    fn photo(animation: &str) -> Value {
        serde_json::from_str(&format!(
            r#"{{"@type":"chatPhoto","id":"77","added_date":1,"sizes":[{{"@type":"photoSize","type":"a","photo":{},"width":160,"height":160}},{{"@type":"photoSize","type":"c","photo":{},"width":640,"height":640}}]{animation}}}"#,
            file(1),
            file(2)
        ))
        .unwrap()
    }

    #[test]
    fn the_large_clip_wins_and_starts_at_its_main_frame() {
        let value = photo(&format!(
            r#","animation":{{"@type":"animatedChatPhoto","length":800,"file":{},"main_frame_timestamp":1.25}},"small_animation":{{"@type":"animatedChatPhoto","length":160,"file":{},"main_frame_timestamp":0}}"#,
            file(3),
            file(4)
        ));
        let (animation, clips) = parse_chat_photo_animation(&value).unwrap();
        assert_eq!(animation.file_id, FileId(3));
        assert_eq!(animation.start_ms, 1250);
        assert!((animation.start_secs() - 1.25).abs() < 1e-9);
        assert_eq!(animation.small_file_id, Some(FileId(4)));
        assert_eq!(animation.userpic_file_id(), FileId(4));
        let ids: Vec<FileId> = clips.iter().map(|file| file.id).collect();
        assert_eq!(ids, vec![FileId(3), FileId(4)]);
    }

    #[test]
    fn a_userpic_without_a_small_clip_plays_the_large_one() {
        let value = photo(&format!(
            r#","animation":{{"@type":"animatedChatPhoto","length":800,"file":{},"main_frame_timestamp":0}}"#,
            file(3)
        ));
        let (animation, clips) = parse_chat_photo_animation(&value).unwrap();
        assert_eq!(animation.small_file_id, None);
        assert_eq!(animation.userpic_file_id(), FileId(3));
        assert_eq!(clips.len(), 1);
    }

    #[test]
    fn a_photo_with_only_the_small_clip_plays_it() {
        let value = photo(&format!(
            r#","animation":null,"small_animation":{{"@type":"animatedChatPhoto","length":160,"file":{},"main_frame_timestamp":-3}}"#,
            file(4)
        ));
        let (animation, clips) = parse_chat_photo_animation(&value).unwrap();
        assert_eq!(animation.file_id, FileId(4));
        assert_eq!(animation.userpic_file_id(), FileId(4));
        assert_eq!(animation.start_ms, 0);
        assert_eq!(clips.len(), 1);
    }

    #[test]
    fn a_still_or_sticker_photo_has_no_clip() {
        assert!(parse_chat_photo_animation(&photo("")).is_none());
        let sticker = photo(
            r#","sticker":{"@type":"chatPhotoSticker","type":{"@type":"chatPhotoStickerTypeCustomEmoji","custom_emoji_id":"5"},"background_fill":{"@type":"backgroundFillSolid","color":0}}"#,
        );
        assert!(parse_chat_photo_animation(&sticker).is_none());
        assert_eq!(parse_profile_photo(&sticker).unwrap().animation, None);
    }

    #[test]
    fn a_profile_photo_keeps_its_clip_and_caches_the_clip_file() {
        let value = photo(&format!(
            r#","animation":{{"@type":"animatedChatPhoto","length":800,"file":{},"main_frame_timestamp":0.5}}"#,
            file(9)
        ));
        let parsed = parse_profile_photo(&value).unwrap();
        assert_eq!(
            parsed.animation,
            Some(ChatPhotoAnimation {
                file_id: FileId(9),
                start_ms: 500,
                small_file_id: None,
            })
        );
        assert_eq!(parsed.full_file_id, FileId(2));
        assert!(parsed.files.iter().any(|file| file.id == FileId(9)));
    }
}
