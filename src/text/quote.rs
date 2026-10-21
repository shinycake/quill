//! Entities of a quoted part of a message (`textQuote` / `inputTextQuote`).
//!
//! A quote is a substring of the original text, so the formatting it keeps is
//! the original entities cut to that substring and moved to its start.
#![deny(clippy::indexing_slicing)]

use super::{TextEntity, TextEntityKind};
use std::ops::Range;

/// Whether a quote keeps this formatting: Bold, Italic, Underline,
/// Strikethrough, Spoiler, CustomEmoji and DateTime only
/// (`schema/td_api.tl:3481`, TDLib `is_allowed_quote_entity_type`).
pub fn quote_keeps(kind: &TextEntityKind) -> bool {
    matches!(
        kind,
        TextEntityKind::Bold
            | TextEntityKind::Italic
            | TextEntityKind::Underline
            | TextEntityKind::Strikethrough
            | TextEntityKind::Spoiler
            | TextEntityKind::CustomEmoji { .. }
            | TextEntityKind::DateTime { .. }
    )
}

/// The entities of `entities` that a quote keeps.
pub fn quote_entities(entities: &[TextEntity]) -> Vec<TextEntity> {
    entities
        .iter()
        .filter(|entity| quote_keeps(&entity.kind))
        .cloned()
        .collect()
}

/// The entities that overlap `range` (UTF-8 byte offsets), clipped to it and
/// rebased so offset 0 is the start of the range. Entities that end up empty
/// are dropped.
pub fn slice_entities(entities: &[TextEntity], range: Range<usize>) -> Vec<TextEntity> {
    entities
        .iter()
        .filter_map(|entity| {
            let start = entity.utf8_start.max(range.start);
            let end = entity.utf8_end.min(range.end);
            (start < end).then(|| TextEntity {
                utf8_start: start - range.start,
                utf8_end: end - range.start,
                kind: entity.kind.clone(),
            })
        })
        .collect()
}

/// `text` without surrounding whitespace and its entities moved to match.
pub fn trim_with_entities<'a>(
    text: &'a str,
    entities: &[TextEntity],
) -> (&'a str, Vec<TextEntity>) {
    let lead = text.len() - text.trim_start().len();
    let trimmed = text.trim();
    let kept = slice_entities(entities, lead..lead + trimmed.len());
    (trimmed, kept)
}

#[cfg(test)]
mod tests {
    use super::{slice_entities, trim_with_entities};
    use crate::text::{TextEntity, TextEntityKind};

    fn entity(start: usize, end: usize, kind: TextEntityKind) -> TextEntity {
        TextEntity {
            utf8_start: start,
            utf8_end: end,
            kind,
        }
    }

    #[test]
    fn slicing_clips_and_rebases_entities() {
        let entities = [
            entity(0, 5, TextEntityKind::Bold),
            entity(6, 11, TextEntityKind::Italic),
        ];
        // "lo wo": the tail of the bold word and the head of the italic one.
        let cut = slice_entities(&entities, 3..8);
        assert_eq!(
            cut,
            vec![
                entity(0, 2, TextEntityKind::Bold),
                entity(3, 5, TextEntityKind::Italic),
            ]
        );
        assert!(slice_entities(&entities, 5..6).is_empty());
    }

    #[test]
    fn trimming_moves_entities_with_the_text() {
        let entities = [entity(2, 6, TextEntityKind::Code)];
        let (text, kept) = trim_with_entities("  code  ", &entities);
        assert_eq!(text, "code");
        assert_eq!(kept, vec![entity(0, 4, TextEntityKind::Code)]);
    }
}
