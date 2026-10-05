//! Chat-list style settings (`parity:chatlist-list-style`): preview line
//! count (two/three lines), media-type icons, and formatted-text
//! rendering in the chat-list preview. Pure helpers only — the GPUI row
//! renderer lives in `ui`, persistence in `settings::AppearancePrefs`.
//!
//! Design notes (the honest choices):
//! - Three lines = title + sender line + preview line. The list never
//!   parses `message.sender_id`, so the sender is "You" for own
//!   messages, the author signature for signed channel posts, and the
//!   chat title otherwise (the same rule the peek preview uses).
//! - Media icons are a visibility toggle over a glyph derived from the
//!   content type. Types whose preview already carries a glyph
//!   (location/venue/contact) or shows the emoji itself (sticker) get no
//!   icon.
//! - Formatted preview covers `messageText` entities and media-caption
//!   entities (bold/italic/etc.) via the existing `styled_runs`;
//!   rich-message blocks keep a plain preview (different entity model).
//!   Spoilers stay hidden in the list (open the chat to reveal); links
//!   render unstyled and don't open from the list row.

use crate::telegram::envelope::MessageContent;
use crate::text::TextEntity;

/// Chat-list preview lines: title + one preview line (current) or title
/// + sender line + preview line.
pub const PREVIEW_LINES_MIN: u8 = 2;
pub const PREVIEW_LINES_MAX: u8 = 3;
pub const PREVIEW_LINES_DEFAULT: u8 = 2;

/// Clamp the preview-line setting (prefs files are user-editable).
pub fn clamp_preview_lines(n: u8) -> u8 {
    n.clamp(PREVIEW_LINES_MIN, PREVIEW_LINES_MAX)
}

/// Row-level style inputs for the chat-list renderer, built from
/// `AppearancePrefs` by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatListRowStyle {
    pub preview_lines: u8,
    pub media_icons: bool,
    pub rich_preview: bool,
}

impl ChatListRowStyle {
    pub fn new(preview_lines: u8, media_icons: bool, rich_preview: bool) -> Self {
        Self {
            preview_lines: clamp_preview_lines(preview_lines),
            media_icons,
            rich_preview,
        }
    }
}

/// Style inputs captured next to `ChatSummary::last_preview` at the
/// update sites that compute it: the media icon glyph (if any) and the
/// formatted-text entities of the preview source, clipped to the
/// preview's byte length (entities index the full message text or
/// caption; the preview keeps its first 80 chars).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatPreviewStyle {
    pub icon: Option<&'static str>,
    pub entities: Vec<TextEntity>,
}

/// Media-type icon glyph for a message, or `None` when the preview text
/// already names the type (the "Photo"/"Video" label or an embedded
/// 📍/👤 glyph) or the content is plain/rich text.
pub fn preview_media_icon(content: &MessageContent) -> Option<&'static str> {
    match content {
        MessageContent::Photo(_) => Some("\u{1F5BC}"),
        MessageContent::Video(_) => Some("\u{1F3AC}"),
        MessageContent::VideoNote(_) => Some("\u{1F3A5}"),
        MessageContent::Animation(_) => Some("\u{1F39E}"),
        MessageContent::VoiceNote(_) => Some("\u{1F399}"),
        MessageContent::Audio(_) => Some("\u{1F3B5}"),
        MessageContent::Document(_) => Some("\u{1F4CE}"),
        MessageContent::Poll(_) => Some("\u{1F4CA}"),
        _ => None,
    }
}

/// The preview style for a last-message `content` whose preview text is
/// `preview`: icon glyph plus text entities clipped to the preview's
/// byte length (the preview is a byte-prefix of the message text or
/// caption).
pub fn preview_style(content: &MessageContent, preview: &str) -> ChatPreviewStyle {
    let entities = match content {
        MessageContent::Text(text) => clip_entities(&text.entities, preview.len()),
        // The preview is the caption when one is set; its entities index
        // the caption text. (Empty caption ⇒ empty entities ⇒ nothing.)
        MessageContent::Photo(p) => clip_entities(&p.caption_entities, preview.len()),
        MessageContent::Video(v) => clip_entities(&v.caption_entities, preview.len()),
        MessageContent::Animation(a) => clip_entities(&a.caption_entities, preview.len()),
        MessageContent::Document(d) => clip_entities(&d.caption_entities, preview.len()),
        MessageContent::VoiceNote(n) => clip_entities(&n.caption_entities, preview.len()),
        MessageContent::Audio(a) => clip_entities(&a.caption_entities, preview.len()),
        _ => Vec::new(),
    };
    ChatPreviewStyle {
        icon: preview_media_icon(content),
        entities,
    }
}

/// Sender line for the 3-line row: "You" for own messages, the author
/// signature for signed channel posts, else the chat title. Mirrors the
/// peek preview's name rule (`chat_preview_line`).
pub fn preview_sender_name(
    is_outgoing: bool,
    author_signature: Option<&str>,
    chat_title: &str,
) -> String {
    if is_outgoing {
        return "You".to_string();
    }
    match author_signature {
        Some(sig) if !sig.trim().is_empty() => sig.to_string(),
        _ => chat_title.to_string(),
    }
}

/// Fixed chat-row height for the virtual list: 64px base (avatar 46 +
/// padding), 88px with the folder-tag strip; the third line adds
/// one text_xs line (16px). The row renderer enforces the same height.
pub fn chat_row_height_px(has_tags: bool, preview_lines: u8) -> f32 {
    let base = if has_tags { 88.0 } else { 64.0 };
    base + (clamp_preview_lines(preview_lines) - PREVIEW_LINES_MIN) as f32 * 16.0
}

/// Clip entities to `max_bytes`, dropping degenerate ones (zero length
/// or starting past the end). `styled_runs` would drop out-of-range
/// entities anyway; clipping first keeps partial overlaps styled.
pub fn clip_entities(entities: &[TextEntity], max_bytes: usize) -> Vec<TextEntity> {
    entities
        .iter()
        .filter(|e| e.utf8_start < max_bytes && e.utf8_start < e.utf8_end)
        .map(|e| TextEntity {
            utf8_start: e.utf8_start,
            utf8_end: e.utf8_end.min(max_bytes),
            kind: e.kind.clone(),
        })
        .collect()
}
