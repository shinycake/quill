//! What message text drops before it is shaped, after Telegram Desktop's
//! `BlockParser::parseCurrentChar` (lib_ui `text_block_parser.cpp`):
//! control and formatting characters that break shaping, combining marks
//! past the second on one character (Zalgo text), and Unicode tag
//! characters. The emoji sequences keep what they need: ZWJ, U+FE0F, and
//! the tags of a subdivision flag such as England's. Unlike tdesktop on
//! macOS, U+FE0F stays everywhere, since Quill draws emoji with the system
//! font, which needs it to pick the emoji form.
#![deny(clippy::indexing_slicing)]

use std::borrow::Cow;
use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};

/// tdesktop `kMaxDiacAfterSymbol`.
pub const MAX_MARKS_AFTER_SYMBOL: u8 = 2;

/// U+1F3F4 WAVING BLACK FLAG, the base of the subdivision flags.
const BLACK_FLAG: char = '\u{1F3F4}';
/// U+E007F CANCEL TAG, which ends a tag sequence.
const CANCEL_TAG: char = '\u{E007F}';
/// The longest subdivision code a flag carries ("gbsct" is five).
const MAX_FLAG_TAGS: usize = 6;

/// The marks counted so far, carried between the pieces of one paragraph
/// so marks split across styled runs count as one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FilterState {
    marks: u8,
}

/// `text` without what tdesktop drops before layout. Borrowed when nothing
/// had to go, which is the usual case.
pub fn filter_display_text<'a>(text: &'a str, state: &mut FilterState) -> Cow<'a, str> {
    let mut kept: Option<String> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        let keep = if is_tag(ch) {
            false
        } else if ch == BLACK_FLAG {
            let tags = flag_tags(text.get(at + ch.len_utf8()..).unwrap_or_default());
            state.marks = 0;
            if let Some(tags) = tags {
                // The whole tag sequence goes through with its flag.
                let end = at + ch.len_utf8() + tags;
                if let Some(out) = kept.as_mut() {
                    out.push_str(text.get(at..end).unwrap_or_default());
                }
                while chars.peek().is_some_and(|&(next, _)| next < end) {
                    chars.next();
                }
                continue;
            }
            true
        } else if is_bad(ch) {
            false
        } else if ch.general_category() == GeneralCategory::NonspacingMark {
            state.marks = state.marks.saturating_add(1);
            state.marks <= MAX_MARKS_AFTER_SYMBOL
        } else {
            state.marks = 0;
            true
        };
        match (&mut kept, keep) {
            (Some(out), true) => out.push(ch),
            (None, false) => kept = Some(text.get(..at).unwrap_or_default().to_string()),
            _ => {}
        }
    }
    kept.map_or(Cow::Borrowed(text), Cow::Owned)
}

/// The byte length of a valid subdivision tag run at the start of `rest`
/// (tag letters then CANCEL TAG), if there is one.
fn flag_tags(rest: &str) -> Option<usize> {
    let mut len = 0;
    for (count, ch) in rest.chars().enumerate() {
        if ch == CANCEL_TAG {
            return (count > 0).then_some(len + ch.len_utf8());
        }
        let tag =
            ('\u{E0061}'..='\u{E007A}').contains(&ch) || ('\u{E0030}'..='\u{E0039}').contains(&ch);
        if count >= MAX_FLAG_TAGS || !tag {
            return None;
        }
        len += ch.len_utf8();
    }
    None
}

/// tdesktop skips every code point from U+E0000 up: tags and the
/// variation selectors supplement.
fn is_tag(ch: char) -> bool {
    ch >= '\u{E0000}'
}

/// tdesktop `IsBad`: NUL, the line and paragraph separators and the
/// embedding controls U+2028–U+202C, variation selectors U+FE00–U+FE0E
/// (U+FE0F stays), and the C1 controls except U+009C.
fn is_bad(ch: char) -> bool {
    matches!(ch,
        '\0'
        | '\u{2028}'..='\u{202C}'
        | '\u{FE00}'..='\u{FE0E}'
        | '\u{7F}'..='\u{9B}'
        | '\u{9D}'..='\u{9F}')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filtered(text: &str) -> String {
        filter_display_text(text, &mut FilterState::default()).into_owned()
    }

    #[test]
    fn zalgo_keeps_two_marks_per_letter() {
        let zalgo = "Z\u{0351}\u{036B}\u{0343}\u{036A}a\u{0306}\u{0310}\u{0357}l\u{0300}";
        assert_eq!(
            filtered(zalgo),
            "Z\u{0351}\u{036B}a\u{0306}\u{0310}l\u{0300}"
        );
        // A thousand marks on one letter shape as at most two.
        let heavy = format!("x{}", "\u{0300}".repeat(1000));
        assert_eq!(filtered(&heavy), "x\u{0300}\u{0300}");
    }

    #[test]
    fn marks_split_across_runs_count_once() {
        let mut state = FilterState::default();
        assert_eq!(filter_display_text("e\u{0301}", &mut state), "e\u{0301}");
        assert_eq!(
            filter_display_text("\u{0302}\u{0303}f", &mut state),
            "\u{0302}f"
        );
    }

    #[test]
    fn plain_and_script_text_is_left_alone() {
        for text in [
            "Hello, world",
            "Привет",
            "नमस्ते क्षत्रिय",
            "สวัสดีครับ",
            "مرحبًا",
            "שָׁלוֹם",
            "Tiếng Việt",
        ] {
            assert!(matches!(
                filter_display_text(text, &mut FilterState::default()),
                Cow::Borrowed(_)
            ));
        }
    }

    #[test]
    fn emoji_sequences_keep_zwj_variation_selector_and_flag_tags() {
        for emoji in [
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
            "\u{2764}\u{FE0F}",
            "\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}",
            "\u{1F44B}\u{1F3FD}",
            "1\u{FE0F}\u{20E3}",
            "\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}",
        ] {
            assert_eq!(filtered(emoji), emoji);
        }
    }

    #[test]
    fn stray_tags_controls_and_separators_go() {
        // The text from tdesktop issue 7005: tags after a book emoji.
        let crashy = "\u{1F4DA}\u{E0069}\u{E0064}\u{E006A}\u{E0077}\u{E007F}2";
        assert_eq!(filtered(crashy), "\u{1F4DA}2");
        // A black flag with tags that never end keeps only the flag.
        assert_eq!(filtered("\u{1F3F4}\u{E0067}\u{E0062}x"), "\u{1F3F4}x");
        assert_eq!(filtered("a\u{0}b\u{2028}c\u{202E}d"), "abc\u{202E}d");
        assert_eq!(filtered("a\u{85}b\u{9C}c\u{FE0E}"), "ab\u{9C}c");
        assert_eq!(filtered("\u{8FBA}\u{E0101}"), "\u{8FBA}");
    }
}
