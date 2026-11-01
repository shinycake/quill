//! What message text drops before it is shaped, after Telegram Desktop's
//! `BlockParser::parseCurrentChar` (lib_ui `text_block_parser.cpp`):
//! control and formatting characters that break shaping, generic combining
//! marks past the second on one character (Zalgo text), a script's marks
//! past the sixth, and Unicode tag
//! characters. The emoji sequences keep what they need: ZWJ, U+FE0F, and
//! the tags of a subdivision flag such as England's. Unlike tdesktop on
//! macOS, U+FE0F stays everywhere, since Quill draws emoji with the system
//! font, which needs it to pick the emoji form.
//!
//! The marks counted are nonspacing (Mn) and enclosing (Me) marks.
//! tdesktop's `IsDiac` counts only Mn, so a run of enclosing marks such as
//! U+0489 is unbounded there; Quill bounds it like any other mark.
#![deny(clippy::indexing_slicing)]

use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};

/// tdesktop `kMaxDiacAfterSymbol`, for the generic combining marks that
/// Zalgo text stacks.
pub const MAX_MARKS_AFTER_SYMBOL: u8 = 2;
/// The marks one letter of a script may carry: a fully pointed Hebrew
/// letter with cantillation, or a Devanagari letter with nukta, vowel sign
/// and candrabindu, has three or four.
pub const MAX_SCRIPT_MARKS_AFTER_SYMBOL: u8 = 6;

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

/// What a filtered string lost, by the offset in it where each dropped run
/// was, so a range of it maps back to the source text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMap {
    dropped: Vec<(usize, String)>,
}

impl SourceMap {
    pub fn is_empty(&self) -> bool {
        self.dropped.is_empty()
    }

    fn note(&mut self, at: usize, ch: char) {
        match self.dropped.last_mut() {
            Some((last, text)) if *last == at => text.push(ch),
            _ => self.dropped.push((at, ch.to_string())),
        }
    }

    /// The source text of `filtered[range]`. What was dropped inside the
    /// range or right at its end comes back; what was dropped right at its
    /// start belongs to the character before it, or to the range when
    /// there is none. `None` when `range` is not on `filtered`'s char
    /// boundaries.
    pub fn source_of(&self, filtered: &str, range: std::ops::Range<usize>) -> Option<String> {
        let mut out = String::with_capacity(range.len());
        let mut from = range.start;
        let leading = range.start == 0;
        for (at, text) in self
            .dropped
            .iter()
            .filter(|(at, _)| (range.start < *at || (leading && *at == 0)) && *at <= range.end)
        {
            out.push_str(filtered.get(from..*at)?);
            out.push_str(text);
            from = *at;
        }
        out.push_str(filtered.get(from..range.end)?);
        Some(out)
    }
}

/// Append `text` to `out` without what tdesktop drops before layout, and
/// note in `map` what was dropped where.
pub fn push_filtered(out: &mut String, text: &str, state: &mut FilterState, map: &mut SourceMap) {
    let mut kept_from = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        let keep = if ch < '\u{300}' {
            // Latin and ASCII: no mark, no tag, and only controls are bad.
            let bad = is_bad(ch);
            if !bad {
                state.marks = 0;
            }
            !bad
        } else if is_tag(ch) {
            false
        } else if ch == BLACK_FLAG {
            state.marks = 0;
            // The whole tag sequence goes through with its flag.
            if let Some(tags) = flag_tags(text.get(at + ch.len_utf8()..).unwrap_or_default()) {
                let end = at + ch.len_utf8() + tags;
                while chars.peek().is_some_and(|&(next, _)| next < end) {
                    chars.next();
                }
            }
            true
        } else if is_bad(ch) {
            false
        } else if matches!(
            ch.general_category(),
            GeneralCategory::NonspacingMark | GeneralCategory::EnclosingMark
        ) {
            state.marks = state.marks.saturating_add(1);
            let limit = if is_generic_mark(ch) {
                MAX_MARKS_AFTER_SYMBOL
            } else {
                MAX_SCRIPT_MARKS_AFTER_SYMBOL
            };
            state.marks <= limit
        } else {
            state.marks = 0;
            true
        };
        if !keep {
            out.push_str(text.get(kept_from..at).unwrap_or_default());
            map.note(out.len(), ch);
            kept_from = at + ch.len_utf8();
        }
    }
    out.push_str(text.get(kept_from..).unwrap_or_default());
}

