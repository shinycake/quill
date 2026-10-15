//! Cutting text into plain stretches and emoji the active pack can draw.
#![deny(clippy::indexing_slicing)]

use std::collections::HashSet;
use std::ops::Range;
use std::sync::OnceLock;

use unicode_segmentation::UnicodeSegmentation;

use super::pack::{Cell, SpritePack};

/// One stretch of a text: plain characters, or one emoji drawn from
/// `cell`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    /// Byte range in the text.
    pub range: Range<usize>,
    pub cell: Option<Cell>,
}

/// Characters that are text unless U+FE0F follows (Unicode
/// `Emoji_Presentation=No`: ©, ®, ™, ↔, ☺, ❤…): the catalog lists their
/// emoji form with the selector. A bare "©" in prose stays a letter.
fn text_default(c: char) -> bool {
    static SET: OnceLock<HashSet<char>> = OnceLock::new();
    SET.get_or_init(|| {
        crate::emoji_catalog::catalog()
            .filter_map(|entry| {
                let mut chars = entry.emoji.chars();
                let first = chars.next()?;
                (chars.next() == Some('\u{fe0f}') && chars.next().is_none()).then_some(first)
            })
            .collect()
    })
    .contains(&c)
}

/// The cell for one grapheme, if `pack` draws it.
pub fn grapheme_cell(pack: &SpritePack, grapheme: &str) -> Option<Cell> {
    // Keycaps start with an ASCII digit but end in U+20E3.
    if grapheme.is_ascii() {
        return None;
    }
    let mut chars = grapheme.chars();
    if let (Some(only), None) = (chars.next(), chars.next())
        && text_default(only)
    {
        return None;
    }
    pack.lookup(grapheme)
}

/// `text` as plain stretches and pack emoji, in order. `None` when it has
/// no emoji the pack draws (the common case: nothing to change).
pub fn pieces(pack: &SpritePack, text: &str) -> Option<Vec<Piece>> {
    if text.is_ascii() {
        return None;
    }
    let mut out = Vec::new();
    let mut plain_start = 0;
    let mut any = false;
    for (at, grapheme) in text.grapheme_indices(true) {
        let Some(cell) = grapheme_cell(pack, grapheme) else {
            continue;
        };
        any = true;
        if plain_start < at {
            out.push(Piece {
                range: plain_start..at,
                cell: None,
            });
        }
        let end = at + grapheme.len();
        out.push(Piece {
            range: at..end,
            cell: Some(cell),
        });
        plain_start = end;
    }
    if !any {
        return None;
    }
    if plain_start < text.len() {
        out.push(Piece {
            range: plain_start..text.len(),
            cell: None,
        });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack() -> SpritePack {
        let config = format!(
            r#"{{"version":1,"cell":72,"columns":16,"rows":16,"sheets":1,"index":{{{}}}}}"#,
            r#""😀":[0,0],"❤":[0,1],"©":[0,2],"🇺🇸":[0,3],"1⃣":[0,4],"👍🏽":[0,5],"👨‍👩‍👧":[0,6]"#
        );
        SpritePack::parse(config.as_bytes()).unwrap()
    }

    fn shown(text: &str) -> Vec<(String, bool)> {
        pieces(&pack(), text)
            .unwrap_or_default()
            .into_iter()
            .map(|p| (text.get(p.range).unwrap().to_string(), p.cell.is_some()))
            .collect()
    }

    #[test]
    fn plain_text_has_no_pieces() {
        assert_eq!(pieces(&pack(), "hello"), None);
        assert_eq!(pieces(&pack(), "héllo wörld"), None);
        assert_eq!(pieces(&pack(), "🙂 not in the pack"), None);
    }

    #[test]
    fn mixed_text_splits_at_emoji() {
        assert_eq!(
            shown("Hi 😀 there 👍🏽!"),
            vec![
                ("Hi ".into(), false),
                ("😀".into(), true),
                (" there ".into(), false),
                ("👍🏽".into(), true),
                ("!".into(), false),
            ]
        );
    }

    #[test]
    fn flags_keycaps_and_zwj_are_one_emoji() {
        assert_eq!(
            shown("🇺🇸1\u{fe0f}\u{20e3}👨‍👩‍👧"),
            vec![
                ("🇺🇸".into(), true),
                ("1\u{fe0f}\u{20e3}".into(), true),
                ("👨‍👩‍👧".into(), true),
            ]
        );
        // A digit alone is text.
        assert_eq!(pieces(&pack(), "1 2 3 é"), None);
    }

    #[test]
    fn text_style_characters_need_the_selector() {
        assert_eq!(pieces(&pack(), "© 2026 ❤ ü"), None);
        assert_eq!(
            shown("❤\u{fe0f}©\u{fe0f}"),
            vec![("❤\u{fe0f}".into(), true), ("©\u{fe0f}".into(), true)]
        );
    }
}
