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
#![deny(clippy::indexing_slicing)]

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
    pub density: crate::density::Density,
    /// The layout's row sizes (Web K's in Floating).
    pub rows: crate::layout_style::RowScale,
    /// How the open chat's row is drawn.
    pub selected: crate::layout_style::SelectedRow,
}

impl ChatListRowStyle {
    pub fn new(
        preview_lines: u8,
        media_icons: bool,
        rich_preview: bool,
        density: crate::density::Density,
        metrics: &crate::layout_style::ChromeMetrics,
    ) -> Self {
        Self {
            preview_lines: clamp_preview_lines(preview_lines),
            media_icons,
            rich_preview,
            density,
            rows: metrics.rows,
            selected: metrics.look.selected_row(),
        }
    }

    /// The sizes the row draws with.
    pub fn metrics(&self) -> &'static crate::density::DensityMetrics {
        self.density.metrics_in(self.rows)
    }
}

/// Style inputs captured next to `ChatSummary::last_preview` at the
/// update sites that compute it: the media icon glyph (if any) and the
/// formatted-text entities of the preview source, clipped to the
/// preview's byte length (entities index the full message text or
/// caption; the preview keeps its first 80 chars).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatPreviewStyle {
    /// A service message ("X pinned..."): the text already names the actor, so the row shows no sender prefix.
    pub service: bool,
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
        service: matches!(
            content,
            MessageContent::Action(_)
                | MessageContent::ChatTtlChanged { .. }
                | MessageContent::ScreenshotTaken
        ),
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

/// Fixed chat-row height for the virtual list at a density: the two-line
/// row, one more line per preview line past the second, and the folder-tag
/// strip (Standard: 64px, 80px with three lines, +24px with tags). The row
/// renderer enforces the same height.
pub fn chat_row_height_px(
    has_tags: bool,
    preview_lines: u8,
    metrics: &crate::density::DensityMetrics,
) -> f32 {
    let extra_lines = (clamp_preview_lines(preview_lines) - PREVIEW_LINES_MIN) as f32;
    let tags = if has_tags { metrics.row_tags } else { 0.0 };
    metrics.row_base + extra_lines * metrics.row_line + tags
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

/// One piece of a chat-list preview line, in typing order: some text, or a
/// picture (a custom emoji or emoji sprite) that reads as a neutral object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewPiece<'a> {
    Text(&'a str),
    Picture,
}

/// Where a piece of [`visual_pieces`] comes from: the index of the typing-order
/// piece, and for text the byte range of it that this visual run shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisualPiece {
    pub piece: usize,
    pub text: Option<std::ops::Range<usize>>,
}

/// The pieces of a preview line from left to right, as UAX #9 orders them for
/// the line's own direction (its first strong letter). A row of separate
/// elements cannot reorder itself, so a Hebrew preview with a custom emoji
/// drew its pieces in typing order, left to right, and read backwards. Text
/// pieces are split where the embedding level changes, so each part is
/// one direction and lays itself out correctly.
pub fn visual_pieces(pieces: &[PreviewPiece<'_>]) -> Vec<VisualPiece> {
    let mut line = String::new();
    let mut spans = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let start = line.len();
        match piece {
            // Line breaks read as spaces in a preview (one paragraph).
            PreviewPiece::Text(text) => line.extend(text.chars().map(|c| match c {
                '\n' | '\r' | '\u{2029}' | '\u{85}' | '\u{1C}'..='\u{1E}' => ' ',
                c => c,
            })),
            PreviewPiece::Picture => line.push('\u{FFFC}'),
        }
        spans.push(start..line.len());
    }
    let info = unicode_bidi::BidiInfo::new(&line, None);
    let Some(paragraph) = info.paragraphs.first() else {
        return Vec::new();
    };
    let (levels, runs) = info.visual_runs(paragraph, paragraph.range.clone());
    let mut out = Vec::new();
    for run in runs {
        let rtl = levels.get(run.start).is_some_and(|level| level.is_rtl());
        let mut hits: Vec<VisualPiece> = spans
            .iter()
            .enumerate()
            .filter_map(|(index, span)| {
                let start = span.start.max(run.start);
                let end = span.end.min(run.end);
                (start < end).then(|| VisualPiece {
                    piece: index,
                    text: match pieces.get(index) {
                        Some(PreviewPiece::Text(_)) => Some(start - span.start..end - span.start),
                        _ => None,
                    },
                })
            })
            .collect();
        if rtl {
            hits.reverse();
        }
        out.extend(hits);
    }
    out
}

#[cfg(test)]
mod visual_pieces_tests {
    use super::{PreviewPiece as P, VisualPiece, visual_pieces};

    fn order(pieces: &[P<'_>]) -> Vec<(usize, Option<String>)> {
        visual_pieces(pieces)
            .into_iter()
            .map(|VisualPiece { piece, text }| {
                let text = text.and_then(|range| match pieces.get(piece) {
                    Some(P::Text(t)) => t.get(range).map(str::to_string),
                    _ => None,
                });
                (piece, text)
            })
            .collect()
    }

    #[test]
    fn a_left_to_right_line_keeps_typing_order() {
        let pieces = [P::Text("hi "), P::Picture, P::Text(" there")];
        assert_eq!(
            order(&pieces),
            vec![
                (0, Some("hi ".into())),
                (1, None),
                (2, Some(" there".into()))
            ]
        );
    }

    #[test]
    fn a_hebrew_line_puts_its_first_piece_on_the_right() {
        let pieces = [P::Text("שלום "), P::Picture, P::Text(" לכולם")];
        assert_eq!(
            order(&pieces),
            vec![
                (2, Some(" לכולם".into())),
                (1, None),
                (0, Some("שלום ".into()))
            ]
        );
    }

    #[test]
    fn an_english_word_in_a_hebrew_line_is_its_own_left_to_right_run() {
        let pieces = [P::Text("קראתי את Dune "), P::Picture];
        assert_eq!(
            order(&pieces),
            vec![
                (1, None),
                (0, Some(" ".into())),
                (0, Some("Dune".into())),
                (0, Some("קראתי את ".into()))
            ]
        );
    }
}
