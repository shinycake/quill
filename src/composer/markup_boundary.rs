//! Where a markdown marker may open or close (lib_ui `TagStartExpression`
//! in `input_field.cpp`, with the separators of `text_entity.cpp`): an
//! opening marker needs a separator, a line start or the text start before
//! it, a closing marker a separator or the text end after it, and neither may
//! touch another marker of its own kind. So `my_cmd_x`, `a_b_c` and `**x**y`
//! stay plain text while `_a_` and `(**x**)` format.
//!
//! One departure: the characters markers are made of count as separators.
//! Telegram Desktop turns a span into a tag the moment it is closed, so its
//! markers never meet; Quill writes nested and adjacent spans as markup
//! (`**_x_**`) and has to read them back.

/// Written by the markup writer between a marker and a neighbouring word
/// character, so a span that really starts or ends inside a word keeps its
/// formatting. It is tdesktop's own separator character (`\x10`) and is
/// dropped when the markup is parsed.
pub(crate) const BARRIER: char = '\u{10}';

/// `markup` without barriers, for a field that shows raw markup.
pub fn strip_barrier(markup: &str) -> String {
    markup.chars().filter(|&ch| ch != BARRIER).collect()
}

/// The characters markers are made of.
pub(crate) fn is_marker_char(ch: char) -> bool {
    matches!(ch, '*' | '_' | '~' | '|' | '`')
}

/// Telegram Desktop's `Separators()` (blank, punctuation, quotes) plus the
/// marker characters and `/`.
pub(crate) fn is_separator(ch: char) -> bool {
    matches!(
        ch,
        ' ' | '\u{10}'
            | '\n'
            | '\r'
            | '\t'
            | '.'
            | ','
            | ':'
            | ';'
            | '<'
            | '>'
            | '\''
            | '"'
            | '['
            | ']'
            | '{'
            | '}'
            | '!'
            | '?'
            | '%'
            | '^'
            | '('
            | ')'
            | '-'
            | '+'
            | '='
            | '/'
            | '\u{fdd0}'
            | '\u{fdd1}'
            | '\u{2028}'
            | '\u{2029}'
            | '«'
            | '»'
            | '\u{201c}'
            | '\u{201d}'
            | '\u{2018}'
            | '\u{2019}'
            | '…'
    ) || is_marker_char(ch)
}

fn char_before(text: &str, at: usize) -> Option<char> {
    text.get(..at)?.chars().next_back()
}

fn char_at(text: &str, at: usize) -> Option<char> {
    text.get(at..)?.chars().next()
}

/// Whether `marker` found at byte `at` of `text` may open a span.
pub(crate) fn opener_ok(text: &str, at: usize, marker: &str) -> bool {
    let own = marker.chars().next();
    let after = char_at(text, at + marker.len());
    char_before(text, at).is_none_or(is_separator)
        && after.is_none_or(|ch| Some(ch) != own && !(marker == "`" && matches!(ch, '\n' | '\r')))
}

/// Whether `marker` found at byte `at` of `text` may close a span.
pub(crate) fn closer_ok(text: &str, at: usize, marker: &str) -> bool {
    let own = marker.chars().next();
    let before = char_before(text, at);
    before.is_none_or(|ch| Some(ch) != own && !(marker == "`" && matches!(ch, '\n' | '\r')))
        // lib_ui gives `~~` no "good after" characters: it may touch a word.
        && (marker == "~~" || char_at(text, at + marker.len()).is_none_or(is_separator))
}

/// The byte offset of the first closing `marker` in `text[from..to]` that
/// leaves something between it and the opener.
pub(crate) fn find_closer(text: &str, from: usize, to: usize, marker: &str) -> Option<usize> {
    let window = text.get(from..to)?;
    let closer = window
        .match_indices(marker)
        .map(|(rel, _)| from + rel)
        .find(|&at| at > from && closer_ok(text, at, marker));
    super::scan_work::add(closer.unwrap_or(to).saturating_sub(from));
    closer
}

/// Whether a writer must put a [`BARRIER`] between a marker and `neighbour`.
pub(crate) fn needs_barrier(neighbour: Option<char>) -> bool {
    neighbour.is_some_and(|ch| !is_separator(ch))
}

#[cfg(test)]
mod tests {
    use super::{BARRIER, closer_ok, find_closer, needs_barrier, opener_ok};

    #[test]
    fn an_opener_needs_a_separator_before_it() {
        assert!(opener_ok("_a_", 0, "_"));
        assert!(opener_ok("x _a_", 2, "_"));
        assert!(opener_ok("(_a_)", 1, "_"));
        assert!(!opener_ok("my_cmd_x", 2, "_"));
        assert!(!opener_ok("a**b**", 1, "**"));
        assert!(opener_ok(&format!("a{BARRIER}**b**"), 2, "**"));
    }

    #[test]
    fn a_closer_needs_a_separator_after_it() {
        assert!(closer_ok("_a_", 2, "_"));
        assert!(closer_ok("_a_ b", 2, "_"));
        assert!(!closer_ok("**x**y", 3, "**"));
        assert!(!closer_ok("_a_b_c", 2, "_"));
        assert_eq!(find_closer("**x**y**", 2, 8, "**"), Some(6));
        assert_eq!(find_closer("_a_b_c", 1, 6, "_"), None);
        // Something has to sit between the markers.
        assert_eq!(find_closer("____", 2, 4, "_"), None);
    }

    #[test]
    fn a_marker_of_a_longer_run_is_not_a_boundary() {
        assert!(!opener_ok("***x**", 0, "**"));
        assert!(!closer_ok("**x***", 4, "**"));
        assert!(!opener_ok("`\nx`", 0, "`"));
    }

    #[test]
    fn only_word_characters_need_a_barrier() {
        assert!(needs_barrier(Some('a')));
        assert!(needs_barrier(Some('7')));
        assert!(needs_barrier(Some('é')));
        assert!(!needs_barrier(Some(' ')));
        assert!(!needs_barrier(Some('(')));
        assert!(!needs_barrier(Some('*')));
        assert!(!needs_barrier(None));
    }
}