/// The combining marks of no one script (Combining Diacritical Marks, its
/// Extended and Supplement blocks, marks for symbols, half marks).
fn is_generic_mark(ch: char) -> bool {
    matches!(ch,
        '\u{0300}'..='\u{036F}'
        | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}'
        | '\u{20D0}'..='\u{20FF}'
        | '\u{FE20}'..='\u{FE2F}')
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
        let mut out = String::new();
        push_filtered(
            &mut out,
            text,
            &mut FilterState::default(),
            &mut SourceMap::default(),
        );
        out
    }

    #[test]
    fn zalgo_keeps_two_generic_marks_per_letter() {
        let zalgo = "Z\u{0351}\u{036B}\u{0343}\u{036A}a\u{0306}\u{0310}\u{0357}l\u{0300}";
        assert_eq!(
            filtered(zalgo),
            "Z\u{0351}\u{036B}a\u{0306}\u{0310}l\u{0300}"
        );
        // A thousand marks on one letter shape as at most two.
        let heavy = format!("x{}", "\u{0300}".repeat(1000));
        assert_eq!(filtered(&heavy), "x\u{0300}\u{0300}");
        // Every generic block counts the same.
        let mixed = "o\u{1AB0}\u{1DC0}\u{20D0}\u{FE20}\u{0301}";
        assert_eq!(filtered(mixed), "o\u{1AB0}\u{1DC0}");
    }

    #[test]
    fn enclosing_marks_count_as_marks_and_keep_the_count() {
        // U+0489, a common Zalgo mark, is a script mark: six stay.
        let rings = format!("x{}", "\u{0489}".repeat(1000));
        assert_eq!(filtered(&rings), format!("x{}", "\u{0489}".repeat(6)));
        // U+20DD is a generic mark and does not start the count over.
        let rounds = format!("x{}", "\u{0300}\u{0300}\u{20DD}".repeat(300));
        assert_eq!(filtered(&rounds), "x\u{0300}\u{0300}");
        // A keycap's enclosing mark stays on its digit.
        assert_eq!(filtered("1\u{FE0F}\u{20E3}"), "1\u{FE0F}\u{20E3}");
    }

    #[test]
    fn script_marks_keep_up_to_six_counted_with_generic_ones() {
        // Hebrew points past six on one letter go.
        let piled = format!("\u{05E9}{}", "\u{05B8}".repeat(9));
        assert_eq!(
            filtered(&piled),
            format!("\u{05E9}{}", "\u{05B8}".repeat(6))
        );
        // A generic mark third in the run goes; the script mark after it,
        // fourth, stays.
        assert_eq!(
            filtered("\u{05E9}\u{05BC}\u{05C1}\u{0301}\u{05B8}"),
            "\u{05E9}\u{05BC}\u{05C1}\u{05B8}"
        );
    }

    #[test]
    fn pointed_hebrew_and_devanagari_keep_every_mark() {
        for text in [
            // "the heavens", Genesis 1:1: shin with dagesh, shin dot and qamats.
            "\u{05D4}\u{05B7}\u{05E9}\u{05BC}\u{05C1}\u{05B8}\u{05DE}\u{05B7}\u{05D9}\u{05B4}\u{05DD}",
            // With cantillation: the tipcha stays.
            "\u{05D1}\u{05BC}\u{05B0}\u{05E8}\u{05B5}\u{05D0}\u{05E9}\u{05C1}\u{05B4}\u{0596}\u{05D9}\u{05EA}",
            // Nukta, u sign and candrabindu.
            "\u{0915}\u{093C}\u{0941}\u{0901}",
        ] {
            assert_eq!(filtered(text), text);
        }
    }

    #[test]
    fn marks_split_across_runs_count_once() {
        let (mut state, mut map) = (FilterState::default(), SourceMap::default());
        let mut out = String::new();
        push_filtered(&mut out, "e\u{0301}", &mut state, &mut map);
        push_filtered(&mut out, "\u{0302}\u{0303}f", &mut state, &mut map);
        assert_eq!(out, "e\u{0301}\u{0302}f");
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
            let mut map = SourceMap::default();
            let mut out = String::new();
            push_filtered(&mut out, text, &mut FilterState::default(), &mut map);
            assert_eq!(out, text);
            assert!(map.is_empty());
        }
    }

    #[test]
    fn a_range_of_the_filtered_text_maps_back_to_its_source() {
        let source = "ab\u{0300}\u{0300}\u{0300}\u{0300}c\u{2028}d";
        let mut map = SourceMap::default();
        let mut out = String::new();
        push_filtered(&mut out, source, &mut FilterState::default(), &mut map);
        assert_eq!(out, "ab\u{0300}\u{0300}cd");
        assert_eq!(map.source_of(&out, 0..out.len()).as_deref(), Some(source));
        // The marks dropped after "b" go with "b", not with "c".
        let c = out.find('c').unwrap();
        assert_eq!(
            map.source_of(&out, 0..c).as_deref(),
            Some("ab\u{0300}\u{0300}\u{0300}\u{0300}")
        );
        assert_eq!(
            map.source_of(&out, c..out.len()).as_deref(),
            Some("c\u{2028}d")
        );
        assert_eq!(map.source_of(&out, 1..2).as_deref(), Some("b"));
        assert_eq!(map.source_of(&out, 0..3), None, "inside a mark");
    }

    #[test]
    fn what_was_dropped_before_the_first_character_comes_back() {
        let mut leading = SourceMap::default();
        let mut shown = String::new();
        push_filtered(
            &mut shown,
            "\u{2028}\u{0}hi",
            &mut FilterState::default(),
            &mut leading,
        );
        assert_eq!(shown, "hi");
        assert_eq!(
            leading.source_of(&shown, 0..1).as_deref(),
            Some("\u{2028}\u{0}h")
        );
        assert_eq!(leading.source_of(&shown, 1..2).as_deref(), Some("i"));
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
