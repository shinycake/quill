use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// How far a double-click looks on each side of the clicked character, in characters.
const WORD_WINDOW: usize = 128;

// QUILL: words follow UAX #29 word boundaries, as Qt's WordUnderCursor does in tdesktop.
// The old rule only knew Latin and Cyrillic letters, so a double-click on a Hebrew or
// Arabic word selected one letter. UAX #29 also keeps `צה"ל`, `don't` and words with
// niqqud, harakat or ZWNJ whole, and still takes CJK one ideograph at a time.
pub(crate) fn word_range_from_chars(
    offset: usize,
    character: char,
    previous: impl Iterator<Item = char>,
    following: impl Iterator<Item = char>,
) -> Range<usize> {
    let mut window: Vec<char> = previous.take(WORD_WINDOW).collect();
    window.reverse();
    let before: usize = window.iter().map(|c| c.len_utf8()).sum();
    window.push(character);
    window.extend(following.take(WORD_WINDOW));
    let window: String = window.into_iter().collect();

    let clicked = before..before + character.len_utf8();
    let word = window
        .split_word_bound_indices()
        .map(|(start, word)| start..start + word.len())
        .find(|word| word.start <= clicked.start && clicked.end <= word.end)
        .unwrap_or(clicked);
    // `before` bytes of the window lie in front of `offset`.
    offset - (before - word.start)..offset + (word.end - before)
}

pub(crate) fn word_range_at(text: &str, offset: usize) -> Option<Range<usize>> {
    let offset = clip_offset_left(text, offset);
    let character = text[offset..].chars().next()?;
    let end = offset + character.len_utf8();
    Some(word_range_from_chars(
        offset,
        character,
        text[..offset].chars().rev(),
        text[end..].chars(),
    ))
}

pub(crate) fn line_range_at(text: &str, offset: usize) -> Range<usize> {
    let offset = clip_offset_left(text, offset);
    let start = text[..offset].rfind('\n').map_or(0, |newline| newline + 1);
    let end = text[offset..]
        .find('\n')
        .map_or(text.len(), |newline| offset + newline);
    start..end
}

fn clip_offset_left(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// QUILL: a Hebrew or Arabic point (niqqud, cantillation, harakat) that a
/// writer deletes on its own: Backspace after it removes just the point,
/// as Hebrew and Arabic keyboards and editors do, not the letter it sits on.
pub(crate) fn is_detachable_point(c: char) -> bool {
    matches!(c,
        '\u{0591}'..='\u{05BD}'
        | '\u{05BF}'
        | '\u{05C1}'..='\u{05C2}'
        | '\u{05C4}'..='\u{05C5}'
        | '\u{05C7}'
        | '\u{0610}'..='\u{061A}'
        | '\u{064B}'..='\u{065F}'
        | '\u{0670}'
        | '\u{06D6}'..='\u{06DC}'
        | '\u{06DF}'..='\u{06E4}'
        | '\u{06E7}'..='\u{06E8}'
        | '\u{06EA}'..='\u{06ED}'
        | '\u{FB1E}')
}

#[cfg(test)]
mod tests {
    use super::word_range_at;

    fn word_at(text: &str, needle: &str) -> String {
        let offset = text.find(needle).unwrap_or_default() + needle.len() / 2;
        let offset = (0..=offset)
            .rev()
            .find(|offset| text.is_char_boundary(*offset))
            .unwrap_or_default();
        word_range_at(text, offset)
            .map(|range| text[range].to_string())
            .unwrap_or_default()
    }

    #[test]
    fn right_to_left_words_select_whole() {
        for (text, word) in [
            ("שלום עולם", "עולם"),
            ("שלום עולם", "שלום"),
            ("שָׁלוֹם עולם", "שָׁלוֹם"),
            ("مرحبا مُحَمَّد هنا", "مُحَمَّد"),
            ("من می\u{200c}خواهم", "می\u{200c}خواهم"),
            ("אני בצה\"ל עכשיו", "בצה\"ל"),
            ("ג'ירפה גדולה", "ג'ירפה"),
            ("hello שלום world", "שלום"),
        ] {
            assert_eq!(word_at(text, word), word, "{text}");
        }
    }

    #[test]
    fn ideographs_and_punctuation_stay_single() {
        assert_eq!(word_at("中文", "文"), "文");
        assert_eq!(word_at("שלום, עולם", ","), ",");
        assert_eq!(word_at("don't stop", "don't"), "don't");
    }
}
