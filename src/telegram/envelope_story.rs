//! Phase 9.7 story-album payload types: one `storyAlbum` row and its
//! parser. Split out of `envelope.rs` per the file-size directive —
//! `envelope.rs` keeps the payload enum variants and the thin parse
//! dispatch arms.

use serde_json::Value;

/// Phase 9.7: one `storyAlbum` row (TDLib 1.8.67, `schema/td_api.tl:6758`:
/// `storyAlbum id:int32 name:string photo_icon:photo video_icon:video =
/// StoryAlbum`). The icons are dropped — the story page lists albums by
/// name (covers are a future slice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedStoryAlbum {
    pub id: i32,
    pub name: String,
}

pub(crate) fn parse_story_album(value: &Value) -> Option<ParsedStoryAlbum> {
    if value.get("@type").and_then(Value::as_str) != Some("storyAlbum") {
        return None;
    }
    Some(ParsedStoryAlbum {
        id: value.get("id")?.as_i64()? as i32,
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}
